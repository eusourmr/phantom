//! JavaScript language foundation for the Phantom browser engine.
//!
//! Foundation 9 keeps the bounded lexer/expression compatibility and extends
//! statement control flow with `while` and classic `for` loops. There is still no DOM
//! binding, runtime, bytecode, `window`, or browser integration in this milestone.

#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;

// PHANTOM_JS_FOUNDATION_1_PRIMARY_EXPRESSIONS
// PHANTOM_JS_FOUNDATION_2_ARITHMETIC_EXPRESSIONS
// PHANTOM_JS_FOUNDATION_3_COMPARISON_LOGICAL
// PHANTOM_JS_FOUNDATION_4_ASSIGNMENT_STATEMENTS
// PHANTOM_JS_FOUNDATION_5_COLLECTIONS_MEMBERS
// PHANTOM_JS_FOUNDATION_6_CALLS_FUNCTIONS
// PHANTOM_JS_FOUNDATION_7_CONDITIONAL_THIS
// PHANTOM_JS_FOUNDATION_8_CONTROL_FLOW
// PHANTOM_JS_FOUNDATION_9_LOOPS_I
/// Syntax tree nodes for JavaScript Foundation 9.
pub mod ast;
mod parser;

pub use parser::{
    ParseError, ParseErrorKind, parse_expression, parse_primary_expression, parse_program,
};

const MAX_SOURCE_BYTES: usize = 1024 * 1024;
const MAX_TOKENS: usize = 131_072;
const MAX_IDENTIFIER_BYTES: usize = 4 * 1024;
const MAX_STRING_BYTES: usize = 64 * 1024;
const MAX_COMMENT_BYTES: usize = 256 * 1024;

/// Byte range inside one UTF-8 JavaScript source buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    start: usize,
    end: usize,
}

impl Span {
    /// Creates a source span.
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Returns the inclusive start byte offset.
    #[must_use]
    pub const fn start(self) -> usize {
        self.start
    }

    /// Returns the exclusive end byte offset.
    #[must_use]
    pub const fn end(self) -> usize {
        self.end
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

/// Reserved word recognized by JavaScript Foundation 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    /// `let`.
    Let,
    /// `const`.
    Const,
    /// `var`.
    Var,
    /// `function`.
    Function,
    /// `return`.
    Return,
    /// `if`.
    If,
    /// `else`.
    Else,
    /// `while`.
    While,
    /// `for`.
    For,
    /// `true`.
    True,
    /// `false`.
    False,
    /// `null`.
    Null,
    /// `new`.
    New,
    /// `this`.
    This,
}

/// Operator or punctuation token recognized by Foundation 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Punctuator {
    /// `(`.
    LeftParen,
    /// `)`.
    RightParen,
    /// `{`.
    LeftBrace,
    /// `}`.
    RightBrace,
    /// `[`.
    LeftBracket,
    /// `]`.
    RightBracket,
    /// `;`.
    Semicolon,
    /// `,`.
    Comma,
    /// `.`.
    Dot,
    /// `:`.
    Colon,
    /// `?`.
    Question,
    /// `+`.
    Plus,
    /// `++`.
    PlusPlus,
    /// `-`.
    Minus,
    /// `--`.
    MinusMinus,
    /// `*`.
    Star,
    /// `/`.
    Slash,
    /// `%`.
    Percent,
    /// `=`.
    Assign,
    /// `==`.
    Equal,
    /// `===`.
    StrictEqual,
    /// `=>`.
    Arrow,
    /// `!`.
    Bang,
    /// `!=`.
    NotEqual,
    /// `!==`.
    StrictNotEqual,
    /// `<`.
    Less,
    /// `<=`.
    LessEqual,
    /// `>`.
    Greater,
    /// `>=`.
    GreaterEqual,
    /// `&&`.
    LogicalAnd,
    /// `||`.
    LogicalOr,
}

/// Coarse token kind emitted by the Foundation 0 lexer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// ASCII identifier not recognized as a keyword.
    Identifier,
    /// Decimal numeric literal.
    Number,
    /// Single- or double-quoted string literal.
    String,
    /// Reserved keyword.
    Keyword(Keyword),
    /// Operator or punctuation.
    Punctuator(Punctuator),
    /// End of input.
    Eof,
}

