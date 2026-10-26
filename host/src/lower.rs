//! Type checking and lowering from the syntax tree to IR, in one pass.
//!
//! Lowering evaluates every subexpression into an operand in source order. A
//! variable operand is read when its operation runs rather than copied first;
//! that is sound only because no expression can assign a variable, so nothing
//! evaluated in between can change it.

use crate::ast::{self, Arg, Expr, ExprKind, FnDecl, TypeName};
use crate::diag::{Diagnostic, Span};
use crate::ir::{BinOp, FuncId, Function, Local, LocalId, Operand, Program, Rvalue, Stmt, Ty};

type LResult<T> = Result<T, Diagnostic>;

struct Sig {
    name: String,
    params: Vec<(String, Ty)>,
    ret: Option<Ty>,
}

/// Checks the program and lowers it. Reports every signature error, or else
/// the first error in each function body.
pub fn lower(decls: &[FnDecl]) -> Result<Program, Vec<Diagnostic>> {
    let mut errors = Vec::new();
    let mut sigs = Vec::new();
    for (index, decl) in decls.iter().enumerate() {
        match signature(decl, &decls[..index]) {
            Ok(sig) => sigs.push(sig),
            Err(error) => errors.push(error),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let main = match sigs.iter().position(|s| s.name == "main") {
        None => {
            let start = Span { line: 1, col: 1 };
            return Err(vec![Diagnostic::new(
                start,
                "the program has no `fn main()`",
            )]);
        }
        Some(main) if !sigs[main].params.is_empty() || sigs[main].ret.is_some() => {
            let message = "`main` must take no parameters and return no value";
            return Err(vec![Diagnostic::new(decls[main].span, message)]);
        }
        Some(main) => main,
    };

    let mut strings = Vec::new();
    let mut functions = Vec::new();
    for (decl, sig) in decls.iter().zip(&sigs) {
        match lower_function(decl, sig, &sigs, &mut strings) {
            Ok(function) => functions.push(function),
            Err(error) => errors.push(error),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(Program {
        functions,
        strings,
        main,
    })
}

fn signature(decl: &FnDecl, earlier: &[FnDecl]) -> LResult<Sig> {
    if decl.name == "print" {
        let message = "`print` is a prelude function and cannot be redefined";
        return Err(Diagnostic::new(decl.span, message));
    }
    if let Some(first) = earlier.iter().find(|d| d.name == decl.name) {
        let message = format!(
            "function `{}` is already defined on line {}",
            decl.name, first.span.line
        );
        return Err(Diagnostic::new(decl.span, message));
    }
    let mut params: Vec<(String, Ty)> = Vec::new();
    for param in &decl.params {
        if params.iter().any(|(name, _)| *name == param.name) {
            let message = format!("parameter `{}` is declared twice", param.name);
            return Err(Diagnostic::new(param.span, message));
        }
        params.push((param.name.clone(), resolve_type(&param.ty)?));
    }
    let ret = decl.ret.as_ref().map(resolve_type).transpose()?;
    Ok(Sig {
        name: decl.name.clone(),
        params,
        ret,
    })
}

fn resolve_type(ty: &TypeName) -> LResult<Ty> {
    match ty.name.as_str() {
        "Int" => Ok(Ty::Int),
        "Bool" => Ok(Ty::Bool),
        "String" => Ok(Ty::String),
        other => Err(Diagnostic::new(
            ty.span,
            format!("unknown type `{other}`; this alpha supports Int, Bool, and String"),
        )),
    }
}

fn lower_function(
    decl: &FnDecl,
    sig: &Sig,
    sigs: &[Sig],
    strings: &mut Vec<String>,
) -> LResult<Function> {
    let mut lower = FnLower {
        sig,
        sigs,
        strings,
        locals: Vec::new(),
        scopes: vec![Vec::new()],
    };
    for (name, ty) in &sig.params {
        lower.declare(name, *ty);
    }
    let body = lower.block(&decl.body)?;
    if sig.ret.is_some() && !always_returns(&decl.body) {
        let message = format!(
            "`{}` can reach the end of its body without returning a value",
            sig.name
        );
        return Err(Diagnostic::new(decl.span, message));
    }
    Ok(Function {
        name: sig.name.clone(),
        param_count: sig.params.len(),
        locals: lower.locals,
        ret: sig.ret,
        body,
    })
}

/// A structural check: a block returns when it contains a `return`, or an
/// `if` whose branches both return. Loops never count.
fn always_returns(stmts: &[ast::Stmt]) -> bool {
    stmts.iter().any(|stmt| match stmt {
        ast::Stmt::Return { .. } => true,
        ast::Stmt::If {
            then_body,
            else_body,
            ..
        } => always_returns(then_body) && always_returns(else_body),
        ast::Stmt::Let { .. }
        | ast::Stmt::Assign { .. }
        | ast::Stmt::While { .. }
        | ast::Stmt::Expr(_) => false,
    })
}

struct FnLower<'a> {
    sig: &'a Sig,
    sigs: &'a [Sig],
    strings: &'a mut Vec<String>,
    locals: Vec<Local>,
    /// Visible variables, innermost block last.
    scopes: Vec<Vec<(String, LocalId)>>,
}

impl FnLower<'_> {
    fn block(&mut self, stmts: &[ast::Stmt]) -> LResult<Vec<Stmt>> {
        self.scopes.push(Vec::new());
        let mut out = Vec::new();
        for stmt in stmts {
            self.stmt(stmt, &mut out)?;
        }
        self.scopes.pop();
        Ok(out)
    }

    fn stmt(&mut self, stmt: &ast::Stmt, out: &mut Vec<Stmt>) -> LResult<()> {
        match stmt {
            ast::Stmt::Let {
                name,
                span,
                ty,
                value,
            } => {
                let (rvalue, value_ty) = self.rvalue(value, out)?;
                if let Some(ty) = ty {
                    let declared = resolve_type(ty)?;
                    if declared != value_ty {
                        let message = format!(
                            "`{name}` is declared as {}, but its value is {}",
                            declared.name(),
                            value_ty.name()
                        );
                        return Err(Diagnostic::new(value.span, message));
                    }
                }
                if self.lookup(name).is_some() {
                    let message = format!("a variable named `{name}` is already visible here");
                    return Err(Diagnostic::new(*span, message));
                }
                let dst = self.declare(name, value_ty);
                out.push(Stmt::Assign { dst, value: rvalue });
            }
            ast::Stmt::Assign { name, span, value } => {
                let Some(dst) = self.lookup(name) else {
                    return Err(unknown_variable(name, *span));
                };
                let (rvalue, value_ty) = self.rvalue(value, out)?;
                let ty = self.locals[dst].ty;
                if ty != value_ty {
                    let message = format!(
                        "cannot assign {} to `{name}`, which is {}",
                        value_ty.name(),
                        ty.name()
                    );
                    return Err(Diagnostic::new(value.span, message));
                }
                out.push(Stmt::Assign { dst, value: rvalue });
            }
            ast::Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let cond = self.condition(cond, "if", out)?;
                let then_body = self.block(then_body)?;
                let else_body = self.block(else_body)?;
                out.push(Stmt::If {
                    cond,
                    then_body,
                    else_body,
                });
            }
            ast::Stmt::While { cond, body } => {
                // The condition is lowered inside the loop so it reruns before
                // every iteration.
                let mut looped = Vec::new();
                let cond = self.condition(cond, "while", &mut looped)?;
                looped.push(Stmt::If {
                    cond,
                    then_body: Vec::new(),
                    else_body: vec![Stmt::Break],
                });
                looped.extend(self.block(body)?);
                out.push(Stmt::Loop(looped));
            }
            ast::Stmt::Return { span, value } => {
                let name = &self.sig.name;
                let value = match (value, self.sig.ret) {
                    (None, None) => None,
                    (Some(value), Some(ret)) => {
                        let (operand, ty) = self.operand(value, out)?;
                        if ty != ret {
                            let message = format!(
                                "`{name}` returns {}, but this value is {}",
                                ret.name(),
                                ty.name()
                            );
                            return Err(Diagnostic::new(value.span, message));
                        }
                        Some(operand)
                    }
                    (Some(value), None) => {
                        let message =
                            format!("`{name}` returns no value, but this `return` has one");
                        return Err(Diagnostic::new(value.span, message));
                    }
                    (None, Some(ret)) => {
                        let message =
                            format!("`{name}` returns {}, so `return` needs a value", ret.name());
                        return Err(Diagnostic::new(*span, message));
                    }
                };
                out.push(Stmt::Return(value));
            }
            ast::Stmt::Expr(expr) => match &expr.kind {
                ExprKind::Call { callee, args } => self.call_stmt(callee, args, expr.span, out)?,
                ExprKind::Int(_)
                | ExprKind::Bool(_)
                | ExprKind::Str(_)
                | ExprKind::Var(_)
                | ExprKind::Neg(_)
                | ExprKind::Not(_)
                | ExprKind::Binary(..) => {
                    self.operand(expr, out)?;
                }
            },
        }
        Ok(())
    }

    fn condition(&mut self, cond: &Expr, keyword: &str, out: &mut Vec<Stmt>) -> LResult<Operand> {
        let (operand, ty) = self.operand(cond, out)?;
        if ty != Ty::Bool {
            let message = format!("`{keyword}` condition must be Bool, found {}", ty.name());
            return Err(Diagnostic::new(cond.span, message));
        }
        Ok(operand)
    }

    /// A call whose result is discarded, including `print`.
    fn call_stmt(
        &mut self,
        callee: &str,
        args: &[Arg],
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> LResult<()> {
        if callee == "print" {
            let value = self.print_argument(args, span, out)?;
            out.push(Stmt::Print(value));
            return Ok(());
        }
        let func = self.function(callee, span)?;
        let args = self.arguments(func, args, span, out)?;
        out.push(Stmt::Call { func, args, span });
        Ok(())
    }

    fn print_argument(
        &mut self,
        args: &[Arg],
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> LResult<Operand> {
        let [arg] = args else {
            return Err(Diagnostic::new(span, "`print` takes exactly one argument"));
        };
        if let Some((name, name_span)) = &arg.name
            && name != "value"
        {
            let message =
                format!("`print` has no parameter named `{name}`; its parameter is `value`");
            return Err(Diagnostic::new(*name_span, message));
        }
        Ok(self.operand(&arg.value, out)?.0)
    }

    fn function(&self, name: &str, span: Span) -> LResult<FuncId> {
        self.sigs
            .iter()
            .position(|s| s.name == name)
            .ok_or_else(|| Diagnostic::new(span, format!("unknown function `{name}`")))
    }

    /// Evaluates arguments in the order written and returns them in
    /// parameter order.
    fn arguments(
        &mut self,
        func: FuncId,
        args: &[Arg],
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> LResult<Vec<Operand>> {
        // Copy the slice reference so `sig` does not borrow `self`.
        let sigs = self.sigs;
        let sig = &sigs[func];
        let mut slots: Vec<Option<Operand>> = vec![None; sig.params.len()];
        for (position, arg) in args.iter().enumerate() {
            let index = match &arg.name {
                None if position < sig.params.len() => position,
                None => {
                    let message = format!(
                        "`{}` takes {} argument(s), but more were given",
                        sig.name,
                        sig.params.len()
                    );
                    return Err(Diagnostic::new(arg.value.span, message));
                }
                Some((name, name_span)) => {
                    let Some(index) = sig.params.iter().position(|(p, _)| p == name) else {
                        let message = format!("`{}` has no parameter named `{name}`", sig.name);
                        return Err(Diagnostic::new(*name_span, message));
                    };
                    if slots[index].is_some() {
                        let message = format!("argument `{name}` is given more than once");
                        return Err(Diagnostic::new(*name_span, message));
                    }
                    index
                }
            };
            let (operand, ty) = self.operand(&arg.value, out)?;
            let (param, param_ty) = &sig.params[index];
            if ty != *param_ty {
                let message = format!(
                    "argument `{param}` of `{}` expects {}, found {}",
                    sig.name,
                    param_ty.name(),
                    ty.name()
                );
                return Err(Diagnostic::new(arg.value.span, message));
            }
            slots[index] = Some(operand);
        }
        slots
            .into_iter()
            .zip(&sig.params)
            .map(|(slot, (param, _))| {
                slot.ok_or_else(|| {
                    let message = format!("missing argument `{param}` in call to `{}`", sig.name);
                    Diagnostic::new(span, message)
                })
            })
            .collect()
    }

    /// Lowers an expression to an operand, storing any operation in a fresh
    /// temporary.
    fn operand(&mut self, expr: &Expr, out: &mut Vec<Stmt>) -> LResult<(Operand, Ty)> {
        let (rvalue, ty) = self.rvalue(expr, out)?;
        if let Rvalue::Use(operand) = rvalue {
            return Ok((operand, ty));
        }
        let dst = self.temp(ty);
        out.push(Stmt::Assign { dst, value: rvalue });
        Ok((Operand::Local(dst), ty))
    }

    fn rvalue(&mut self, expr: &Expr, out: &mut Vec<Stmt>) -> LResult<(Rvalue, Ty)> {
        let span = expr.span;
        Ok(match &expr.kind {
            ExprKind::Int(n) => (Rvalue::Use(Operand::Int(*n)), Ty::Int),
            ExprKind::Bool(b) => (Rvalue::Use(Operand::Bool(*b)), Ty::Bool),
            ExprKind::Str(text) => {
                self.strings.push(text.clone());
                (
                    Rvalue::Use(Operand::Str(self.strings.len() - 1)),
                    Ty::String,
                )
            }
            ExprKind::Var(name) => {
                let id = self
                    .lookup(name)
                    .ok_or_else(|| unknown_variable(name, span))?;
                (Rvalue::Use(Operand::Local(id)), self.locals[id].ty)
            }
            ExprKind::Neg(inner) => {
                let operand = self.typed_operand(inner, Ty::Int, "unary `-`", out)?;
                (Rvalue::Neg { operand, span }, Ty::Int)
            }
            ExprKind::Not(inner) => {
                let operand = self.typed_operand(inner, Ty::Bool, "`not`", out)?;
                (Rvalue::Not(operand), Ty::Bool)
            }
            ExprKind::Binary(op @ (ast::BinOp::And | ast::BinOp::Or), lhs, rhs) => {
                (self.short_circuit(*op, lhs, rhs, out)?, Ty::Bool)
            }
            ExprKind::Binary(op, lhs, rhs) => self.binary(*op, lhs, rhs, span, out)?,
            ExprKind::Call { callee, args } => {
                if callee == "print" {
                    return Err(Diagnostic::new(
                        span,
                        "`print` returns no value, so its result cannot be used",
                    ));
                }
                let func = self.function(callee, span)?;
                let args = self.arguments(func, args, span, out)?;
                let Some(ty) = self.sigs[func].ret else {
                    let message =
                        format!("`{callee}` returns no value, so its result cannot be used");
                    return Err(Diagnostic::new(span, message));
                };
                (Rvalue::Call { func, args, span }, ty)
            }
        })
    }

    /// `a and b` stores `a` in a fresh temporary and overwrites it with `b`
    /// only when `a` is true; `or` only when `a` is false. The temporary is
    /// never a source variable, so `b` still reads the old value of any
    /// variable the result is assigned to.
    fn short_circuit(
        &mut self,
        op: ast::BinOp,
        lhs: &Expr,
        rhs: &Expr,
        out: &mut Vec<Stmt>,
    ) -> LResult<Rvalue> {
        let what = format!("`{}`", op.symbol());
        let lhs = self.typed_operand(lhs, Ty::Bool, &what, out)?;
        let result = self.temp(Ty::Bool);
        out.push(Stmt::Assign {
            dst: result,
            value: Rvalue::Use(lhs),
        });
        let mut rhs_body = Vec::new();
        let rhs = self.typed_operand(rhs, Ty::Bool, &what, &mut rhs_body)?;
        rhs_body.push(Stmt::Assign {
            dst: result,
            value: Rvalue::Use(rhs),
        });
        let (then_body, else_body) = if op == ast::BinOp::And {
            (rhs_body, Vec::new())
        } else {
            (Vec::new(), rhs_body)
        };
        out.push(Stmt::If {
            cond: Operand::Local(result),
            then_body,
            else_body,
        });
        Ok(Rvalue::Use(Operand::Local(result)))
    }

    fn binary(
        &mut self,
        op: ast::BinOp,
        lhs: &Expr,
        rhs: &Expr,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> LResult<(Rvalue, Ty)> {
        let (lhs, lhs_ty) = self.operand(lhs, out)?;
        let (rhs, rhs_ty) = self.operand(rhs, out)?;
        let symbol = op.symbol();
        let (op, ty) = match op {
            ast::BinOp::Add => (BinOp::Add, Ty::Int),
            ast::BinOp::Sub => (BinOp::Sub, Ty::Int),
            ast::BinOp::Mul => (BinOp::Mul, Ty::Int),
            ast::BinOp::Div => (BinOp::Div, Ty::Int),
            ast::BinOp::Rem => (BinOp::Rem, Ty::Int),
            ast::BinOp::Lt => (BinOp::Lt, Ty::Bool),
            ast::BinOp::Le => (BinOp::Le, Ty::Bool),
            ast::BinOp::Gt => (BinOp::Gt, Ty::Bool),
            ast::BinOp::Ge => (BinOp::Ge, Ty::Bool),
            ast::BinOp::Eq => (BinOp::Eq, Ty::Bool),
            ast::BinOp::Ne => (BinOp::Ne, Ty::Bool),
            ast::BinOp::And | ast::BinOp::Or => unreachable!("lowered by short_circuit"),
        };
        let equality = matches!(op, BinOp::Eq | BinOp::Ne);
        let valid = if equality {
            lhs_ty == rhs_ty && lhs_ty != Ty::String
        } else {
            lhs_ty == Ty::Int && rhs_ty == Ty::Int
        };
        if !valid {
            let message = if equality && lhs_ty == Ty::String && rhs_ty == Ty::String {
                format!("`{symbol}` on String is not supported in this alpha")
            } else if equality {
                format!(
                    "`{symbol}` compares two Int or two Bool values, found {} and {}",
                    lhs_ty.name(),
                    rhs_ty.name()
                )
            } else {
                format!(
                    "`{symbol}` expects Int operands, found {} and {}",
                    lhs_ty.name(),
                    rhs_ty.name()
                )
            };
            return Err(Diagnostic::new(span, message));
        }
        Ok((Rvalue::Binary { op, lhs, rhs, span }, ty))
    }

    fn typed_operand(
        &mut self,
        expr: &Expr,
        expected: Ty,
        what: &str,
        out: &mut Vec<Stmt>,
    ) -> LResult<Operand> {
        let (operand, ty) = self.operand(expr, out)?;
        if ty != expected {
            let message = format!("{what} expects {}, found {}", expected.name(), ty.name());
            return Err(Diagnostic::new(expr.span, message));
        }
        Ok(operand)
    }

    fn lookup(&self, name: &str) -> Option<LocalId> {
        self.scopes
            .iter()
            .rev()
            .flat_map(|scope| scope.iter().rev())
            .find(|(n, _)| n == name)
            .map(|(_, id)| *id)
    }

    fn declare(&mut self, name: &str, ty: Ty) -> LocalId {
        let id = self.push_local(name, ty);
        self.scopes
            .last_mut()
            .expect("a function always has a scope")
            .push((name.to_string(), id));
        id
    }

    fn temp(&mut self, ty: Ty) -> LocalId {
        self.push_local("tmp", ty)
    }

    fn push_local(&mut self, name: &str, ty: Ty) -> LocalId {
        self.locals.push(Local {
            name: name.to_string(),
            ty,
        });
        self.locals.len() - 1
    }
}

fn unknown_variable(name: &str, span: Span) -> Diagnostic {
    Diagnostic::new(span, format!("unknown variable `{name}`"))
}
