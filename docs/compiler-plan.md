# Chasm design notes: compiler plan

> **Status:** Implementation plan, 2026-10-26. The [scalar alpha](alpha.md) implements the initial interpreter and C backend. Runtime embedding is a product priority; the wider subset and feasibility gates remain proposals and have not been validated.
>
> A small Rust compiler implements the bootstrap subset, with a reference interpreter over typed IR, a C backend, and a JavaScript backend. A Chasm compiler then replaces the native compiler pass by pass, and a stage0 to stage3 fixed point completes native self-hosting.

Four labels mark how settled each statement is:

- **Recommended** is the default this plan builds on. Mia can overrule it.
- **Open** is a product decision for Mia. Work proceeds on the stated default.
- **Gated** waits for a named condition. No gate has been performed.
- **Hypothesis** is expected but not yet shown.

The plan keeps the language's principles: readable, imperative, data-oriented application code, one semantics on every lane, inferred regions, and no tracing collector in native programs. The subset is in [Bootstrap language](bootstrap-language.md). The failure cases it must close are in the [compiler audit](compiler-audit.md). Claims about other tools link a primary source.

## Four separate pieces

```text
stage0 host (Rust, temporary)
  source → lexer → parser → checker → typed IR → region pass → IR validator
                                                                  ├── reference interpreter (checked mode)
                                                                  ├── C emitter → gcc or clang → native binary
                                                                  └── JavaScript emitter
native binaries link runtime/c: regions, strings, arrays, maps, panic, I/O
```

1. **Compiler host.** Rust writes the first compiler. Chasm replaces it in two milestones.
2. **Native code generator.** Chasm emits C, and gcc or clang produces machine code.
3. **Generated-program runtime.** A small C library links into every native program. The JavaScript lane has its own small runtime module.
4. **Memory.** The host's memory model never reaches Chasm semantics. Native Chasm programs use regions and no tracing collector. The interpreter tracks storage validity itself, so no host collector can keep an expired value readable.

## Compiler host

