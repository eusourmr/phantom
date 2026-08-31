//! 2E-1 deterministic tokenizer foundation tests.

use phantom_html::{
    MAX_ATTRIBUTE_BYTES_PER_ELEMENT, MAX_ATTRIBUTES_PER_ELEMENT, MAX_COMMENT_BYTES,
    MAX_HTML_SOURCE_BYTES, MAX_RAW_START_TAG_BYTES,
    tokenizer::{
        CharacterReferenceContext, ParseErrorCode, SourceSpan, Token, TokenizerError,
        TokenizerState, tokenize,
    },
};

#[test]
fn token_stream_is_deterministic() -> Result<(), TokenizerError> {
    let source = "<!DOCTYPE html><!--x--><div id='a'>Hello</div>";
    let first = tokenize(source)?;
    let second = tokenize(source)?;

    assert_eq!(first, second);
    assert_eq!(first.final_state, TokenizerState::Data);
    Ok(())
}

#[test]
fn start_end_tags_attributes_and_utf8_spans_are_exact() -> Result<(), Box<dyn std::error::Error>> {
    let source = "<DIV ID='A'>Olá</DIV>";
    let output = tokenize(source)?;

    let start = output.tokens.iter().find_map(|token| match token {
        Token::StartTag(tag) => Some(tag),
        _ => None,
    });
    let start = start.ok_or_else(|| std::io::Error::other("start tag token"))?;
    assert_eq!(start.name, "div");
    assert_eq!(start.name_span, SourceSpan::new(1, 4));
    assert_eq!(start.span, SourceSpan::new(0, 12));
    assert_eq!(start.attributes.len(), 1);
    assert_eq!(start.attributes[0].name, "id");
    assert_eq!(start.attributes[0].value, "A");
    assert_eq!(start.attributes[0].name_span, SourceSpan::new(5, 7));
    assert_eq!(start.attributes[0].value_span, Some(SourceSpan::new(9, 10)));

    let text = output.tokens.iter().find_map(|token| match token {
        Token::Character(text) if text.data == "Olá" => Some(text),
        _ => None,
    });
    let text = text.ok_or_else(|| std::io::Error::other("UTF-8 character token"))?;
    assert_eq!(text.span, SourceSpan::new(12, 16));

    let end = output.tokens.iter().find_map(|token| match token {
        Token::EndTag(tag) => Some(tag),
        _ => None,
    });
    let end = end.ok_or_else(|| std::io::Error::other("end tag token"))?;
    assert_eq!(end.name, "div");
    assert_eq!(end.name_span, SourceSpan::new(18, 21));
    assert_eq!(end.span, SourceSpan::new(16, 22));
    Ok(())
}

#[test]
fn doctype_and_comment_tokens_are_emitted() -> Result<(), TokenizerError> {
    let output = tokenize("<!DOCTYPE html><!--Phantom-->")?;

    assert!(output.tokens.iter().any(|token| {
        matches!(
            token,
            Token::Doctype(doctype)
                if doctype.name.as_deref() == Some("html") && !doctype.force_quirks
        )
    }));
    assert!(
        output
            .tokens
            .iter()
            .any(|token| { matches!(token, Token::Comment(comment) if comment.data == "Phantom") })
    );
    assert!(output.parse_errors.is_empty());
    Ok(())
}

#[test]
fn malformed_tag_open_records_error_and_recovers_as_text() -> Result<(), TokenizerError> {
    let output = tokenize("<1>text")?;

    assert!(output.parse_errors.iter().any(|error| {
        error.code == ParseErrorCode::InvalidFirstCharacterOfTagName
            && error.span == SourceSpan::new(1, 2)
    }));

    let mut reconstructed = String::new();
    for token in &output.tokens {
        if let Token::Character(character) = token {
            reconstructed.push_str(&character.data);
        }
    }
    assert_eq!(reconstructed, "<1>text");
    Ok(())
}

#[test]
fn duplicate_attributes_keep_first_value_and_account_error()
-> Result<(), Box<dyn std::error::Error>> {
    let output = tokenize("<div A='first' a='second'></div>")?;
    let tag = output.tokens.iter().find_map(|token| match token {
        Token::StartTag(tag) => Some(tag),
        _ => None,
    });
    let tag = tag.ok_or_else(|| std::io::Error::other("start tag token"))?;

    assert_eq!(tag.attributes.len(), 1);
    assert_eq!(tag.attributes[0].name, "a");
    assert_eq!(tag.attributes[0].value, "first");
    assert!(
        output
            .parse_errors
            .iter()
            .any(|error| error.code == ParseErrorCode::DuplicateAttribute)
    );
    Ok(())
}

#[test]
fn missing_whitespace_after_quoted_value_is_recoverable() -> Result<(), Box<dyn std::error::Error>>
{
    let output = tokenize("<div a='1'b='2'>")?;
    let tag = output.tokens.iter().find_map(|token| match token {
        Token::StartTag(tag) => Some(tag),
        _ => None,
    });
    let tag = tag.ok_or_else(|| std::io::Error::other("start tag token"))?;

    assert_eq!(tag.attributes.len(), 2);
    assert!(
        output
            .parse_errors
            .iter()
            .any(|error| error.code == ParseErrorCode::MissingWhitespaceBetweenAttributes)
    );
    Ok(())
}

