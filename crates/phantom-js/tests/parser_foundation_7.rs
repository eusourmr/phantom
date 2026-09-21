//! Phantom JavaScript Foundation 7 regression corpus.

use std::io;

use phantom_js::ast::{AssignmentOperator, BinaryOperator, ExpressionKind, MemberProperty};
use phantom_js::{ParseErrorKind, TokenKind, parse_expression, parse_primary_expression};

#[test]
fn parses_conditional_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = "ready ? primary : fallback";
    let expression = parse_expression(source)?;

    let ExpressionKind::Conditional {
        test,
        consequent,
        alternate,
    } = expression.kind()
    else {
        return Err(io::Error::other("expected conditional expression").into());
    };

    assert_eq!(test.lexeme(source), Some("ready"));
    assert_eq!(consequent.lexeme(source), Some("primary"));
    assert_eq!(alternate.lexeme(source), Some("fallback"));
    Ok(())
}

#[test]
fn logical_or_binds_tighter_than_conditional() -> Result<(), Box<dyn std::error::Error>> {
    let expression = parse_expression("a || b ? c : d")?;
    let ExpressionKind::Conditional { test, .. } = expression.kind() else {
        return Err(io::Error::other("expected conditional expression").into());
    };

    let ExpressionKind::Binary { operator, .. } = test.kind() else {
        return Err(io::Error::other("expected logical-or test").into());
    };
    assert_eq!(*operator, BinaryOperator::LogicalOr);
    Ok(())
}

#[test]
fn conditional_is_right_associative() -> Result<(), Box<dyn std::error::Error>> {
    let expression = parse_expression("a ? b : c ? d : e")?;
    let ExpressionKind::Conditional { alternate, .. } = expression.kind() else {
        return Err(io::Error::other("expected outer conditional").into());
    };
    assert!(matches!(
        alternate.kind(),
        ExpressionKind::Conditional { .. }
    ));
    Ok(())
}

#[test]
fn assignment_binds_looser_than_conditional() -> Result<(), Box<dyn std::error::Error>> {
    let expression = parse_expression("value = ready ? 1 : 2")?;
    let ExpressionKind::Assignment {
        operator, right, ..
    } = expression.kind()
    else {
        return Err(io::Error::other("expected assignment expression").into());
    };

    assert_eq!(*operator, AssignmentOperator::Assign);
    assert!(matches!(right.kind(), ExpressionKind::Conditional { .. }));
    Ok(())
}

#[test]
fn conditional_branches_accept_assignments() -> Result<(), Box<dyn std::error::Error>> {
    let expression = parse_expression("ready ? left = 1 : right = 2")?;
    let ExpressionKind::Conditional {
        consequent,
        alternate,
        ..
    } = expression.kind()
    else {
        return Err(io::Error::other("expected conditional expression").into());
    };

    assert!(matches!(
        consequent.kind(),
        ExpressionKind::Assignment { .. }
    ));
    assert!(matches!(
        alternate.kind(),
        ExpressionKind::Assignment { .. }
    ));
    Ok(())
}

#[test]
fn this_is_a_modern_primary_and_can_be_member_base() -> Result<(), Box<dyn std::error::Error>> {
    let source = "this.value";
    let expression = parse_expression(source)?;

    let ExpressionKind::Member { object, property } = expression.kind() else {
        return Err(io::Error::other("expected member expression").into());
    };
    assert!(matches!(object.kind(), ExpressionKind::This));
    assert!(matches!(property, MemberProperty::Identifier(_)));
    Ok(())
}

#[test]
fn foundation_one_primary_entry_point_keeps_this_outside_old_boundary() {
    let error = parse_primary_expression("this").err();
    assert_eq!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::UnexpectedToken(TokenKind::Keyword(
            phantom_js::Keyword::This
        )))
    );
}

#[test]
fn this_is_not_a_simple_assignment_target() {
    let error = parse_expression("this = 1").err();
    assert_eq!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::InvalidAssignmentTarget)
    );
}

#[test]
fn missing_conditional_colon_is_rejected() {
    let error = parse_expression("ready ? 1").err();
    assert_eq!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::ExpectedColon)
    );
}
