//! Deterministic, bounded HTML tokenization foundation for Phantom.
//!
//! This module is intentionally separate from the current DOM tree builder.
//! `2E-1` establishes a standards-shaped token stream that `2E-2` can consume
//! without changing the already-homologated parser behavior in one step.
//!
//! Character references are identified as candidates but are not resolved yet;
//! resolution and malformed-reference recovery belong to the `2E-3` scope.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::{
    MAX_ATTRIBUTE_BYTES_PER_ELEMENT, MAX_ATTRIBUTES_PER_ELEMENT, MAX_COMMENT_BYTES,
    MAX_HTML_SOURCE_BYTES, MAX_RAW_START_TAG_BYTES,
};

/// Half-open byte range in the original UTF-8 source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceSpan {
    /// Inclusive byte offset.
    pub start: usize,
    /// Exclusive byte offset.
    pub end: usize,
}

impl SourceSpan {
    /// Creates a source span.
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Returns the span length in bytes.
    #[must_use]
    pub const fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// Returns whether the span is empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start >= self.end
    }
}

/// Tokenizer states established by the 2E-1 foundation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenizerState {
    /// Ordinary character data.
    Data,
    /// Immediately after `<`.
    TagOpen,
    /// Immediately after `</`.
    EndTagOpen,
    /// Start- or end-tag name.
    TagName,
    /// Before an attribute name.
    BeforeAttributeName,
    /// Attribute name.
    AttributeName,
    /// After an attribute name.
    AfterAttributeName,
    /// Before an attribute value.
    BeforeAttributeValue,
    /// Double-quoted attribute value.
    AttributeValueDoubleQuoted,
    /// Single-quoted attribute value.
    AttributeValueSingleQuoted,
    /// Unquoted attribute value.
    AttributeValueUnquoted,
    /// Immediately after a quoted attribute value.
    AfterAttributeValueQuoted,
    /// After `/` in a tag.
    SelfClosingStartTag,
    /// Immediately after `<!`.
    MarkupDeclarationOpen,
    /// HTML comment.
    Comment,
    /// DOCTYPE token.
    Doctype,
    /// Error-recovery comment state.
    BogusComment,
}

/// Attribute retained by a start/end tag token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeToken {
    /// ASCII-normalized attribute name.
    pub name: String,
    /// Attribute value. Character references are not resolved in 2E-1.
    pub value: String,
    /// Source span containing the attribute name.
    pub name_span: SourceSpan,
    /// Source span containing the raw value without surrounding quotes.
    pub value_span: Option<SourceSpan>,
}

/// Start/end tag token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagToken {
    /// ASCII-normalized tag name.
    pub name: String,
    /// Source span containing the tag name only.
    pub name_span: SourceSpan,
    /// Attributes in deterministic source order.
    pub attributes: Vec<AttributeToken>,
    /// Whether a trailing self-closing solidus was present.
    pub self_closing: bool,
    /// Source span covering the complete token.
    pub span: SourceSpan,
}

/// Character-data token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterToken {
    /// Character data.
    pub data: String,
    /// Source span that produced the data.
    pub span: SourceSpan,
}

/// Comment token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentToken {
    /// Comment contents without delimiters.
    pub data: String,
    /// Source span covering the complete comment or recovery token.
    pub span: SourceSpan,
}

/// DOCTYPE token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctypeToken {
    /// ASCII-normalized doctype name, when present.
    pub name: Option<String>,
    /// Whether malformed input requires quirks-mode treatment downstream.
    pub force_quirks: bool,
    /// Source span covering the complete declaration.
    pub span: SourceSpan,
}

/// Deterministic tokenizer output token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// Ordinary character data.
    Character(CharacterToken),
    /// Start tag.
    StartTag(TagToken),
    /// End tag.
    EndTag(TagToken),
    /// Comment.
    Comment(CommentToken),
    /// DOCTYPE declaration.
    Doctype(DoctypeToken),
    /// End of input.
    Eof {
        /// Final source byte offset.
        position: usize,
    },
}

