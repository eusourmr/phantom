//! JavaScript Foundation 4 assignment and statement-list regression corpus.

use std::io;

use phantom_js::ast::{
    AssignmentOperator, BinaryOperator, ExpressionKind, StatementKind, VariableKind,
};
use phantom_js::{ParseErrorKind, parse_expression, parse_program};

#[test]
fn simple_assignment_is_right_associative() -> Result<(), Box<dyn std::error::Error>> {
    let expression = parse_expression("a = b = 1")?;

    let ExpressionKind::Assignment {
        left,
        operator: AssignmentOperator::Assign,
        right,
    } = expression.kind()
    else {
        return Err(io::Error::other("outer assignment missing").into());
    };

    assert_eq!(left.kind(), &ExpressionKind::Identifier);
    assert!(matches!(
        right.kind(),
        ExpressionKind::Assignment {
            operator: AssignmentOperator::Assign,
            ..
        }
    ));
    Ok(())
}

#[test]
fn assignment_binds_looser_than_foundation_three_operators()
-> Result<(), Box<dyn std::error::Error>> {
    let expression = parse_expression("value = 1 + 2 >= 3 && true")?;

    let ExpressionKind::Assignment { right, .. } = expression.kind() else {
        return Err(io::Error::other("assignment root missing").into());
    };

    let ExpressionKind::Binary {
        left,
        operator: BinaryOperator::LogicalAnd,
        ..
    } = right.kind()
    else {
        return Err(io::Error::other("logical-and assignment value missing").into());
    };

    assert!(matches!(
        left.kind(),
        ExpressionKind::Binary {
            operator: BinaryOperator::GreaterEqual,
            ..
        }
    ));
    Ok(())
}

#[test]
fn unsupported_assignment_target_is_rejected() {
    let error = parse_expression("1 = value").err();
    assert_eq!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::InvalidAssignmentTarget)
    );
}

#[test]
fn parses_let_const_var_statement_list() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let a = 1; const b = a + 2; var c; c = b;";
    let program = parse_program(source)?;

    assert_eq!(program.statements().len(), 4);

    let StatementKind::VariableDeclaration(let_decl) = program.statements()[0].kind() else {
        return Err(io::Error::other("let declaration missing").into());
    };
    assert_eq!(let_decl.kind(), VariableKind::Let);
    assert_eq!(let_decl.declarators().len(), 1);
    assert_eq!(
        source
            .get(let_decl.declarators()[0].name().start()..let_decl.declarators()[0].name().end()),
        Some("a")
    );
    assert!(let_decl.declarators()[0].initializer().is_some());

    let StatementKind::VariableDeclaration(const_decl) = program.statements()[1].kind() else {
        return Err(io::Error::other("const declaration missing").into());
    };
    assert_eq!(const_decl.kind(), VariableKind::Const);

    let StatementKind::VariableDeclaration(var_decl) = program.statements()[2].kind() else {
        return Err(io::Error::other("var declaration missing").into());
    };
    assert_eq!(var_decl.kind(), VariableKind::Var);
    assert!(var_decl.declarators()[0].initializer().is_none());

    let StatementKind::Expression(expression) = program.statements()[3].kind() else {
        return Err(io::Error::other("assignment statement missing").into());
    };
    assert!(matches!(
        expression.kind(),
        ExpressionKind::Assignment { .. }
    ));
    Ok(())
}

#[test]
fn declaration_lists_support_multiple_bindings() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let first = 1, second = first + 1;";
    let program = parse_program(source)?;

    let StatementKind::VariableDeclaration(declaration) = program.statements()[0].kind() else {
        return Err(io::Error::other("declaration missing").into());
    };

    assert_eq!(declaration.declarators().len(), 2);
    assert_eq!(
        source.get(
            declaration.declarators()[1].name().start()..declaration.declarators()[1].name().end()
        ),
        Some("second")
    );
    Ok(())
}

#[test]
fn const_requires_initializer() {
    let error = parse_program("const value;").err();
    assert_eq!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::ConstRequiresInitializer)
    );
}

#[test]
fn adjacent_statements_require_semicolon_without_asi() {
    let error = parse_program("let a = 1 let b = 2;").err();
    assert_eq!(
        error.map(|error| error.kind()),
        Some(ParseErrorKind::ExpectedSemicolon)
    );
}