/// One token referencing the original source by byte span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    kind: TokenKind,
    span: Span,
}

impl Token {
    /// Creates a token.
    #[must_use]
    pub const fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// Returns the token kind.
    #[must_use]
    pub const fn kind(self) -> TokenKind {
        self.kind
    }

    /// Returns the source span.
    #[must_use]
    pub const fn span(self) -> Span {
        self.span
    }

    /// Returns this token's original source slice when the span is valid.
    #[must_use]
    pub fn lexeme(self, source: &str) -> Option<&str> {
        source.get(self.span.start..self.span.end)
    }
}

/// Failure category produced by the bounded lexer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexErrorKind {
    /// Source exceeded the 1 MiB Foundation 0 budget.
    SourceTooLarge,
    /// Token count exceeded the bounded token budget.
    TokenLimitExceeded,
    /// Identifier exceeded its byte budget.
    IdentifierTooLong,
    /// String literal exceeded its byte budget.
    StringTooLong,
    /// Comment exceeded its byte budget.
    CommentTooLong,
    /// Quoted string reached an invalid end.
    UnterminatedString,
    /// Block comment reached end of input without `*/`.
    UnterminatedBlockComment,
    /// Decimal exponent was syntactically incomplete.
    InvalidNumber,
    /// Character is outside the Foundation 0 lexical subset.
    InvalidCharacter,
}

/// Bounded JavaScript lexical error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexError {
    kind: LexErrorKind,
    offset: usize,
}

impl LexError {
    const fn new(kind: LexErrorKind, offset: usize) -> Self {
        Self { kind, offset }
    }

    /// Returns the failure category.
    #[must_use]
    pub const fn kind(self) -> LexErrorKind {
        self.kind
    }

    /// Returns the byte offset where the failure was detected.
    #[must_use]
    pub const fn offset(self) -> usize {
        self.offset
    }
}

impl fmt::Display for LexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = self.kind;
        let offset = self.offset;
        write!(
            formatter,
            "JavaScript lexical error {kind:?} at byte {offset}"
        )
    }
}

impl Error for LexError {}

/// Tokenizes JavaScript source within Foundation 0 limits.
///
/// The first milestone deliberately supports ASCII identifiers. UTF-8 remains
/// fully valid inside string literals and comments. Unicode identifier rules,
/// template literals, regular-expression literals, numeric prefixes, and
/// automatic semicolon insertion belong to later JavaScript milestones.
///
/// # Errors
///
/// Returns [`LexError`] when a security budget is exceeded or when source uses
/// syntax outside the current lexical subset.
pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(LexError::new(LexErrorKind::SourceTooLarge, source.len()));
    }

    Lexer::new(source).run()
}

