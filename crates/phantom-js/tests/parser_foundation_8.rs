//! Phantom JavaScript Foundation 8 control-flow regression corpus.

use std::io;

use phantom_js::ast::{ExpressionKind, StatementKind, VariableKind};
use phantom_js::{ParseErrorKind, parse_program};

#[test]
fn parses_braced_statement_block() -> Result<(), Box<dyn std::error::Error>> {
    let program = parse_program("{ let value = 1; value = value + 1; }")?;
    let Some(statement) = program.statements().first() else {
        return Err(io::Error::other("block statement missing").into());
    };
    let StatementKind::Block(statements) = statement.kind() else {
        return Err(io::Error::other("expected block statement").into());
    };

    assert_eq!(statements.len(), 2);
    assert!(matches!(
        statements[0].kind(),
        StatementKind::VariableDeclaration(_)
    ));
    assert!(matches!(statements[1].kind(), StatementKind::Expression(_)));
    Ok(())
}

#[test]
fn parses_if_without_else() -> Result<(), Box<dyn std::error::Error>> {
    let program = parse_program("if (ready) value = 1;")?;
    let Some(statement) = program.statements().first() else {
        return Err(io::Error::other("if statement missing").into());
    };
    let StatementKind::If {
        test,
        consequent,
        alternate,
    } = statement.kind()
    else {
        return Err(io::Error::other("expected if statement").into());
    };

    assert_eq!(test.kind(), &ExpressionKind::Identifier);
    assert!(matches!(consequent.kind(), StatementKind::Expression(_)));
    assert!(alternate.is_none());
    Ok(())
}

#[test]
fn parses_if_else_with_blocks() -> Result<(), Box<dyn std::error::Error>> {
    let program = parse_program("if (ready) { let a = 1; } else { const b = 2; }")?;
    let Some(statement) = program.statements().first() else {
        return Err(io::Error::other("if statement missing").into());
    };
    let StatementKind::If {
        consequent,
        alternate,
        ..
    } = statement.kind()
    else {
        return Err(io::Error::other("expected if statement").into());
    };

    let StatementKind::Block(consequent_statements) = consequent.kind() else {
        return Err(io::Error::other("consequent block missing").into());
    };
    let Some(alternate) = alternate.as_deref() else {
        return Err(io::Error::other("alternate missing").into());
    };
    let StatementKind::Block(alternate_statements) = alternate.kind() else {
        return Err(io::Error::other("alternate block missing").into());
    };

    assert_eq!(consequent_statements.len(), 1);
    assert_eq!(alternate_statements.len(), 1);
    Ok(())
}

#[test]
fn dangling_else_binds_to_nearest_if() -> Result<(), Box<dyn std::error::Error>> {
    let program = parse_program("if (a) if (b) value = 1; else value = 2;")?;
    let Some(outer) = program.statements().first() else {
        return Err(io::Error::other("outer if missing").into());
    };
    let StatementKind::If {
        consequent,
        alternate: outer_alternate,
        ..
    } = outer.kind()
    else {
        return Err(io::Error::other("expected outer if").into());
    };
    assert!(outer_alternate.is_none());

    let StatementKind::If {
        alternate: inner_alternate,
        ..
    } = consequent.kind()
    else {
        return Err(io::Error::other("expected nested if").into());
    };
    assert!(inner_alternate.is_some());
    Ok(())
}

#[test]
fn function_body_if_can_return() -> Result<(), Box<dyn std::error::Error>> {
    let program = parse_program(
        "const choose = function(flag) { if (flag) { return 1; } else { return 2; } };",
    )?;
    let Some(statement) = program.statements().first() else {
        return Err(io::Error::other("declaration missing").into());
    };
    let StatementKind::VariableDeclaration(declaration) = statement.kind() else {
        return Err(io::Error::other("variable declaration missing").into());
    };
    assert_eq!(declaration.kind(), VariableKind::Const);
    let Some(initializer) = declaration
        .declarators()
        .first()
        .and_then(|item| item.initializer())
    else {
        return Err(io::Error::other("function initializer missing").into());
    };
    let ExpressionKind::Function(function) = initializer.kind() else {
        return Err(io::Error::other("function expression missing").into());
    };
    assert!(matches!(
        function.body().first().map(|statement| statement.kind()),
        Some(StatementKind::If { .. })
    ));
    Ok(())
}

#[test]
fn if_requires_parenthesized_test() {
    let error = parse_program("if ready value = 1;").err();
    assert_eq!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::ExpectedLeftParen)
    );
}

#[test]
fn if_requires_closing_parenthesis() {
    let error = parse_program("if (ready value = 1;").err();
    assert_eq!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::ExpectedRightParen)
    );
}

#[test]
fn unterminated_block_is_rejected() {
    let error = parse_program("{ let value = 1;").err();
    assert_eq!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::ExpectedRightBrace)
    );
}
