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

/// Maximum accepted HTML source size per document.
pub const MAX_HTML_SOURCE_BYTES: usize = 4 * 1024 * 1024;
/// Maximum open-element nesting depth, excluding the synthetic document root.
pub const MAX_HTML_NESTING_DEPTH: usize = 256;
/// Maximum attributes scanned for one start tag.
pub const MAX_ATTRIBUTES_PER_ELEMENT: usize = 128;
/// Maximum aggregate normalized attribute bytes retained for one element.
pub const MAX_ATTRIBUTE_BYTES_PER_ELEMENT: usize = 1024 * 1024;
/// Maximum bytes scanned inside one raw start tag, excluding angle brackets.
pub const MAX_RAW_START_TAG_BYTES: usize = 2 * 1024 * 1024;
/// Maximum bytes retained by one ordinary text node.
pub const MAX_TEXT_NODE_BYTES: usize = 1024 * 1024;
/// Maximum aggregate bytes retained by document text nodes.
pub const MAX_RETAINED_TEXT_BYTES: usize = 3 * 1024 * 1024;
/// Maximum bytes retained by one HTML comment.
pub const MAX_COMMENT_BYTES: usize = 256 * 1024;
/// Maximum bytes retained by one style raw-text body.
pub const MAX_STYLE_BODY_BYTES: usize = 1024 * 1024;

/// Errors produced while transforming HTML into a Phantom DOM.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HtmlError {
    /// The DOM rejected a requested tree mutation.
    #[error("DOM error: {0}")]
    Dom(#[from] DomError),

    /// The input document exceeded the source admission budget.
    #[error("HTML source exceeds {MAX_HTML_SOURCE_BYTES} bytes")]
    SourceTooLarge,

    /// Open-element nesting exceeded the parser depth budget.
    #[error("HTML nesting exceeds {MAX_HTML_NESTING_DEPTH} elements")]
    NestingTooDeep,

    /// A start tag exceeded the raw scan budget.
    #[error("HTML start tag exceeds {MAX_RAW_START_TAG_BYTES} bytes")]
    StartTagTooLarge,

    /// A start tag contained more attributes than the parser will scan.
    #[error("HTML element exceeds {MAX_ATTRIBUTES_PER_ELEMENT} attributes")]
    TooManyAttributes,

    /// Retained attributes for one element exceeded their byte budget.
    #[error("HTML element attributes exceed {MAX_ATTRIBUTE_BYTES_PER_ELEMENT} bytes")]
    AttributeBytesExceeded,

    /// An ordinary text node exceeded its individual retention budget.
    #[error("HTML text node exceeds {MAX_TEXT_NODE_BYTES} bytes")]
    TextNodeTooLarge,

    /// Aggregate retained text exceeded the document budget.
    #[error("HTML retained text exceeds {MAX_RETAINED_TEXT_BYTES} bytes")]
    RetainedTextTooLarge,

    /// A comment exceeded its individual retention budget.
    #[error("HTML comment exceeds {MAX_COMMENT_BYTES} bytes")]
    CommentTooLarge,

    /// A style body exceeded its raw-text retention budget.
    #[error("HTML style body exceeds {MAX_STYLE_BODY_BYTES} bytes")]
    StyleBodyTooLarge,
}

