//! Syntax tree nodes for Phantom JavaScript Foundation 9.

use crate::Span;

/// JavaScript expression recognized by Foundation 9.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expression {
    kind: ExpressionKind,
    span: Span,
}

impl Expression {
    /// Creates an expression node for the internal parser.
    pub(crate) const fn new(kind: ExpressionKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// Returns the expression category.
    #[must_use]
    pub const fn kind(&self) -> &ExpressionKind {
        &self.kind
    }

    /// Returns the complete source span represented by this node.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// Returns the original source slice represented by this node.
    #[must_use]
    pub fn lexeme<'a>(&self, source: &'a str) -> Option<&'a str> {
        source.get(self.span.start()..self.span.end())
    }
}

/// Prefix unary operator recognized by Foundation 9.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOperator {
    /// Unary plus.
    Plus,
    /// Unary minus.
    Minus,
    /// Logical negation.
    LogicalNot,
}

/// Binary operator recognized by Foundation 9.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    /// Multiplication.
    Multiply,
    /// Division.
    Divide,
    /// Remainder.
    Remainder,
    /// Addition.
    Add,
    /// Subtraction.
    Subtract,
    /// Less-than comparison.
    Less,
    /// Less-than-or-equal comparison.
    LessEqual,
    /// Greater-than comparison.
    Greater,
    /// Greater-than-or-equal comparison.
    GreaterEqual,
    /// Abstract equality.
    Equal,
    /// Abstract inequality.
    NotEqual,
    /// Strict equality.
    StrictEqual,
    /// Strict inequality.
    StrictNotEqual,
    /// Logical AND.
    LogicalAnd,
    /// Logical OR.
    LogicalOr,
}

/// Assignment operator recognized by Foundation 9.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignmentOperator {
    /// Simple assignment `=`.
    Assign,
}

/// Property key/value pair inside a Foundation 9 object literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectProperty {
    key: Span,
    value: Expression,
    span: Span,
}

impl ObjectProperty {
    pub(crate) const fn new(key: Span, value: Expression, span: Span) -> Self {
        Self { key, value, span }
    }

    /// Returns the source span of the literal property key.
    #[must_use]
    pub const fn key(&self) -> Span {
        self.key
    }

    /// Returns the property value expression.
    #[must_use]
    pub const fn value(&self) -> &Expression {
        &self.value
    }

    /// Returns the full property source span.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }
}

/// Property selector inside a Foundation 9 member expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberProperty {
    /// Dot member such as `object.name`; the span points at `name`.
    Identifier(Span),
    /// Computed member such as `object[index]`.
    Computed(Box<Expression>),
}

/// Minimal Foundation 9 function expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionExpression {
    name: Option<Span>,
    parameters: Vec<Span>,
    body: Vec<Statement>,
    span: Span,
}

impl FunctionExpression {
    pub(crate) fn new(
        name: Option<Span>,
        parameters: Vec<Span>,
        body: Vec<Statement>,
        span: Span,
    ) -> Self {
        Self {
            name,
            parameters,
            body,
            span,
        }
    }

    /// Returns the optional local function name.
    #[must_use]
    pub const fn name(&self) -> Option<Span> {
        self.name
    }

    /// Returns parameter identifier spans in source order.
    #[must_use]
    pub fn parameters(&self) -> &[Span] {
        &self.parameters
    }

    /// Returns parsed function-body statements.
    #[must_use]
    pub fn body(&self) -> &[Statement] {
        &self.body
    }

    /// Returns the complete function expression span.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }
}

/// Expression category.
///
/// Literal payload decoding is intentionally deferred. Identifier, number, and
/// string nodes retain their source span so later runtime milestones can apply
/// ECMAScript semantic conversion without duplicating source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpressionKind {
    /// Identifier reference, including the ordinary identifier `undefined`.
    Identifier,

    /// Decimal numeric literal retained by source span.
    NumberLiteral,

    /// Single- or double-quoted string literal retained by source span.
    StringLiteral,

    /// Boolean literal.
    Boolean(bool),

    /// Null literal.
    Null,

    /// Current receiver keyword retained for future runtime binding.
    This,

    /// Parenthesized expression.
    Grouped(Box<Expression>),

    /// Prefix unary expression.
    Unary {
        /// Unary operator.
        operator: UnaryOperator,
        /// Operand expression.
        argument: Box<Expression>,
    },

    /// Left-associative binary expression.
    Binary {
        /// Left operand.
        left: Box<Expression>,
        /// Binary operator.
        operator: BinaryOperator,
        /// Right operand.
        right: Box<Expression>,
    },

    /// Array literal. Foundation 5 accepts explicit elements without holes.
    ArrayLiteral(Vec<Expression>),

    /// Object literal with explicit `key: value` properties.
    ObjectLiteral(Vec<ObjectProperty>),

    /// Static or computed member access.
    Member {
        /// Object expression being accessed.
        object: Box<Expression>,
        /// Static identifier or computed expression.
        property: MemberProperty,
    },

    /// Function or method call expression.
    Call {
        /// Callee expression.
        callee: Box<Expression>,
        /// Arguments in source order.
        arguments: Vec<Expression>,
    },

    /// Minimal function expression with parsed parameters and body statements.
    Function(FunctionExpression),

    /// Right-associative conditional expression `test ? consequent : alternate`.
    Conditional {
        /// Test expression.
        test: Box<Expression>,
        /// Expression selected when the test is truthy.
        consequent: Box<Expression>,
        /// Expression selected when the test is falsy.
        alternate: Box<Expression>,
    },

    /// Right-associative simple assignment expression.
    Assignment {
        /// Assignment target. Foundation 5 admits identifiers and member expressions.
        left: Box<Expression>,
        /// Assignment operator.
        operator: AssignmentOperator,
        /// Assigned expression.
        right: Box<Expression>,
    },
}

