//! Black-box checks of the alpha's documented language and compiler commands.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    directory: PathBuf,
    source: PathBuf,
}

impl Fixture {
    fn new(source: &str) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "chasm-test-{}-{timestamp}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let source_path = directory.join("program with spaces.chasm");
        fs::write(&source_path, source).unwrap();
        Self {
            directory,
            source: source_path,
        }
    }

    fn command(&self, command: &str) -> Output {
        Command::new(env!("CARGO_BIN_EXE_chasm"))
            .arg(command)
            .arg(&self.source)
            .output()
            .unwrap()
    }

    fn native(&self) -> Output {
        let executable = self.directory.join("native with spaces");
        let build = Command::new(env!("CARGO_BIN_EXE_chasm"))
            .arg("build")
            .arg(&self.source)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        success(&build);
        Command::new(executable).output().unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.directory).expect("remove this test's scratch directory");
    }
}

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "status: {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn real_programs_agree_with_their_expected_results() {
    // Proves representative scalar programs preserve their behavior when compiled.
    let cases = [
        ("arithmetic", "fn main() { print(40 + 2) }", "42\n"),
        (
            "recursive factorial",
            "fn factorial(n: Int) -> Int {\nif n <= 1 { return 1 }\nreturn n * factorial(n - 1)\n}\nfn main() { print(factorial(6)) }",
            "720\n",
        ),
        (
            "mutable loop",
            "fn main() {\nlet n = 0\nlet sum: Int = 0\nwhile n < 10 {\nsum = sum + n\nn = n + 1\n}\nprint(sum)\n}",
            "45\n",
        ),
        (
            "short circuit",
            "fn mark(n: Int) -> Bool {\nprint(n)\nreturn true\n}\nfn main() {\nprint(false and mark(1))\nprint(true or mark(2))\nprint(not false)\n}",
            "false\ntrue\ntrue\n",
        ),
        (
            "named argument order",
            "fn mark(n: Int) -> Int {\nprint(n)\nreturn n\n}\nfn pair(a: Int, b: Int) -> Int { return a * 10 + b }\nfn main() { print(pair(b: mark(2), a: mark(1))) }",
            "2\n1\n12\n",
        ),
        (
            "UTF-8 output",
            "fn main() { print(\"Hello, Mia 👋\") }",
            "Hello, Mia 👋\n",
        ),
        (
            "escaped output",
            r#"fn main() { print("\"); puts(\"injected\"); //") }"#,
            "\"); puts(\"injected\"); //\n",
        ),
        (
            "integer boundaries",
            "fn main() {\nprint(-2147483648)\nprint(2147483647)\nprint(-7 / 2)\nprint(-7 % 2)\n}",
            "-2147483648\n2147483647\n-3\n-1\n",
        ),
    ];
    for (name, source, expected) in cases {
        let fixture = Fixture::new(source);
        success(&fixture.command("check"));
        for output in [fixture.command("run"), fixture.native()] {
            success(&output);
            assert_eq!(
                String::from_utf8(output.stdout).unwrap(),
                expected,
                "{name}"
            );
            assert!(output.stderr.is_empty(), "{name}: unexpected stderr");
        }
    }
}

#[test]
fn arithmetic_failures_remain_failures_after_native_compilation() {
    // Proves overflow and invalid division cannot silently wrap or change with compilation.
    for expression in [
        "2147483647 + 1",
        "-2147483648 - 1",
        "-2147483648 / -1",
        "46341 * 46341",
        "7 / 0",
        "7 % 0",
    ] {
        let fixture = Fixture::new(&format!("fn main() {{ print({expression}) }}"));
        let interpreted = fixture.command("run");
        let native = fixture.native();
        assert!(!interpreted.status.success(), "{expression}");
        assert_eq!(
            native.status.code(),
            interpreted.status.code(),
            "{expression}"
        );
        assert_eq!(native.stdout, interpreted.stdout, "{expression}");
        assert_eq!(native.stderr, interpreted.stderr, "{expression}");
        assert!(
            !native.stderr.is_empty(),
            "{expression}: missing native error"
        );
        assert!(
            !interpreted.stderr.is_empty(),
            "{expression}: missing interpreter error"
        );
    }
}

#[test]
fn invalid_or_unsupported_source_has_a_diagnostic() {
    // Proves invalid programs stop at checking instead of reaching generated code.
    for source in [
        "fn main() { print(missing) }",
        "fn main() { let x: Int = true }",
        "fn wrong() -> Int { return true }\nfn main() { print(wrong()) }",
        "fn missing() -> Int {}\nfn main() { print(missing()) }",
        "fn main() { print(2147483648) }",
        "fn main() { print(1) } unexpected",
        "fn main() { let value = &other }",
        "fn main() { print(\"unterminated) }",
    ] {
        let fixture = Fixture::new(source);
        let output = fixture.command("check");
        assert!(!output.status.success(), "accepted: {source}");
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        assert!(
            diagnostic.contains(".chasm:"),
            "missing source location: {diagnostic}"
        );
        assert!(
            !diagnostic.contains("panicked at"),
            "compiler crashed: {diagnostic}"
        );
    }
}

#[test]
fn building_cannot_destroy_the_input_program() {
    // Proves accidentally choosing the source as the output leaves the source intact.
    let source = "fn main() { print(42) }";
    let fixture = Fixture::new(source);
    let output = Command::new(env!("CARGO_BIN_EXE_chasm"))
        .arg("build")
        .arg(&fixture.source)
        .arg("-o")
        .arg(&fixture.source)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(fs::read_to_string(&fixture.source).unwrap(), source);
}

#[test]
fn emitted_c_works_under_optimization_and_undefined_behavior_checks() {
    // Proves the emitted artifact itself works independently of the build command.
    let fixture = Fixture::new(
        "fn fib(n: Int) -> Int {\nif n < 2 { return n }\nreturn fib(n - 1) + fib(n - 2)\n}\nfn main() { print(fib(10)) }",
    );
    let emitted = Command::new(env!("CARGO_BIN_EXE_chasm"))
        .args(["emit", "c"])
        .arg(&fixture.source)
        .output()
        .unwrap();
    success(&emitted);
    let c_source = fixture.directory.join("emitted.c");
    fs::write(&c_source, emitted.stdout).unwrap();
    for (name, flags) in [
        ("optimized", vec!["-O2"]),
        (
            "checked",
            vec!["-O1", "-fsanitize=undefined", "-fno-sanitize-recover=all"],
        ),
    ] {
        let executable = fixture.directory.join(name);
        let compiled = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
            .args(["-std=c11", "-Wall", "-Wextra"])
            .args(flags)
            .arg(&c_source)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        success(&compiled);
        let output = Command::new(executable).output().unwrap();
        success(&output);
        assert_eq!(output.stdout, b"55\n");
        assert!(output.stderr.is_empty());
    }
}
