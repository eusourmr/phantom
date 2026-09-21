//! JavaScript Foundation 2 arithmetic-expression parser regression corpus.

use std::io;

use phantom_js::ast::{BinaryOperator, ExpressionKind, UnaryOperator};
use phantom_js::{
    ParseErrorKind, Punctuator, TokenKind, parse_expression, parse_primary_expression,
};

#[test]
fn multiplication_binds_tighter_than_addition() -> Result<(), Box<dyn std::error::Error>> {
    let expression = parse_expression("1 + 2 * 3")?;

    let ExpressionKind::Binary {
        left,
        operator: BinaryOperator::Add,
        right,
    } = expression.kind()
    else {
        return Err(io::Error::other("additive root missing").into());
    };

    assert_eq!(left.kind(), &ExpressionKind::NumberLiteral);

    let ExpressionKind::Binary {
        operator: BinaryOperator::Multiply,
        ..
    } = right.kind()
    else {
        return Err(io::Error::other("multiplicative right branch missing").into());
    };

    Ok(())
}

#[test]
fn additive_operators_are_left_associative() -> Result<(), Box<dyn std::error::Error>> {
    let expression = parse_expression("10 - 3 - 2")?;

    let ExpressionKind::Binary {
        left,
        operator: BinaryOperator::Subtract,
        right,
    } = expression.kind()
    else {
        return Err(io::Error::other("outer subtraction missing").into());
    };

    assert_eq!(right.kind(), &ExpressionKind::NumberLiteral);

    let ExpressionKind::Binary {
        operator: BinaryOperator::Subtract,
        ..
    } = left.kind()
    else {
        return Err(io::Error::other("left-associated subtraction missing").into());
    };

    Ok(())
}

#[test]
fn unary_operators_nest_before_binary_operators() -> Result<(), Box<dyn std::error::Error>> {
    let expression = parse_expression("!-value + 1")?;

    let ExpressionKind::Binary {
        left,
        operator: BinaryOperator::Add,
        ..
    } = expression.kind()
    else {
        return Err(io::Error::other("additive root missing").into());
    };

    let ExpressionKind::Unary {
        operator: UnaryOperator::LogicalNot,
        argument,
    } = left.kind()
    else {
        return Err(io::Error::other("logical-not node missing").into());
    };

    let ExpressionKind::Unary {
        operator: UnaryOperator::Minus,
        ..
    } = argument.kind()
    else {
        return Err(io::Error::other("unary-minus node missing").into());
    };

    Ok(())
}

#[test]
fn grouped_expression_overrides_precedence() -> Result<(), Box<dyn std::error::Error>> {
    let source = "(1 + 2) * 3";
    let expression = parse_expression(source)?;

    assert_eq!(expression.lexeme(source), Some(source));

    let ExpressionKind::Binary {
        left,
        operator: BinaryOperator::Multiply,
        ..
    } = expression.kind()
    else {
        return Err(io::Error::other("multiplication root missing").into());
    };

    let ExpressionKind::Grouped(inner) = left.kind() else {
        return Err(io::Error::other("grouped left branch missing").into());
    };

    assert!(matches!(
        inner.kind(),
        ExpressionKind::Binary {
            operator: BinaryOperator::Add,
            ..
        }
    ));

    Ok(())
}

#[test]
fn primary_entry_point_keeps_assignment_outside_foundation_two_boundary() {
    let error = parse_primary_expression("value = 1").err();

    assert_eq!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::TrailingToken(TokenKind::Punctuator(
            Punctuator::Assign,
        )))
    );
}

#[test]
fn foundation_one_primary_entry_point_keeps_its_old_boundary() {
    let error = parse_primary_expression("1 + 2").err();

    assert_eq!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::TrailingToken(TokenKind::Punctuator(
            Punctuator::Plus,
        )))
    );
}

#[test]
fn unary_expression_nesting_budget_is_enforced() {
    let source = format!("{}1", "!".repeat(257));
    let error = parse_expression(&source).err();

    assert_eq!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::NestingLimitExceeded)
    );
}
