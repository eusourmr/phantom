//! JavaScript Foundation 6 calls/functions regression corpus.

use std::io;

use phantom_js::ast::{ExpressionKind, StatementKind};
use phantom_js::{Keyword, ParseErrorKind, TokenKind, parse_expression, parse_program};

#[test]
fn parses_call_arguments_in_source_order() -> Result<(), Box<dyn std::error::Error>> {
    let expression = parse_expression("send(1, value, state.item)")?;
    let ExpressionKind::Call { callee, arguments } = expression.kind() else {
        return Err(io::Error::other("call expression missing").into());
    };

    assert_eq!(callee.kind(), &ExpressionKind::Identifier);
    assert_eq!(arguments.len(), 3);
    assert!(matches!(arguments[2].kind(), ExpressionKind::Member { .. }));
    Ok(())
}

#[test]
fn call_and_member_chains_associate_left_to_right() -> Result<(), Box<dyn std::error::Error>> {
    let expression = parse_expression("factory(1).items[0](value)")?;
    let ExpressionKind::Call { callee, arguments } = expression.kind() else {
        return Err(io::Error::other("outer call missing").into());
    };

    assert_eq!(arguments.len(), 1);
    assert!(matches!(callee.kind(), ExpressionKind::Member { .. }));
    Ok(())
}

#[test]
fn calls_bind_tighter_than_arithmetic() -> Result<(), Box<dyn std::error::Error>> {
    let expression = parse_expression("measure(1) + 2")?;
    let ExpressionKind::Binary { left, .. } = expression.kind() else {
        return Err(io::Error::other("binary root missing").into());
    };

    assert!(matches!(left.kind(), ExpressionKind::Call { .. }));
    Ok(())
}

#[test]
fn parses_anonymous_function_expression_with_return() -> Result<(), Box<dyn std::error::Error>> {
    let expression = parse_expression("function(a, b) { return a + b; }")?;
    let ExpressionKind::Function(function) = expression.kind() else {
        return Err(io::Error::other("function expression missing").into());
    };

    assert!(function.name().is_none());
    assert_eq!(function.parameters().len(), 2);
    assert_eq!(function.body().len(), 1);
    assert!(matches!(
        function.body()[0].kind(),
        StatementKind::Return(Some(_))
    ));
    Ok(())
}

#[test]
fn named_function_expression_can_be_called_immediately() -> Result<(), Box<dyn std::error::Error>> {
    let expression = parse_expression("function identity(value) { return value; }(1)")?;
    let ExpressionKind::Call { callee, arguments } = expression.kind() else {
        return Err(io::Error::other("immediate call missing").into());
    };
    let ExpressionKind::Function(function) = callee.kind() else {
        return Err(io::Error::other("function callee missing").into());
    };

    assert!(function.name().is_some());
    assert_eq!(function.parameters().len(), 1);
    assert_eq!(arguments.len(), 1);
    Ok(())
}

#[test]
fn function_body_reuses_foundation_four_declarations() -> Result<(), Box<dyn std::error::Error>> {
    let program = parse_program(
        "const add = function(a, b) { let total = a + b; return total; }; add(1, 2);",
    )?;
    assert_eq!(program.statements().len(), 2);
    Ok(())
}

#[test]
fn return_remains_invalid_at_top_level() {
    let error = parse_program("return 1;").err();
    assert_eq!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::UnexpectedToken(TokenKind::Keyword(
            Keyword::Return
        )))
    );
}

#[test]
fn call_argument_holes_remain_unsupported() {
    let error = parse_expression("send(1,,2)").err();
    assert!(matches!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::UnexpectedToken(_))
    ));
}
