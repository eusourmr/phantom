//! JavaScript Foundation 3 comparison, equality, and logical regression corpus.

use std::io;

use phantom_js::ast::{BinaryOperator, ExpressionKind};
use phantom_js::{
    ParseErrorKind, Punctuator, TokenKind, parse_expression, parse_primary_expression,
};

fn binary_operator(source: &str) -> Result<BinaryOperator, Box<dyn std::error::Error>> {
    let expression = parse_expression(source)?;

    let ExpressionKind::Binary { operator, .. } = expression.kind() else {
        return Err(io::Error::other(format!("binary root missing for {source}")).into());
    };

    Ok(*operator)
}

#[test]
fn relational_operators_are_recognized() -> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        ("1 < 2", BinaryOperator::Less),
        ("1 <= 2", BinaryOperator::LessEqual),
        ("2 > 1", BinaryOperator::Greater),
        ("2 >= 1", BinaryOperator::GreaterEqual),
    ];

    for (source, expected) in cases {
        assert_eq!(binary_operator(source)?, expected);
    }

    Ok(())
}

#[test]
fn equality_operators_are_recognized() -> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        ("a == b", BinaryOperator::Equal),
        ("a != b", BinaryOperator::NotEqual),
        ("a === b", BinaryOperator::StrictEqual),
        ("a !== b", BinaryOperator::StrictNotEqual),
    ];

    for (source, expected) in cases {
        assert_eq!(binary_operator(source)?, expected);
    }

    Ok(())
}

#[test]
fn relational_binds_tighter_than_equality() -> Result<(), Box<dyn std::error::Error>> {
    let expression = parse_expression("1 < 2 == true")?;

    let ExpressionKind::Binary {
        left,
        operator: BinaryOperator::Equal,
        right,
    } = expression.kind()
    else {
        return Err(io::Error::other("equality root missing").into());
    };

    assert_eq!(right.kind(), &ExpressionKind::Boolean(true));

    let ExpressionKind::Binary {
        operator: BinaryOperator::Less,
        ..
    } = left.kind()
    else {
        return Err(io::Error::other("relational left branch missing").into());
    };

    Ok(())
}

#[test]
fn arithmetic_binds_tighter_than_relational() -> Result<(), Box<dyn std::error::Error>> {
    let expression = parse_expression("1 + 2 * 3 >= 7")?;

    let ExpressionKind::Binary {
        left,
        operator: BinaryOperator::GreaterEqual,
        ..
    } = expression.kind()
    else {
        return Err(io::Error::other("relational root missing").into());
    };

    let ExpressionKind::Binary {
        operator: BinaryOperator::Add,
        ..
    } = left.kind()
    else {
        return Err(io::Error::other("additive left branch missing").into());
    };

    Ok(())
}

#[test]
fn logical_and_binds_tighter_than_logical_or() -> Result<(), Box<dyn std::error::Error>> {
    let expression = parse_expression("false || true && false")?;

    let ExpressionKind::Binary {
        left,
        operator: BinaryOperator::LogicalOr,
        right,
    } = expression.kind()
    else {
        return Err(io::Error::other("logical-or root missing").into());
    };

    assert_eq!(left.kind(), &ExpressionKind::Boolean(false));

    let ExpressionKind::Binary {
        operator: BinaryOperator::LogicalAnd,
        ..
    } = right.kind()
    else {
        return Err(io::Error::other("logical-and right branch missing").into());
    };

    Ok(())
}

#[test]
fn equality_binds_tighter_than_logical_and() -> Result<(), Box<dyn std::error::Error>> {
    let expression = parse_expression("a === b && c !== d")?;

    let ExpressionKind::Binary {
        left,
        operator: BinaryOperator::LogicalAnd,
        right,
    } = expression.kind()
    else {
        return Err(io::Error::other("logical-and root missing").into());
    };

    assert!(matches!(
        left.kind(),
        ExpressionKind::Binary {
            operator: BinaryOperator::StrictEqual,
            ..
        }
    ));
    assert!(matches!(
        right.kind(),
        ExpressionKind::Binary {
            operator: BinaryOperator::StrictNotEqual,
            ..
        }
    ));

    Ok(())
}

#[test]
fn logical_operators_are_left_associative() -> Result<(), Box<dyn std::error::Error>> {
    let expression = parse_expression("a || b || c")?;

    let ExpressionKind::Binary {
        left,
        operator: BinaryOperator::LogicalOr,
        ..
    } = expression.kind()
    else {
        return Err(io::Error::other("outer logical-or missing").into());
    };

    assert!(matches!(
        left.kind(),
        ExpressionKind::Binary {
            operator: BinaryOperator::LogicalOr,
            ..
        }
    ));

    Ok(())
}

#[test]
fn grouped_logical_expression_overrides_precedence() -> Result<(), Box<dyn std::error::Error>> {
    let source = "(false || true) && false";
    let expression = parse_expression(source)?;

    assert_eq!(expression.lexeme(source), Some(source));

    let ExpressionKind::Binary {
        left,
        operator: BinaryOperator::LogicalAnd,
        ..
    } = expression.kind()
    else {
        return Err(io::Error::other("logical-and root missing").into());
    };

    let ExpressionKind::Grouped(inner) = left.kind() else {
        return Err(io::Error::other("grouped left branch missing").into());
    };

    assert!(matches!(
        inner.kind(),
        ExpressionKind::Binary {
            operator: BinaryOperator::LogicalOr,
            ..
        }
    ));

    Ok(())
}

#[test]
fn foundation_one_primary_entry_point_keeps_its_boundary() {
    let error = parse_primary_expression("1 < 2").err();

    assert_eq!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::TrailingToken(TokenKind::Punctuator(
            Punctuator::Less,
        )))
    );
}
