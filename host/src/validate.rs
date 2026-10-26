//! Independent checks on the IR every backend consumes.
//!
//! A rejection is a compiler bug. The checks cover types, indices, `break`
//! placement, and returns on every path. They catch some lowering mistakes;
//! they do not show that lowering preserved the source program's meaning.

use crate::ir::{BinOp, Function, Operand, Program, Rvalue, Stmt, Ty};

pub fn validate(program: &Program) -> Result<(), String> {
    let main = program
        .functions
        .get(program.main)
        .ok_or("`main` index is out of range")?;
    if main.param_count != 0 || main.ret.is_some() {
        return Err("`main` must take no parameters and return no value".into());
    }
    for function in &program.functions {
        let check = Checker { program, function };
        check
            .function()
            .map_err(|error| format!("in `{}`: {error}", function.name))?;
    }
    Ok(())
}

struct Checker<'a> {
    program: &'a Program,
    function: &'a Function,
}

impl Checker<'_> {
    fn function(&self) -> Result<(), String> {
        if self.function.param_count > self.function.locals.len() {
            return Err("more parameters than locals".into());
        }
        self.block(&self.function.body, 0)?;
        if self.function.ret.is_some() && !returns(&self.function.body) {
            return Err("a path reaches the end without returning a value".into());
        }
        Ok(())
    }

    fn block(&self, stmts: &[Stmt], loop_depth: u32) -> Result<(), String> {
        for stmt in stmts {
            self.stmt(stmt, loop_depth)?;
        }
        Ok(())
    }

    fn stmt(&self, stmt: &Stmt, loop_depth: u32) -> Result<(), String> {
        match stmt {
            Stmt::Assign { dst, value } => {
                let dst_ty = self.local(*dst)?;
                let value_ty = self.rvalue(value)?;
                expect(dst_ty, value_ty, "assignment")
            }
            Stmt::Call { func, args, .. } => self.call(*func, args).map(|_| ()),
            Stmt::Print(operand) => self.operand(*operand).map(|_| ()),
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                expect(Ty::Bool, self.operand(*cond)?, "`if` condition")?;
                self.block(then_body, loop_depth)?;
                self.block(else_body, loop_depth)
            }
            Stmt::Loop(body) => self.block(body, loop_depth + 1),
            Stmt::Break if loop_depth == 0 => Err("`break` outside a loop".into()),
            Stmt::Break => Ok(()),
            Stmt::Return(value) => match (value, self.function.ret) {
                (None, None) => Ok(()),
                (Some(value), Some(ret)) => expect(ret, self.operand(*value)?, "return value"),
                (Some(_), None) => {
                    Err("value returned from a function without a return type".into())
                }
                (None, Some(_)) => Err("missing return value".into()),
            },
        }
    }

    fn rvalue(&self, rvalue: &Rvalue) -> Result<Ty, String> {
        match rvalue {
            Rvalue::Use(operand) => self.operand(*operand),
            Rvalue::Binary { op, lhs, rhs, .. } => {
                let lhs = self.operand(*lhs)?;
                let rhs = self.operand(*rhs)?;
                match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
                        expect(Ty::Int, lhs, "arithmetic operand")?;
                        expect(Ty::Int, rhs, "arithmetic operand")?;
                        Ok(Ty::Int)
                    }
                    BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        expect(Ty::Int, lhs, "ordering operand")?;
                        expect(Ty::Int, rhs, "ordering operand")?;
                        Ok(Ty::Bool)
                    }
                    BinOp::Eq | BinOp::Ne => {
                        if lhs == Ty::String {
                            return Err("equality on String".into());
                        }
                        expect(lhs, rhs, "equality operand")?;
                        Ok(Ty::Bool)
                    }
                }
            }
            Rvalue::Neg { operand, .. } => {
                expect(Ty::Int, self.operand(*operand)?, "negation operand")?;
                Ok(Ty::Int)
            }
            Rvalue::Not(operand) => {
                expect(Ty::Bool, self.operand(*operand)?, "`not` operand")?;
                Ok(Ty::Bool)
            }
            Rvalue::Call { func, args, .. } => self
                .call(*func, args)?
                .ok_or_else(|| "using the result of a call that returns no value".into()),
        }
    }

    fn call(&self, func: usize, args: &[Operand]) -> Result<Option<Ty>, String> {
        let callee = self
            .program
            .functions
            .get(func)
            .ok_or("call to an unknown function index")?;
        if args.len() != callee.param_count {
            return Err(format!("wrong argument count for `{}`", callee.name));
        }
        for (arg, param) in args.iter().zip(&callee.locals) {
            expect(param.ty, self.operand(*arg)?, "argument")?;
        }
        Ok(callee.ret)
    }

    fn operand(&self, operand: Operand) -> Result<Ty, String> {
        match operand {
            Operand::Local(id) => self.local(id),
            Operand::Int(_) => Ok(Ty::Int),
            Operand::Bool(_) => Ok(Ty::Bool),
            Operand::Str(id) if id < self.program.strings.len() => Ok(Ty::String),
            Operand::Str(id) => Err(format!("string {id} is out of range")),
        }
    }

    fn local(&self, id: usize) -> Result<Ty, String> {
        self.function
            .locals
            .get(id)
            .map(|local| local.ty)
            .ok_or_else(|| format!("local {id} is out of range"))
    }
}

fn expect(expected: Ty, found: Ty, what: &str) -> Result<(), String> {
    if expected == found {
        Ok(())
    } else {
        Err(format!(
            "{what} expects {}, found {}",
            expected.name(),
            found.name()
        ))
    }
}

/// Every path through `stmts` ends in a `Return`. Loops never count.
fn returns(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|stmt| match stmt {
        Stmt::Return(_) => true,
        Stmt::If {
            then_body,
            else_body,
            ..
        } => returns(then_body) && returns(else_body),
        Stmt::Assign { .. } | Stmt::Call { .. } | Stmt::Print(_) | Stmt::Loop(_) | Stmt::Break => {
            false
        }
    })
}
