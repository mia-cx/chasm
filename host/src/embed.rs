//! Embedding API: compile Chasm source in memory, register host functions,
//! and call Chasm functions from Rust. `docs/embedding.md` is the guide.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::diag::{Diagnostic, Span};
use crate::interp::{self, Env, Trap};
use crate::ir::{HostSig, Program, Ty};
use crate::lexer::{self, Tok};

/// Fuel for each call unless the host sets a budget: one unit per IR
/// statement executed and per loop iteration.
const DEFAULT_FUEL: u64 = 1_000_000;

static NEXT_RUNTIME: AtomicU64 = AtomicU64::new(0);

/// A scalar crossing between Rust and Chasm: `Int`, `Bool`, or `String`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Int(i32),
    Bool(bool),
    String(String),
}

impl Value {
    fn ty(&self) -> Ty {
        match self {
            Value::Int(_) => Ty::Int,
            Value::Bool(_) => Ty::Bool,
            Value::String(_) => Ty::String,
        }
    }

    fn into_interp(self) -> interp::Value {
        match self {
            Value::Int(n) => interp::Value::Int(n),
            Value::Bool(b) => interp::Value::Bool(b),
            Value::String(text) => interp::Value::Str(text.into()),
        }
    }

    fn from_interp(value: interp::Value) -> Self {
        match value {
            interp::Value::Int(n) => Value::Int(n),
            interp::Value::Bool(b) => Value::Bool(b),
            interp::Value::Str(text) => Value::String(text.to_string()),
        }
    }
}

