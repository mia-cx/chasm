//! Untyped syntax tree produced by the parser.

use crate::diag::Span;

pub struct FnDecl {
    pub name: String,
    pub span: Span,
    pub params: Vec<Param>,
    pub ret: Option<TypeName>,
    pub body: Vec<Stmt>,
}

pub struct Param {
    pub name: String,
    pub span: Span,
    pub ty: TypeName,
}

/// A type as written; the checker resolves it.
pub struct TypeName {
    pub name: String,
    pub span: Span,
}

pub enum Stmt {
    Let {
        name: String,
        span: Span,
        ty: Option<TypeName>,
        value: Expr,
    },
    Assign {
        name: String,
        span: Span,
        value: Expr,
    },
    If {
        cond: Expr,
        then_body: Vec<Stmt>,
        /// `else if` is an else body holding one `If`.
        else_body: Vec<Stmt>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    Return {
        span: Span,
        value: Option<Expr>,
    },
    Expr(Expr),
}

/// An expression. `span` points at the operator for unary and binary
/// expressions and at the callee for calls, so runtime panics name the
/// operation that failed.
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
    /// Levels of expression below this one; 0 for a leaf. The parser bounds it
    /// so the recursive passes stay off the end of the host stack.
    pub height: u32,
}

pub enum ExprKind {
    Int(i32),
    Bool(bool),
    Str(String),
    Var(String),
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Call { callee: String, args: Vec<Arg> },
}

pub struct Arg {
    pub name: Option<(String, Span)>,
    pub value: Expr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    And,
    Or,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::And => "and",
            BinOp::Or => "or",
        }
    }
}
