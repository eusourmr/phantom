//! Independent HTML tokenizer and tree builder for Phantom.
//!
//! This crate converts HTML source into Phantom's own DOM representation.
//! It implements a deliberately constrained first subset of HTML while the
//! standards-compliant parser evolves incrementally.
//!
//! It does not delegate parsing to Chromium, WebKit, Gecko, Servo, or another
//! browser engine.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use phantom_dom::{Document, DomError, ElementData, NodeId, NodeKind};
use thiserror::Error;

/// Errors produced while transforming HTML into a Phantom DOM.
#[derive(Debug, Error)]
pub enum HtmlError {
    /// The DOM rejected a requested tree mutation.
    #[error("DOM error: {0}")]
    Dom(#[from] DomError),
}

/// Parses HTML source into a new Phantom [`Document`].
///
/// This initial parser recognizes ordinary tags, attributes, text, comments,
/// void elements, basic character entities, and raw `<style>` contents.
/// JavaScript source is deliberately ignored.
///
/// # Errors
///
/// Returns [`HtmlError`] if the DOM rejects a tree mutation.
pub fn parse(source: &str) -> Result<Document, HtmlError> {
    let mut document = Document::new();
    let root = document.root();

    let mut stack = vec![root];
    let mut cursor = 0;

    while cursor < source.len() {
        let remaining = &source[cursor..];

        if remaining.starts_with("<!--") {
            if let Some(relative_end) = remaining.find("-->") {
                let comment = &remaining[4..relative_end];

                document.append_child(
                    current_parent(&stack, root),
                    NodeKind::Comment(comment.to_owned()),
                )?;

                cursor += relative_end + 3;
                continue;
            }

            break;
        }

        if remaining.starts_with("</") {
            if let Some(relative_end) = remaining.find('>') {
                let raw_name = &remaining[2..relative_end];
                let tag_name = normalize_name(raw_name);

                close_element(&document, &mut stack, &tag_name);

                cursor += relative_end + 1;
                continue;
            }

            break;
        }

        if remaining.starts_with("<!") {
            if let Some(relative_end) = remaining.find('>') {
                cursor += relative_end + 1;
                continue;
            }

            break;
        }

        if remaining.starts_with('<') {
            if let Some(relative_end) = remaining.find('>') {
                let raw_tag = &remaining[1..relative_end];

                let parsed = parse_start_tag(raw_tag);

                if !parsed.name.is_empty() {
                    let element =
                        ElementData::with_attributes(parsed.name.clone(), parsed.attributes);

                    let node_id = document
                        .append_child(current_parent(&stack, root), NodeKind::Element(element))?;

                    let next_cursor = cursor + relative_end + 1;

                    if parsed.name == "script" {
                        if let Some((_, after_close)) =
                            find_raw_text_close(source, next_cursor, "script")
                        {
                            cursor = after_close;
                            continue;
                        }

                        cursor = source.len();
                        continue;
                    }

                    if parsed.name == "style" {
                        if let Some((content_end, after_close)) =
                            find_raw_text_close(source, next_cursor, "style")
                        {
                            if let Some(css) = source.get(next_cursor..content_end)
                                && !css.is_empty()
                            {
                                document.append_child(node_id, NodeKind::Text(css.to_owned()))?;
                            }

                            cursor = after_close;
                            continue;
                        }

                        cursor = source.len();
                        continue;
                    }

                    if !parsed.self_closing && !is_void_element(&parsed.name) {
                        stack.push(node_id);
                    }
                }

                cursor += relative_end + 1;
                continue;
            }

            break;
        }

        let next_tag = remaining.find('<').unwrap_or(remaining.len());

        if next_tag == 0 {
            cursor += 1;
            continue;
        }

        let text = &remaining[..next_tag];

        let decoded = decode_basic_entities(text);

        if !decoded.trim().is_empty() {
            document.append_child(current_parent(&stack, root), NodeKind::Text(decoded))?;
        }

        cursor += next_tag;
    }

    Ok(document)
}

#[derive(Debug)]
struct ParsedStartTag {
    name: String,
    attributes: BTreeMap<String, String>,
    self_closing: bool,
}

fn parse_start_tag(raw: &str) -> ParsedStartTag {
    let trimmed = raw.trim();

    let self_closing = trimmed.ends_with('/');

    let content = if self_closing {
        trimmed.strip_suffix('/').unwrap_or(trimmed).trim_end()
    } else {
        trimmed
    };

    let bytes = content.as_bytes();
    let mut cursor = 0;

    skip_ascii_whitespace(bytes, &mut cursor);

    let name_start = cursor;

    while cursor < bytes.len() && !bytes[cursor].is_ascii_whitespace() && bytes[cursor] != b'/' {
        cursor += 1;
    }

    let name = normalize_name(&content[name_start..cursor]);

    let mut attributes = BTreeMap::new();

    while cursor < bytes.len() {
        skip_ascii_whitespace(bytes, &mut cursor);

        if cursor >= bytes.len() {
            break;
        }

        let attribute_start = cursor;

        while cursor < bytes.len()
            && !bytes[cursor].is_ascii_whitespace()
            && bytes[cursor] != b'='
            && bytes[cursor] != b'/'
        {
            cursor += 1;
        }

        let attribute_name = normalize_name(&content[attribute_start..cursor]);

        if attribute_name.is_empty() {
            cursor += 1;
            continue;
        }

        skip_ascii_whitespace(bytes, &mut cursor);

        let mut value = String::new();

        if cursor < bytes.len() && bytes[cursor] == b'=' {
            cursor += 1;

            skip_ascii_whitespace(bytes, &mut cursor);

            value = parse_attribute_value(content, bytes, &mut cursor);
        }

        attributes.insert(attribute_name, value);
    }

    ParsedStartTag {
        name,
        attributes,
        self_closing,
    }
}

fn parse_attribute_value(content: &str, bytes: &[u8], cursor: &mut usize) -> String {
    if *cursor >= bytes.len() {
        return String::new();
    }

    let quote = bytes[*cursor];

    if quote == b'"' || quote == b'\'' {
        *cursor += 1;

        let value_start = *cursor;

        while *cursor < bytes.len() && bytes[*cursor] != quote {
            *cursor += 1;
        }

        let value = content[value_start..*cursor].to_owned();

        if *cursor < bytes.len() {
            *cursor += 1;
        }

        return decode_basic_entities(&value);
    }

    let value_start = *cursor;

    while *cursor < bytes.len() && !bytes[*cursor].is_ascii_whitespace() && bytes[*cursor] != b'/' {
        *cursor += 1;
    }

    decode_basic_entities(&content[value_start..*cursor])
}

fn skip_ascii_whitespace(bytes: &[u8], cursor: &mut usize) {
    while *cursor < bytes.len() && bytes[*cursor].is_ascii_whitespace() {
        *cursor += 1;
    }
}

fn current_parent(stack: &[NodeId], root: NodeId) -> NodeId {
    stack.last().copied().unwrap_or(root)
}

fn close_element(document: &Document, stack: &mut Vec<NodeId>, tag_name: &str) {
    while stack.len() > 1 {
        let Some(node_id) = stack.last().copied() else {
            return;
        };

        let matches = document
            .node(node_id)
            .and_then(|node| match node.kind() {
                NodeKind::Element(element) => Some(element.tag_name() == tag_name),
                _ => None,
            })
            .unwrap_or(false);

        stack.pop();

        if matches {
            return;
        }
    }
}

fn normalize_name(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn is_void_element(tag_name: &str) -> bool {
    matches!(
        tag_name,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

fn find_raw_text_close(
    source: &str,
    search_start: usize,
    tag_name: &str,
) -> Option<(usize, usize)> {
    let remaining = source.get(search_start..)?;
    let needle = format!("</{tag_name}>");
    let lower = remaining.to_ascii_lowercase();
    let relative_start = lower.find(&needle)?;
    let content_end = search_start + relative_start;
    let after_close = content_end + needle.len();

    Some((content_end, after_close))
}

fn decode_basic_entities(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", "\u{00a0}")
}

#[cfg(test)]
mod tests {
    use super::{HtmlError, parse};
    use phantom_dom::NodeKind;

    #[test]
    fn parses_heading_and_paragraph() -> Result<(), HtmlError> {
        let document = parse("<html><body><h1>Phantom</h1><p>Hello world</p></body></html>")?;

        let contains_phantom = document.nodes().any(|node| {
            matches!(
                node.kind(),
                NodeKind::Text(text) if text == "Phantom"
            )
        });

        assert!(contains_phantom);

        Ok(())
    }

    #[test]
    fn parses_element_attributes() -> Result<(), HtmlError> {
        let document = parse(r#"<a href="https://example.com" target="_blank">Example</a>"#)?;

        let href = document.nodes().find_map(|node| match node.kind() {
            NodeKind::Element(element) if element.tag_name() == "a" => element.attribute("href"),
            _ => None,
        });

        assert_eq!(href, Some("https://example.com"));

        Ok(())
    }

    #[test]
    fn preserves_style_contents_for_css_engine() -> Result<(), HtmlError> {
        let document = parse("<style>p { color: red; }</style><p>Visible</p>")?;

        let contains_css = document.nodes().any(|node| {
            matches!(
                node.kind(),
                NodeKind::Text(text) if text.contains("color: red")
            )
        });

        assert!(contains_css);

        Ok(())
    }

    #[test]
    fn ignores_script_contents() -> Result<(), HtmlError> {
        let document = parse("<body><script>if (a < b) {}</script><p>Visible</p></body>")?;

        let contains_visible = document.nodes().any(|node| {
            matches!(
                node.kind(),
                NodeKind::Text(text) if text == "Visible"
            )
        });

        let contains_script_text = document.nodes().any(|node| {
            matches!(
                node.kind(),
                NodeKind::Text(text) if text.contains("if (a")
            )
        });

        assert!(contains_visible);
        assert!(!contains_script_text);

        Ok(())
    }
}
