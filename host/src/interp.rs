//! Reference interpreter over validated IR.
//!
//! Execution keeps calls, locals, and block cursors in heap vectors, so Chasm
//! call and block nesting never consumes the host thread's stack. Each call
//! into the interpreter starts with fresh stacks, fuel, and call depth.

use std::io::{self, Write};
use std::rc::Rc;

use crate::diag::Span;
use crate::ir::{
    BinOp, Callee, FuncId, HostId, LocalId, MAX_CALL_DEPTH, Operand, PANIC_CALL_DEPTH,
    PANIC_DIVIDE_BY_ZERO, PANIC_OVERFLOW, Program, Rvalue, Stmt, Ty,
};

/// Why a run stopped early.
pub enum Trap {
    Panic {
        message: &'static str,
        span: Span,
    },
    /// The run used up its fuel before finishing.
    OutOfFuel,
    /// A host function returned an error from the call at `span`.
    Host {
        host: HostId,
        message: String,
        span: Span,
    },
    Output(io::Error),
}

/// A runtime value. Strings are shared rather than indexed, so a string
/// outlives the program whose literal or host call produced it.
#[derive(Clone, PartialEq, Eq)]
pub enum Value {
    Int(i32),
    Bool(bool),
    Str(Rc<str>),
}

/// Runs a host function. Arguments match its signature; the result must too.
pub type HostCall<'h> = dyn FnMut(HostId, Vec<Value>) -> Result<Option<Value>, String> + 'h;

/// Everything a run reaches outside the program.
pub struct Env<'e> {
    /// Receives `print` output.
    pub out: &'e mut dyn Write,
    pub host: &'e mut HostCall<'e>,
    /// Statements and loop iterations the run may execute; `None` is
    /// unlimited.
    pub fuel: Option<u64>,
}

/// Runs `main` with unlimited fuel and no host functions, writing `print`
/// output to `out`.
pub fn run(program: &Program, out: &mut dyn Write) -> Result<(), Trap> {
    let main = program.main.expect("the CLI compiles programs with `main`");
    let mut no_hosts = |_, _| unreachable!("the CLI registers no host functions");
    let env = Env {
        out,
        host: &mut no_hosts,
        fuel: None,
    };
    call(program, main, Vec::new(), env).map(|_| ())
}

/// Calls `func` with arguments that match its parameters and returns its
/// result.
pub fn call(
    program: &Program,
    func: FuncId,
    args: Vec<Value>,
    env: Env,
) -> Result<Option<Value>, Trap> {
    let mut interp = Interp {
        program,
        env,
        locals: Vec::new(),
        calls: Vec::new(),
        control: Vec::new(),
    };
    let base = interp.enter(func, None);
    for (slot, arg) in interp.locals[base..].iter_mut().zip(args) {
        *slot = arg;
    }
    interp.execute()
}

struct Call {
    locals_base: usize,
    control_base: usize,
    /// Absolute slot in the caller, or `None` when the result is discarded.
    dst: Option<usize>,
}

struct Block<'a> {
    stmts: &'a [Stmt],
    next: usize,
    repeat: bool,
}

struct Interp<'a, 'e> {
    program: &'a Program,
    env: Env<'e>,
    /// Contiguous local slots for active calls, reclaimed on return.
    locals: Vec<Value>,
    /// Active calls, counting the entry call; no tail-call elision at the
    /// depth limit.
    calls: Vec<Call>,
    control: Vec<Block<'a>>,
}

