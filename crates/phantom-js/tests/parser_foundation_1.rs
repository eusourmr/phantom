//! JavaScript Foundation 1 primary-expression parser regression corpus.

use std::io;

use phantom_js::ast::ExpressionKind;
use phantom_js::{Keyword, ParseErrorKind, TokenKind, parse_primary_expression};

#[test]
fn parses_identifier_and_preserves_source_span() -> Result<(), Box<dyn std::error::Error>> {
    let source = "answer";
    let expression = parse_primary_expression(source)?;

    assert_eq!(expression.kind(), &ExpressionKind::Identifier);
    assert_eq!(expression.lexeme(source), Some("answer"));
    Ok(())
}

#[test]
fn undefined_remains_an_identifier() -> Result<(), Box<dyn std::error::Error>> {
    let expression = parse_primary_expression("undefined")?;

    assert_eq!(expression.kind(), &ExpressionKind::Identifier);
    Ok(())
}

#[test]
fn parses_number_string_boolean_and_null_literals() -> Result<(), Box<dyn std::error::Error>> {
    let number = parse_primary_expression("12.5e2")?;
    let string = parse_primary_expression("\"phantom\"")?;
    let truthy = parse_primary_expression("true")?;
    let falsy = parse_primary_expression("false")?;
    let null = parse_primary_expression("null")?;

    assert_eq!(number.kind(), &ExpressionKind::NumberLiteral);
    assert_eq!(string.kind(), &ExpressionKind::StringLiteral);
    assert_eq!(truthy.kind(), &ExpressionKind::Boolean(true));
    assert_eq!(falsy.kind(), &ExpressionKind::Boolean(false));
    assert_eq!(null.kind(), &ExpressionKind::Null);
    Ok(())
}

#[test]
fn parses_nested_parenthesized_primary_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = "((null))";
    let expression = parse_primary_expression(source)?;

    assert_eq!(expression.lexeme(source), Some("((null))"));

    let ExpressionKind::Grouped(first) = expression.kind() else {
        return Err(io::Error::other("outer grouped expression missing").into());
    };

    let ExpressionKind::Grouped(second) = first.kind() else {
        return Err(io::Error::other("inner grouped expression missing").into());
    };

    assert_eq!(second.kind(), &ExpressionKind::Null);
    Ok(())
}

#[test]
fn rejects_trailing_operator_tokens() {
    let error = parse_primary_expression("1 + 2").err();

    assert_eq!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::TrailingToken(TokenKind::Punctuator(
            phantom_js::Punctuator::Plus,
        )))
    );
}

#[test]
fn rejects_keyword_that_is_not_a_primary_literal() {
    let error = parse_primary_expression("function").err();

    assert_eq!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::UnexpectedToken(TokenKind::Keyword(
            Keyword::Function,
        )))
    );
}

#[test]
fn requires_closing_parenthesis() {
    let error = parse_primary_expression("(true").err();

    assert_eq!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::ExpectedRightParen)
    );
}

#[test]
fn lexical_failures_are_preserved() {
    let error = parse_primary_expression("ação").err();

    assert!(matches!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::Lexical(_))
    ));
}

#[test]
fn primary_expression_nesting_budget_is_enforced() {
    let source = format!("{}null{}", "(".repeat(257), ")".repeat(257),);

    let error = parse_primary_expression(&source).err();

    assert_eq!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::NestingLimitExceeded)
    );
}
