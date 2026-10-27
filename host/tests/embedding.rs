//! Black-box checks of the embedding API in `docs/embedding.md`.

use std::cell::RefCell;
use std::rc::Rc;

use chasm::{CallError, CompileError, RegisterError, Runtime, Span, Value};

fn int(n: i32) -> Option<Value> {
    Some(Value::Int(n))
}

#[test]
fn a_library_without_main_returns_every_scalar_type_across_repeated_calls() {
    let mut runtime = Runtime::new();
    let script = runtime
        .compile(
            r#"
fn add(a: Int, b: Int) -> Int {
    return a + b
}
fn is_even(n: Int) -> Bool {
    return n % 2 == 0
}
fn greeting() -> String {
    return "héllo"
}
fn echo(text: String) -> String {
    return text
}
fn nothing() {
}
fn main(n: Int) -> Int {
    return n
}
"#,
        )
        .unwrap();

    for _ in 0..3 {
        assert_eq!(
            runtime.call(&script, "add", &[2.into(), 3.into()]).unwrap(),
            int(5)
        );
    }
    assert_eq!(
        runtime.call(&script, "is_even", &[7.into()]).unwrap(),
        Some(false.into())
    );
    assert_eq!(
        runtime.call(&script, "greeting", &[]).unwrap(),
        Some("héllo".into())
    );
    assert_eq!(runtime.call(&script, "nothing", &[]).unwrap(), None);
    // `main` is an ordinary function in a library.
    assert_eq!(runtime.call(&script, "main", &[9.into()]).unwrap(), int(9));

    // A string from one script stays itself inside another, whose literal
    // table holds different text.
    let other = runtime
        .compile("fn first() -> String {\n return \"beta\"\n}\nfn echo(text: String) -> String {\n return text\n}")
        .unwrap();
    let alpha = runtime.call(&script, "greeting", &[]).unwrap().unwrap();
    assert_eq!(
        runtime.call(&other, "echo", &[alpha]).unwrap(),
        Some("héllo".into())
    );
}

#[test]
fn host_functions_run_in_source_order_and_exchange_typed_values() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = Runtime::new();
    let marks = Rc::clone(&log);
    runtime
        .register("mark", move |n: i32| -> Result<i32, String> {
            marks.borrow_mut().push(n.to_string());
            Ok(n)
        })
        .unwrap();
    let notes = Rc::clone(&log);
    runtime
        .register(
            "note",
            move |text: String, loud: bool| -> Result<(), String> {
                notes
                    .borrow_mut()
                    .push(if loud { text.to_uppercase() } else { text });
                Ok(())
            },
        )
        .unwrap();
    runtime
        .register("shout", |text: String| -> Result<String, String> {
            Ok(format!("{text}!"))
        })
        .unwrap();
    let script = runtime
        .compile(
            r#"
fn pair(a: Int, b: Int) -> Int {
    return a * 10 + b
}
fn run(name: String) -> String {
    let result = pair(b: mark(2), a: mark(1))
    mark(result)
    note(name, true)
    note("quiet", false)
    return shout(name)
}
"#,
        )
        .unwrap();

    let result = runtime.call(&script, "run", &["ada".into()]).unwrap();

    assert_eq!(result, Some("ada!".into()));
    assert_eq!(*log.borrow(), ["2", "1", "12", "ADA", "quiet"]);
}

#[test]
fn invalid_calls_are_rejected_before_anything_runs() {
    let calls = Rc::new(RefCell::new(0));
    let mut runtime = Runtime::new();
    let counter = Rc::clone(&calls);
    runtime
        .register("touch", move || -> Result<(), String> {
            *counter.borrow_mut() += 1;
            Ok(())
        })
        .unwrap();
    let script = runtime
        .compile("fn go(n: Int, label: String) {\n touch()\n print(label)\n}")
        .unwrap();
    let foreign = Runtime::new()
        .compile("fn go(n: Int, label: String) {\n}")
        .unwrap();

    let rejected = [
        runtime.call(&script, "go", &[1.into()]),
        runtime.call(&script, "go", &[1.into(), 2.into()]),
        runtime.call(&script, "go", &["1".into(), "x".into()]),
        runtime.call(&script, "touch", &[]),
        runtime.call(&script, "missing", &[]),
        runtime.call(&foreign, "go", &[1.into(), "x".into()]),
    ];

    let messages: Vec<String> = rejected
        .iter()
        .map(|r| r.as_ref().unwrap_err().to_string())
        .collect();
    assert_eq!(
        messages,
        [
            "`go` takes 2 argument(s), but 1 were given",
            "argument `label` of `go` expects String, found Int",
            "argument `n` of `go` expects Int, found String",
            "the script has no function `touch`",
            "the script has no function `missing`",
            "the script was compiled by another runtime",
        ]
    );
    assert_eq!(*calls.borrow(), 0);
    assert_eq!(runtime.take_output(), "");
}