/// Recoverable tokenizer parse-error category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ParseErrorCode {
    /// U+0000 occurred where HTML replaces it with U+FFFD.
    UnexpectedNullCharacter,
    /// EOF followed `<` before a tag name could be read.
    EofBeforeTagName,
    /// A character cannot start a tag name.
    InvalidFirstCharacterOfTagName,
    /// `?` appeared after `<`; HTML recovers as a bogus comment.
    UnexpectedQuestionMarkInsteadOfTagName,
    /// Input ended while a tag was incomplete.
    EofInTag,
    /// `=` unexpectedly started an attribute name.
    UnexpectedEqualsSignBeforeAttributeName,
    /// Invalid punctuation appeared inside an attribute name.
    UnexpectedCharacterInAttributeName,
    /// A stray solidus appeared where a self-closing marker was not valid.
    UnexpectedSolidusInTag,
    /// An attribute had `=` but no following value.
    MissingAttributeValue,
    /// Forbidden syntax appeared inside an unquoted attribute value.
    UnexpectedCharacterInUnquotedAttributeValue,
    /// An attribute started immediately after a quoted value without whitespace.
    MissingWhitespaceBetweenAttributes,
    /// The same normalized attribute name occurred more than once.
    DuplicateAttribute,
    /// An end tag carried attributes.
    EndTagWithAttributes,
    /// An end tag used a self-closing solidus.
    EndTagWithTrailingSolidus,
    /// EOF occurred before a comment terminator.
    EofInComment,
    /// `<!` was not followed by a supported comment/DOCTYPE opener.
    IncorrectlyOpenedComment,
    /// DOCTYPE omitted required whitespace before its name.
    MissingWhitespaceBeforeDoctypeName,
    /// DOCTYPE had no name.
    MissingDoctypeName,
    /// EOF occurred while parsing a DOCTYPE.
    EofInDoctype,
}

/// Recoverable parse error with an exact source location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// Error category.
    pub code: ParseErrorCode,
    /// Source location associated with the error.
    pub span: SourceSpan,
}

/// Context in which a future character-reference resolver must operate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharacterReferenceContext {
    /// Ordinary data state.
    Data,
    /// Attribute value.
    Attribute,
}

/// An ampersand that forms the seam for 2E-3 character-reference handling.
///
/// The 2E-1 tokenizer deliberately leaves the source text unchanged. A later
/// resolver can consume these deterministic candidates using their exact
/// source positions and parsing context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharacterReferenceCandidate {
    /// Span covering the source ampersand.
    pub span: SourceSpan,
    /// Parsing context in which the candidate occurred.
    pub context: CharacterReferenceContext,
}

/// Successful deterministic tokenization result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tokenization {
    /// Ordered token stream, always terminated by [`Token::Eof`].
    pub tokens: Vec<Token>,
    /// Ordered recoverable parse errors.
    pub parse_errors: Vec<ParseError>,
    /// Ordered character-reference candidates for 2E-3.
    pub character_references: Vec<CharacterReferenceCandidate>,
    /// Last tokenizer state observed before EOF.
    pub final_state: TokenizerState,
}

/// Fatal tokenizer admission-budget violation.
///
/// Recoverable malformed HTML is represented by [`ParseError`] instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenizerError {
    /// Document source exceeded the 2D-6 source budget.
    SourceTooLarge,
    /// A start tag exceeded the 2D-6 scan budget.
    StartTagTooLarge,
    /// A start tag exceeded the 2D-6 attribute-count budget.
    TooManyAttributes,
    /// A start tag exceeded the 2D-6 retained attribute-byte budget.
    AttributeBytesExceeded,
    /// A comment exceeded the 2D-6 comment budget.
    CommentTooLarge,
}

impl Display for TokenizerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceTooLarge => {
                write!(
                    formatter,
                    "HTML source exceeds {MAX_HTML_SOURCE_BYTES} bytes"
                )
            }
            Self::StartTagTooLarge => {
                write!(
                    formatter,
                    "HTML start tag exceeds {MAX_RAW_START_TAG_BYTES} bytes"
                )
            }
            Self::TooManyAttributes => {
                write!(
                    formatter,
                    "HTML element exceeds {MAX_ATTRIBUTES_PER_ELEMENT} attributes"
                )
            }
            Self::AttributeBytesExceeded => {
                write!(
                    formatter,
                    "HTML element attributes exceed {MAX_ATTRIBUTE_BYTES_PER_ELEMENT} bytes"
                )
            }
            Self::CommentTooLarge => {
                write!(formatter, "HTML comment exceeds {MAX_COMMENT_BYTES} bytes")
            }
        }
    }
}

