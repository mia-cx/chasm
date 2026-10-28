# Embedding Chasm

> **Status:** prototype on the `0.1.0-alpha.1` scalar language. The API may
> change in any release. It runs trusted host code with untrusted-script
> limits on fuel and call depth only; it is not a sandbox.

The `chasm` crate in `host/` is a library as well as the CLI. A Rust
application compiles Chasm source in memory, registers host functions that
scripts may call, and calls script functions by name. Execution uses the
interpreter, so no C compiler is involved. The language is the one
[the alpha contract](alpha.md) describes.

```rust
use chasm::{CallError, Runtime, Value};

let mut runtime = Runtime::new();
runtime.register("double", |n: i32| -> Result<i32, String> {
    n.checked_mul(2).ok_or_else(|| "too large".into())
})?;

let script = runtime.compile("fn quadruple(n: Int) -> Int {\n return double(double(n))\n}")?;
assert_eq!(runtime.call(&script, "quadruple", &[5.into()])?, Some(Value::Int(20)));

match runtime.call(&script, "quadruple", &[i32::MAX.into()]) {
    Err(CallError::Host { function, message, span }) => { /* "double", "too large", 2:16 */ }
    other => { /* ... */ }
}
```

`host/examples/embed.rs` is a complete program with a stateful wallet
capability, a host error, an arithmetic panic, fuel exhaustion, and a
successful call afterwards. Run it from `host/` with
`cargo run --example embed`.

## API

| Item | Purpose |
| --- | --- |
| `Runtime::new()` | A runtime with no host functions and the default fuel. |
| `runtime.register(name, f)` | Lets scripts compiled afterwards call `f` as `name`. |
| `runtime.compile(source)` | Checks source into a `Script`, or returns `CompileError`. |
| `runtime.call(&script, name, &args)` | Calls a script function; returns `Option<Value>` or `CallError`. |
| `runtime.set_fuel(n)` | Sets the fuel each later call starts with. |
| `runtime.take_output()` | Returns and clears the text `print` has written. |

`Value` is `Int(i32)`, `Bool(bool)`, or `String(String)`, with `From`
conversions from `i32`, `bool`, `String`, and `&str`. A function without a
return type returns `None`. Returned strings are owned by the caller.

### Libraries

`compile` treats source as a library: a list of functions with no `main`
required. `main` is an ordinary function name there. The CLI still requires
`fn main()`.

### Host functions

A host function's Rust signature sets its Chasm signature. Parameters are
`i32` (Int), `bool` (Bool), or `String`, up to four of them. The closure
returns `Result<R, String>`, where `R` is one of those types or `()` for no
value. The compiler checks every script call against that signature, so a
callback only ever receives arguments of its declared types, and its result
type is fixed by Rust.

- Scripts call host functions by bare name, positionally. Host parameters
  have no names; diagnostics call them `#1`, `#2`, and so on.
- `register` rejects names that are not identifiers or are keywords or
  reserved words (`RegisterError::InvalidName`), and `print` or an already
  registered name (`RegisterError::Taken`).
- A script that defines a function with a host function's name does not
  compile.
- Registrations are permanent and append-only. A script's host calls are
  bound when it compiles, and it runs only on the runtime that compiled it
  (`CallError::ForeignScript` otherwise). A function registered later is
  visible only to scripts compiled later.
- An `Err(message)` from a callback stops the script with
  `CallError::Host { function, message, span }`, where `span` is the call site.

### Errors

`CompileError::Source` holds the diagnostics the CLI would print, each with a
`span` (1-based line and column) and a `message`. `CompileError::Internal`
reports a compiler bug.

`CallError` variants:

| Variant | Meaning | Did anything run? |
| --- | --- | --- |
| `UnknownFunction` | The script has no function by that name. | No |
| `Arguments` | Wrong argument count or types. | No |
| `ForeignScript` | Another runtime compiled the script. | No |
| `Panic { message, span }` | `integer overflow`, `division by zero`, or `call depth limit exceeded`. | Yes |
| `OutOfFuel` | The call used up its fuel. | Yes |
| `Host { function, message, span }` | A host function returned `Err`. | Yes |

Errors are values: nothing is printed and the process does not exit. A
failed call's effects stay: host callbacks it already made and output it
already printed are not rolled back. The runtime keeps no other state
between calls, so the next call starts with fresh fuel and an empty call
stack. A rejected source leaves the runtime unchanged.

### Output

`print` appends to a buffer the runtime owns, never to the process's stdout
or stderr. `take_output` returns and clears it. The buffer grows until taken;
fuel bounds how many lines one call can print.

## Execution limits

| Limit | Default | Notes |
| --- | --- | --- |
| Fuel | 1,000,000 per call | One unit per IR statement executed and per loop iteration. Configurable. |
| Call depth | 10,000 active calls | The language's limit, counting the entry call and host calls. Not configurable. |

- **Fuel** is deterministic: the same script, arguments, and host results use
  the same fuel and fail at the same point, including loops whose conditions
  call host functions. The unit depends on the compiler's IR, so counts may
  change between releases. Fuel is not a time limit: it does not run while a
  host callback runs. `set_fuel(0)` fails any call that executes a statement;
  `set_fuel(u64::MAX)` is effectively unlimited, so an infinite loop runs
  until it exhausts that budget or execution is terminated externally.
- **Call depth** past the limit is `CallError::Panic` with
  `call depth limit exceeded`, as in the CLI.
- **Host stack.** The interpreter keeps calls and nested blocks in heap
  vectors, so script recursion and nesting do not grow the Rust stack. The
  compiler passes and IR destruction still use the Rust stack. The
  200-level syntax limit bounds nesting, not stack usage in bytes. The
  integration suite compiles and runs a 120-block recursive function on
  a 2 MiB thread in debug and release; arbitrary small host stacks are
  unsupported.
- **Heap.** Memory grows with the active call depth times each function's
  locals and nesting, plus string sizes and the output buffer. There is no
  memory budget; allocation failure behaves as it does elsewhere in Rust.

## Trust model

Host callbacks are trusted code. They run synchronously on the calling thread,
fuel cannot interrupt them, and they may block, loop, or panic. A callback
panic unwinds out of `Runtime::call`; the runtime holds no partial state, so
a host that catches the panic can keep using it. Callbacks cannot call back
into the runtime, since `call` holds it mutably.

Scripts can affect the world only through registered host functions and
`print`. The runtime does not limit memory or wall-clock time and gives no
process isolation, so do not treat it as a security boundary.

`Runtime` and `Script` are not `Send`: callbacks may capture `Rc` state, and
scripts share string literals through `Rc`. Create and use them on one
thread.

## Not included

Modules and imports, borrows and resources, typed call entry points (`call`
takes `&[Value]`), host functions with more than four parameters,
reentrant callbacks, async or suspended execution, hot reloading, a JIT, and
memory or time budgets.