#[test]
fn host_function_mistakes_are_rejected_at_registration_or_compile_time() {
    let mut runtime = Runtime::new();
    let noop = || -> Result<(), String> { Ok(()) };
    runtime.register("noop", noop).unwrap();
    runtime
        .register("twice", |n: i32| -> Result<i32, String> { Ok(n * 2) })
        .unwrap();

    for name in ["", "two words", "if", "struct", "x//", "9lives"] {
        assert_eq!(
            runtime.register(name, noop),
            Err(RegisterError::InvalidName(name.into()))
        );
    }
    for name in ["print", "noop"] {
        assert_eq!(
            runtime.register(name, noop),
            Err(RegisterError::Taken(name.into()))
        );
    }

    let diagnostics = |source: &str| match runtime.compile(source) {
        Err(CompileError::Source(diagnostics)) => diagnostics
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        _ => panic!("expected source diagnostics for {source:?}"),
    };
    assert_eq!(
        diagnostics("fn twice(n: Int) -> Int {\n return n\n}"),
        ["1:4: error: `twice` is a host function and cannot be redefined"]
    );
    assert_eq!(
        diagnostics("fn f() -> Int {\n return twice(true)\n}"),
        ["2:15: error: argument `#1` of `twice` expects Int, found Bool"]
    );
    assert_eq!(
        diagnostics("fn f() -> Int {\n return noop()\n}"),
        ["2:9: error: `noop` returns no value, so its result cannot be used"]
    );
}

#[test]
fn rejected_source_is_reported_as_data_and_the_runtime_keeps_working() {
    let mut runtime = Runtime::new();

    let Err(CompileError::Source(diagnostics)) = runtime.compile("fn broken( {\n}") else {
        panic!("broken source must not compile");
    };
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].span, Span { line: 1, col: 12 });

    let script = runtime.compile("fn ok() -> Int {\n return 1\n}").unwrap();
    assert_eq!(runtime.call(&script, "ok", &[]).unwrap(), int(1));
}

#[test]
fn runtime_failures_are_returned_and_the_next_call_succeeds() {
    let mut runtime = Runtime::new();
    runtime
        .register("check", |n: i32| -> Result<i32, String> {
            if n < 0 {
                return Err(format!("{n} is negative"));
            }
            Ok(n)
        })
        .unwrap();
    let script = runtime
        .compile(
            r#"
fn run(n: Int, d: Int) -> Int {
    print(n)
    let checked = check(n)
    return checked / d
}
"#,
        )
        .unwrap();

    let host = runtime.call(&script, "run", &[(-1).into(), 1.into()]);
    let divide = runtime.call(&script, "run", &[1.into(), 0.into()]);
    let ok = runtime.call(&script, "run", &[6.into(), 3.into()]);

    match host.unwrap_err() {
        CallError::Host {
            function,
            message,
            span,
        } => {
            assert_eq!(
                (function.as_str(), message.as_str()),
                ("check", "-1 is negative")
            );
            assert_eq!(span, Span { line: 4, col: 19 });
        }
        other => panic!("expected a host error, got {other:?}"),
    }
    assert!(matches!(
        divide.unwrap_err(),
        CallError::Panic {
            message: "division by zero",
            span: Span { line: 5, col: 20 }
        }
    ));
    assert_eq!(ok.unwrap(), int(2));
    // Output before each failure is kept, not rolled back.
    assert_eq!(runtime.take_output(), "-1\n1\n6\n");
}

