//! Bounded parser for Phantom JavaScript Foundation 9.

use std::error::Error;
use std::fmt;

use crate::ast::{
    AssignmentOperator, BinaryOperator, Expression, ExpressionKind, ForInitializer,
    FunctionExpression, MemberProperty, ObjectProperty, Program, Statement, StatementKind,
    UnaryOperator, VariableDeclaration, VariableDeclarator, VariableKind,
};
use crate::{Keyword, LexError, LexErrorKind, Punctuator, Span, Token, TokenKind, lex};

const MAX_EXPRESSION_NESTING_DEPTH: usize = 256;

/// Failure category produced by the Foundation 9 parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseErrorKind {
    /// Lexing failed before syntax analysis could begin.
    Lexical(LexErrorKind),

    /// Source contained no expression.
    EmptyInput,

    /// Current token cannot begin a supported expression.
    UnexpectedToken(TokenKind),

    /// A parenthesized expression was not closed with `)`.
    ExpectedRightParen,

    /// An array/computed-member expression was not closed with `]`.
    ExpectedRightBracket,

    /// An object literal was not closed with `}`.
    ExpectedRightBrace,

    /// Object/member syntax expected a property name.
    ExpectedPropertyName,

    /// Object literal property name was not followed by `:`.
    ExpectedColon,

    /// A valid expression was followed by additional unsupported tokens.
    TrailingToken(TokenKind),

    /// A declaration expected a binding identifier.
    ExpectedIdentifier,

    /// Foundation 9 simple assignment received an unsupported target.
    InvalidAssignmentTarget,

    /// A function expression omitted its opening `(`.
    ExpectedLeftParen,

    /// A function expression omitted its opening `{`.
    ExpectedLeftBrace,

    /// Two supported statements were not separated by `;`.
    ExpectedSemicolon,

    /// A `const` declaration omitted its required initializer.
    ConstRequiresInitializer,

    /// Recursive expression nesting exceeded the parser budget.
    NestingLimitExceeded,
}

/// Bounded JavaScript parser error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseError {
    kind: ParseErrorKind,
    offset: usize,
}

impl ParseError {
    const fn new(kind: ParseErrorKind, offset: usize) -> Self {
        Self { kind, offset }
    }

    fn from_lex(error: LexError) -> Self {
        Self::new(ParseErrorKind::Lexical(error.kind()), error.offset())
    }

    /// Returns the parser failure category.
    #[must_use]
    pub const fn kind(self) -> ParseErrorKind {
        self.kind
    }

    /// Returns the byte offset where parsing failed.
    #[must_use]
    pub const fn offset(self) -> usize {
        self.offset
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "JavaScript parse error {:?} at byte {}",
            self.kind, self.offset
        )
    }
}

impl Error for ParseError {}

/// Parses exactly one Foundation 1 primary expression.
///
/// This compatibility entry point intentionally keeps Foundation 1 behavior:
/// operators remain trailing tokens and grouped expressions recursively contain
/// only primary expressions.
///
/// # Errors
///
/// Returns [`ParseError`] for lexical failures, unsupported primary forms,
/// missing closing parentheses, excessive nesting, or trailing tokens.
pub fn parse_primary_expression(source: &str) -> Result<Expression, ParseError> {
    let tokens = lex(source).map_err(ParseError::from_lex)?;
    let mut parser = Parser::new(tokens);
    let expression = parser.parse_primary_compat(0)?;
    parser.finish(expression)
}

/// Parses exactly one Foundation 9 expression.
///
/// Supported precedence, from tighter to looser:
/// primary/grouped/collection/function literals -> member/call access -> unary `+ - !` ->
/// multiplicative `* / %` -> additive `+ -` -> relational `< <= > >=` ->
/// equality `== != === !==` -> logical AND `&&` -> logical OR `||` ->
/// simple assignment `=`.
///
/// Foundation 9 preserves the Foundation 7 expression grammar. Statement-level
/// control flow is exposed through [`parse_program`]; runtime semantics, closures,
/// constructors, compound assignment, and DOM bindings remain deferred.
///
/// # Errors
///
/// Returns [`ParseError`] for lexical failures, unsupported forms, invalid
/// assignment targets, missing closing parentheses, excessive nesting, or
/// trailing tokens.
pub fn parse_expression(source: &str) -> Result<Expression, ParseError> {
    let tokens = lex(source).map_err(ParseError::from_lex)?;
    let mut parser = Parser::new(tokens);
    let expression = parser.parse_assignment(0)?;
    parser.finish(expression)
}

/// Parses a minimal Foundation 9 statement list.
///
/// Supported statements are expression statements, `let`/`const`/`var`, braced
/// blocks, and `if`/`else`. Function-expression bodies additionally recognize
/// `return`. Semicolon insertion remains deliberately deferred.
///
/// # Errors
///
/// Returns [`ParseError`] for lexical or expression failures, malformed
/// declarations, missing statement separators, or excessive nesting.
pub fn parse_program(source: &str) -> Result<Program, ParseError> {
    let tokens = lex(source).map_err(ParseError::from_lex)?;
    let mut parser = Parser::new(tokens);
    parser.parse_program()
}

struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, cursor: 0 }
    }

    fn current(&self) -> Option<Token> {
        self.tokens.get(self.cursor).copied()
    }

    fn advance(&mut self) {
        if self.cursor < self.tokens.len() {
            self.cursor = self.cursor.saturating_add(1);
        }
    }

    fn finish(&self, expression: Expression) -> Result<Expression, ParseError> {
        let Some(token) = self.current() else {
            return Ok(expression);
        };

        match token.kind() {
            TokenKind::Eof => Ok(expression),
            trailing => Err(ParseError::new(
                ParseErrorKind::TrailingToken(trailing),
                token.span().start(),
            )),
        }
    }

    fn ensure_depth(&self, depth: usize, offset: usize) -> Result<(), ParseError> {
        if depth > MAX_EXPRESSION_NESTING_DEPTH {
            Err(ParseError::new(
                ParseErrorKind::NestingLimitExceeded,
                offset,
            ))
        } else {
            Ok(())
        }
    }

    fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut statements = Vec::new();
        let mut first_start = None;
        let mut last_end = 0;

        while let Some(token) = self.current() {
            match token.kind() {
                TokenKind::Eof => {
                    if first_start.is_none() {
                        last_end = token.span().start();
                    }
                    break;
                }
                TokenKind::Punctuator(Punctuator::Semicolon) => {
                    if first_start.is_none() {
                        first_start = Some(token.span().start());
                    }
                    last_end = token.span().end();
                    self.advance();
                }
                _ => {
                    let statement = self.parse_statement(0, false)?;
                    if first_start.is_none() {
                        first_start = Some(statement.span().start());
                    }
                    last_end = statement.span().end();
                    statements.push(statement);
                }
            }
        }

        let start = first_start.unwrap_or(0);
        Ok(Program::new(statements, Span::new(start, last_end)))
    }

    fn parse_statement(
        &mut self,
        depth: usize,
        allow_return: bool,
    ) -> Result<Statement, ParseError> {
        let Some(token) = self.current() else {
            return Err(ParseError::new(ParseErrorKind::EmptyInput, 0));
        };
        self.ensure_depth(depth, token.span().start())?;

        match token.kind() {
            TokenKind::Keyword(Keyword::Let) => {
                self.parse_variable_statement(VariableKind::Let, depth)
            }
            TokenKind::Keyword(Keyword::Const) => {
                self.parse_variable_statement(VariableKind::Const, depth)
            }
            TokenKind::Keyword(Keyword::Var) => {
                self.parse_variable_statement(VariableKind::Var, depth)
            }
            TokenKind::Keyword(Keyword::If) => self.parse_if_statement(depth, allow_return),
            TokenKind::Keyword(Keyword::While) => self.parse_while_statement(depth, allow_return),
            TokenKind::Keyword(Keyword::For) => self.parse_for_statement(depth, allow_return),
            TokenKind::Keyword(Keyword::Return) if allow_return => {
                self.parse_return_statement(depth)
            }
            TokenKind::Punctuator(Punctuator::LeftBrace) => {
                self.parse_block_statement(depth, allow_return)
            }
            _ => self.parse_expression_statement(depth),
        }
    }

    fn parse_block_statement(
        &mut self,
        depth: usize,
        allow_return: bool,
    ) -> Result<Statement, ParseError> {
        let Some(open) = self.current() else {
            return Err(ParseError::new(ParseErrorKind::EmptyInput, 0));
        };
        self.ensure_depth(depth, open.span().start())?;
        self.advance();

        let mut statements = Vec::new();
        loop {
            let Some(token) = self.current() else {
                return Err(ParseError::new(
                    ParseErrorKind::ExpectedRightBrace,
                    open.span().end(),
                ));
            };

            match token.kind() {
                TokenKind::Punctuator(Punctuator::RightBrace) => {
                    self.advance();
                    return Ok(Statement::new(
                        StatementKind::Block(statements),
                        Span::new(open.span().start(), token.span().end()),
                    ));
                }
                TokenKind::Punctuator(Punctuator::Semicolon) => self.advance(),
                TokenKind::Eof => {
                    return Err(ParseError::new(
                        ParseErrorKind::ExpectedRightBrace,
                        token.span().start(),
                    ));
                }
                _ => statements.push(self.parse_statement(depth.saturating_add(1), allow_return)?),
            }
        }
    }

    fn parse_if_statement(
        &mut self,
        depth: usize,
        allow_return: bool,
    ) -> Result<Statement, ParseError> {
        let Some(keyword) = self.current() else {
            return Err(ParseError::new(ParseErrorKind::EmptyInput, 0));
        };
        self.ensure_depth(depth, keyword.span().start())?;
        let start = keyword.span().start();
        self.advance();

        let Some(left_paren) = self.current() else {
            return Err(ParseError::new(
                ParseErrorKind::ExpectedLeftParen,
                keyword.span().end(),
            ));
        };
        if left_paren.kind() != TokenKind::Punctuator(Punctuator::LeftParen) {
            return Err(ParseError::new(
                ParseErrorKind::ExpectedLeftParen,
                left_paren.span().start(),
            ));
        }
        self.advance();

        let test = self.parse_assignment(depth.saturating_add(1))?;
        let Some(right_paren) = self.current() else {
            return Err(ParseError::new(
                ParseErrorKind::ExpectedRightParen,
                test.span().end(),
            ));
        };
        if right_paren.kind() != TokenKind::Punctuator(Punctuator::RightParen) {
            return Err(ParseError::new(
                ParseErrorKind::ExpectedRightParen,
                right_paren.span().start(),
            ));
        }
        self.advance();

        let consequent = self.parse_statement(depth.saturating_add(1), allow_return)?;
        let alternate = if self
            .current()
            .is_some_and(|token| token.kind() == TokenKind::Keyword(Keyword::Else))
        {
            self.advance();
            Some(Box::new(
                self.parse_statement(depth.saturating_add(1), allow_return)?,
            ))
        } else {
            None
        };

        let end = alternate
            .as_ref()
            .map_or(consequent.span().end(), |statement| statement.span().end());
        Ok(Statement::new(
            StatementKind::If {
                test: Box::new(test),
                consequent: Box::new(consequent),
                alternate,
            },
            Span::new(start, end),
        ))
    }

    // PHANTOM_JS_FOUNDATION_9_LOOPS_I
    fn parse_while_statement(
        &mut self,
        depth: usize,
        allow_return: bool,
    ) -> Result<Statement, ParseError> {
        let Some(keyword) = self.current() else {
            return Err(ParseError::new(ParseErrorKind::EmptyInput, 0));
        };
        self.ensure_depth(depth, keyword.span().start())?;
        let start = keyword.span().start();
        self.advance();

        let Some(left_paren) = self.current() else {
            return Err(ParseError::new(
                ParseErrorKind::ExpectedLeftParen,
                keyword.span().end(),
            ));
        };
        if left_paren.kind() != TokenKind::Punctuator(Punctuator::LeftParen) {
            return Err(ParseError::new(
                ParseErrorKind::ExpectedLeftParen,
                left_paren.span().start(),
            ));
        }
        self.advance();

        let test = self.parse_assignment(depth.saturating_add(1))?;
        let Some(right_paren) = self.current() else {
            return Err(ParseError::new(
                ParseErrorKind::ExpectedRightParen,
                test.span().end(),
            ));
        };
        if right_paren.kind() != TokenKind::Punctuator(Punctuator::RightParen) {
            return Err(ParseError::new(
                ParseErrorKind::ExpectedRightParen,
                right_paren.span().start(),
            ));
        }
        self.advance();

        let body = self.parse_statement(depth.saturating_add(1), allow_return)?;
        let end = body.span().end();
        Ok(Statement::new(
            StatementKind::While {
                test: Box::new(test),
                body: Box::new(body),
            },
            Span::new(start, end),
        ))
    }

    fn parse_for_statement(
        &mut self,
        depth: usize,
        allow_return: bool,
    ) -> Result<Statement, ParseError> {
        let Some(keyword) = self.current() else {
            return Err(ParseError::new(ParseErrorKind::EmptyInput, 0));
        };
        self.ensure_depth(depth, keyword.span().start())?;
        let start = keyword.span().start();
        self.advance();

        let Some(left_paren) = self.current() else {
            return Err(ParseError::new(
                ParseErrorKind::ExpectedLeftParen,
                keyword.span().end(),
            ));
        };
        if left_paren.kind() != TokenKind::Punctuator(Punctuator::LeftParen) {
            return Err(ParseError::new(
                ParseErrorKind::ExpectedLeftParen,
                left_paren.span().start(),
            ));
        }
        self.advance();

        let initializer = match self.current().map(Token::kind) {
            Some(TokenKind::Punctuator(Punctuator::Semicolon)) => None,
            Some(TokenKind::Keyword(Keyword::Let)) => {
                Some(ForInitializer::VariableDeclaration(Box::new(
                    self.parse_variable_declaration(VariableKind::Let, depth.saturating_add(1))?,
                )))
            }
            Some(TokenKind::Keyword(Keyword::Const)) => {
                Some(ForInitializer::VariableDeclaration(Box::new(
                    self.parse_variable_declaration(VariableKind::Const, depth.saturating_add(1))?,
                )))
            }
            Some(TokenKind::Keyword(Keyword::Var)) => {
                Some(ForInitializer::VariableDeclaration(Box::new(
                    self.parse_variable_declaration(VariableKind::Var, depth.saturating_add(1))?,
                )))
            }
            Some(_) => Some(ForInitializer::Expression(Box::new(
                self.parse_assignment(depth.saturating_add(1))?,
            ))),
            None => {
                return Err(ParseError::new(
                    ParseErrorKind::ExpectedSemicolon,
                    left_paren.span().end(),
                ));
            }
        };
        self.consume_required_punctuator(Punctuator::Semicolon, ParseErrorKind::ExpectedSemicolon)?;

        let test = if self
            .current()
            .is_some_and(|token| token.kind() == TokenKind::Punctuator(Punctuator::Semicolon))
        {
            None
        } else {
            Some(Box::new(self.parse_assignment(depth.saturating_add(1))?))
        };
        self.consume_required_punctuator(Punctuator::Semicolon, ParseErrorKind::ExpectedSemicolon)?;

        let update = if self
            .current()
            .is_some_and(|token| token.kind() == TokenKind::Punctuator(Punctuator::RightParen))
        {
            None
        } else {
            Some(Box::new(self.parse_assignment(depth.saturating_add(1))?))
        };
        self.consume_required_punctuator(
            Punctuator::RightParen,
            ParseErrorKind::ExpectedRightParen,
        )?;

        let body = self.parse_statement(depth.saturating_add(1), allow_return)?;
        let end = body.span().end();
        Ok(Statement::new(
            StatementKind::For {
                initializer,
                test,
                update,
                body: Box::new(body),
            },
            Span::new(start, end),
        ))
    }

    fn consume_required_punctuator(
        &mut self,
        punctuator: Punctuator,
        error_kind: ParseErrorKind,
    ) -> Result<(), ParseError> {
        let Some(token) = self.current() else {
            let offset = self.tokens.last().map_or(0, |token| token.span().end());
            return Err(ParseError::new(error_kind, offset));
        };
        if token.kind() != TokenKind::Punctuator(punctuator) {
            return Err(ParseError::new(error_kind, token.span().start()));
        }
        self.advance();
        Ok(())
    }

    fn parse_variable_statement(
        &mut self,
        kind: VariableKind,
        depth: usize,
    ) -> Result<Statement, ParseError> {
        let declaration = self.parse_variable_declaration(kind, depth)?;
        let start = declaration.span().start();
        let declaration_end = declaration.span().end();
        let end = self.consume_statement_terminator(declaration_end)?;

        Ok(Statement::new(
            StatementKind::VariableDeclaration(declaration),
            Span::new(start, end),
        ))
    }

    fn parse_variable_declaration(
        &mut self,
        kind: VariableKind,
        depth: usize,
    ) -> Result<VariableDeclaration, ParseError> {
        let Some(keyword) = self.current() else {
            return Err(ParseError::new(ParseErrorKind::EmptyInput, 0));
        };
        let start = keyword.span().start();
        self.advance();

        let mut declarators = Vec::new();
        let declaration_end;

        loop {
            let Some(name_token) = self.current() else {
                return Err(ParseError::new(
                    ParseErrorKind::ExpectedIdentifier,
                    keyword.span().end(),
                ));
            };

            if name_token.kind() != TokenKind::Identifier {
                return Err(ParseError::new(
                    ParseErrorKind::ExpectedIdentifier,
                    name_token.span().start(),
                ));
            }

            self.advance();
            let name = name_token.span();
            let initializer = if self
                .current()
                .is_some_and(|token| token.kind() == TokenKind::Punctuator(Punctuator::Assign))
            {
                self.advance();
                Some(self.parse_assignment(depth)?)
            } else {
                None
            };

            if kind == VariableKind::Const && initializer.is_none() {
                let offset = self
                    .current()
                    .map_or(name.end(), |token| token.span().start());
                return Err(ParseError::new(
                    ParseErrorKind::ConstRequiresInitializer,
                    offset,
                ));
            }

            let end = initializer
                .as_ref()
                .map_or(name.end(), |value| value.span().end());
            declarators.push(VariableDeclarator::new(
                name,
                initializer,
                Span::new(name.start(), end),
            ));

            let Some(next) = self.current() else {
                declaration_end = end;
                break;
            };

            if next.kind() == TokenKind::Punctuator(Punctuator::Comma) {
                self.advance();
                continue;
            }

            declaration_end = end;
            break;
        }

        Ok(VariableDeclaration::new(
            kind,
            declarators,
            Span::new(start, declaration_end),
        ))
    }

    fn parse_expression_statement(&mut self, depth: usize) -> Result<Statement, ParseError> {
        let expression = self.parse_assignment(depth)?;
        let start = expression.span().start();
        let expression_end = expression.span().end();
        let end = self.consume_statement_terminator(expression_end)?;

        Ok(Statement::new(
            StatementKind::Expression(expression),
            Span::new(start, end),
        ))
    }

    fn consume_statement_terminator(&mut self, default_end: usize) -> Result<usize, ParseError> {
        let Some(token) = self.current() else {
            return Ok(default_end);
        };

        match token.kind() {
            TokenKind::Punctuator(Punctuator::Semicolon) => {
                self.advance();
                Ok(token.span().end())
            }
            TokenKind::Eof => Ok(default_end),
            _ => Err(ParseError::new(
                ParseErrorKind::ExpectedSemicolon,
                token.span().start(),
            )),
        }
    }

    fn parse_assignment(&mut self, depth: usize) -> Result<Expression, ParseError> {
        let left = self.parse_conditional(depth)?;
        let Some(token) = self.current() else {
            return Ok(left);
        };

        if token.kind() != TokenKind::Punctuator(Punctuator::Assign) {
            return Ok(left);
        }

        if !matches!(
            left.kind(),
            ExpressionKind::Identifier | ExpressionKind::Member { .. }
        ) {
            return Err(ParseError::new(
                ParseErrorKind::InvalidAssignmentTarget,
                left.span().start(),
            ));
        }

        self.ensure_depth(depth, token.span().start())?;
        self.advance();
        let right = self.parse_assignment(depth.saturating_add(1))?;
        let span = Span::new(left.span().start(), right.span().end());

        Ok(Expression::new(
            ExpressionKind::Assignment {
                left: Box::new(left),
                operator: AssignmentOperator::Assign,
                right: Box::new(right),
            },
            span,
        ))
    }

    // PHANTOM_JS_FOUNDATION_7_CONDITIONAL_THIS
    fn parse_conditional(&mut self, depth: usize) -> Result<Expression, ParseError> {
        let test = self.parse_logical_or(depth)?;
        let Some(question) = self.current() else {
            return Ok(test);
        };

        if question.kind() != TokenKind::Punctuator(Punctuator::Question) {
            return Ok(test);
        }

        self.ensure_depth(depth, question.span().start())?;
        self.advance();
        let consequent = self.parse_assignment(depth.saturating_add(1))?;

        let Some(colon) = self.current() else {
            return Err(ParseError::new(
                ParseErrorKind::ExpectedColon,
                consequent.span().end(),
            ));
        };
        if colon.kind() != TokenKind::Punctuator(Punctuator::Colon) {
            return Err(ParseError::new(
                ParseErrorKind::ExpectedColon,
                colon.span().start(),
            ));
        }
        self.advance();

        let alternate = self.parse_assignment(depth.saturating_add(1))?;
        let span = Span::new(test.span().start(), alternate.span().end());

        Ok(Expression::new(
            ExpressionKind::Conditional {
                test: Box::new(test),
                consequent: Box::new(consequent),
                alternate: Box::new(alternate),
            },
            span,
        ))
    }

    fn parse_logical_or(&mut self, depth: usize) -> Result<Expression, ParseError> {
        let mut left = self.parse_logical_and(depth)?;

        loop {
            let Some(token) = self.current() else {
                return Ok(left);
            };

            if token.kind() != TokenKind::Punctuator(Punctuator::LogicalOr) {
                return Ok(left);
            }

            self.advance();
            let right = self.parse_logical_and(depth)?;
            let span = Span::new(left.span().start(), right.span().end());

            left = Expression::new(
                ExpressionKind::Binary {
                    left: Box::new(left),
                    operator: BinaryOperator::LogicalOr,
                    right: Box::new(right),
                },
                span,
            );
        }
    }

    fn parse_logical_and(&mut self, depth: usize) -> Result<Expression, ParseError> {
        let mut left = self.parse_equality(depth)?;

        loop {
            let Some(token) = self.current() else {
                return Ok(left);
            };

            if token.kind() != TokenKind::Punctuator(Punctuator::LogicalAnd) {
                return Ok(left);
            }

            self.advance();
            let right = self.parse_equality(depth)?;
            let span = Span::new(left.span().start(), right.span().end());

            left = Expression::new(
                ExpressionKind::Binary {
                    left: Box::new(left),
                    operator: BinaryOperator::LogicalAnd,
                    right: Box::new(right),
                },
                span,
            );
        }
    }

    fn parse_equality(&mut self, depth: usize) -> Result<Expression, ParseError> {
        let mut left = self.parse_relational(depth)?;

        loop {
            let Some(token) = self.current() else {
                return Ok(left);
            };

            let operator = match token.kind() {
                TokenKind::Punctuator(Punctuator::Equal) => BinaryOperator::Equal,
                TokenKind::Punctuator(Punctuator::NotEqual) => BinaryOperator::NotEqual,
                TokenKind::Punctuator(Punctuator::StrictEqual) => BinaryOperator::StrictEqual,
                TokenKind::Punctuator(Punctuator::StrictNotEqual) => BinaryOperator::StrictNotEqual,
                _ => return Ok(left),
            };

            self.advance();
            let right = self.parse_relational(depth)?;
            let span = Span::new(left.span().start(), right.span().end());

            left = Expression::new(
                ExpressionKind::Binary {
                    left: Box::new(left),
                    operator,
                    right: Box::new(right),
                },
                span,
            );
        }
    }

    fn parse_relational(&mut self, depth: usize) -> Result<Expression, ParseError> {
        let mut left = self.parse_additive(depth)?;

        loop {
            let Some(token) = self.current() else {
                return Ok(left);
            };

            let operator = match token.kind() {
                TokenKind::Punctuator(Punctuator::Less) => BinaryOperator::Less,
                TokenKind::Punctuator(Punctuator::LessEqual) => BinaryOperator::LessEqual,
                TokenKind::Punctuator(Punctuator::Greater) => BinaryOperator::Greater,
                TokenKind::Punctuator(Punctuator::GreaterEqual) => BinaryOperator::GreaterEqual,
                _ => return Ok(left),
            };

            self.advance();
            let right = self.parse_additive(depth)?;
            let span = Span::new(left.span().start(), right.span().end());

            left = Expression::new(
                ExpressionKind::Binary {
                    left: Box::new(left),
                    operator,
                    right: Box::new(right),
                },
                span,
            );
        }
    }

    fn parse_additive(&mut self, depth: usize) -> Result<Expression, ParseError> {
        let mut left = self.parse_multiplicative(depth)?;

        loop {
            let Some(token) = self.current() else {
                return Ok(left);
            };

            let operator = match token.kind() {
                TokenKind::Punctuator(Punctuator::Plus) => BinaryOperator::Add,
                TokenKind::Punctuator(Punctuator::Minus) => BinaryOperator::Subtract,
                _ => return Ok(left),
            };

            self.advance();
            let right = self.parse_multiplicative(depth)?;
            let span = Span::new(left.span().start(), right.span().end());

            left = Expression::new(
                ExpressionKind::Binary {
                    left: Box::new(left),
                    operator,
                    right: Box::new(right),
                },
                span,
            );
        }
    }

    fn parse_multiplicative(&mut self, depth: usize) -> Result<Expression, ParseError> {
        let mut left = self.parse_unary(depth)?;

        loop {
            let Some(token) = self.current() else {
                return Ok(left);
            };

            let operator = match token.kind() {
                TokenKind::Punctuator(Punctuator::Star) => BinaryOperator::Multiply,
                TokenKind::Punctuator(Punctuator::Slash) => BinaryOperator::Divide,
                TokenKind::Punctuator(Punctuator::Percent) => BinaryOperator::Remainder,
                _ => return Ok(left),
            };

            self.advance();
            let right = self.parse_unary(depth)?;
            let span = Span::new(left.span().start(), right.span().end());

            left = Expression::new(
                ExpressionKind::Binary {
                    left: Box::new(left),
                    operator,
                    right: Box::new(right),
                },
                span,
            );
        }
    }

    fn parse_unary(&mut self, depth: usize) -> Result<Expression, ParseError> {
        let Some(token) = self.current() else {
            return Err(ParseError::new(ParseErrorKind::EmptyInput, 0));
        };

        self.ensure_depth(depth, token.span().start())?;

        let operator = match token.kind() {
            TokenKind::Punctuator(Punctuator::Plus) => Some(UnaryOperator::Plus),
            TokenKind::Punctuator(Punctuator::Minus) => Some(UnaryOperator::Minus),
            TokenKind::Punctuator(Punctuator::Bang) => Some(UnaryOperator::LogicalNot),
            _ => None,
        };

        let Some(operator) = operator else {
            return self.parse_member_expression(depth);
        };

        self.advance();
        let argument = self.parse_unary(depth.saturating_add(1))?;
        let span = Span::new(token.span().start(), argument.span().end());

        Ok(Expression::new(
            ExpressionKind::Unary {
                operator,
                argument: Box::new(argument),
            },
            span,
        ))
    }

    fn parse_member_expression(&mut self, depth: usize) -> Result<Expression, ParseError> {
        let mut object = self.parse_primary_expression(depth)?;

        while let Some(token) = self.current() {
            match token.kind() {
                TokenKind::Punctuator(Punctuator::Dot) => {
                    self.advance();
                    let Some(property_token) = self.current() else {
                        return Err(ParseError::new(
                            ParseErrorKind::ExpectedPropertyName,
                            token.span().end(),
                        ));
                    };

                    if property_token.kind() != TokenKind::Identifier {
                        return Err(ParseError::new(
                            ParseErrorKind::ExpectedPropertyName,
                            property_token.span().start(),
                        ));
                    }

                    self.advance();
                    let span = Span::new(object.span().start(), property_token.span().end());
                    object = Expression::new(
                        ExpressionKind::Member {
                            object: Box::new(object),
                            property: MemberProperty::Identifier(property_token.span()),
                        },
                        span,
                    );
                }
                TokenKind::Punctuator(Punctuator::LeftBracket) => {
                    self.ensure_depth(depth, token.span().start())?;
                    self.advance();
                    let property = self.parse_assignment(depth.saturating_add(1))?;
                    let Some(close) = self.current() else {
                        return Err(ParseError::new(
                            ParseErrorKind::ExpectedRightBracket,
                            token.span().end(),
                        ));
                    };
                    if close.kind() != TokenKind::Punctuator(Punctuator::RightBracket) {
                        return Err(ParseError::new(
                            ParseErrorKind::ExpectedRightBracket,
                            close.span().start(),
                        ));
                    }
                    self.advance();
                    let span = Span::new(object.span().start(), close.span().end());
                    object = Expression::new(
                        ExpressionKind::Member {
                            object: Box::new(object),
                            property: MemberProperty::Computed(Box::new(property)),
                        },
                        span,
                    );
                }
                TokenKind::Punctuator(Punctuator::LeftParen) => {
                    let (arguments, close) = self.parse_call_arguments(depth)?;
                    let span = Span::new(object.span().start(), close.span().end());
                    object = Expression::new(
                        ExpressionKind::Call {
                            callee: Box::new(object),
                            arguments,
                        },
                        span,
                    );
                }
                _ => return Ok(object),
            }
        }

        Ok(object)
    }

    fn parse_primary_expression(&mut self, depth: usize) -> Result<Expression, ParseError> {
        let Some(token) = self.current() else {
            return Err(ParseError::new(ParseErrorKind::EmptyInput, 0));
        };

        self.ensure_depth(depth, token.span().start())?;

        match token.kind() {
            TokenKind::Punctuator(Punctuator::LeftParen) => {
                self.advance();
                let inner = self.parse_assignment(depth.saturating_add(1))?;
                self.finish_group(token, inner)
            }
            TokenKind::Punctuator(Punctuator::LeftBracket) => self.parse_array_literal(depth),
            TokenKind::Punctuator(Punctuator::LeftBrace) => self.parse_object_literal(depth),
            TokenKind::Keyword(Keyword::Function) => self.parse_function_expression(depth),
            TokenKind::Keyword(Keyword::This) => {
                self.advance();
                Ok(Expression::new(ExpressionKind::This, token.span()))
            }
            _ => self.parse_atomic_primary(),
        }
    }

    fn parse_call_arguments(
        &mut self,
        depth: usize,
    ) -> Result<(Vec<Expression>, Token), ParseError> {
        let Some(open) = self.current() else {
            return Err(ParseError::new(ParseErrorKind::ExpectedLeftParen, 0));
        };
        self.ensure_depth(depth, open.span().start())?;
        self.advance();

        let mut arguments = Vec::new();
        while let Some(token) = self.current() {
            if token.kind() == TokenKind::Punctuator(Punctuator::RightParen) {
                self.advance();
                return Ok((arguments, token));
            }

            if token.kind() == TokenKind::Punctuator(Punctuator::Comma) {
                return Err(ParseError::new(
                    ParseErrorKind::UnexpectedToken(token.kind()),
                    token.span().start(),
                ));
            }

            arguments.push(self.parse_assignment(depth.saturating_add(1))?);
            let Some(separator) = self.current() else {
                return Err(ParseError::new(
                    ParseErrorKind::ExpectedRightParen,
                    open.span().end(),
                ));
            };

            match separator.kind() {
                TokenKind::Punctuator(Punctuator::Comma) => {
                    self.advance();
                }
                TokenKind::Punctuator(Punctuator::RightParen) => {}
                _ => {
                    return Err(ParseError::new(
                        ParseErrorKind::ExpectedRightParen,
                        separator.span().start(),
                    ));
                }
            }
        }

        Err(ParseError::new(
            ParseErrorKind::ExpectedRightParen,
            open.span().end(),
        ))
    }

    fn parse_function_expression(&mut self, depth: usize) -> Result<Expression, ParseError> {
        let Some(function_token) = self.current() else {
            return Err(ParseError::new(ParseErrorKind::EmptyInput, 0));
        };
        self.ensure_depth(depth, function_token.span().start())?;
        self.advance();

        let name = match self.current() {
            Some(token) if token.kind() == TokenKind::Identifier => {
                self.advance();
                Some(token.span())
            }
            _ => None,
        };

        let Some(open_paren) = self.current() else {
            return Err(ParseError::new(
                ParseErrorKind::ExpectedLeftParen,
                function_token.span().end(),
            ));
        };
        if open_paren.kind() != TokenKind::Punctuator(Punctuator::LeftParen) {
            return Err(ParseError::new(
                ParseErrorKind::ExpectedLeftParen,
                open_paren.span().start(),
            ));
        }
        self.advance();

        let mut parameters = Vec::new();
        loop {
            let Some(token) = self.current() else {
                return Err(ParseError::new(
                    ParseErrorKind::ExpectedRightParen,
                    open_paren.span().end(),
                ));
            };

            if token.kind() == TokenKind::Punctuator(Punctuator::RightParen) {
                self.advance();
                break;
            }

            if token.kind() != TokenKind::Identifier {
                return Err(ParseError::new(
                    ParseErrorKind::ExpectedIdentifier,
                    token.span().start(),
                ));
            }
            parameters.push(token.span());
            self.advance();

            let Some(separator) = self.current() else {
                return Err(ParseError::new(
                    ParseErrorKind::ExpectedRightParen,
                    token.span().end(),
                ));
            };
            match separator.kind() {
                TokenKind::Punctuator(Punctuator::Comma) => self.advance(),
                TokenKind::Punctuator(Punctuator::RightParen) => {}
                _ => {
                    return Err(ParseError::new(
                        ParseErrorKind::ExpectedRightParen,
                        separator.span().start(),
                    ));
                }
            }
        }

        let Some(open_brace) = self.current() else {
            return Err(ParseError::new(
                ParseErrorKind::ExpectedLeftBrace,
                open_paren.span().end(),
            ));
        };
        if open_brace.kind() != TokenKind::Punctuator(Punctuator::LeftBrace) {
            return Err(ParseError::new(
                ParseErrorKind::ExpectedLeftBrace,
                open_brace.span().start(),
            ));
        }
        self.advance();

        let mut body = Vec::new();
        let close_brace = loop {
            let Some(token) = self.current() else {
                return Err(ParseError::new(
                    ParseErrorKind::ExpectedRightBrace,
                    open_brace.span().end(),
                ));
            };

            match token.kind() {
                TokenKind::Punctuator(Punctuator::RightBrace) => {
                    self.advance();
                    break token;
                }
                TokenKind::Punctuator(Punctuator::Semicolon) => self.advance(),
                TokenKind::Eof => {
                    return Err(ParseError::new(
                        ParseErrorKind::ExpectedRightBrace,
                        token.span().start(),
                    ));
                }
                _ => body.push(self.parse_statement(depth.saturating_add(1), true)?),
            }
        };

        let span = Span::new(function_token.span().start(), close_brace.span().end());
        Ok(Expression::new(
            ExpressionKind::Function(FunctionExpression::new(name, parameters, body, span)),
            span,
        ))
    }

    fn parse_return_statement(&mut self, depth: usize) -> Result<Statement, ParseError> {
        let Some(keyword) = self.current() else {
            return Err(ParseError::new(ParseErrorKind::EmptyInput, 0));
        };
        let start = keyword.span().start();
        self.advance();

        if self
            .current()
            .is_some_and(|token| token.kind() == TokenKind::Punctuator(Punctuator::Semicolon))
        {
            let end = self
                .current()
                .map_or(keyword.span().end(), |token| token.span().end());
            self.advance();
            return Ok(Statement::new(
                StatementKind::Return(None),
                Span::new(start, end),
            ));
        }

        let expression = self.parse_assignment(depth)?;
        let expression_end = expression.span().end();
        let end = self.consume_statement_terminator(expression_end)?;
        Ok(Statement::new(
            StatementKind::Return(Some(expression)),
            Span::new(start, end),
        ))
    }

    fn parse_array_literal(&mut self, depth: usize) -> Result<Expression, ParseError> {
        let Some(open) = self.current() else {
            return Err(ParseError::new(ParseErrorKind::EmptyInput, 0));
        };
        self.ensure_depth(depth, open.span().start())?;
        self.advance();

        let mut elements = Vec::new();
        while let Some(token) = self.current() {
            if token.kind() == TokenKind::Punctuator(Punctuator::RightBracket) {
                self.advance();
                return Ok(Expression::new(
                    ExpressionKind::ArrayLiteral(elements),
                    Span::new(open.span().start(), token.span().end()),
                ));
            }

            if token.kind() == TokenKind::Punctuator(Punctuator::Comma) {
                return Err(ParseError::new(
                    ParseErrorKind::UnexpectedToken(token.kind()),
                    token.span().start(),
                ));
            }

            elements.push(self.parse_assignment(depth.saturating_add(1))?);
            let Some(separator) = self.current() else {
                return Err(ParseError::new(
                    ParseErrorKind::ExpectedRightBracket,
                    open.span().end(),
                ));
            };

            match separator.kind() {
                TokenKind::Punctuator(Punctuator::Comma) => {
                    self.advance();
                }
                TokenKind::Punctuator(Punctuator::RightBracket) => {}
                _ => {
                    return Err(ParseError::new(
                        ParseErrorKind::ExpectedRightBracket,
                        separator.span().start(),
                    ));
                }
            }
        }

        Err(ParseError::new(
            ParseErrorKind::ExpectedRightBracket,
            open.span().end(),
        ))
    }

    fn parse_object_literal(&mut self, depth: usize) -> Result<Expression, ParseError> {
        let Some(open) = self.current() else {
            return Err(ParseError::new(ParseErrorKind::EmptyInput, 0));
        };
        self.ensure_depth(depth, open.span().start())?;
        self.advance();

        let mut properties = Vec::new();
        while let Some(token) = self.current() {
            if token.kind() == TokenKind::Punctuator(Punctuator::RightBrace) {
                self.advance();
                return Ok(Expression::new(
                    ExpressionKind::ObjectLiteral(properties),
                    Span::new(open.span().start(), token.span().end()),
                ));
            }

            if !matches!(
                token.kind(),
                TokenKind::Identifier | TokenKind::String | TokenKind::Number
            ) {
                return Err(ParseError::new(
                    ParseErrorKind::ExpectedPropertyName,
                    token.span().start(),
                ));
            }
            self.advance();
            let key = token.span();

            let Some(colon) = self.current() else {
                return Err(ParseError::new(ParseErrorKind::ExpectedColon, key.end()));
            };
            if colon.kind() != TokenKind::Punctuator(Punctuator::Colon) {
                return Err(ParseError::new(
                    ParseErrorKind::ExpectedColon,
                    colon.span().start(),
                ));
            }
            self.advance();

            let value = self.parse_assignment(depth.saturating_add(1))?;
            let property_span = Span::new(key.start(), value.span().end());
            properties.push(ObjectProperty::new(key, value, property_span));

            let Some(separator) = self.current() else {
                return Err(ParseError::new(
                    ParseErrorKind::ExpectedRightBrace,
                    open.span().end(),
                ));
            };

            match separator.kind() {
                TokenKind::Punctuator(Punctuator::Comma) => {
                    self.advance();
                }
                TokenKind::Punctuator(Punctuator::RightBrace) => {}
                _ => {
                    return Err(ParseError::new(
                        ParseErrorKind::ExpectedRightBrace,
                        separator.span().start(),
                    ));
                }
            }
        }

        Err(ParseError::new(
            ParseErrorKind::ExpectedRightBrace,
            open.span().end(),
        ))
    }

    fn parse_primary_compat(&mut self, depth: usize) -> Result<Expression, ParseError> {
        let Some(token) = self.current() else {
            return Err(ParseError::new(ParseErrorKind::EmptyInput, 0));
        };

        self.ensure_depth(depth, token.span().start())?;

        match token.kind() {
            TokenKind::Punctuator(Punctuator::LeftParen) => {
                self.advance();
                let inner = self.parse_primary_compat(depth.saturating_add(1))?;
                self.finish_group(token, inner)
            }
            _ => self.parse_atomic_primary(),
        }
    }

    fn parse_atomic_primary(&mut self) -> Result<Expression, ParseError> {
        let Some(token) = self.current() else {
            return Err(ParseError::new(ParseErrorKind::EmptyInput, 0));
        };

        match token.kind() {
            TokenKind::Identifier => {
                self.advance();
                Ok(Expression::new(ExpressionKind::Identifier, token.span()))
            }
            TokenKind::Number => {
                self.advance();
                Ok(Expression::new(ExpressionKind::NumberLiteral, token.span()))
            }
            TokenKind::String => {
                self.advance();
                Ok(Expression::new(ExpressionKind::StringLiteral, token.span()))
            }
            TokenKind::Keyword(Keyword::True) => {
                self.advance();
                Ok(Expression::new(ExpressionKind::Boolean(true), token.span()))
            }
            TokenKind::Keyword(Keyword::False) => {
                self.advance();
                Ok(Expression::new(
                    ExpressionKind::Boolean(false),
                    token.span(),
                ))
            }
            TokenKind::Keyword(Keyword::Null) => {
                self.advance();
                Ok(Expression::new(ExpressionKind::Null, token.span()))
            }
            TokenKind::Eof => Err(ParseError::new(
                ParseErrorKind::EmptyInput,
                token.span().start(),
            )),
            unexpected => Err(ParseError::new(
                ParseErrorKind::UnexpectedToken(unexpected),
                token.span().start(),
            )),
        }
    }

    fn finish_group(&mut self, open: Token, inner: Expression) -> Result<Expression, ParseError> {
        let Some(close) = self.current() else {
            return Err(ParseError::new(
                ParseErrorKind::ExpectedRightParen,
                open.span().end(),
            ));
        };

        if close.kind() != TokenKind::Punctuator(Punctuator::RightParen) {
            return Err(ParseError::new(
                ParseErrorKind::ExpectedRightParen,
                close.span().start(),
            ));
        }

        self.advance();

        Ok(Expression::new(
            ExpressionKind::Grouped(Box::new(inner)),
            Span::new(open.span().start(), close.span().end()),
        ))
    }
}