struct Lexer<'a> {
    source: &'a str,
    bytes: &'a [u8],
    cursor: usize,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            cursor: 0,
            tokens: Vec::new(),
        }
    }

    fn run(mut self) -> Result<Vec<Token>, LexError> {
        while self.cursor < self.bytes.len() {
            self.skip_trivia()?;

            if self.cursor >= self.bytes.len() {
                break;
            }

            let start = self.cursor;
            let byte = self.bytes[self.cursor];

            if is_identifier_start(byte) {
                self.scan_identifier(start)?;
                continue;
            }

            if byte.is_ascii_digit() {
                self.scan_number(start)?;
                continue;
            }

            if byte == b'\'' || byte == b'"' {
                self.scan_string(start, byte)?;
                continue;
            }

            let kind = self.scan_punctuator(start)?;
            self.push(kind, start, self.cursor)?;
        }

        self.push(TokenKind::Eof, self.cursor, self.cursor)?;
        Ok(self.tokens)
    }

    fn skip_trivia(&mut self) -> Result<(), LexError> {
        loop {
            while self
                .bytes
                .get(self.cursor)
                .is_some_and(u8::is_ascii_whitespace)
            {
                self.cursor = self.cursor.saturating_add(1);
            }

            let Some(current) = self.bytes.get(self.cursor).copied() else {
                return Ok(());
            };

            let next = self.bytes.get(self.cursor.saturating_add(1)).copied();

            if current == b'/' && next == Some(b'/') {
                let start = self.cursor;
                self.cursor = self.cursor.saturating_add(2);

                while let Some(byte) = self.bytes.get(self.cursor).copied() {
                    if byte == b'\n' || byte == b'\r' {
                        break;
                    }

                    self.cursor = self.cursor.saturating_add(1);

                    if self.cursor.saturating_sub(start) > MAX_COMMENT_BYTES {
                        return Err(LexError::new(LexErrorKind::CommentTooLong, start));
                    }
                }

                continue;
            }

            if current == b'/' && next == Some(b'*') {
                let start = self.cursor;
                self.cursor = self.cursor.saturating_add(2);
                let mut closed = false;

                while self.cursor < self.bytes.len() {
                    if self.bytes.get(self.cursor) == Some(&b'*')
                        && self.bytes.get(self.cursor.saturating_add(1)) == Some(&b'/')
                    {
                        self.cursor = self.cursor.saturating_add(2);
                        closed = true;
                        break;
                    }

                    self.cursor = self.cursor.saturating_add(1);

                    if self.cursor.saturating_sub(start) > MAX_COMMENT_BYTES {
                        return Err(LexError::new(LexErrorKind::CommentTooLong, start));
                    }
                }

                if !closed {
                    return Err(LexError::new(LexErrorKind::UnterminatedBlockComment, start));
                }

                continue;
            }

            return Ok(());
        }
    }

    fn scan_identifier(&mut self, start: usize) -> Result<(), LexError> {
        self.cursor = self.cursor.saturating_add(1);

        while self
            .bytes
            .get(self.cursor)
            .is_some_and(|byte| is_identifier_continue(*byte))
        {
            self.cursor = self.cursor.saturating_add(1);

            if self.cursor.saturating_sub(start) > MAX_IDENTIFIER_BYTES {
                return Err(LexError::new(LexErrorKind::IdentifierTooLong, start));
            }
        }

        let Some(lexeme) = self.source.get(start..self.cursor) else {
            return Err(LexError::new(LexErrorKind::InvalidCharacter, start));
        };

        let kind = keyword_for(lexeme)
            .map(TokenKind::Keyword)
            .unwrap_or(TokenKind::Identifier);

        self.push(kind, start, self.cursor)
    }

    fn scan_number(&mut self, start: usize) -> Result<(), LexError> {
        while self.bytes.get(self.cursor).is_some_and(u8::is_ascii_digit) {
            self.cursor = self.cursor.saturating_add(1);
        }

        if self.bytes.get(self.cursor) == Some(&b'.')
            && self
                .bytes
                .get(self.cursor.saturating_add(1))
                .is_some_and(u8::is_ascii_digit)
        {
            self.cursor = self.cursor.saturating_add(1);

            while self.bytes.get(self.cursor).is_some_and(u8::is_ascii_digit) {
                self.cursor = self.cursor.saturating_add(1);
            }
        }

        if self
            .bytes
            .get(self.cursor)
            .copied()
            .is_some_and(|byte| byte == b'e' || byte == b'E')
        {
            self.cursor = self.cursor.saturating_add(1);

            if self
                .bytes
                .get(self.cursor)
                .copied()
                .is_some_and(|byte| byte == b'+' || byte == b'-')
            {
                self.cursor = self.cursor.saturating_add(1);
            }

            let exponent_start = self.cursor;

            while self.bytes.get(self.cursor).is_some_and(u8::is_ascii_digit) {
                self.cursor = self.cursor.saturating_add(1);
            }

            if self.cursor == exponent_start {
                return Err(LexError::new(LexErrorKind::InvalidNumber, start));
            }
        }

        self.push(TokenKind::Number, start, self.cursor)
    }

    fn scan_string(&mut self, start: usize, quote: u8) -> Result<(), LexError> {
        self.cursor = self.cursor.saturating_add(1);
        let content_start = self.cursor;

        while let Some(byte) = self.bytes.get(self.cursor).copied() {
            if byte == quote {
                self.cursor = self.cursor.saturating_add(1);
                return self.push(TokenKind::String, start, self.cursor);
            }

            if byte == b'\n' || byte == b'\r' {
                return Err(LexError::new(LexErrorKind::UnterminatedString, start));
            }

            if byte == b'\\' {
                self.cursor = self.cursor.saturating_add(1);

                if self.cursor >= self.bytes.len() {
                    return Err(LexError::new(LexErrorKind::UnterminatedString, start));
                }
            }

            self.cursor = self.cursor.saturating_add(1);

            if self.cursor.saturating_sub(content_start) > MAX_STRING_BYTES {
                return Err(LexError::new(LexErrorKind::StringTooLong, start));
            }
        }

        Err(LexError::new(LexErrorKind::UnterminatedString, start))
    }

    fn scan_punctuator(&mut self, start: usize) -> Result<TokenKind, LexError> {
        let Some(byte) = self.bytes.get(self.cursor).copied() else {
            return Err(LexError::new(LexErrorKind::InvalidCharacter, start));
        };

        self.cursor = self.cursor.saturating_add(1);

        let punctuator = match byte {
            b'(' => Punctuator::LeftParen,
            b')' => Punctuator::RightParen,
            b'{' => Punctuator::LeftBrace,
            b'}' => Punctuator::RightBrace,
            b'[' => Punctuator::LeftBracket,
            b']' => Punctuator::RightBracket,
            b';' => Punctuator::Semicolon,
            b',' => Punctuator::Comma,
            b'.' => Punctuator::Dot,
            b':' => Punctuator::Colon,
            b'?' => Punctuator::Question,
            b'+' => {
                if self.consume_if(b'+') {
                    Punctuator::PlusPlus
                } else {
                    Punctuator::Plus
                }
            }
            b'-' => {
                if self.consume_if(b'-') {
                    Punctuator::MinusMinus
                } else {
                    Punctuator::Minus
                }
            }
            b'*' => Punctuator::Star,
            b'/' => Punctuator::Slash,
            b'%' => Punctuator::Percent,
            b'=' => {
                if self.consume_if(b'>') {
                    Punctuator::Arrow
                } else if self.consume_if(b'=') {
                    if self.consume_if(b'=') {
                        Punctuator::StrictEqual
                    } else {
                        Punctuator::Equal
                    }
                } else {
                    Punctuator::Assign
                }
            }
            b'!' => {
                if self.consume_if(b'=') {
                    if self.consume_if(b'=') {
                        Punctuator::StrictNotEqual
                    } else {
                        Punctuator::NotEqual
                    }
                } else {
                    Punctuator::Bang
                }
            }
            b'<' => {
                if self.consume_if(b'=') {
                    Punctuator::LessEqual
                } else {
                    Punctuator::Less
                }
            }
            b'>' => {
                if self.consume_if(b'=') {
                    Punctuator::GreaterEqual
                } else {
                    Punctuator::Greater
                }
            }
            b'&' if self.consume_if(b'&') => Punctuator::LogicalAnd,
            b'|' if self.consume_if(b'|') => Punctuator::LogicalOr,
            b'&' | b'|' => {
                return Err(LexError::new(LexErrorKind::InvalidCharacter, start));
            }
            _ => {
                return Err(LexError::new(LexErrorKind::InvalidCharacter, start));
            }
        };

        Ok(TokenKind::Punctuator(punctuator))
    }

    fn consume_if(&mut self, expected: u8) -> bool {
        if self.bytes.get(self.cursor) == Some(&expected) {
            self.cursor = self.cursor.saturating_add(1);
            true
        } else {
            false
        }
    }

    fn push(&mut self, kind: TokenKind, start: usize, end: usize) -> Result<(), LexError> {
        if self.tokens.len() >= MAX_TOKENS {
            return Err(LexError::new(LexErrorKind::TokenLimitExceeded, start));
        }

        self.tokens.push(Token::new(kind, Span::new(start, end)));
        Ok(())
    }
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$'
}

fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

fn keyword_for(identifier: &str) -> Option<Keyword> {
    match identifier {
        "let" => Some(Keyword::Let),
        "const" => Some(Keyword::Const),
        "var" => Some(Keyword::Var),
        "function" => Some(Keyword::Function),
        "return" => Some(Keyword::Return),
        "if" => Some(Keyword::If),
        "else" => Some(Keyword::Else),
        "while" => Some(Keyword::While),
        "for" => Some(Keyword::For),
        "true" => Some(Keyword::True),
        "false" => Some(Keyword::False),
        "null" => Some(Keyword::Null),
        "new" => Some(Keyword::New),
        "this" => Some(Keyword::This),
        _ => None,
    }
}
