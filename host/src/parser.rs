//! Recursive-descent parser. Stops at the first syntax error.

use crate::ast::{Arg, BinOp, Expr, ExprKind, FnDecl, Param, Stmt, TypeName};
use crate::diag::{Diagnostic, Span};
use crate::lexer::{MAX_LITERAL_MAGNITUDE, Tok, Token};

/// Deepest nesting of blocks and expressions; keeps the recursive passes off
/// the end of the host stack.
const MAX_NESTING: u32 = 200;

type PResult<T> = Result<T, Diagnostic>;

pub fn parse(tokens: Vec<Token>) -> PResult<Vec<FnDecl>> {
    let mut parser = Parser {
        tokens,
        pos: 0,
        depth: 0,
    };
    parser.file()
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    depth: u32,
}

impl Parser {
    fn file(&mut self) -> PResult<Vec<FnDecl>> {
        let mut fns = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek() {
                Tok::Eof => return Ok(fns),
                Tok::Fn => fns.push(self.fn_decl()?),
                Tok::Let => {
                    return Err(self.error(
                        "module-level `let` is not supported in this alpha; only functions may appear at the top level",
                    ));
                }
                _ => return Err(self.unexpected("`fn`")),
            }
            if !matches!(self.peek(), Tok::Newline | Tok::Eof) {
                return Err(self.unexpected("a newline after the function"));
            }
        }
    }

    fn fn_decl(&mut self) -> PResult<FnDecl> {
        self.bump();
        let (name, span) = self.ident("a function name")?;
        self.expect(Tok::LParen, "`(`")?;
        let mut params = Vec::new();
        while *self.peek() != Tok::RParen {
            let (name, span) = self.ident("a parameter name or `)`")?;
            self.expect(Tok::Colon, "`:` and a parameter type")?;
            let ty = self.type_name()?;
            if *self.peek() == Tok::Assign {
                return Err(self.error("default parameter values are not supported in this alpha"));
            }
            params.push(Param { name, span, ty });
            if !self.eat(&Tok::Comma) && *self.peek() != Tok::RParen {
                return Err(self.unexpected("`,` or `)`"));
            }
        }
        self.bump();
        let ret = if self.eat(&Tok::Arrow) {
            Some(self.type_name()?)
        } else {
            None
        };
        let body = self.block()?;
        Ok(FnDecl {
            name,
            span,
            params,
            ret,
            body,
        })
    }

    fn type_name(&mut self) -> PResult<TypeName> {
        let (name, span) = self.ident("a type")?;
        Ok(TypeName { name, span })
    }

    fn block(&mut self) -> PResult<Vec<Stmt>> {
        self.expect(Tok::LBrace, "`{`")?;
        self.enter()?;
        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            if self.eat(&Tok::RBrace) {
                break;
            }
            stmts.push(self.stmt()?);
            match self.peek() {
                Tok::Newline => self.bump(),
                Tok::RBrace => {}
                _ => return Err(self.unexpected("a newline or `}` after the statement")),
            }
        }
        self.depth -= 1;
        Ok(stmts)
    }

    fn stmt(&mut self) -> PResult<Stmt> {
        match self.peek() {
            Tok::Let => {
                self.bump();
                let (name, span) = self.ident("a variable name")?;
                let ty = if self.eat(&Tok::Colon) {
                    Some(self.type_name()?)
                } else {
                    None
                };
                self.expect(Tok::Assign, "`=`")?;
                let value = self.expr()?;
                Ok(Stmt::Let {
                    name,
                    span,
                    ty,
                    value,
                })
            }
            Tok::If => self.if_stmt(),
            Tok::While => {
                self.bump();
                let cond = self.expr()?;
                let body = self.block()?;
                Ok(Stmt::While { cond, body })
            }
            Tok::Return => {
                let span = self.span();
                self.bump();
                let value = match self.peek() {
                    Tok::Newline | Tok::RBrace | Tok::Eof => None,
                    _ => Some(self.expr()?),
                };
                Ok(Stmt::Return { span, value })
            }
            Tok::Else => {
                Err(self
                    .error("`else` must be on the same line as the `}` that closes the `if` block"))
            }
            Tok::Ident(name) if self.peek_at(1) == &Tok::Assign => {
                let name = name.clone();
                let span = self.span();
                self.bump();
                self.bump();
                let value = self.expr()?;
                Ok(Stmt::Assign { name, span, value })
            }
            _ => Ok(Stmt::Expr(self.expr()?)),
        }
    }

    fn if_stmt(&mut self) -> PResult<Stmt> {
        self.bump();
        let cond = self.expr()?;
        let then_body = self.block()?;
        let else_body = if !self.eat(&Tok::Else) {
            Vec::new()
        } else if *self.peek() == Tok::If {
            // Each `else if` nests one level deeper in the tree.
            self.enter()?;
            let nested = self.if_stmt()?;
            self.depth -= 1;
            vec![nested]
        } else {
            self.block()?
        };
        Ok(Stmt::If {
            cond,
            then_body,
            else_body,
        })
    }

    fn expr(&mut self) -> PResult<Expr> {
        self.enter()?;
        let expr = self.or();
        self.depth -= 1;
        expr
    }

    fn or(&mut self) -> PResult<Expr> {
        let start = self.depth;
        let mut lhs = self.and()?;
        while *self.peek() == Tok::Or {
            lhs = self.binary(lhs, BinOp::Or, Self::and)?;
        }
        self.depth = start;
        Ok(lhs)
    }

    fn and(&mut self) -> PResult<Expr> {
        let start = self.depth;
        let mut lhs = self.not()?;
        while *self.peek() == Tok::And {
            lhs = self.binary(lhs, BinOp::And, Self::not)?;
        }
        self.depth = start;
        Ok(lhs)
    }

    fn not(&mut self) -> PResult<Expr> {
        if *self.peek() != Tok::Not {
            return self.compare();
        }
        let span = self.span();
        self.bump();
        self.enter()?;
        let operand = self.not()?;
        self.depth -= 1;
        Ok(Expr {
            kind: ExprKind::Not(Box::new(operand)),
            span,
        })
    }

    fn compare(&mut self) -> PResult<Expr> {
        let lhs = self.sum()?;
        let Some(op) = comparison(self.peek()) else {
            return Ok(lhs);
        };
        let expr = self.binary(lhs, op, Self::sum)?;
        if comparison(self.peek()).is_some() {
            return Err(self.error("comparisons cannot be chained; combine them with `and`"));
        }
        self.depth -= 1;
        Ok(expr)
    }

    fn sum(&mut self) -> PResult<Expr> {
        let start = self.depth;
        let mut lhs = self.product()?;
        loop {
            let op = match self.peek() {
                Tok::Plus => BinOp::Add,
                Tok::Minus => BinOp::Sub,
                _ => break,
            };
            lhs = self.binary(lhs, op, Self::product)?;
        }
        self.depth = start;
        Ok(lhs)
    }

    fn product(&mut self) -> PResult<Expr> {
        let start = self.depth;
        let mut lhs = self.unary()?;
        loop {
            let op = match self.peek() {
                Tok::Star => BinOp::Mul,
                Tok::Slash => BinOp::Div,
                Tok::Percent => BinOp::Rem,
                _ => break,
            };
            lhs = self.binary(lhs, op, Self::unary)?;
        }
        self.depth = start;
        Ok(lhs)
    }

    /// Consumes the operator at the cursor and parses its right operand.
    ///
    /// Each operator in a chain deepens the left-leaning tree by one, so it
    /// counts toward the nesting limit until the caller's chain ends and
    /// restores the depth. Later passes recurse over that tree.
    fn binary(
        &mut self,
        lhs: Expr,
        op: BinOp,
        rhs: fn(&mut Self) -> PResult<Expr>,
    ) -> PResult<Expr> {
        let span = self.span();
        self.enter()?;
        self.bump();
        let rhs = rhs(self)?;
        Ok(Expr {
            kind: ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)),
            span,
        })
    }

    fn unary(&mut self) -> PResult<Expr> {
        if *self.peek() != Tok::Minus {
            return self.primary();
        }
        let span = self.span();
        self.bump();
        // A minus applied to a literal is part of the literal, so the most
        // negative Int is writable.
        if let Tok::Int(magnitude) = *self.peek() {
            self.bump();
            let value =
                i32::try_from(-i64::from(magnitude)).expect("lexer caps literal magnitude at 2^31");
            return Ok(Expr {
                kind: ExprKind::Int(value),
                span,
            });
        }
        self.enter()?;
        let operand = self.unary()?;
        self.depth -= 1;
        Ok(Expr {
            kind: ExprKind::Neg(Box::new(operand)),
            span,
        })
    }

    fn primary(&mut self) -> PResult<Expr> {
        let span = self.span();
        let kind = match self.peek().clone() {
            Tok::Int(magnitude) => {
                if magnitude == MAX_LITERAL_MAGNITUDE {
                    return Err(self.error(format!(
                        "integer literal `{magnitude}` is out of range for Int"
                    )));
                }
                ExprKind::Int(magnitude.try_into().expect("checked against 2^31"))
            }
            Tok::True => ExprKind::Bool(true),
            Tok::False => ExprKind::Bool(false),
            Tok::Str(text) => ExprKind::Str(text),
            Tok::Ident(name) if self.peek_at(1) == &Tok::LParen => return self.call(name),
            Tok::Ident(name) => ExprKind::Var(name),
            Tok::LParen => {
                self.bump();
                let inner = self.expr()?;
                self.expect(Tok::RParen, "`)`")?;
                return Ok(inner);
            }
            _ => return Err(self.unexpected("an expression")),
        };
        self.bump();
        Ok(Expr { kind, span })
    }

    fn call(&mut self, callee: String) -> PResult<Expr> {
        let span = self.span();
        self.bump();
        self.bump();
        let mut args: Vec<Arg> = Vec::new();
        while *self.peek() != Tok::RParen {
            let name = match self.peek() {
                Tok::Ident(name) if self.peek_at(1) == &Tok::Colon => {
                    let name = (name.clone(), self.span());
                    self.bump();
                    self.bump();
                    Some(name)
                }
                _ => None,
            };
            if name.is_none() && args.iter().any(|a| a.name.is_some()) {
                return Err(self.error("positional arguments must come before named arguments"));
            }
            let value = self.expr()?;
            args.push(Arg { name, value });
            if !self.eat(&Tok::Comma) && *self.peek() != Tok::RParen {
                return Err(self.unexpected("`,` or `)`"));
            }
        }
        self.bump();
        Ok(Expr {
            kind: ExprKind::Call { callee, args },
            span,
        })
    }

    fn ident(&mut self, expected: &str) -> PResult<(String, Span)> {
        let Tok::Ident(name) = self.peek().clone() else {
            return Err(self.unexpected(expected));
        };
        let span = self.span();
        self.bump();
        Ok((name, span))
    }

    fn expect(&mut self, tok: Tok, expected: &str) -> PResult<()> {
        if self.eat(&tok) {
            Ok(())
        } else {
            Err(self.unexpected(expected))
        }
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        let found = self.peek() == tok;
        if found {
            self.bump();
        }
        found
    }

    fn enter(&mut self) -> PResult<()> {
        self.depth += 1;
        if self.depth > MAX_NESTING {
            return Err(self.error(format!(
                "nesting is too deep; the limit is {MAX_NESTING} levels"
            )));
        }
        Ok(())
    }

    fn skip_newlines(&mut self) {
        while *self.peek() == Tok::Newline {
            self.bump();
        }
    }

    fn peek(&self) -> &Tok {
        self.peek_at(0)
    }

    /// The token `ahead` positions on, clamped to the final `Eof` or `Invalid`.
    fn peek_at(&self, ahead: usize) -> &Tok {
        let last = self.tokens.len() - 1;
        &self.tokens[(self.pos + ahead).min(last)].tok
    }

    fn span(&self) -> Span {
        self.tokens[self.pos].span
    }

    fn bump(&mut self) {
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
    }

    fn error(&self, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(self.span(), message)
    }

    /// Reports the token at the cursor. An `Invalid` token reports the lexer's
    /// message, since no rule can accept it.
    fn unexpected(&self, expected: &str) -> Diagnostic {
        let found = match self.peek() {
            Tok::Invalid(message) => return self.error(message.clone()),
            Tok::Ident(name) => format!("`{name}`"),
            Tok::Int(n) => format!("`{n}`"),
            Tok::Str(_) => "a string literal".into(),
            Tok::Newline => "the end of the line".into(),
            Tok::Eof => "the end of the file".into(),
            tok => format!("`{}`", keyword_or_symbol(tok)),
        };
        self.error(format!("expected {expected}, found {found}"))
    }
}