#[test]
fn runaway_loops_and_recursion_stop_deterministically() {
    let ticks = Rc::new(RefCell::new(0));
    let mut runtime = Runtime::new();
    let counter = Rc::clone(&ticks);
    runtime
        .register("tick", move || -> Result<bool, String> {
            *counter.borrow_mut() += 1;
            Ok(true)
        })
        .unwrap();
    let source = String::from(
        r#"
fn spin() {
    while tick() {
    }
}
fn forever() -> Int {
    return forever()
}
fn even(n: Int) -> Bool {
    if n == 0 {
        return true
    }
    return odd(n - 1)
}
fn odd(n: Int) -> Bool {
    if n == 0 {
        return false
    }
    return even(n - 1)
}
"#,
    );
    let script = runtime.compile(&source).unwrap();

    runtime.set_fuel(100);
    let first = runtime.call(&script, "spin", &[]);
    let first_ticks = ticks.replace(0);
    let second = runtime.call(&script, "spin", &[]);
    assert!(matches!(first, Err(CallError::OutOfFuel)));
    assert!(matches!(second, Err(CallError::OutOfFuel)));
    assert!(first_ticks > 0);
    assert_eq!(ticks.replace(0), first_ticks, "fuel must be deterministic");

    runtime.set_fuel(0);
    assert!(matches!(
        runtime.call(&script, "spin", &[]),
        Err(CallError::OutOfFuel)
    ));
    assert_eq!(*ticks.borrow(), 0);

    runtime.set_fuel(u64::MAX);
    assert!(matches!(
        runtime.call(&script, "forever", &[]),
        Err(CallError::Panic {
            message: "call depth limit exceeded",
            span: Span { line: 7, col: 12 }
        })
    ));
    // 10000 active calls, counting the entry call, is the limit.
    assert_eq!(
        runtime.call(&script, "even", &[9999.into()]).unwrap(),
        Some(false.into())
    );
    assert!(matches!(
        runtime.call(&script, "even", &[10000.into()]),
        Err(CallError::Panic {
            message: "call depth limit exceeded",
            ..
        })
    ));
}

#[test]
fn deep_recursion_through_nested_blocks_fits_a_default_thread_stack() {
    // Many nested blocks per call; with deep recursion this would overflow
    // a 2 MiB stack, Rust's default for spawned threads, if blocks or calls
    // recursed in Rust.
    let mut source = String::from("fn dive(n: Int) -> Int {\nif n == 0 {\nreturn 0\n}\n");
    source.push_str(&"if true {\n".repeat(120));
    source.push_str("return dive(n - 1) + 1\n");
    source.push_str(&"}\n".repeat(120));
    source.push_str("return -1\n}\n");

    let dive = std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(move || {
            let mut runtime = Runtime::new();
            runtime.set_fuel(10_000_000);
            let script = runtime.compile(&source).unwrap();
            runtime.call(&script, "dive", &[9999.into()]).unwrap()
        })
        .unwrap()
        .join()
        .unwrap();

    assert_eq!(dive, int(9999));
}

/// Set in the child process of `print_writes_to_runtime_output_not_stdout`.
const CHILD: &str = "CHASM_EMBED_STDOUT_CHILD";
const MARKER: &str = "printed-by-chasm";

#[test]
fn print_writes_to_runtime_output_not_stdout() {
    if std::env::var_os(CHILD).is_some() {
        let mut runtime = Runtime::new();
        let script = runtime
            .compile(&format!("fn shout() {{\n print(\"{MARKER}\")\n}}"))
            .unwrap();
        runtime.call(&script, "shout", &[]).unwrap();
        assert_eq!(runtime.take_output(), format!("{MARKER}\n"));
        return;
    }
    // Rerun just this test in a child process with capture off, so any
    // write to the process's stdout would reach the pipe read here.
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "print_writes_to_runtime_output_not_stdout",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&child.stdout);
    assert!(child.status.success(), "child failed:\n{stdout}");
    assert!(
        stdout.contains("1 passed"),
        "the child must run the test:\n{stdout}"
    );
    assert!(
        !stdout.contains(MARKER),
        "print leaked to stdout:\n{stdout}"
    );
}
