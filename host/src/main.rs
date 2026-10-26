//! `chasm`: the stage-zero Chasm compiler for the scalar alpha.
//! `docs/alpha.md` specifies the commands and language it accepts.

mod ast;
mod diag;
mod emit_c;
mod interp;
mod ir;
mod lexer;
mod lower;
mod native;
mod parser;
mod validate;

use std::ffi::OsString;
use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::thread;

use interp::Trap;

const USAGE: &str = "\
Usage:
  chasm check PATH             check a program
  chasm run PATH               check, then interpret a program
  chasm emit c PATH            print the generated C to stdout
  chasm build PATH -o OUTPUT   compile a program to a native executable
  chasm help                   show this help
  chasm --version              show the version

PATH is a single .chasm source file. `build` runs the C compiler named by $CC,
or `cc`, without a shell.

Exit status: 0 success, 1 program or build error, 2 usage error,
70 internal compiler error, 101 runtime panic.";

const PROGRAM_ERROR: u8 = 1;
const USAGE_ERROR: u8 = 2;
const INTERNAL_ERROR: u8 = 70;
const PANIC_STATUS: u8 = 101;

/// Room for the interpreter's recursion at the call depth limit.
const STACK_BYTES: usize = 512 << 20;

enum Cli {
    Check(PathBuf),
    Run(PathBuf),
    EmitC(PathBuf),
    Build { source: PathBuf, output: PathBuf },
    Help,
    Version,
}

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let worker = thread::Builder::new()
        .name("chasm".into())
        .stack_size(STACK_BYTES)
        .spawn(move || execute(&args))
        .expect("spawn the compiler thread");
    // A host panic has already printed its message.
    worker.join().unwrap_or(ExitCode::from(INTERNAL_ERROR))
}

fn execute(args: &[OsString]) -> ExitCode {
    let cli = match parse_args(args) {
        Ok(cli) => cli,
        Err(message) => {
            eprintln!("chasm: error: {message}\nRun `chasm help` for usage.");
            return ExitCode::from(USAGE_ERROR);
        }
    };
    match cli {
        Cli::Help => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Cli::Version => {
            println!("chasm {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Cli::Check(path) => match compile(&path) {
            Ok(_) => ExitCode::SUCCESS,
            Err(code) => code,
        },
        Cli::Run(path) => match compile(&path) {
            Ok(program) => run(&program, &path),
            Err(code) => code,
        },
        Cli::EmitC(path) => match compile(&path) {
            Ok(program) => {
                let c = emit_c::emit(&program, &path.to_string_lossy());
                write_stdout(c.as_bytes())
            }
            Err(code) => code,
        },
        Cli::Build { source, output } => match compile(&source) {
            Ok(program) => {
                let c = emit_c::emit(&program, &source.to_string_lossy());
                match native::build(&c, &source, &output) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(message) => fail(&message),
                }
            }
            Err(code) => code,
        },
    }
}

fn parse_args(args: &[OsString]) -> Result<Cli, String> {
    let Some((command, rest)) = args.split_first() else {
        return Err("no command given".into());
    };
    let rest: Vec<&OsString> = rest.iter().collect();
    match command.to_str() {
        Some("help" | "-h" | "--help") if rest.is_empty() => Ok(Cli::Help),
        Some("--version" | "-V") if rest.is_empty() => Ok(Cli::Version),
        Some("help" | "-h" | "--help" | "--version" | "-V") => Err(format!(
            "`{}` takes no arguments",
            command.to_string_lossy()
        )),
        Some("check") => Ok(Cli::Check(single_path("check", &rest)?)),
        Some("run") => Ok(Cli::Run(single_path("run", &rest)?)),
        Some("emit") => match rest.split_first() {
            Some((target, rest)) if target.to_str() == Some("c") => {
                Ok(Cli::EmitC(single_path("emit c", rest)?))
            }
            Some((target, _)) => Err(format!(
                "unknown emit target `{}`; this alpha emits only `c`",
                target.to_string_lossy()
            )),
            None => Err("`emit` needs a target: `chasm emit c PATH`".into()),
        },
        Some("build") => parse_build(&rest),
        _ => Err(format!("unknown command `{}`", command.to_string_lossy())),
    }
}