impl Error for TokenizerError {}

/// Tokenizes HTML source into Phantom's deterministic, bounded token stream.
///
/// This is the 2E-1 tokenizer foundation. It does not yet replace [`crate::parse`];
/// migration of token output into the DOM tree builder is the 2E-2 milestone.
///
/// # Errors
///
/// Returns [`TokenizerError`] when source, start-tag, attribute, or comment
/// admission budgets inherited from 2D-6 are exceeded.
pub fn tokenize(source: &str) -> Result<Tokenization, TokenizerError> {
    if source.len() > MAX_HTML_SOURCE_BYTES {
        return Err(TokenizerError::SourceTooLarge);
    }

    Tokenizer::new(source).run()
}

struct Tokenizer<'a> {
    source: &'a str,
    bytes: &'a [u8],
    cursor: usize,
    state: TokenizerState,
    tokens: Vec<Token>,
    parse_errors: Vec<ParseError>,
    character_references: Vec<CharacterReferenceCandidate>,
}

impl<'a> Tokenizer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            cursor: 0,
            state: TokenizerState::Data,
            tokens: Vec::new(),
            parse_errors: Vec::new(),
            character_references: Vec::new(),
        }
    }

    fn run(mut self) -> Result<Tokenization, TokenizerError> {
        while self.cursor < self.bytes.len() {
            self.state = TokenizerState::Data;
            self.consume_data()?;
        }

        self.tokens.push(Token::Eof {
            position: self.source.len(),
        });

        Ok(Tokenization {
            tokens: self.tokens,
            parse_errors: self.parse_errors,
            character_references: self.character_references,
            final_state: self.state,
        })
    }

    fn consume_data(&mut self) -> Result<(), TokenizerError> {
        let start = self.cursor;

        while self.cursor < self.bytes.len() {
            match self.bytes[self.cursor] {
                b'<' | b'&' | b'\0' => break,
                _ => self.advance_char(),
            }
        }

        self.emit_source_characters(start, self.cursor);

        if self.cursor >= self.bytes.len() {
            return Ok(());
        }

        match self.bytes[self.cursor] {
            b'<' => self.consume_tag_open(),
            b'&' => {
                let start = self.cursor;
                self.character_references.push(CharacterReferenceCandidate {
                    span: SourceSpan::new(start, start.saturating_add(1)),
                    context: CharacterReferenceContext::Data,
                });
                self.cursor = self.cursor.saturating_add(1);
                self.emit_owned_character("&".to_owned(), start, self.cursor);
                Ok(())
            }
            b'\0' => {
                let start = self.cursor;
                self.push_error(
                    ParseErrorCode::UnexpectedNullCharacter,
                    SourceSpan::new(start, start.saturating_add(1)),
                );
                self.cursor = self.cursor.saturating_add(1);
                self.emit_owned_character("\u{fffd}".to_owned(), start, self.cursor);
                Ok(())
            }
            _ => {
                self.advance_char();
                Ok(())
            }
        }
    }

    fn consume_tag_open(&mut self) -> Result<(), TokenizerError> {
        self.state = TokenizerState::TagOpen;
        let tag_start = self.cursor;
        self.cursor = self.cursor.saturating_add(1);

        if self.cursor >= self.bytes.len() {
            self.push_error(
                ParseErrorCode::EofBeforeTagName,
                SourceSpan::new(tag_start, self.cursor),
            );
            self.emit_owned_character("<".to_owned(), tag_start, self.cursor);
            return Ok(());
        }

        match self.bytes[self.cursor] {
            b'!' => {
                self.cursor = self.cursor.saturating_add(1);
                self.consume_markup_declaration(tag_start)
            }
            b'/' => {
                self.cursor = self.cursor.saturating_add(1);
                self.consume_end_tag_open(tag_start)
            }
            b'?' => {
                self.push_error(
                    ParseErrorCode::UnexpectedQuestionMarkInsteadOfTagName,
                    self.current_char_span(),
                );
                self.consume_bogus_comment(tag_start, self.cursor)
            }
            byte if byte.is_ascii_alphabetic() => self.consume_tag(tag_start, false),
            _ => {
                self.push_error(
                    ParseErrorCode::InvalidFirstCharacterOfTagName,
                    self.current_char_span(),
                );
                self.emit_owned_character("<".to_owned(), tag_start, tag_start.saturating_add(1));
                Ok(())
            }
        }
    }

    fn consume_end_tag_open(&mut self, tag_start: usize) -> Result<(), TokenizerError> {
        self.state = TokenizerState::EndTagOpen;

        if self.cursor >= self.bytes.len() {
            self.push_error(
                ParseErrorCode::EofBeforeTagName,
                SourceSpan::new(tag_start, self.cursor),
            );
            self.emit_owned_character("</".to_owned(), tag_start, self.cursor);
            return Ok(());
        }

        match self.bytes[self.cursor] {
            byte if byte.is_ascii_alphabetic() => self.consume_tag(tag_start, true),
            b'>' => {
                self.push_error(
                    ParseErrorCode::InvalidFirstCharacterOfTagName,
                    self.current_char_span(),
                );
                self.cursor = self.cursor.saturating_add(1);
                Ok(())
            }
            _ => {
                self.push_error(
                    ParseErrorCode::InvalidFirstCharacterOfTagName,
                    self.current_char_span(),
                );
                self.consume_bogus_comment(tag_start, self.cursor)
            }
        }
    }

    fn consume_tag(&mut self, tag_start: usize, end_tag: bool) -> Result<(), TokenizerError> {
        self.state = TokenizerState::TagName;
        let name_start = self.cursor;
        let mut name = String::new();

        while self.cursor < self.bytes.len() {
            match self.bytes[self.cursor] {
                byte if byte.is_ascii_whitespace() || matches!(byte, b'/' | b'>') => break,
                b'\0' => {
                    let span = self.current_char_span();
                    self.push_error(ParseErrorCode::UnexpectedNullCharacter, span);
                    name.push('\u{fffd}');
                    self.cursor = self.cursor.saturating_add(1);
                }
                _ => {
                    let character = self.current_char();
                    name.push(ascii_lowercase(character));
                    self.advance_char();
                }
            }
            self.check_start_tag_budget(tag_start, end_tag)?;
        }

        let name_span = SourceSpan::new(name_start, self.cursor);
        let mut attributes = Vec::new();
        let mut seen_attributes = BTreeSet::new();
        let mut scanned_attribute_count = 0_usize;
        let mut scanned_attribute_bytes = 0_usize;
        let mut self_closing = false;

        loop {
            self.check_start_tag_budget(tag_start, end_tag)?;

            if self.cursor >= self.bytes.len() {
                self.push_error(
                    ParseErrorCode::EofInTag,
                    SourceSpan::new(self.cursor, self.cursor),
                );
                return Ok(());
            }

            match self.bytes[self.cursor] {
                byte if byte.is_ascii_whitespace() => {
                    self.state = TokenizerState::BeforeAttributeName;
                    self.skip_ascii_whitespace();
                }
                b'>' => {
                    self.cursor = self.cursor.saturating_add(1);
                    break;
                }
                b'/' => {
                    self.state = TokenizerState::SelfClosingStartTag;
                    let slash_span = self.current_char_span();
                    self.cursor = self.cursor.saturating_add(1);

                    if self.cursor < self.bytes.len() && self.bytes[self.cursor] == b'>' {
                        self_closing = true;
                        self.cursor = self.cursor.saturating_add(1);
                        break;
                    }

                    self.push_error(ParseErrorCode::UnexpectedSolidusInTag, slash_span);
                }
                _ => {
                    let attribute = self.consume_attribute();
                    scanned_attribute_count = scanned_attribute_count.saturating_add(1);

                    if scanned_attribute_count > MAX_ATTRIBUTES_PER_ELEMENT {
                        return Err(TokenizerError::TooManyAttributes);
                    }

                    scanned_attribute_bytes = scanned_attribute_bytes
                        .saturating_add(attribute.name.len())
                        .saturating_add(attribute.value.len());

                    if scanned_attribute_bytes > MAX_ATTRIBUTE_BYTES_PER_ELEMENT {
                        return Err(TokenizerError::AttributeBytesExceeded);
                    }

                    if seen_attributes.insert(attribute.name.clone()) {
                        attributes.push(attribute);
                    } else {
                        self.push_error(ParseErrorCode::DuplicateAttribute, attribute.name_span);
                    }
                }
            }
        }

        if end_tag && !attributes.is_empty() {
            self.push_error(
                ParseErrorCode::EndTagWithAttributes,
                SourceSpan::new(tag_start, self.cursor),
            );
        }

        if end_tag && self_closing {
            self.push_error(
                ParseErrorCode::EndTagWithTrailingSolidus,
                SourceSpan::new(tag_start, self.cursor),
            );
        }

        let token = TagToken {
            name,
            name_span,
            attributes,
            self_closing,
            span: SourceSpan::new(tag_start, self.cursor),
        };

        if end_tag {
            self.tokens.push(Token::EndTag(token));
        } else {
            self.tokens.push(Token::StartTag(token));
        }

        self.state = TokenizerState::Data;
        Ok(())
    }

    fn consume_attribute(&mut self) -> AttributeToken {
        self.state = TokenizerState::BeforeAttributeName;
        let name_start = self.cursor;
        let mut name = String::new();

        if self.bytes[self.cursor] == b'=' {
            let span = self.current_char_span();
            self.push_error(
                ParseErrorCode::UnexpectedEqualsSignBeforeAttributeName,
                span,
            );
            name.push('=');
            self.cursor = self.cursor.saturating_add(1);
        }

        self.state = TokenizerState::AttributeName;
        while self.cursor < self.bytes.len() {
            match self.bytes[self.cursor] {
                byte if byte.is_ascii_whitespace() || matches!(byte, b'/' | b'>' | b'=') => break,
                b'\0' => {
                    let span = self.current_char_span();
                    self.push_error(ParseErrorCode::UnexpectedNullCharacter, span);
                    name.push('\u{fffd}');
                    self.cursor = self.cursor.saturating_add(1);
                }
                b'"' | b'\'' | b'<' => {
                    let span = self.current_char_span();
                    self.push_error(ParseErrorCode::UnexpectedCharacterInAttributeName, span);
                    name.push(self.current_char());
                    self.advance_char();
                }
                _ => {
                    let character = self.current_char();
                    name.push(ascii_lowercase(character));
                    self.advance_char();
                }
            }
        }

        let name_span = SourceSpan::new(name_start, self.cursor);
        self.state = TokenizerState::AfterAttributeName;
        self.skip_ascii_whitespace();

        if self.cursor >= self.bytes.len() || matches!(self.bytes[self.cursor], b'/' | b'>') {
            return AttributeToken {
                name,
                value: String::new(),
                name_span,
                value_span: None,
            };
        }

        if self.bytes[self.cursor] != b'=' {
            return AttributeToken {
                name,
                value: String::new(),
                name_span,
                value_span: None,
            };
        }

        self.cursor = self.cursor.saturating_add(1);
        self.state = TokenizerState::BeforeAttributeValue;
        self.skip_ascii_whitespace();

        if self.cursor >= self.bytes.len() || self.bytes[self.cursor] == b'>' {
            self.push_error(
                ParseErrorCode::MissingAttributeValue,
                SourceSpan::new(self.cursor, self.cursor),
            );
            return AttributeToken {
                name,
                value: String::new(),
                name_span,
                value_span: None,
            };
        }

        let (value, value_span, quoted) = match self.bytes[self.cursor] {
            b'"' => self.consume_quoted_attribute_value(b'"'),
            b'\'' => self.consume_quoted_attribute_value(b'\''),
            _ => self.consume_unquoted_attribute_value(),
        };

        if quoted {
            self.state = TokenizerState::AfterAttributeValueQuoted;
            if self.cursor < self.bytes.len()
                && !self.bytes[self.cursor].is_ascii_whitespace()
                && !matches!(self.bytes[self.cursor], b'/' | b'>')
            {
                self.push_error(
                    ParseErrorCode::MissingWhitespaceBetweenAttributes,
                    self.current_char_span(),
                );
            }
        }

        AttributeToken {
            name,
            value,
            name_span,
            value_span: Some(value_span),
        }
    }

    fn consume_quoted_attribute_value(&mut self, quote: u8) -> (String, SourceSpan, bool) {
        self.state = if quote == b'"' {
            TokenizerState::AttributeValueDoubleQuoted
        } else {
            TokenizerState::AttributeValueSingleQuoted
        };

        self.cursor = self.cursor.saturating_add(1);
        let value_start = self.cursor;
        let mut value = String::new();

        while self.cursor < self.bytes.len() {
            match self.bytes[self.cursor] {
                byte if byte == quote => {
                    let value_end = self.cursor;
                    self.cursor = self.cursor.saturating_add(1);
                    return (value, SourceSpan::new(value_start, value_end), true);
                }
                b'&' => {
                    let start = self.cursor;
                    self.character_references.push(CharacterReferenceCandidate {
                        span: SourceSpan::new(start, start.saturating_add(1)),
                        context: CharacterReferenceContext::Attribute,
                    });
                    value.push('&');
                    self.cursor = self.cursor.saturating_add(1);
                }
                b'\0' => {
                    let span = self.current_char_span();
                    self.push_error(ParseErrorCode::UnexpectedNullCharacter, span);
                    value.push('\u{fffd}');
                    self.cursor = self.cursor.saturating_add(1);
                }
                _ => {
                    value.push(self.current_char());
                    self.advance_char();
                }
            }
        }

        (value, SourceSpan::new(value_start, self.cursor), true)
    }

    fn consume_unquoted_attribute_value(&mut self) -> (String, SourceSpan, bool) {
        self.state = TokenizerState::AttributeValueUnquoted;
        let value_start = self.cursor;
        let mut value = String::new();

        while self.cursor < self.bytes.len() {
            match self.bytes[self.cursor] {
                byte if byte.is_ascii_whitespace() || byte == b'>' => break,
                b'&' => {
                    let start = self.cursor;
                    self.character_references.push(CharacterReferenceCandidate {
                        span: SourceSpan::new(start, start.saturating_add(1)),
                        context: CharacterReferenceContext::Attribute,
                    });
                    value.push('&');
                    self.cursor = self.cursor.saturating_add(1);
                }
                b'\0' => {
                    let span = self.current_char_span();
                    self.push_error(ParseErrorCode::UnexpectedNullCharacter, span);
                    value.push('\u{fffd}');
                    self.cursor = self.cursor.saturating_add(1);
                }
                b'"' | b'\'' | b'<' | b'=' | b'`' => {
                    let span = self.current_char_span();
                    self.push_error(
                        ParseErrorCode::UnexpectedCharacterInUnquotedAttributeValue,
                        span,
                    );
                    value.push(self.current_char());
                    self.advance_char();
                }
                _ => {
                    value.push(self.current_char());
                    self.advance_char();
                }
            }
        }

        (value, SourceSpan::new(value_start, self.cursor), false)
    }

    fn consume_markup_declaration(&mut self, token_start: usize) -> Result<(), TokenizerError> {
        self.state = TokenizerState::MarkupDeclarationOpen;

        if self.remaining().starts_with("--") {
            self.cursor = self.cursor.saturating_add(2);
            return self.consume_comment(token_start);
        }

        if self.starts_with_ascii_case_insensitive("DOCTYPE") {
            self.cursor = self.cursor.saturating_add("DOCTYPE".len());
            self.consume_doctype(token_start);
            return Ok(());
        }

        self.push_error(
            ParseErrorCode::IncorrectlyOpenedComment,
            SourceSpan::new(token_start, self.cursor),
        );
        self.consume_bogus_comment(token_start, self.cursor)
    }

    fn consume_comment(&mut self, token_start: usize) -> Result<(), TokenizerError> {
        self.state = TokenizerState::Comment;
        let content_start = self.cursor;
        let mut data = String::new();

        while self.cursor < self.bytes.len() {
            if self.remaining().starts_with("-->") {
                if data.len() > MAX_COMMENT_BYTES {
                    return Err(TokenizerError::CommentTooLarge);
                }

                self.cursor = self.cursor.saturating_add(3);
                self.tokens.push(Token::Comment(CommentToken {
                    data,
                    span: SourceSpan::new(token_start, self.cursor),
                }));
                self.state = TokenizerState::Data;
                return Ok(());
            }

            match self.bytes[self.cursor] {
                b'\0' => {
                    let span = self.current_char_span();
                    self.push_error(ParseErrorCode::UnexpectedNullCharacter, span);
                    data.push('\u{fffd}');
                    self.cursor = self.cursor.saturating_add(1);
                }
                _ => {
                    data.push(self.current_char());
                    self.advance_char();
                }
            }

            if self.cursor.saturating_sub(content_start) > MAX_COMMENT_BYTES
                || data.len() > MAX_COMMENT_BYTES
            {
                return Err(TokenizerError::CommentTooLarge);
            }
        }

        self.push_error(
            ParseErrorCode::EofInComment,
            SourceSpan::new(self.cursor, self.cursor),
        );
        self.tokens.push(Token::Comment(CommentToken {
            data,
            span: SourceSpan::new(token_start, self.cursor),
        }));
        Ok(())
    }

    fn consume_bogus_comment(
        &mut self,
        token_start: usize,
        content_start: usize,
    ) -> Result<(), TokenizerError> {
        self.state = TokenizerState::BogusComment;
        self.cursor = content_start;
        let mut data = String::new();

        while self.cursor < self.bytes.len() && self.bytes[self.cursor] != b'>' {
            match self.bytes[self.cursor] {
                b'\0' => {
                    let span = self.current_char_span();
                    self.push_error(ParseErrorCode::UnexpectedNullCharacter, span);
                    data.push('\u{fffd}');
                    self.cursor = self.cursor.saturating_add(1);
                }
                _ => {
                    data.push(self.current_char());
                    self.advance_char();
                }
            }

            if data.len() > MAX_COMMENT_BYTES {
                return Err(TokenizerError::CommentTooLarge);
            }
        }

        let closed = self.cursor < self.bytes.len() && self.bytes[self.cursor] == b'>';
        if closed {
            self.cursor = self.cursor.saturating_add(1);
        }

        self.tokens.push(Token::Comment(CommentToken {
            data,
            span: SourceSpan::new(token_start, self.cursor),
        }));
        if closed {
            self.state = TokenizerState::Data;
        }
        Ok(())
    }

    fn consume_doctype(&mut self, token_start: usize) {
        self.state = TokenizerState::Doctype;

        if self.cursor >= self.bytes.len() {
            self.push_error(
                ParseErrorCode::EofInDoctype,
                SourceSpan::new(self.cursor, self.cursor),
            );
            self.tokens.push(Token::Doctype(DoctypeToken {
                name: None,
                force_quirks: true,
                span: SourceSpan::new(token_start, self.cursor),
            }));
            return;
        }

        if self.bytes[self.cursor].is_ascii_whitespace() {
            self.skip_ascii_whitespace();
        } else if self.bytes[self.cursor] == b'>' {
            self.push_error(ParseErrorCode::MissingDoctypeName, self.current_char_span());
            self.cursor = self.cursor.saturating_add(1);
            self.tokens.push(Token::Doctype(DoctypeToken {
                name: None,
                force_quirks: true,
                span: SourceSpan::new(token_start, self.cursor),
            }));
            self.state = TokenizerState::Data;
            return;
        } else {
            self.push_error(
                ParseErrorCode::MissingWhitespaceBeforeDoctypeName,
                self.current_char_span(),
            );
        }

        if self.cursor >= self.bytes.len() {
            self.push_error(
                ParseErrorCode::EofInDoctype,
                SourceSpan::new(self.cursor, self.cursor),
            );
            self.tokens.push(Token::Doctype(DoctypeToken {
                name: None,
                force_quirks: true,
                span: SourceSpan::new(token_start, self.cursor),
            }));
            return;
        }

        let mut name = String::new();
        while self.cursor < self.bytes.len()
            && !self.bytes[self.cursor].is_ascii_whitespace()
            && self.bytes[self.cursor] != b'>'
        {
            if self.bytes[self.cursor] == b'\0' {
                let span = self.current_char_span();
                self.push_error(ParseErrorCode::UnexpectedNullCharacter, span);
                name.push('\u{fffd}');
                self.cursor = self.cursor.saturating_add(1);
            } else {
                let character = self.current_char();
                name.push(ascii_lowercase(character));
                self.advance_char();
            }
        }

        if name.is_empty() {
            self.push_error(
                ParseErrorCode::MissingDoctypeName,
                SourceSpan::new(self.cursor, self.cursor),
            );
        }

        let mut force_quirks = name.is_empty();

        while self.cursor < self.bytes.len() && self.bytes[self.cursor] != b'>' {
            self.advance_char();
        }

        let closed = self.cursor < self.bytes.len();
        if closed {
            self.cursor = self.cursor.saturating_add(1);
        } else {
            self.push_error(
                ParseErrorCode::EofInDoctype,
                SourceSpan::new(self.cursor, self.cursor),
            );
            force_quirks = true;
        }

        self.tokens.push(Token::Doctype(DoctypeToken {
            name: (!name.is_empty()).then_some(name),
            force_quirks,
            span: SourceSpan::new(token_start, self.cursor),
        }));
        if closed {
            self.state = TokenizerState::Data;
        }
    }

    fn check_start_tag_budget(
        &self,
        tag_start: usize,
        end_tag: bool,
    ) -> Result<(), TokenizerError> {
        if end_tag {
            return Ok(());
        }

        let raw_bytes = self.cursor.saturating_sub(tag_start).saturating_sub(1);

        if raw_bytes > MAX_RAW_START_TAG_BYTES {
            return Err(TokenizerError::StartTagTooLarge);
        }

        Ok(())
    }

    fn emit_source_characters(&mut self, start: usize, end: usize) {
        if start >= end {
            return;
        }

        if let Some(data) = self.source.get(start..end) {
            self.tokens.push(Token::Character(CharacterToken {
                data: data.to_owned(),
                span: SourceSpan::new(start, end),
            }));
        }
    }

    fn emit_owned_character(&mut self, data: String, start: usize, end: usize) {
        self.tokens.push(Token::Character(CharacterToken {
            data,
            span: SourceSpan::new(start, end),
        }));
    }

    fn push_error(&mut self, code: ParseErrorCode, span: SourceSpan) {
        self.parse_errors.push(ParseError { code, span });
    }

    fn remaining(&self) -> &'a str {
        self.source.get(self.cursor..).unwrap_or_default()
    }

    fn starts_with_ascii_case_insensitive(&self, needle: &str) -> bool {
        let remaining = self.bytes.get(self.cursor..).unwrap_or_default();
        let needle_bytes = needle.as_bytes();

        remaining.len() >= needle_bytes.len()
            && remaining[..needle_bytes.len()].eq_ignore_ascii_case(needle_bytes)
    }

    fn current_char(&self) -> char {
        self.remaining().chars().next().unwrap_or('\u{fffd}')
    }

    fn current_char_span(&self) -> SourceSpan {
        let length = self.current_char().len_utf8();
        SourceSpan::new(self.cursor, self.cursor.saturating_add(length))
    }

    fn advance_char(&mut self) {
        self.cursor = self
            .cursor
            .saturating_add(self.current_char().len_utf8())
            .min(self.bytes.len());
    }

    fn skip_ascii_whitespace(&mut self) {
        while self.cursor < self.bytes.len() && self.bytes[self.cursor].is_ascii_whitespace() {
            self.cursor = self.cursor.saturating_add(1);
        }
    }
}

fn ascii_lowercase(character: char) -> char {
    if character.is_ascii_uppercase() {
        character.to_ascii_lowercase()
    } else {
        character
    }
}
