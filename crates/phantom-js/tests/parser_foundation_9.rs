//! Phantom JavaScript Foundation 9 regression corpus.

use std::io;

use phantom_js::ast::{ExpressionKind, ForInitializer, StatementKind, VariableKind};
use phantom_js::{ParseErrorKind, parse_expression, parse_program};

#[test]
fn parses_while_statement_with_block_body() -> Result<(), Box<dyn std::error::Error>> {
    let program = parse_program("while (ready) { value = value + 1; }")?;
    let Some(statement) = program.statements().first() else {
        return Err(io::Error::other("while statement missing").into());
    };
    let StatementKind::While { test, body } = statement.kind() else {
        return Err(io::Error::other("expected while statement").into());
    };

    assert!(matches!(test.kind(), ExpressionKind::Identifier));
    assert!(matches!(body.kind(), StatementKind::Block(_)));
    Ok(())
}

#[test]
fn while_test_reuses_existing_expression_precedence() -> Result<(), Box<dyn std::error::Error>> {
    let program = parse_program("while (ready ? left : right) value = next;")?;
    let Some(statement) = program.statements().first() else {
        return Err(io::Error::other("while statement missing").into());
    };
    let StatementKind::While { test, .. } = statement.kind() else {
        return Err(io::Error::other("expected while statement").into());
    };

    assert!(matches!(test.kind(), ExpressionKind::Conditional { .. }));
    Ok(())
}

#[test]
fn parses_classic_for_with_expression_clauses() -> Result<(), Box<dyn std::error::Error>> {
    let program = parse_program("for (i = 0; i < 10; i = i + 1) total = total + i;")?;
    let Some(statement) = program.statements().first() else {
        return Err(io::Error::other("for statement missing").into());
    };
    let StatementKind::For {
        initializer,
        test,
        update,
        body,
    } = statement.kind()
    else {
        return Err(io::Error::other("expected for statement").into());
    };

    assert!(matches!(initializer, Some(ForInitializer::Expression(_))));
    assert!(test.is_some());
    assert!(update.is_some());
    assert!(matches!(body.kind(), StatementKind::Expression(_)));
    Ok(())
}

#[test]
fn parses_for_with_variable_declaration_initializer() -> Result<(), Box<dyn std::error::Error>> {
    let program = parse_program("for (let i = 0; i < 3; i = i + 1) { value = i; }")?;
    let Some(statement) = program.statements().first() else {
        return Err(io::Error::other("for statement missing").into());
    };
    let StatementKind::For { initializer, .. } = statement.kind() else {
        return Err(io::Error::other("expected for statement").into());
    };
    let Some(ForInitializer::VariableDeclaration(declaration)) = initializer else {
        return Err(io::Error::other("expected variable declaration initializer").into());
    };

    assert_eq!(declaration.kind(), VariableKind::Let);
    assert_eq!(declaration.declarators().len(), 1);
    Ok(())
}

#[test]
fn parses_for_with_all_optional_clauses_empty() -> Result<(), Box<dyn std::error::Error>> {
    let program = parse_program("for (;;) { value = value + 1; }")?;
    let Some(statement) = program.statements().first() else {
        return Err(io::Error::other("for statement missing").into());
    };
    let StatementKind::For {
        initializer,
        test,
        update,
        ..
    } = statement.kind()
    else {
        return Err(io::Error::other("expected for statement").into());
    };

    assert!(initializer.is_none());
    assert!(test.is_none());
    assert!(update.is_none());
    Ok(())
}

#[test]
fn return_inside_loop_remains_bound_to_function_context() -> Result<(), Box<dyn std::error::Error>>
{
    let expression = parse_expression("function(){ while (ready) { return value; } }")?;
    let ExpressionKind::Function(function) = expression.kind() else {
        return Err(io::Error::other("expected function expression").into());
    };
    let Some(statement) = function.body().first() else {
        return Err(io::Error::other("function loop missing").into());
    };
    let StatementKind::While { body, .. } = statement.kind() else {
        return Err(io::Error::other("expected while in function").into());
    };
    let StatementKind::Block(statements) = body.kind() else {
        return Err(io::Error::other("expected loop block").into());
    };

    assert!(
        statements
            .first()
            .is_some_and(|statement| matches!(statement.kind(), StatementKind::Return(Some(_))))
    );
    Ok(())
}

#[test]
fn const_for_initializer_still_requires_value() {
    let error = parse_program("for (const i; i < 3; i = i + 1) value = i;").err();
    assert_eq!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::ConstRequiresInitializer)
    );
}

#[test]
fn for_header_requires_both_semicolon_separators() {
    let error = parse_program("for (i = 0 i < 3; i = i + 1) value = i;").err();
    assert_eq!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::ExpectedSemicolon)
    );
}

#[test]
fn loops_participate_in_existing_nesting_budget() {
    let source = format!(
        "{}value = 1;{}",
        "while (ready) {".repeat(257),
        "}".repeat(257)
    );
    let error = parse_program(&source).err();
    assert_eq!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::NestingLimitExceeded)
    );
}