/// Top-level bounded statement list recognized by Foundation 9.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    statements: Vec<Statement>,
    span: Span,
}

impl Program {
    pub(crate) fn new(statements: Vec<Statement>, span: Span) -> Self {
        Self { statements, span }
    }

    /// Returns top-level statements in source order.
    #[must_use]
    pub fn statements(&self) -> &[Statement] {
        &self.statements
    }

    /// Returns the complete program span.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }
}

/// One Foundation 9 statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    kind: StatementKind,
    span: Span,
}

impl Statement {
    pub(crate) fn new(kind: StatementKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// Returns the statement category.
    #[must_use]
    pub const fn kind(&self) -> &StatementKind {
        &self.kind
    }

    /// Returns the statement source span.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }
}

/// Initializer accepted by a Foundation 9 classic `for` statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForInitializer {
    /// `let`, `const`, or `var` declaration without the loop separator.
    VariableDeclaration(Box<VariableDeclaration>),
    /// Ordinary expression initializer.
    Expression(Box<Expression>),
}

/// Foundation 9 statement category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatementKind {
    /// `let`, `const`, or `var` declaration statement.
    VariableDeclaration(VariableDeclaration),
    /// Expression statement.
    Expression(Expression),
    /// Braced statement list.
    Block(Vec<Statement>),
    /// `if (test) consequent` with an optional `else` branch.
    If {
        /// Conditional test expression.
        test: Box<Expression>,
        /// Statement selected when the test is truthy.
        consequent: Box<Statement>,
        /// Optional statement selected when the test is falsy.
        alternate: Option<Box<Statement>>,
    },
    /// `while (test) body` loop.
    While {
        /// Loop test expression.
        test: Box<Expression>,
        /// Loop body statement.
        body: Box<Statement>,
    },
    /// Classic `for (init; test; update) body` loop.
    For {
        /// Optional initializer.
        initializer: Option<ForInitializer>,
        /// Optional loop test.
        test: Option<Box<Expression>>,
        /// Optional update expression.
        update: Option<Box<Expression>>,
        /// Loop body statement.
        body: Box<Statement>,
    },
    /// `return` statement inside a function expression.
    Return(Option<Expression>),
}

/// Variable declaration keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableKind {
    /// `let`.
    Let,
    /// `const`.
    Const,
    /// `var`.
    Var,
}

/// One variable declaration statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableDeclaration {
    kind: VariableKind,
    declarators: Vec<VariableDeclarator>,
    span: Span,
}

impl VariableDeclaration {
    pub(crate) fn new(
        kind: VariableKind,
        declarators: Vec<VariableDeclarator>,
        span: Span,
    ) -> Self {
        Self {
            kind,
            declarators,
            span,
        }
    }

    /// Returns the declaration keyword.
    #[must_use]
    pub const fn kind(&self) -> VariableKind {
        self.kind
    }

    /// Returns declarators in source order.
    #[must_use]
    pub fn declarators(&self) -> &[VariableDeclarator] {
        &self.declarators
    }

    /// Returns the declaration source span.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }
}

/// One binding inside a variable declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableDeclarator {
    name: Span,
    initializer: Option<Expression>,
    span: Span,
}

impl VariableDeclarator {
    pub(crate) fn new(name: Span, initializer: Option<Expression>, span: Span) -> Self {
        Self {
            name,
            initializer,
            span,
        }
    }

    /// Returns the identifier span for this binding.
    #[must_use]
    pub const fn name(&self) -> Span {
        self.name
    }

    /// Returns the optional initializer expression.
    #[must_use]
    pub fn initializer(&self) -> Option<&Expression> {
        self.initializer.as_ref()
    }

    /// Returns the complete declarator source span.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }
}