impl From<i32> for Value {
    fn from(n: i32) -> Self {
        Value::Int(n)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl From<String> for Value {
    fn from(text: String) -> Self {
        Value::String(text)
    }
}

impl From<&str> for Value {
    fn from(text: &str) -> Self {
        Value::String(text.into())
    }
}

/// Compiles Chasm source and calls its functions, with the host functions
/// registered on it.
///
/// A runtime keeps no execution state between calls. Each call starts with
/// a fresh fuel budget and call stack, so a call that fails leaves the
/// runtime ready for the next one. Host callbacks run on the calling thread
/// and are trusted: fuel does not interrupt them, and a callback panic
/// unwinds out of [`Runtime::call`].
pub struct Runtime {
    id: u64,
    /// Append-only, so a compiled script's host indices stay valid.
    hosts: Vec<Host>,
    output: Vec<u8>,
    fuel: u64,
}

type Callback = Box<dyn FnMut(Vec<interp::Value>) -> Result<Option<interp::Value>, String>>;

struct Host {
    sig: HostSig,
    callback: Callback,
}

/// Checked source, ready to call. Calls to host functions are bound when the
/// script compiles, so it runs only on the runtime that compiled it.
pub struct Script {
    runtime: u64,
    program: Program,
}

impl Default for Runtime {
    fn default() -> Self {
        Runtime {
            id: NEXT_RUNTIME.fetch_add(1, Ordering::Relaxed),
            hosts: Vec::new(),
            output: Vec::new(),
            fuel: DEFAULT_FUEL,
        }
    }
}

impl Runtime {
    /// A runtime with no host functions and the default fuel budget.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the fuel each later call starts with. A call fails with
    /// [`CallError::OutOfFuel`] when it would execute an IR statement or
    /// start a loop iteration with none left.
    pub fn set_fuel(&mut self, fuel: u64) {
        self.fuel = fuel;
    }

    /// Makes `function` callable by `name` from scripts compiled afterwards.
    /// Its Rust signature sets its Chasm one: parameters and the `Ok` value
    /// are `i32` (Int), `bool` (Bool), or `String`; `()` returns no value.
    /// An `Err` stops the script with [`CallError::Host`].
    ///
    /// ```
    /// let mut runtime = chasm::Runtime::new();
    /// runtime
    ///     .register("double", |n: i32| -> Result<i32, String> {
    ///         n.checked_mul(2).ok_or_else(|| "too large".into())
    ///     })
    ///     .unwrap();
    /// ```
    pub fn register<F, Args>(&mut self, name: &str, mut function: F) -> Result<(), RegisterError>
    where
        F: HostFn<Args>,
    {
        if !is_identifier(name) {
            return Err(RegisterError::InvalidName(name.into()));
        }
        if name == "print" || self.hosts.iter().any(|host| host.sig.name == name) {
            return Err(RegisterError::Taken(name.into()));
        }
        let (params, ret) = F::signature();
        self.hosts.push(Host {
            sig: HostSig {
                name: name.into(),
                params,
                ret,
            },
            callback: Box::new(move |args| function.call(args)),
        });
        Ok(())
    }

    /// Checks `source` as a library: a list of functions, with no `main`
    /// required. It may call the host functions registered so far.
    pub fn compile(&self, source: &str) -> Result<Script, CompileError> {
        let hosts: Vec<HostSig> = self.hosts.iter().map(|host| host.sig.clone()).collect();
        let program = crate::compile(source, &hosts, false)?;
        Ok(Script {
            runtime: self.id,
            program,
        })
    }

    /// Calls the script function `name` and returns its result, or `None`
    /// when it returns no value. Arguments are checked against its
    /// parameters before anything runs.
    pub fn call(
        &mut self,
        script: &Script,
        name: &str,
        args: &[Value],
    ) -> Result<Option<Value>, CallError> {
        if script.runtime != self.id {
            return Err(CallError::ForeignScript);
        }
        let program = &script.program;
        let Some(func) = program.functions.iter().position(|f| f.name == name) else {
            return Err(CallError::UnknownFunction(name.into()));
        };
        let params = &program.functions[func].locals[..program.functions[func].param_count];
        if args.len() != params.len() {
            return Err(CallError::Arguments(format!(
                "`{name}` takes {} argument(s), but {} were given",
                params.len(),
                args.len()
            )));
        }
        for (arg, param) in args.iter().zip(params) {
            if arg.ty() != param.ty {
                return Err(CallError::Arguments(format!(
                    "argument `{}` of `{name}` expects {}, found {}",
                    param.name,
                    param.ty.name(),
                    arg.ty().name()
                )));
            }
        }

        let args = args.iter().cloned().map(Value::into_interp).collect();
        let hosts = &mut self.hosts;
        let mut host = |id: usize, args| (hosts[id].callback)(args);
        let env = Env {
            out: &mut self.output,
            host: &mut host,
            fuel: Some(self.fuel),
        };
        match interp::call(program, func, args, env) {
            Ok(value) => Ok(value.map(Value::from_interp)),
            Err(Trap::Panic { message, span }) => Err(CallError::Panic { message, span }),
            Err(Trap::OutOfFuel) => Err(CallError::OutOfFuel),
            Err(Trap::Host {
                host,
                message,
                span,
            }) => Err(CallError::Host {
                function: program.hosts[host].name.clone(),
                message,
                span,
            }),
            Err(Trap::Output(_)) => unreachable!("writing to a Vec cannot fail"),
        }
    }

    /// Returns the text `print` has written since the last call to this
    /// method, including output from calls that failed.
    pub fn take_output(&mut self) -> String {
        String::from_utf8(std::mem::take(&mut self.output)).expect("`print` writes UTF-8 text")
    }
}

/// Whether scripts can call a function by this name: a single identifier,
/// not a keyword or reserved word.
fn is_identifier(name: &str) -> bool {
    matches!(
        lexer::lex(name).as_slice(),
        [ident, end] if ident.tok == Tok::Ident(name.into()) && end.tok == Tok::Eof
    )
}

mod sealed {
    pub trait Sealed {}
    pub trait Callable<Args> {}
    impl Sealed for i32 {}
    impl Sealed for bool {}
    impl Sealed for String {}
    impl Sealed for () {}
}

/// A Rust type that stands for a Chasm type in a host function signature:
/// `i32` is Int, `bool` is Bool, `String` is String.
pub trait ScriptType: sealed::Sealed + Sized {
    #[doc(hidden)]
    const TY: Ty;
    #[doc(hidden)]
    fn from_value(value: interp::Value) -> Self;
    #[doc(hidden)]
    fn into_value(self) -> interp::Value;
}

impl ScriptType for i32 {
    const TY: Ty = Ty::Int;
    fn from_value(value: interp::Value) -> Self {
        match value {
            interp::Value::Int(n) => n,
            _ => unreachable!("the checker matched the argument types"),
        }
    }
    fn into_value(self) -> interp::Value {
        interp::Value::Int(self)
    }
}

impl ScriptType for bool {
    const TY: Ty = Ty::Bool;
    fn from_value(value: interp::Value) -> Self {
        match value {
            interp::Value::Bool(b) => b,
            _ => unreachable!("the checker matched the argument types"),
        }
    }
    fn into_value(self) -> interp::Value {
        interp::Value::Bool(self)
    }
}

impl ScriptType for String {
    const TY: Ty = Ty::String;
    fn from_value(value: interp::Value) -> Self {
        match value {
            interp::Value::Str(text) => text.to_string(),
            _ => unreachable!("the checker matched the argument types"),
        }
    }
    fn into_value(self) -> interp::Value {
        interp::Value::Str(self.into())
    }
}

/// The `Ok` type of a host function: a [`ScriptType`], or `()` for a
/// function that returns no value.
pub trait HostReturn: sealed::Sealed {
    #[doc(hidden)]
    const TY: Option<Ty>;
    #[doc(hidden)]
    fn into_value(self) -> Option<interp::Value>;
}

impl<T: ScriptType> HostReturn for T {
    const TY: Option<Ty> = Some(T::TY);
    fn into_value(self) -> Option<interp::Value> {
        Some(ScriptType::into_value(self))
    }
}

impl HostReturn for () {
    const TY: Option<Ty> = None;
    fn into_value(self) -> Option<interp::Value> {
        None
    }
}

/// A Rust closure or function that [`Runtime::register`] accepts: up to four
/// [`ScriptType`] parameters, returning `Result<R, String>` where `R` is a
/// [`HostReturn`]. `Args` is the parameter tuple; Rust infers it.
/// This trait is sealed so a registration cannot supply a signature that
/// disagrees with its argument or return conversions.
pub trait HostFn<Args>: sealed::Callable<Args> + 'static {
    #[doc(hidden)]
    fn signature() -> (Vec<Ty>, Option<Ty>);
    #[doc(hidden)]
    fn call(&mut self, args: Vec<interp::Value>) -> Result<Option<interp::Value>, String>;
}

macro_rules! host_fn {
    ($($arg:ident),*) => {
        impl<F, R, $($arg),*> sealed::Callable<($($arg,)*)> for F
        where
            F: FnMut($($arg),*) -> Result<R, String> + 'static,
            R: HostReturn,
            $($arg: ScriptType,)*
        {}

        impl<F, R, $($arg),*> HostFn<($($arg,)*)> for F
        where
            F: FnMut($($arg),*) -> Result<R, String> + 'static,
            R: HostReturn,
            $($arg: ScriptType,)*
        {
            fn signature() -> (Vec<Ty>, Option<Ty>) {
                (vec![$($arg::TY),*], R::TY)
            }

            #[allow(non_snake_case, unused_mut, unused_variables)]
            fn call(&mut self, args: Vec<interp::Value>) -> Result<Option<interp::Value>, String> {
                let mut args = args.into_iter();
                $(let $arg = $arg::from_value(args.next().expect("the checker matched the arity"));)*
                self($($arg),*).map(HostReturn::into_value)
            }
        }
    };
}

host_fn!();
host_fn!(A);
host_fn!(A, B);
host_fn!(A, B, C);
host_fn!(A, B, C, D);

/// Source that failed to compile.
#[derive(Debug)]
pub enum CompileError {
    /// The source is invalid. Diagnostics follow the order `docs/alpha.md`
    /// describes.
    Source(Vec<Diagnostic>),
    /// The compiler rejected its own output: a compiler bug, not a source
    /// error.
    Internal(String),
}

/// A call that did not finish. Effects that happened before the failure,
/// such as host callbacks and `print` output, are not rolled back.
#[derive(Debug)]
pub enum CallError {
    /// The script has no function by this name. Nothing ran.
    UnknownFunction(String),
    /// The arguments do not match the function's parameters. Nothing ran.
    Arguments(String),
    /// The script was compiled by a different runtime. Nothing ran.
    ForeignScript,
    /// A Chasm panic: `integer overflow`, `division by zero`, or
    /// `call depth limit exceeded`, at the operation or call that caused it.
    Panic { message: &'static str, span: Span },
    /// The call used up its fuel.
    OutOfFuel,
    /// The host function called at `span` returned an error.
    Host {
        function: String,
        message: String,
        span: Span,
    },
}

/// A host function name that [`Runtime::register`] refused.
#[derive(Debug, PartialEq, Eq)]
pub enum RegisterError {
    /// Scripts cannot call this name: it is not an identifier, or it is a
    /// keyword or reserved word.
    InvalidName(String),
    /// `print` or an already registered host function has this name.
    Taken(String),
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CompileError::Source(diagnostics) => {
                let lines: Vec<String> = diagnostics.iter().map(|d| d.to_string()).collect();
                f.write_str(&lines.join("\n"))
            }
            CompileError::Internal(error) => {
                write!(f, "internal compiler error: invalid IR {error}")
            }
        }
    }
}

impl fmt::Display for CallError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CallError::UnknownFunction(name) => write!(f, "the script has no function `{name}`"),
            CallError::Arguments(message) => f.write_str(message),
            CallError::ForeignScript => f.write_str("the script was compiled by another runtime"),
            CallError::Panic { message, span } => {
                write!(f, "panic: {message} at {}:{}", span.line, span.col)
            }
            CallError::OutOfFuel => f.write_str("the call ran out of fuel"),
            CallError::Host {
                function,
                message,
                span,
            } => write!(
                f,
                "host function `{function}` failed at {}:{}: {message}",
                span.line, span.col
            ),
        }
    }
}

impl fmt::Display for RegisterError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RegisterError::InvalidName(name) => {
                write!(f, "`{name}` is not a name scripts can call")
            }
            RegisterError::Taken(name) => write!(f, "a function named `{name}` already exists"),
        }
    }
}

impl std::error::Error for CompileError {}
impl std::error::Error for CallError {}
impl std::error::Error for RegisterError {}
