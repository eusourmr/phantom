//! JavaScript Foundation 5 collection/member regression corpus.

use std::io;

use phantom_js::ast::{AssignmentOperator, ExpressionKind, MemberProperty};
use phantom_js::{ParseErrorKind, parse_expression, parse_program};

#[test]
fn parses_array_literals_and_trailing_comma() -> Result<(), Box<dyn std::error::Error>> {
    let expression = parse_expression("[1, value, true,]")?;
    let ExpressionKind::ArrayLiteral(elements) = expression.kind() else {
        return Err(io::Error::other("array literal missing").into());
    };
    assert_eq!(elements.len(), 3);
    assert_eq!(elements[1].kind(), &ExpressionKind::Identifier);
    Ok(())
}

#[test]
fn array_holes_remain_explicitly_unsupported() {
    let error = parse_expression("[1,,2]").err();
    assert!(matches!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::UnexpectedToken(_))
    ));
}

#[test]
fn parses_object_literal_properties() -> Result<(), Box<dyn std::error::Error>> {
    let source = "{name: 1, 'two': value}";
    let expression = parse_expression(source)?;
    let ExpressionKind::ObjectLiteral(properties) = expression.kind() else {
        return Err(io::Error::other("object literal missing").into());
    };
    assert_eq!(properties.len(), 2);
    assert_eq!(
        source.get(properties[0].key().start()..properties[0].key().end()),
        Some("name")
    );
    Ok(())
}

#[test]
fn object_literal_requires_colon() {
    let error = parse_expression("{name 1}").err();
    assert_eq!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::ExpectedColon)
    );
}

#[test]
fn static_and_computed_members_chain_left_to_right() -> Result<(), Box<dyn std::error::Error>> {
    let expression = parse_expression("items[0].name")?;
    let ExpressionKind::Member { object, property } = expression.kind() else {
        return Err(io::Error::other("outer member missing").into());
    };
    assert!(matches!(property, MemberProperty::Identifier(_)));
    assert!(matches!(object.kind(), ExpressionKind::Member { .. }));
    Ok(())
}

#[test]
fn member_access_binds_tighter_than_arithmetic() -> Result<(), Box<dyn std::error::Error>> {
    let expression = parse_expression("item.value + 1")?;
    let ExpressionKind::Binary { left, .. } = expression.kind() else {
        return Err(io::Error::other("binary root missing").into());
    };
    assert!(matches!(left.kind(), ExpressionKind::Member { .. }));
    Ok(())
}

#[test]
fn member_expression_is_a_valid_simple_assignment_target() -> Result<(), Box<dyn std::error::Error>>
{
    let expression = parse_expression("state.value = items[0]")?;
    let ExpressionKind::Assignment {
        left,
        operator: AssignmentOperator::Assign,
        right,
    } = expression.kind()
    else {
        return Err(io::Error::other("assignment missing").into());
    };
    assert!(matches!(left.kind(), ExpressionKind::Member { .. }));
    assert!(matches!(right.kind(), ExpressionKind::Member { .. }));
    Ok(())
}

#[test]
fn literals_work_inside_foundation_four_declarations() -> Result<(), Box<dyn std::error::Error>> {
    let program = parse_program("const state = {items: [1, 2]}; let first = state.items[0];")?;
    assert_eq!(program.statements().len(), 2);
    Ok(())
}