fn single_path(command: &str, rest: &[&OsString]) -> Result<PathBuf, String> {
    match rest {
        [path] if !is_option(path) => Ok(PathBuf::from(path)),
        [option] => Err(format!(
            "unknown option `{}` for `{command}`",
            option.to_string_lossy()
        )),
        _ => Err(format!("`{command}` takes exactly one PATH")),
    }
}

/// Accepts `PATH -o OUTPUT` with the option before or after the path.
fn parse_build(rest: &[&OsString]) -> Result<Cli, String> {
    let mut source = None;
    let mut output = None;
    let mut args = rest.iter();
    while let Some(arg) = args.next() {
        if arg.to_str() == Some("-o") {
            let Some(path) = args.next() else {
                return Err("`-o` needs an OUTPUT path".into());
            };
            if output.replace(PathBuf::from(path)).is_some() {
                return Err("`-o` is given more than once".into());
            }
        } else if is_option(arg) {
            return Err(format!(
                "unknown option `{}` for `build`",
                arg.to_string_lossy()
            ));
        } else if source.replace(PathBuf::from(arg)).is_some() {
            return Err("`build` takes exactly one PATH".into());
        }
    }
    match (source, output) {
        (Some(source), Some(output)) => Ok(Cli::Build { source, output }),
        (None, _) => Err("`build` needs a PATH".into()),
        (Some(_), None) => Err("`build` needs `-o OUTPUT`".into()),
    }
}

fn is_option(arg: &OsString) -> bool {
    arg.to_string_lossy().starts_with('-')
}

/// Reads, checks, and lowers one source file, printing any diagnostics.
fn compile(path: &Path) -> Result<ir::Program, ExitCode> {
    let shown = path.to_string_lossy();
    let bytes = fs::read(path).map_err(|error| fail(&format!("cannot read `{shown}`: {error}")))?;
    let report = |diagnostics: &[diag::Diagnostic]| {
        for diagnostic in diagnostics {
            eprintln!("{}", diagnostic.render(&shown));
        }
        ExitCode::from(PROGRAM_ERROR)
    };
    let source = String::from_utf8(bytes).map_err(|error| {
        let prefix = &error.as_bytes()[..error.utf8_error().valid_up_to()];
        let prefix = std::str::from_utf8(prefix).expect("UTF-8 error identifies a valid prefix");
        let span = prefix
            .chars()
            .fold(diag::Span { line: 1, col: 1 }, |span, c| {
                if c == '\n' {
                    diag::Span {
                        line: span.line + 1,
                        col: 1,
                    }
                } else {
                    diag::Span {
                        col: span.col + 1,
                        ..span
                    }
                }
            });
        report(&[diag::Diagnostic::new(
            span,
            "source is not valid UTF-8 text",
        )])
    })?;
    let decls = parser::parse(lexer::lex(&source)).map_err(|d| report(&[d]))?;
    let program = lower::lower(&decls).map_err(|d| report(&d))?;
    if let Err(error) = validate::validate(&program) {
        eprintln!("chasm: internal compiler error: invalid IR {error}");
        return Err(ExitCode::from(INTERNAL_ERROR));
    }
    Ok(program)
}

fn run(program: &ir::Program, path: &Path) -> ExitCode {
    let mut out = BufWriter::new(io::stdout().lock());
    let result = interp::run(program, &mut out);
    // Flush before reporting a panic so stdout output precedes it.
    let flushed = out.flush();
    match (result, flushed) {
        (Err(Trap::Panic { message, span }), _) => {
            eprintln!(
                "panic: {message}\n  at {}:{}:{}",
                path.to_string_lossy(),
                span.line,
                span.col
            );
            ExitCode::from(PANIC_STATUS)
        }
        (Err(Trap::Output(error)), _) | (Ok(()), Err(error)) => {
            fail(&format!("cannot write output: {error}"))
        }
        (Ok(()), Ok(())) => ExitCode::SUCCESS,
    }
}

fn write_stdout(bytes: &[u8]) -> ExitCode {
    let mut stdout = io::stdout().lock();
    match stdout.write_all(bytes).and_then(|()| stdout.flush()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail(&format!("cannot write output: {error}")),
    }
}

fn fail(message: &str) -> ExitCode {
    eprintln!("chasm: error: {message}");
    ExitCode::from(PROGRAM_ERROR)
}
