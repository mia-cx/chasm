//! Typed, structured IR shared by the interpreter and the C emitter.
//!
//! Every operand is a variable or a constant, and every operation stores its
//! result in a variable. A backend executes statements in order and evaluates
//! nothing implicitly, so source evaluation order survives unchanged.

use std::rc::Rc;

use crate::diag::Span;

pub type FuncId = usize;
pub type HostId = usize;
pub type LocalId = usize;
pub type StrId = usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ty {
    Int,
    Bool,
    String,
}

impl Ty {
    pub fn name(self) -> &'static str {
        match self {
            Ty::Int => "Int",
            Ty::Bool => "Bool",
            Ty::String => "String",
        }
    }
}

pub struct Program {
    pub functions: Vec<Function>,
    /// Host functions the program was checked against, indexed by `HostId`.
    pub hosts: Vec<HostSig>,
    /// String literal contents, indexed by `StrId`. Shared so a running
    /// program can hand a literal around without copying it.
    pub strings: Vec<Rc<str>>,
    /// `fn main()`, when compiled as a program rather than a library.
    pub main: Option<FuncId>,
}

/// The signature of a function the embedding application provides.
#[derive(Clone)]
pub struct HostSig {
    pub name: String,
    pub params: Vec<Ty>,
    /// `None` for functions that return no value.
    pub ret: Option<Ty>,
}

pub struct Function {
    pub name: String,
    /// Parameters are locals `0..param_count`.
    pub param_count: usize,
    pub locals: Vec<Local>,
    /// `None` for functions that return no value.
    pub ret: Option<Ty>,
    pub body: Vec<Stmt>,
}

pub struct Local {
    /// Source name, or a description for compiler temporaries.
    pub name: String,
    pub ty: Ty,
}

/// The function a call runs: one defined in the program, or one the host
/// registered.
#[derive(Clone, Copy, Debug)]
pub enum Callee {
    Fn(FuncId),
    Host(HostId),
}

#[derive(Clone, Copy, Debug)]
pub enum Operand {
    Local(LocalId),
    Int(i32),
    Bool(bool),
    Str(StrId),
}

pub enum Rvalue {
    Use(Operand),
    Binary {
        op: BinOp,
        lhs: Operand,
        rhs: Operand,
        span: Span,
    },
    Neg {
        operand: Operand,
        span: Span,
    },
    Not(Operand),
    Call {
        callee: Callee,
        args: Vec<Operand>,
        span: Span,
    },
}

pub enum Stmt {
    Assign {
        dst: LocalId,
        value: Rvalue,
    },
    /// A call whose result, if any, is discarded.
    Call {
        callee: Callee,
        args: Vec<Operand>,
        span: Span,
    },
    Print(Operand),
    If {
        cond: Operand,
        then_body: Vec<Stmt>,
        else_body: Vec<Stmt>,
    },
    /// Repeats until a `Break` or `Return`.
    Loop(Vec<Stmt>),
    /// Leaves the innermost `Loop`.
    Break,
    Return(Option<Operand>),
}

/// Operations on `Int` (arithmetic, ordering) and on `Int` or `Bool`
/// (equality). `and` and `or` lower to `If`.
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
}

impl Function {
    pub fn operand_ty(&self, operand: Operand) -> Ty {
        match operand {
            Operand::Local(id) => self.locals[id].ty,
            Operand::Int(_) => Ty::Int,
            Operand::Bool(_) => Ty::Bool,
            Operand::Str(_) => Ty::String,
        }
    }
}

/// The deepest stack of simultaneously active calls, counting `main`. Both
/// lanes panic with `call depth limit exceeded` beyond it.
pub const MAX_CALL_DEPTH: u32 = 10_000;

/// Runtime panic messages shared by every lane.
pub const PANIC_OVERFLOW: &str = "integer overflow";
pub const PANIC_DIVIDE_BY_ZERO: &str = "division by zero";
pub const PANIC_CALL_DEPTH: &str = "call depth limit exceeded";