#[test]
fn unquoted_attribute_value_preserves_slashes() -> Result<(), TokenizerError> {
    let output = tokenize("<a href=/docs/start>Docs</a>")?;
    let href = output.tokens.iter().find_map(|token| match token {
        Token::StartTag(tag) => tag
            .attributes
            .iter()
            .find(|attribute| attribute.name == "href")
            .map(|attribute| attribute.value.as_str()),
        _ => None,
    });

    assert_eq!(href, Some("/docs/start"));
    Ok(())
}

#[test]
fn character_reference_seam_preserves_source_and_context() -> Result<(), TokenizerError> {
    let source = "A&amp;<a href='?x=1&amp;y=2'>B</a>";
    let output = tokenize(source)?;

    assert_eq!(output.character_references.len(), 2);
    assert_eq!(
        output.character_references[0].context,
        CharacterReferenceContext::Data
    );
    assert_eq!(
        output.character_references[1].context,
        CharacterReferenceContext::Attribute
    );

    let href = output.tokens.iter().find_map(|token| match token {
        Token::StartTag(tag) => tag
            .attributes
            .iter()
            .find(|attribute| attribute.name == "href")
            .map(|attribute| attribute.value.as_str()),
        _ => None,
    });
    assert_eq!(href, Some("?x=1&amp;y=2"));
    Ok(())
}

#[test]
fn nul_is_replaced_and_accounted_as_parse_error() -> Result<(), TokenizerError> {
    let output = tokenize("a\0b")?;

    assert!(
        output
            .parse_errors
            .iter()
            .any(|error| error.code == ParseErrorCode::UnexpectedNullCharacter)
    );
    assert!(output.tokens.iter().any(|token| {
        matches!(token, Token::Character(character) if character.data == "\u{fffd}")
    }));
    Ok(())
}

#[test]
fn unterminated_tag_records_eof_error_without_emitting_partial_tag() -> Result<(), TokenizerError> {
    let output = tokenize("<div")?;

    assert!(
        output
            .parse_errors
            .iter()
            .any(|error| error.code == ParseErrorCode::EofInTag)
    );
    assert!(
        !output
            .tokens
            .iter()
            .any(|token| matches!(token, Token::StartTag(_)))
    );
    assert_eq!(output.final_state, TokenizerState::TagName);
    Ok(())
}

#[test]
fn malformed_doctype_accounts_error_and_forces_quirks() -> Result<(), Box<dyn std::error::Error>> {
    let output = tokenize("<!DOCTYPE>")?;
    let doctype = output.tokens.iter().find_map(|token| match token {
        Token::Doctype(doctype) => Some(doctype),
        _ => None,
    });
    let doctype = doctype.ok_or_else(|| std::io::Error::other("doctype token"))?;

    assert!(doctype.force_quirks);
    assert!(doctype.name.is_none());
    assert!(
        output
            .parse_errors
            .iter()
            .any(|error| error.code == ParseErrorCode::MissingDoctypeName)
    );
    Ok(())
}

#[test]
fn rejects_source_above_2d6_budget() {
    let source = "x".repeat(MAX_HTML_SOURCE_BYTES.saturating_add(1));
    assert_eq!(tokenize(&source), Err(TokenizerError::SourceTooLarge));
}

#[test]
fn rejects_start_tag_above_2d6_scan_budget() {
    let padding = " ".repeat(MAX_RAW_START_TAG_BYTES.saturating_add(1));
    let source = format!("<div{padding}>");
    assert_eq!(tokenize(&source), Err(TokenizerError::StartTagTooLarge));
}

#[test]
fn rejects_attribute_fanout_above_2d6_budget() {
    let mut source = String::from("<div");
    for index in 0..=MAX_ATTRIBUTES_PER_ELEMENT {
        source.push_str(&format!(" a{index}=x"));
    }
    source.push('>');

    assert_eq!(tokenize(&source), Err(TokenizerError::TooManyAttributes));
}

#[test]
fn rejects_attribute_bytes_above_2d6_budget() {
    let value = "x".repeat(MAX_ATTRIBUTE_BYTES_PER_ELEMENT.saturating_add(1));
    let source = format!("<div data-x='{value}'>");

    assert_eq!(
        tokenize(&source),
        Err(TokenizerError::AttributeBytesExceeded)
    );
}

#[test]
fn rejects_comment_above_2d6_budget() {
    let comment = "x".repeat(MAX_COMMENT_BYTES.saturating_add(1));
    let source = format!("<!--{comment}-->");

    assert_eq!(tokenize(&source), Err(TokenizerError::CommentTooLarge));
}

#[test]
fn greater_than_inside_quoted_attribute_does_not_close_tag() -> Result<(), TokenizerError> {
    let output = tokenize("<div data-expression='a > b'><p>x</p></div>")?;
    let value = output.tokens.iter().find_map(|token| match token {
        Token::StartTag(tag) if tag.name == "div" => tag
            .attributes
            .iter()
            .find(|attribute| attribute.name == "data-expression")
            .map(|attribute| attribute.value.as_str()),
        _ => None,
    });

    assert_eq!(value, Some("a > b"));
    Ok(())
}