**Recommended, provisionally: Rust.** The deciding question is which host keeps compiler data in a shape the subset can express, so the port translates code instead of redesigning it. The [compiler audit](compiler-audit.md#bootstrap-language-assessment) reaches the same provisional default, with stable F# as the fallback.

| Host | Checked sum types today | 64-bit integers | Host enforces exclusive mutation | Main cost here |
| --- | --- | --- | --- | --- |
| Rust | `enum` and `match` | native, plus [`checked_add`](https://doc.rust-lang.org/std/primitive.i64.html#method.checked_add) | yes, [one `&mut` or many `&`](https://doc.rust-lang.org/book/ch04-02-references-and-borrowing.html) | slower edit-compile loop, denser syntax |
| F# | yes, [FS0025](https://learn.microsoft.com/en-us/dotnet/fsharp/language-reference/compiler-messages/fs0025) | native | no | list and closure idioms must be avoided by rule |
| C# | only from C# 15, which [ships with .NET 11 in November](https://devblogs.microsoft.com/dotnet/explore-csharp-15/); that SDK is [still prerelease](https://learn.microsoft.com/en-us/dotnet/csharp/whats-new/csharp-15) | native, with [`checked`](https://learn.microsoft.com/en-us/dotnet/csharp/language-reference/statements/checked-and-unchecked) | no | shared mutable object graphs are idiomatic |
| OCaml | yes, [warning 8](https://ocaml.org/manual/latest/comp.html) | `Int64`; the default [`int` is 63-bit and wraps](https://ocaml.org/manual/latest/api/Stdlib.html) | no | `Int64` arithmetic throughout the interpreter |
| TypeScript | via [`never` checks](https://www.typescriptlang.org/docs/handbook/2/narrowing.html#exhaustiveness-checking) | `BigInt`; `number` is a double | no | two integer representations, shared references |
| Go | [no variant types](https://go.dev/doc/faq#variant_types) | native, but [overflow wraps silently](https://go.dev/ref/spec#Integer_overflow) | no | AST passes without exhaustiveness |
| Zig | yes, [exhaustive `switch`](https://ziglang.org/documentation/0.17.0/#Exhaustive-Switching) | native | no | [pre-1.0](https://ziglang.org/download/), with breaking changes in [0.17.0](https://ziglang.org/download/0.17.0/release-notes.html) |

The shortlist is Rust, F#, and C# 15. OCaml and TypeScript work at the cost of awkward integer code in the interpreter. Go lacks exhaustive matching, and Zig would need pinning through breaking releases.

Rust's advantage is a **Hypothesis** about the port, not a safety argument. Safe Rust without `Rc` and `RefCell` rejects shared mutable state, as the subset does. Host code built from owned trees, indices, and `&` or `&mut` parameters should therefore translate pass by pass. Rust does not enforce Chasm's implicit copies, region placement, or second-class borrows, so each translated pass still needs checking. F# can follow the same arrays, indices, and free-function style. C# makes shared mutable node graphs the easy design, and the subset cannot express them.

**Recommended host style.** These preferences keep the host close to the subset. They are not correctness rules.

- Free functions in modules, `struct` and `enum` data, owned values, indices for cross-references.
- `&` and `&mut` parameters only. No `Rc`, `RefCell`, `unsafe`, lifetime parameters, closures, or traits beyond `derive`.
- No `_` arms when matching AST or IR, so a new case breaks every pass that ignores it.
- `Vec` for arrays and an insertion-ordered map, so no output depends on hash order.
- No crates. [`rust-toolchain.toml`](https://rust-lang.github.io/rustup/overrides.html#the-toolchain-file) pins the compiler.

**Gated: the host trial.** Do this before broad host investment, inside slice 3.

1. Write one indexed parser and lowering pass over nested values with borrowed mutation.
2. Add one IR variant. Every semantic consumer must fail to build until it handles the variant.
3. Sketch the same pass in the subset and count the copies its rules would insert.

Switch to stable F# if Rust needs disproportionate host-specific scaffolding. If the subset cannot express the pass, fix the subset before continuing.

## Typed IR and the reference interpreter

**Recommended: interpret the typed IR.** The interpreter is the reference lane that every other lane is compared with.

- Every lane shares the front end and lowering. A lowering that reverses two prints still produces well-typed IR, and the interpreter and both emitters then agree on the wrong output. An independent AST evaluator could catch that. It is optional, not a prerequisite. Hand-written expectations, including source-order and snapshot cases, are the planned guard.
- Runtime scripts and plugins use the embedded typed-IR interpreter before self-hosting. A bytecode VM can later serve the same embedding surface, REPL, and HMR, checked against this interpreter. A JIT is a later optimization gated on measured need; runtime execution requires no external C compiler.
- The IR is structured: blocks, `if`, `loop`, `break`, `continue`, `return`, and tag switches. C, JavaScript, and a later VM all consume structured control flow directly.
- The IR spells out each semantic step. Temporaries fix evaluation order. Copies, moves, borrow reservations, integer checks, and allocation sites are explicit.
- Semantic copies, needed for independence, differ from region transfers, needed only by native storage. The JavaScript lane drops region transfers.

**Recommended: an IR validator before every backend.** A separate, deliberately small checker reruns type, initialization, use-after-move, exclusivity, and region checks on the exact IR each backend consumes. A rejection is a compiler bug and stops the build. The validator cross-checks IR invariants. It does not show that lowering preserved source meaning, and it does not detect every invalid program.

**Recommended: checked mode.** The interpreter tracks logical storage. Storage becomes invalid when its value moves or is overwritten, when its array reallocates, and when its region resets. Any later access traps. The native runtime has a matching debug mode that poisons released blocks. Both cover executed paths only.

A planned regression test injects a malformed lowering that resolves the element address in `xs[i] = growAndReturn(&mut xs)` before the right side runs. Checked mode must trap on the stale address. Poisoning reset arenas cannot expose it, because the retired backing storage sits in a live arena.

## Native backend: C first

**Recommended: emit C11 plus GCC and Clang overflow builtins.** [`__builtin_*_overflow`](https://gcc.gnu.org/onlinedocs/gcc/Integer-Overflow-Builtins.html) is an extension, not ISO C11, so gcc and clang are the supported C compilers.

- Any host, including the self-hosted compiler, writes C as text. No bindings are needed.
- Through clang, generated C reaches LLVM, the backend [rustc uses by default](https://rustc-dev-guide.rust-lang.org/backend/codegen.html). That is a route toward the native-speed goal, not a performance claim. Nim already [compiles to C, C++, or JavaScript](https://nim-lang.org/), close to Chasm's target pair.
- The generated C doubles as the bootstrap seed format.

C has traps of its own, so the emitter follows fixed rules. Section numbers refer to the [C11 draft](https://www.open-std.org/jtc1/sc22/wg14/www/docs/n1570.pdf).

- Signed overflow is undefined behavior (6.5p5). All arithmetic goes through checked helpers built on the overflow builtins. Division and remainder test their operands first.
- C leaves operand and argument evaluation unsequenced (6.5p3, 6.5.2.2p10). The emitter assigns every operand with effects to a temporary, in Chasm order.
- Generated code never compares padding bytes or assumes host layout.
- No `restrict`. Adding it needs a proof about the actual C accesses, including aggregate views and nested backing storage.
- `#line` directives (6.10.4) map generated code back to Chasm source for debuggers.

LLVM and Cranelift are **Gated** on a measured need, such as debug info quality or compile time. LLVM's C API is only ["best effort" for stability](https://llvm.org/docs/DeveloperPolicy.html#c-api-changes), so bindings churn. [Cranelift](https://github.com/bytecodealliance/wasmtime/tree/main/cranelift) is a Rust library, so choosing it would tie the native backend to the temporary host.

## Memory in the subset

[Bootstrap language](bootstrap-language.md#regions-and-promotion) defines the rules. The plan depends on three of them.

- Each call gets a temporary region that resets on every exit. The caller passes a result region for returned data. A `&mut` borrow carries its root's region.
- Region choice changes memory use and no specified observation, provided every placement is valid. Programs cannot read the allocation counters, and resource limits sit outside the guarantee. A wrong placement is a compiler bug that the validator and checked mode aim to catch; neither guarantees detection.
- The runtime counts live regions and bytes allocated, copied, and retained. Tests read these counters.

## Conformance and acceptance gates

Conformance starts in slice 1.

- Each test is a Chasm program with hand-written expected stdout, exit status, and panic text.
- Expectations come from the spec, never from a compiler run, because a bug in the shared front end would otherwise pass. Recommended: review expectation changes separately from implementation changes.
- A portable test passes only when every lane matches it. The lanes are the interpreter in checked mode, native at `-O0` and `-O2` with gcc and clang plus UBSan and ASan, and JavaScript under Node from slice 5. Sanitizers cover executed paths only. Each result records which front end and backend produced it.
- Diagnostic tests hold source plus the expected codes and spans.
- Parity covers specified observations, on supported capabilities, within stated resource limits. One divergence list holds the explicit exceptions, each with a test: stack and memory exhaustion, and process exit status on hosts without processes, such as browsers. The list never relaxes arithmetic, copies, or any other portable observation.
- This file format is an interim harness. First-class behavior contracts are an open track in [Behavior contracts](behavior-contracts.md).

The subset is complete only when every semantic gate passes on every lane:

| Gate | Passes when |
| --- | --- |
| Ownership | Nested copies stay independent. `&mut` mutation reaches only the borrowed root. The snapshot and reservation [proof cases](bootstrap-language.md#proof-cases) pass. A move at last use is unobservable. |
| Lifetime | Each rejected borrow form fails with its own code: stored, returned, overlapping, reserved root mutated by a later argument, and borrowed root touched during `for` or `match`. The corpus and the self-hosted compiler run clean in checked mode and poisoned native mode. |
| Cleanup | After a function exits by return or `try` propagation, including from a loop, the live-region count returns to the caller's baseline plus live result storage. `break` cleans exited scopes without resetting an active call's region. A panic exits with status 101 where the host has processes, and promises no cleanup. |
| Arithmetic | The [boundary table](bootstrap-language.md#arithmetic) matches on every lane, constant folding included. |

### Feasibility gates

These budgets are proposals. Nothing has been measured.

| Gate | Passes when |
| --- | --- |
| Scaling | Parser, checker, and emitter workloads of N, 2N, and 4N nodes, below every lane's stack limit, include a skewed expression tree and a growing symbol map. Native peak arena bytes and total copied bytes grow at most 3× per doubling. Absolute peaks are recorded on a pinned reference machine. |
| Resources | A synthetic resource kind, created only by the test harness, logs its finalizer. Nested moves, replacement, partial construction, early errors, and finalizer dependencies match expected traces. This needs no source syntax and no real handles. |
| Long-lived state | A fixed-size map is overwritten 10,000 and 20,000 times. The second run retains at most one live map's footprint more than the first. The arena-only subset fails this by design. |

Scaling and Resources must pass before the port starts, so source syntax for resources can wait while ownership feasibility cannot. Chasm makes no claim to support long-lived state until Long-lived state passes.

## Thin slices

Each slice runs end to end: parse, check, IR, validator, interpreter, C, runtime, and its own conformance tests.

1. **Forty-two.** `fn main() { print(40 + 2) }` prints `42` on the interpreter and natively. `print(2147483647 + 1)` panics with identical text on both. Token, AST, and IR dumps exist.
2. **Functions and control flow.** Calls, named and positional arguments, defaults, `if`, `while`, `for` over ranges, `Bool`, recursion. The evaluation-order cases pass.
3. **Structs and borrows.** Value semantics, `&` and `&mut` parameters, snapshots, reservations, exclusivity errors. The ownership and lifetime gates pass for structs. The host trial runs here.
4. **Enums and errors.** Payload and recursive enums, exhaustive `match`, `Option`, `Result`, `try`, `panic`. The Resources gate runs here.
5. **JavaScript lane.** The JavaScript emitter passes every test so far. This checks the arithmetic and copy rules on a garbage-collected target before the language grows. Every later slice passes on it too.
6. **Text, arrays, maps, files.** Enough containers and I/O to write a compiler. A word-count program matches on every lane.
7. **Modules and diagnostics.** Multi-file packages, `::` and `#` paths, `std`, and human and JSON diagnostics. The Scaling gate runs here.

**Milestone 1: subset complete.** Every "in the subset" item in [Bootstrap language](bootstrap-language.md#what-first-what-later) works, every semantic gate passes on every lane, and the Scaling and Resources gates pass.

## Runtime embedding track

Runtime scripting and plugins are a core product requirement. Start from the
scalar interpreter and develop embedding alongside the language slices, before
self-hosting. The initial embedding milestone requires a host application to:

- Load and check source at runtime, then call Chasm functions with typed values.
- Register typed host functions and receive returned values and diagnostics.
- Recover control after a script error or Chasm panic without exiting the host.
- Own and release runtime state across repeated loads and calls, with lifetime
  tests for host callbacks and values.

This milestone runs without a C compiler and uses the interpreter. A public
embedding API, bytecode VM, and JIT are unimplemented. The
[interop proposal](interop.md#stable-embedding-api) defines the intended host
operations; its stable C ABI follows validation of the embedding lifecycle.

## Porting to Chasm

The Chasm compiler grows in `compiler/`, one pass at a time. Each step must reproduce the host's output byte for byte on the whole corpus, the compiler's own source included.

1. Lexer: identical token dumps.
2. Parser: identical AST dumps.
3. Checker, lowering, region pass, and validator: identical IR dumps and diagnostics.
4. C emitter and driver: identical C.

Port steps run on the host interpreter, or natively once `chasm0` compiles them. Dumps are canonical text with one item per line, and no addresses or hash-ordered output.

**Milestone 2: native self-hosting.** The completion test below passes. The Chasm compiler replaces the host's front end, validator, C emitter, and driver. The host interpreter and JavaScript emitter stay operational as reference lanes, fed by the host front end, so they do not yet exercise the Chasm front end.

**Milestone 3: host replaced.** Port the interpreter and JavaScript emitter, build them with the Chasm compiler, and rerun the complete suite on every lane. Every supported command and behavior must survive. Then freeze the host.

## Self-host completion test

S is the compiler source in `compiler/`. Each stage compiles S with the previous stage. The reference environment pins the source commit, runtime, C compiler, linker, flags, paths, environment variables, and generated inputs.

| Stage | Compiler | Produces |
| --- | --- | --- |
| stage0 | `chasm0`, the Rust host, built with the pinned toolchain | |
| stage1 | `chasm0` compiles S | `s1.c`, then cc builds `chasm1` |
| stage2 | `chasm1` compiles S | `s2.c`, then cc builds `chasm2` |
| stage3 | `chasm2` compiles S | `s3.c`, then cc builds `chasm3` |

Native self-hosting is complete when all five checks hold. Every comparison is exact, with no semantic normalization. Canonical dumps help locate a difference but never replace the comparison.

1. `s2.c` and `s3.c` are byte-identical. This is the fixed point. [GCC's bootstrap](https://gcc.gnu.org/install/build.html) compares its stage2 and stage3 compilers the same way.
2. `chasm2` and `chasm3` are bit-identical in the reference environment. This follows the [reproducible-builds definition](https://reproducible-builds.org/docs/definition/).
3. `s1.c` and `s2.c` are byte-identical, so the host emitter and the Chasm emitter agree.
4. The host interpreter, running S on S, also emits `s2.c`. This checks the interpreter against native execution on the largest program available.
5. `chasm3` passes the full conformance suite on the native lanes. The interpreter and JavaScript lanes still run, and their results are recorded as host-produced.

A fixed point shows that S builds itself consistently for those inputs. Two stages can share one miscompilation. Tests, sanitizers, and fixed points each establish only what they check.

## Seed and stage zero

- The host stays in the repository. After milestone 3 it accepts only the subset, receives only fixes, and always rebuilds the bootstrap tag.
- `bootstrap/seed/` holds `s3.c`, the C runtime, and a manifest. The manifest records SHA-256 hashes, the source commit, and the pinned reference environment.
- A clean checkout plus that C compiler builds the seed compiler, which must reproduce the seed from S.
- Regenerate the seed only when S needs a feature or fix the old seed lacks. Zig keeps the same rule for [its committed `zig1.wasm`](https://ziglang.org/news/goodbye-cpp/).
- New features land only in the Chasm compiler. S may use one once a regenerated seed supports it. The documented chain runs from the host, through each seed, to the current compiler.
- **Gated:** a [diverse double-compiling](https://dwheeler.com/trusting-trust/) check builds stage2 from both the host and the seed, then compares. It addresses compiler trust, a separate question from correctness.

## After self-hosting

All of this happens in Chasm, and each item has its gate in [Bootstrap language](bootstrap-language.md#what-first-what-later).

1. Resource syntax with move-only ownership and finalizers, on the semantics the Resources gate checked.
2. Floats, user generics and traits, structural projection, closures.
3. Named regions and explicit promotion, once the Long-lived state gate shows the need.
4. A bytecode VM for the embedding surface, REPL, and HMR, checked against the interpreter and gated on measured need.
5. Stable C ABI, JavaScript and Lua interop, and async, each with the audit's gates. Initial interpreter embedding precedes self-hosting.

## Aspirations

These come from the brief. None is demonstrated.

| Aspiration | What the plan does | Evidence plan |
| --- | --- | --- |
| Speed near Rust | C through gcc or clang, flat value layouts, arenas | After milestone 2, benchmark against equivalent Rust programs. Report time, peak memory, and bytes copied. |
| A small design space | A deliberately small subset, and a gate for every addition | The "what later" table in Bootstrap language |
| Strict checking | Exhaustive `match`, no null, no implicit conversions, exclusivity, trapping arithmetic, the IR validator | Conformance and checked mode. Mechanized proofs stay **Gated** until the core stops changing. |

## Open decisions for Mia

**Product priority:** runtime scripting and plugins, alongside native compilation.
Interpreter embedding precedes self-hosting. Rust remains the stage-zero host;
validate its suitability for embedding through the runtime milestone.

1. **Host.** Default Rust, confirmed or replaced by the host trial. Stable F# is the fallback, and C# 15 is an option once .NET 11 ships.
2. **Language defaults.** `Int` width, copyable data versus resources, and second-class borrows are in [Bootstrap language](bootstrap-language.md#open-decisions-for-mia).

## Proposed layout and commands

```text
host/              Rust stage0, no dependencies
compiler/          Chasm compiler source S, grown pass by pass
runtime/c/         chasm_rt.h, chasm_rt.c
runtime/js/        chasm_rt.js
tests/run/         programs with expected output
tests/check/       programs with expected diagnostics
bootstrap/seed/    s3.c, runtime, manifest
```

```text
chasm run <dir> [--checked]       interpret a package
chasm build <dir> -o <file>       emit C and invoke cc
chasm emit c|js <dir>             print generated code
chasm dump tokens|ast|ir <file>   canonical dumps for diffs
chasm test                        run conformance on every lane
```
