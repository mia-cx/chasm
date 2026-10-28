//! Stage-zero Chasm: the compiler behind the `chasm` CLI, and an embedding
//! API that runs Chasm functions inside a Rust application.
//!
//! [`Runtime`] is the embedding entry point; `docs/embedding.md` describes
//! it. `docs/alpha.md` specifies the language and the CLI.

mod ast;
mod embed;
mod lexer;
mod lower;
mod parser;
mod validate;

// Compiler internals the `chasm` CLI uses. They are not a stable API.
#[doc(hidden)]
pub mod diag;
#[doc(hidden)]
pub mod emit_c;
#[doc(hidden)]
pub mod interp;
#[doc(hidden)]
pub mod ir;
#[doc(hidden)]
pub mod native;

pub use diag::{Diagnostic, Span};
pub use embed::{
    CallError, CompileError, HostFn, HostReturn, RegisterError, Runtime, Script, ScriptType, Value,
};

/// Checks and lowers source text. `hosts` are the host functions it may call;
/// `require_main` applies the CLI's program contract.
#[doc(hidden)]
pub fn compile(
    source: &str,
    hosts: &[ir::HostSig],
    require_main: bool,
) -> Result<ir::Program, CompileError> {
    let decls =
        parser::parse(lexer::lex(source)).map_err(|error| CompileError::Source(vec![error]))?;
    let program = lower::lower(&decls, hosts, require_main).map_err(CompileError::Source)?;
    validate::validate(&program).map_err(CompileError::Internal)?;
    Ok(program)
}
