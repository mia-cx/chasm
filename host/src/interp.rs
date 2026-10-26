//! Reference interpreter over validated IR.

use std::io::{self, Write};

use crate::diag::Span;
use crate::ir::{
    BinOp, FuncId, Function, MAX_CALL_DEPTH, Operand, PANIC_CALL_DEPTH, PANIC_DIVIDE_BY_ZERO,
    PANIC_OVERFLOW, Program, Rvalue, Stmt, Ty,
};

/// Why a run stopped early.
pub enum Trap {
    Panic { message: &'static str, span: Span },
    Output(io::Error),
}

/// Runs `main`, writing `print` output to `out`.
pub fn run(program: &Program, out: &mut dyn Write) -> Result<(), Trap> {
    let mut interp = Interp {
        program,
        out,
        depth: 1,
    };
    interp.call(program.main, &[])?;
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Value {
    Int(i32),
    Bool(bool),
    Str(usize),
}

enum Flow {
    Next,
    Break,
    Return(Option<Value>),
}

struct Interp<'a> {
    program: &'a Program,
    out: &'a mut dyn Write,
    /// Active calls, counting `main`.
    depth: u32,
}

impl Interp<'_> {
    fn call(&mut self, func: FuncId, args: &[Value]) -> Result<Option<Value>, Trap> {
        let function = &self.program.functions[func];
        let mut locals: Vec<Value> = function
            .locals
            .iter()
            .map(|local| match local.ty {
                Ty::Int => Value::Int(0),
                Ty::Bool => Value::Bool(false),
                Ty::String => Value::Str(0),
            })
            .collect();
        locals[..args.len()].copy_from_slice(args);
        match self.block(function, &mut locals, &function.body)? {
            Flow::Return(value) => Ok(value),
            Flow::Next => Ok(None),
            Flow::Break => unreachable!("the validator rejects `break` outside a loop"),
        }
    }

    fn block(
        &mut self,
        function: &Function,
        locals: &mut [Value],
        stmts: &[Stmt],
    ) -> Result<Flow, Trap> {
        for stmt in stmts {
            match self.stmt(function, locals, stmt)? {
                Flow::Next => {}
                flow @ (Flow::Break | Flow::Return(_)) => return Ok(flow),
            }
        }
        Ok(Flow::Next)
    }

    fn stmt(
        &mut self,
        function: &Function,
        locals: &mut [Value],
        stmt: &Stmt,
    ) -> Result<Flow, Trap> {
        match stmt {
            Stmt::Assign { dst, value } => {
                locals[*dst] = self.rvalue(locals, value)?;
            }
            Stmt::Call { func, args, span } => {
                self.invoke(locals, *func, args, *span)?;
            }
            Stmt::Print(operand) => {
                let result = match read(locals, *operand) {
                    Value::Int(n) => writeln!(self.out, "{n}"),
                    Value::Bool(b) => writeln!(self.out, "{b}"),
                    Value::Str(id) => {
                        let text = &self.program.strings[id];
                        self.out
                            .write_all(text.as_bytes())
                            .and_then(|()| self.out.write_all(b"\n"))
                    }
                };
                result.map_err(Trap::Output)?;
            }
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let body = if read(locals, *cond) == Value::Bool(true) {
                    then_body
                } else {
                    else_body
                };
                return self.block(function, locals, body);
            }
            Stmt::Loop(body) => loop {
                match self.block(function, locals, body)? {
                    Flow::Next => {}
                    Flow::Break => return Ok(Flow::Next),
                    flow @ Flow::Return(_) => return Ok(flow),
                }
            },
            Stmt::Break => return Ok(Flow::Break),
            Stmt::Return(value) => {
                return Ok(Flow::Return(value.map(|operand| read(locals, operand))));
            }
        }
        Ok(Flow::Next)
    }

    fn rvalue(&mut self, locals: &[Value], rvalue: &Rvalue) -> Result<Value, Trap> {
        Ok(match rvalue {
            Rvalue::Use(operand) => read(locals, *operand),
            Rvalue::Binary { op, lhs, rhs, span } => {
                binary(*op, read(locals, *lhs), read(locals, *rhs), *span)?
            }
            Rvalue::Neg { operand, span } => {
                let n = int(read(locals, *operand));
                Value::Int(
                    n.checked_neg()
                        .ok_or_else(|| panic(PANIC_OVERFLOW, *span))?,
                )
            }
            Rvalue::Not(operand) => Value::Bool(read(locals, *operand) != Value::Bool(true)),
            Rvalue::Call { func, args, span } => self
                .invoke(locals, *func, args, *span)?
                .expect("the validator rejects using a call without a value"),
        })
    }

    fn invoke(
        &mut self,
        locals: &[Value],
        func: FuncId,
        args: &[Operand],
        span: Span,
    ) -> Result<Option<Value>, Trap> {
        let args: Vec<Value> = args.iter().map(|arg| read(locals, *arg)).collect();
        if self.depth >= MAX_CALL_DEPTH {
            return Err(panic(PANIC_CALL_DEPTH, span));
        }
        self.depth += 1;
        let result = self.call(func, &args);
        self.depth -= 1;
        result
    }
}

fn binary(op: BinOp, lhs: Value, rhs: Value, span: Span) -> Result<Value, Trap> {
    let overflow = || panic(PANIC_OVERFLOW, span);
    let divide_by_zero = || panic(PANIC_DIVIDE_BY_ZERO, span);
    Ok(match op {
        BinOp::Add => Value::Int(int(lhs).checked_add(int(rhs)).ok_or_else(overflow)?),
        BinOp::Sub => Value::Int(int(lhs).checked_sub(int(rhs)).ok_or_else(overflow)?),
        BinOp::Mul => Value::Int(int(lhs).checked_mul(int(rhs)).ok_or_else(overflow)?),
        BinOp::Div if int(rhs) == 0 => return Err(divide_by_zero()),
        // With a nonzero divisor, only `i32::MIN / -1` overflows.
        BinOp::Div => Value::Int(int(lhs).checked_div(int(rhs)).ok_or_else(overflow)?),
        BinOp::Rem if int(rhs) == 0 => return Err(divide_by_zero()),
        // `i32::MIN % -1` is mathematically 0, which fits.
        BinOp::Rem => Value::Int(int(lhs).wrapping_rem(int(rhs))),
        BinOp::Lt => Value::Bool(int(lhs) < int(rhs)),
        BinOp::Le => Value::Bool(int(lhs) <= int(rhs)),
        BinOp::Gt => Value::Bool(int(lhs) > int(rhs)),
        BinOp::Ge => Value::Bool(int(lhs) >= int(rhs)),
        BinOp::Eq => Value::Bool(lhs == rhs),
        BinOp::Ne => Value::Bool(lhs != rhs),
    })
}

fn read(locals: &[Value], operand: Operand) -> Value {
    match operand {
        Operand::Local(id) => locals[id],
        Operand::Int(n) => Value::Int(n),
        Operand::Bool(b) => Value::Bool(b),
        Operand::Str(id) => Value::Str(id),
    }
}

fn int(value: Value) -> i32 {
    match value {
        Value::Int(n) => n,
        Value::Bool(_) | Value::Str(_) => unreachable!("the validator checks operand types"),
    }
}

fn panic(message: &'static str, span: Span) -> Trap {
    Trap::Panic { message, span }
}
