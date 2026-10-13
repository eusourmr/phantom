//! JavaScript Foundation 0 lexer regression corpus.

use phantom_js::{Keyword, LexErrorKind, Punctuator, TokenKind, lex};

#[test]
fn foundation_zero_tokenizes_keywords_identifiers_and_numbers()
-> Result<(), Box<dyn std::error::Error>> {
    let source = "let answer = 40 + 2;";
    let tokens = lex(source)?;

    let kinds: Vec<TokenKind> = tokens.iter().map(|token| token.kind()).collect();

    assert_eq!(
        kinds,
        vec![
            TokenKind::Keyword(Keyword::Let),
            TokenKind::Identifier,
            TokenKind::Punctuator(Punctuator::Assign),
            TokenKind::Number,
            TokenKind::Punctuator(Punctuator::Plus),
            TokenKind::Number,
            TokenKind::Punctuator(Punctuator::Semicolon),
            TokenKind::Eof,
        ]
    );

    assert_eq!(tokens[1].lexeme(source), Some("answer"));
    Ok(())
}

#[test]
fn longest_supported_operators_win() -> Result<(), Box<dyn std::error::Error>> {
    let tokens = lex("a===b && c!==d || x=>x+1")?;
    let kinds: Vec<TokenKind> = tokens.iter().map(|token| token.kind()).collect();

    assert!(kinds.contains(&TokenKind::Punctuator(Punctuator::StrictEqual)));
    assert!(kinds.contains(&TokenKind::Punctuator(Punctuator::LogicalAnd)));
    assert!(kinds.contains(&TokenKind::Punctuator(Punctuator::StrictNotEqual)));
    assert!(kinds.contains(&TokenKind::Punctuator(Punctuator::LogicalOr)));
    assert!(kinds.contains(&TokenKind::Punctuator(Punctuator::Arrow)));
    Ok(())
}

#[test]
fn comments_are_trivia_and_utf8_strings_keep_original_span()
-> Result<(), Box<dyn std::error::Error>> {
    let source = "/* header */ const city = \"Goiânia\"; // tail";
    let tokens = lex(source)?;

    let string = tokens
        .iter()
        .find(|token| token.kind() == TokenKind::String)
        .ok_or_else(|| std::io::Error::other("string token missing"))?;

    assert_eq!(string.lexeme(source), Some("\"Goiânia\""));
    Ok(())
}

#[test]
fn unterminated_block_comment_is_rejected() {
    let error = lex("let x = 1; /* never closes").err();

    assert_eq!(
        error.map(|error| error.kind()),
        Some(LexErrorKind::UnterminatedBlockComment)
    );
}

#[test]
fn unicode_identifier_is_explicitly_deferred() {
    let error = lex("let ação = 1;").err();

    assert_eq!(
        error.map(|error| error.kind()),
        Some(LexErrorKind::InvalidCharacter)
    );
}

#[test]
fn source_budget_is_enforced_before_tokenization() {
    let source = " ".repeat(1024 * 1024 + 1);
    let error = lex(&source).err();

    assert_eq!(
        error.map(|error| error.kind()),
        Some(LexErrorKind::SourceTooLarge)
    );
}

#[test]
fn token_budget_is_enforced() {
    let source = "x;".repeat(70_000);
    let error = lex(&source).err();

    assert_eq!(
        error.map(|error| error.kind()),
        Some(LexErrorKind::TokenLimitExceeded)
    );
}

#[test]
fn undefined_remains_an_identifier() -> Result<(), Box<dyn std::error::Error>> {
    let source = "undefined";
    let tokens = lex(source)?;
    assert_eq!(tokens[0].kind(), TokenKind::Identifier);
    assert_eq!(tokens[0].lexeme(source), Some("undefined"));
    Ok(())
}

#[test]
fn identifier_string_and_comment_budgets_are_enforced() {
    let identifier = format!("a{}", "b".repeat(4 * 1024));
    assert_eq!(
        lex(&identifier).err().map(|error| error.kind()),
        Some(LexErrorKind::IdentifierTooLong)
    );

    let string = format!("\"{}\"", "x".repeat(64 * 1024 + 1));
    assert_eq!(
        lex(&string).err().map(|error| error.kind()),
        Some(LexErrorKind::StringTooLong)
    );

    let comment = format!("/*{}*/", "x".repeat(256 * 1024 + 1));
    assert_eq!(
        lex(&comment).err().map(|error| error.kind()),
        Some(LexErrorKind::CommentTooLong)
    );
}

#[test]
fn incomplete_decimal_exponent_is_rejected() {
    assert_eq!(
        lex("1e+").err().map(|error| error.kind()),
        Some(LexErrorKind::InvalidNumber)
    );
}