/// Parses HTML source into a new Phantom [`Document`].
///
/// This bounded parser recognizes ordinary tags, attributes, text, comments,
/// void elements, basic character entities, and raw `<style>` contents.
/// JavaScript source is deliberately ignored. Security budgets are admission
/// controls: inputs exceeding them fail deterministically instead of being
/// partially retained.
///
/// # Errors
///
/// Returns [`HtmlError`] when a parser/DOM security budget is exceeded or the
/// DOM rejects a tree mutation.
pub fn parse(source: &str) -> Result<Document, HtmlError> {
    if source.len() > MAX_HTML_SOURCE_BYTES {
        return Err(HtmlError::SourceTooLarge);
    }

    let mut document = Document::new();
    let root = document.root();
    let mut stack = vec![root];
    let mut cursor = 0;
    let mut retained_text_bytes = 0_usize;

    while cursor < source.len() {
        let remaining = &source[cursor..];

        if remaining.starts_with("<!--") {
            if let Some(relative_end) = remaining.find("-->") {
                let comment = &remaining[4..relative_end];
                if comment.len() > MAX_COMMENT_BYTES {
                    return Err(HtmlError::CommentTooLarge);
                }

                document.append_child(
                    current_parent(&stack, root),
                    NodeKind::Comment(comment.to_owned()),
                )?;

                cursor = cursor.saturating_add(relative_end).saturating_add(3);
                continue;
            }

            if remaining.len().saturating_sub(4) > MAX_COMMENT_BYTES {
                return Err(HtmlError::CommentTooLarge);
            }
            break;
        }

        if remaining.starts_with("</") {
            if let Some(relative_end) = remaining.find('>') {
                let raw_name = &remaining[2..relative_end];
                let tag_name = normalize_name(raw_name);
                close_element(&document, &mut stack, &tag_name);
                cursor = cursor.saturating_add(relative_end).saturating_add(1);
                continue;
            }

            break;
        }

        if remaining.starts_with("<!") {
            if let Some(relative_end) = remaining.find('>') {
                cursor = cursor.saturating_add(relative_end).saturating_add(1);
                continue;
            }

            break;
        }

        if remaining.starts_with('<') {
            if let Some(relative_end) = find_start_tag_end(remaining)? {
                let raw_tag = &remaining[1..relative_end];

                let parsed = parse_start_tag(raw_tag)?;

                if !parsed.name.is_empty() {
                    let element =
                        ElementData::with_attributes(parsed.name.clone(), parsed.attributes);
                    let node_id = document
                        .append_child(current_parent(&stack, root), NodeKind::Element(element))?;
                    let next_cursor = cursor.saturating_add(relative_end).saturating_add(1);

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
                                if css.len() > MAX_STYLE_BODY_BYTES {
                                    return Err(HtmlError::StyleBodyTooLarge);
                                }
                                retain_text_bytes(&mut retained_text_bytes, css.len())?;
                                document.append_child(node_id, NodeKind::Text(css.to_owned()))?;
                            }

                            cursor = after_close;
                            continue;
                        }

                        if source.len().saturating_sub(next_cursor) > MAX_STYLE_BODY_BYTES {
                            return Err(HtmlError::StyleBodyTooLarge);
                        }
                        cursor = source.len();
                        continue;
                    }

                    if !parsed.self_closing && !is_void_element(&parsed.name) {
                        if stack.len().saturating_sub(1) >= MAX_HTML_NESTING_DEPTH {
                            return Err(HtmlError::NestingTooDeep);
                        }
                        stack.push(node_id);
                    }
                }

                cursor = cursor.saturating_add(relative_end).saturating_add(1);
                continue;
            }

            break;
        }

        let next_tag = remaining.find('<').unwrap_or(remaining.len());

        if next_tag == 0 {
            cursor = cursor.saturating_add(1);
            continue;
        }

        let text = &remaining[..next_tag];
        if text.len() > MAX_TEXT_NODE_BYTES {
            return Err(HtmlError::TextNodeTooLarge);
        }

        let decoded = decode_basic_entities(text);

        if !decoded.trim().is_empty() {
            if decoded.len() > MAX_TEXT_NODE_BYTES {
                return Err(HtmlError::TextNodeTooLarge);
            }
            retain_text_bytes(&mut retained_text_bytes, decoded.len())?;
            document.append_child(current_parent(&stack, root), NodeKind::Text(decoded))?;
        }

        cursor = cursor.saturating_add(next_tag);
    }

    Ok(document)
}

#[derive(Debug)]
struct ParsedStartTag {
    name: String,
    attributes: BTreeMap<String, String>,
    self_closing: bool,
}

fn find_start_tag_end(remaining: &str) -> Result<Option<usize>, HtmlError> {
    let bytes = remaining.as_bytes();
    if bytes.first() != Some(&b'<') {
        return Ok(None);
    }

    let mut cursor = 1_usize;
    let mut quote = None;

    while cursor < bytes.len() {
        let byte = bytes[cursor];

        if let Some(active_quote) = quote {
            if byte == active_quote {
                quote = None;
            }
        } else {
            match byte {
                b'"' | b'\'' => quote = Some(byte),
                b'>' => {
                    let raw_bytes = cursor.saturating_sub(1);
                    if raw_bytes > MAX_RAW_START_TAG_BYTES {
                        return Err(HtmlError::StartTagTooLarge);
                    }
                    return Ok(Some(cursor));
                }
                _ => {}
            }
        }

        if cursor.saturating_sub(1) > MAX_RAW_START_TAG_BYTES {
            return Err(HtmlError::StartTagTooLarge);
        }
        cursor = cursor.saturating_add(1);
    }

    if bytes.len().saturating_sub(1) > MAX_RAW_START_TAG_BYTES {
        return Err(HtmlError::StartTagTooLarge);
    }

    Ok(None)
}