impl<'a> Interp<'a, '_> {
    /// Pushes a call to `func` with zeroed locals and returns where they
    /// start.
    fn enter(&mut self, func: FuncId, dst: Option<usize>) -> usize {
        let function = &self.program.functions[func];
        let locals_base = self.locals.len();
        self.locals
            .extend(function.locals.iter().map(|local| match local.ty {
                Ty::Int => Value::Int(0),
                Ty::Bool => Value::Bool(false),
                Ty::String => Value::Str(Rc::from("")),
            }));
        self.calls.push(Call {
            locals_base,
            control_base: self.control.len(),
            dst,
        });
        self.control.push(Block {
            stmts: &function.body,
            next: 0,
            repeat: false,
        });
        locals_base
    }

    /// Ends the active call. Returns the value when nothing in the program
    /// receives it: the entry call's result, or a discarded one.
    fn leave(&mut self, value: Option<Value>) -> Option<Value> {
        let call = self.calls.pop().expect("an active call");
        // Return skips all pending branches and loops in this call, while its
        // caller's cursor already points to the statement after the call.
        self.control.truncate(call.control_base);
        self.locals.truncate(call.locals_base);
        let Some(dst) = call.dst else {
            return value;
        };
        self.locals[dst] = value.expect("the validator requires a return value");
        None
    }

    fn execute(&mut self) -> Result<Option<Value>, Trap> {
        loop {
            let call = self.calls.last().expect("an active call");
            let base = call.locals_base;
            let control_base = call.control_base;
            let block = self.control.last_mut().expect("an active block");
            let Some(stmt) = block.stmts.get(block.next) else {
                if block.repeat {
                    // Reuse the loop cursor: iterations do not grow the stack.
                    // Charging the iteration bounds even a loop with no body.
                    spend(&mut self.env.fuel)?;
                    block.next = 0;
                } else {
                    self.control.pop();
                    if self.control.len() == control_base {
                        let value = self.leave(None);
                        if self.calls.is_empty() {
                            return Ok(value);
                        }
                    }
                }
                continue;
            };
            block.next += 1;
            spend(&mut self.env.fuel)?;
            let locals = Slots {
                locals: &self.locals[base..],
                strings: &self.program.strings,
            };
            match stmt {
                Stmt::Assign {
                    dst,
                    value: Rvalue::Call { callee, args, span },
                } => self.invoke(*callee, args, *span, Some(*dst))?,
                Stmt::Assign { dst, value } => {
                    self.locals[base + *dst] = rvalue(locals, value)?;
                }
                Stmt::Call { callee, args, span } => self.invoke(*callee, args, *span, None)?,
                Stmt::Print(operand) => {
                    let result = match read(locals, *operand) {
                        Value::Int(n) => writeln!(self.env.out, "{n}"),
                        Value::Bool(b) => writeln!(self.env.out, "{b}"),
                        Value::Str(text) => self
                            .env
                            .out
                            .write_all(text.as_bytes())
                            .and_then(|()| self.env.out.write_all(b"\n")),
                    };
                    result.map_err(Trap::Output)?;
                }
                Stmt::If {
                    cond,
                    then_body,
                    else_body,
                } => {
                    let stmts = if read(locals, *cond) == Value::Bool(true) {
                        then_body
                    } else {
                        else_body
                    };
                    self.control.push(Block {
                        stmts,
                        next: 0,
                        repeat: false,
                    });
                }
                Stmt::Loop(stmts) => {
                    self.control.push(Block {
                        stmts,
                        next: 0,
                        repeat: true,
                    });
                }
                Stmt::Break => {
                    let loop_index = self.control[control_base..]
                        .iter()
                        .rposition(|block| block.repeat)
                        .expect("the validator rejects `break` outside a loop");
                    self.control.truncate(control_base + loop_index);
                }
                Stmt::Return(value) => {
                    let value = value.map(|operand| read(locals, operand));
                    let value = self.leave(value);
                    if self.calls.is_empty() {
                        return Ok(value);
                    }
                }
            }
        }
    }

    fn invoke(
        &mut self,
        callee: Callee,
        args: &[Operand],
        span: Span,
        dst: Option<LocalId>,
    ) -> Result<(), Trap> {
        if self.calls.len() >= MAX_CALL_DEPTH as usize {
            return Err(panic(PANIC_CALL_DEPTH, span));
        }
        let caller_base = self.calls.last().expect("an active caller").locals_base;
        let dst = dst.map(|dst| caller_base + dst);
        match callee {
            Callee::Fn(func) => {
                let callee_base = self.enter(func, dst);
                // Arguments are already lowered operands. Read caller slots
                // even when growing the shared vector relocated it.
                for (index, arg) in args.iter().enumerate() {
                    let caller = Slots {
                        locals: &self.locals[caller_base..callee_base],
                        strings: &self.program.strings,
                    };
                    self.locals[callee_base + index] = read(caller, *arg);
                }
            }
            Callee::Host(host) => {
                let caller = Slots {
                    locals: &self.locals[caller_base..],
                    strings: &self.program.strings,
                };
                let args = args.iter().map(|arg| read(caller, *arg)).collect();
                let result = (self.env.host)(host, args).map_err(|message| Trap::Host {
                    host,
                    message,
                    span,
                })?;
                if let Some(dst) = dst {
                    self.locals[dst] = result.expect("the host returns its declared type");
                }
            }
        }
        Ok(())
    }
}

/// Charges one unit of fuel, failing when none is left.
fn spend(fuel: &mut Option<u64>) -> Result<(), Trap> {
    match fuel {
        None => Ok(()),
        Some(0) => Err(Trap::OutOfFuel),
        Some(left) => {
            *left -= 1;
            Ok(())
        }
    }
}

fn rvalue(locals: Slots, rvalue: &Rvalue) -> Result<Value, Trap> {
    Ok(match rvalue {
        Rvalue::Use(operand) => read(locals, *operand),
        Rvalue::Binary { op, lhs, rhs, span } => {
            binary(*op, &read(locals, *lhs), &read(locals, *rhs), *span)?
        }
        Rvalue::Neg { operand, span } => {
            let n = int(&read(locals, *operand));
            Value::Int(
                n.checked_neg()
                    .ok_or_else(|| panic(PANIC_OVERFLOW, *span))?,
            )
        }
        Rvalue::Not(operand) => Value::Bool(read(locals, *operand) != Value::Bool(true)),
        Rvalue::Call { .. } => unreachable!("calls suspend in the execution loop"),
    })
}

fn binary(op: BinOp, lhs: &Value, rhs: &Value, span: Span) -> Result<Value, Trap> {
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

/// What an operand can name: the active call's locals and the program's
/// string literals.
#[derive(Clone, Copy)]
struct Slots<'s> {
    locals: &'s [Value],
    strings: &'s [Rc<str>],
}

fn read(slots: Slots, operand: Operand) -> Value {
    match operand {
        Operand::Local(id) => slots.locals[id].clone(),
        Operand::Int(n) => Value::Int(n),
        Operand::Bool(b) => Value::Bool(b),
        Operand::Str(id) => Value::Str(Rc::clone(&slots.strings[id])),
    }
}

fn int(value: &Value) -> i32 {
    match value {
        Value::Int(n) => *n,
        Value::Bool(_) | Value::Str(_) => unreachable!("the validator checks operand types"),
    }
}

fn panic(message: &'static str, span: Span) -> Trap {
    Trap::Panic { message, span }
}