fn comparison(tok: &Tok) -> Option<BinOp> {
    match tok {
        Tok::EqEq => Some(BinOp::Eq),
        Tok::NotEq => Some(BinOp::Ne),
        Tok::Lt => Some(BinOp::Lt),
        Tok::Le => Some(BinOp::Le),
        Tok::Gt => Some(BinOp::Gt),
        Tok::Ge => Some(BinOp::Ge),
        _ => None,
    }
}

fn keyword_or_symbol(tok: &Tok) -> &'static str {
    match tok {
        Tok::Fn => "fn",
        Tok::Let => "let",
        Tok::If => "if",
        Tok::Else => "else",
        Tok::While => "while",
        Tok::Return => "return",
        Tok::True => "true",
        Tok::False => "false",
        Tok::And => "and",
        Tok::Or => "or",
        Tok::Not => "not",
        Tok::LParen => "(",
        Tok::RParen => ")",
        Tok::LBrace => "{",
        Tok::RBrace => "}",
        Tok::Comma => ",",
        Tok::Colon => ":",
        Tok::Arrow => "->",
        Tok::Assign => "=",
        Tok::Plus => "+",
        Tok::Minus => "-",
        Tok::Star => "*",
        Tok::Slash => "/",
        Tok::Percent => "%",
        Tok::EqEq => "==",
        Tok::NotEq => "!=",
        Tok::Lt => "<",
        Tok::Le => "<=",
        Tok::Gt => ">",
        Tok::Ge => ">=",
        Tok::Ident(_) | Tok::Int(_) | Tok::Str(_) | Tok::Newline | Tok::Eof | Tok::Invalid(_) => {
            unreachable!("described by `unexpected`")
        }
    }
}