fn parse_start_tag(raw: &str) -> Result<ParsedStartTag, HtmlError> {
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
    let mut attribute_count = 0_usize;
    let mut attribute_bytes = 0_usize;

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
            cursor = cursor.saturating_add(1);
            continue;
        }

        attribute_count = attribute_count.saturating_add(1);
        if attribute_count > MAX_ATTRIBUTES_PER_ELEMENT {
            return Err(HtmlError::TooManyAttributes);
        }

        skip_ascii_whitespace(bytes, &mut cursor);
        let mut value = String::new();

        if cursor < bytes.len() && bytes[cursor] == b'=' {
            cursor += 1;
            skip_ascii_whitespace(bytes, &mut cursor);
            value = parse_attribute_value(content, bytes, &mut cursor);
        }

        attribute_bytes = attribute_bytes
            .saturating_add(attribute_name.len())
            .saturating_add(value.len());
        if attribute_bytes > MAX_ATTRIBUTE_BYTES_PER_ELEMENT {
            return Err(HtmlError::AttributeBytesExceeded);
        }

        attributes.insert(attribute_name, value);
    }

    Ok(ParsedStartTag {
        name,
        attributes,
        self_closing,
    })
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

        let value = decode_basic_entities(&content[value_start..*cursor]);

        if *cursor < bytes.len() {
            *cursor += 1;
        }

        return value;
    }

    let value_start = *cursor;

    while *cursor < bytes.len() && !bytes[*cursor].is_ascii_whitespace() && bytes[*cursor] != b'/' {
        *cursor += 1;
    }

    decode_basic_entities(&content[value_start..*cursor])
}

fn retain_text_bytes(total: &mut usize, additional: usize) -> Result<(), HtmlError> {
    *total = total.saturating_add(additional);
    if *total > MAX_RETAINED_TEXT_BYTES {
        return Err(HtmlError::RetainedTextTooLarge);
    }
    Ok(())
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
    let needle = format!("</{tag_name}>");
    let needle_bytes = needle.as_bytes();
    let bytes = source.as_bytes();

    if search_start > bytes.len() || needle_bytes.is_empty() || needle_bytes.len() > bytes.len() {
        return None;
    }

    let last_start = bytes.len().saturating_sub(needle_bytes.len());
    let mut cursor = search_start;

    while cursor <= last_start {
        if bytes[cursor] == b'<'
            && bytes[cursor..cursor + needle_bytes.len()].eq_ignore_ascii_case(needle_bytes)
        {
            return Some((cursor, cursor.saturating_add(needle_bytes.len())));
        }
        cursor = cursor.saturating_add(1);
    }

    None
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
        let contains_phantom = document
            .nodes()
            .any(|node| matches!(node.kind(), NodeKind::Text(text) if text == "Phantom"));
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
        let contains_css = document
            .nodes()
            .any(|node| matches!(node.kind(), NodeKind::Text(text) if text.contains("color: red")));
        assert!(contains_css);
        Ok(())
    }

    #[test]
    fn ignores_script_contents() -> Result<(), HtmlError> {
        let document = parse("<body><script>if (a < b) {}</script><p>Visible</p></body>")?;
        let contains_visible = document
            .nodes()
            .any(|node| matches!(node.kind(), NodeKind::Text(text) if text == "Visible"));
        let contains_script_text = document
            .nodes()
            .any(|node| matches!(node.kind(), NodeKind::Text(text) if text.contains("if (a")));
        assert!(contains_visible);
        assert!(!contains_script_text);
        Ok(())
    }

    #[test]
    fn greater_than_inside_quoted_attribute_does_not_close_start_tag() -> Result<(), HtmlError> {
        let document = parse(r#"<div data-expression="a > b"><p>Visible</p></div>"#)?;
        let attribute = document.nodes().find_map(|node| match node.kind() {
            NodeKind::Element(element) if element.tag_name() == "div" => {
                element.attribute("data-expression")
            }
            _ => None,
        });
        assert_eq!(attribute, Some("a > b"));
        assert!(
            document
                .nodes()
                .any(|node| { matches!(node.kind(), NodeKind::Text(text) if text == "Visible") })
        );
        Ok(())
    }

    #[test]
    fn raw_text_close_is_ascii_case_insensitive_without_full_lowercase_copy()
    -> Result<(), HtmlError> {
        let document = parse("<style>p{color:red}</StYlE><p>Visible</p>")?;
        assert!(
            document
                .nodes()
                .any(|node| { matches!(node.kind(), NodeKind::Text(text) if text == "Visible") })
        );
        Ok(())
    }
}
