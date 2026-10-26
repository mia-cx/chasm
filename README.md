# Chasm

> Status: `0.1.0-alpha.1` — a scalar compiler alpha alongside the language proposal.

Chasm is a statically typed, data-oriented programming language with one semantic model across interpreted and compiled execution.

## Try the alpha

Install [Rust](https://rustup.rs/) and GCC or Clang. The repository pins the
Rust toolchain; native builds use `cc`, or the program named by `CC`.

```sh
cargo run -- run examples/hello.chasm
cargo run -- check examples/factorial.chasm
cargo run -- build examples/factorial.chasm -o factorial
./factorial
```

The alpha checks, interprets, emits C, and builds native executables from a
single source file. It supports typed functions, mutable locals, `if`/`else`,
`while`, recursion, named arguments, checked 32-bit integers, booleans, and
string literals. See the [alpha contract](docs/alpha.md) for the implemented
syntax, commands, and limits, and [examples/](examples/) for runnable programs.

```sh
cargo test --workspace
```

Borrowing, regions, resources, modules, JavaScript, and self-hosting remain
future work. The sections below describe the broader language proposal.

## Language proposal

Its syntax makes structural relationships visible. `::` traverses namespaces, `#` looks up callables, and `.` accesses runtime data. Values are independent by default. Borrowing and caller-visible mutation are explicit. Compiler-inferred memory regions provide predictable reclamation without requiring a tracing garbage collector.

This proposal is the entry point for the language. The detailed design notes in [`docs/`](docs/) are canonical. A retained summary of the lost July 15, 2026 conversation supplies provenance and additional decisions. Where the sources conflict, this proposal follows the canonical notes and records the discrepancy.

## Implementation research

The alpha implements an initial scalar slice. These proposals define the work
toward the bootstrap subset; their additional language rules remain recommendations.

- [Compiler plan](docs/compiler-plan.md): bootstrap host, execution backends, milestones, and self-hosting.
- [Bootstrap language](docs/bootstrap-language.md): proposed subset and semantic acceptance cases.
- [Feasibility audit](docs/compiler-audit.md): counterexamples and conditions the implementation must satisfy.
- [Bend lessons](docs/bend-lessons.md): useful mechanisms and their trust boundaries.
- [Behavior contracts](docs/behavior-contracts.md): the open proposal for a test-driven language.

[Project direction](DIRECTION.md) records current priorities.

## Goals

Chasm should be:

- Easy to read at a glance
- Statically typed
- Data-oriented
- Application-focused
- Plugin-friendly
- Interpretable and compilable
- Embeddable
- Honest about memory and resource lifetimes

Chasm intentionally avoids:

- Hidden mutation
- Hidden receiver semantics
- Deep inheritance hierarchies
- Macro-heavy language design
- Different core semantics between development and release builds
- Routine allocator plumbing in application code

## One language, multiple backends

Chasm source is lowered into a shared typed intermediate representation:

```text
Source
    ↓
Lexer
    ↓
Parser
    ↓
AST
    ↓
Type checker
    ↓
Typed IR
    ├── Bytecode VM
    ├── Native compiler
    ├── JavaScript compiler
    └── WebAssembly compiler (future)
```

The bytecode VM supports scripting, the REPL, plugins, hot module replacement, and local development. Native compilation supports release binaries, servers, desktop applications, and command-line tools. JavaScript compilation supports browsers, Node.js, and worker runtimes without requiring WebAssembly.

Every backend implements the same evaluation order, numeric behavior, value semantics, mutation rules, errors, and cleanup behavior. Host APIs and low-level capabilities may differ by target, but those differences must be explicit.

The [compiler plan](docs/compiler-plan.md) recommends an interpreter over typed IR
as the initial semantic reference, with the bytecode VM added later. A shared
conformance suite should compare observable behavior across backends.

## Structural syntax

Access and call expressions use distinct punctuation:

| Syntax | Meaning |
| --- | --- |
| `.` | Access runtime data. |
| `::` | Traverse a namespace. |
| `#` | Look up a callable. |
| `()` | Invoke a callable. |

```chasm
player.position.x

std::fs#readFile(
    path: "notes.txt",
)

engine::renderer#draw(world)
```

These distinctions are semantic, not decorative. In expressions, a reader and parser can tell whether code traverses structure, selects behavior, reads data, or invokes a callable.

## Data and behavior

Structs contain data:

```chasm
struct User {
    name: String
    age: Int
}
```

Behavior lives in namespaces and receives data explicitly:

```chasm
combat#damage(enemy, amount: 10)
```

A namespace associated with a type may group its behavior:

```chasm
Enemy#damage(enemy, amount: 10)
```

Structs have no methods, inheritance, virtual dispatch, or hidden `this` parameter. A struct may store a function value, but that callable remains data.

Traits or interfaces describe shared behavior. Their syntax and dispatch model remain open.

## Modules

Each source file defines exactly one module. The filesystem defines the module hierarchy:

```text
src/
    math.chasm             -> math
    net/
        http.chasm         -> net::http
        websocket.chasm    -> net::websocket
```

Ordinary source files need no module declaration. A file may declare nested namespaces without creating another top-level module.

A directory may provide a curated public API through `module.chasm`:

```text
net/
    http.chasm
    websocket.chasm
    module.chasm
```

```chasm
export http#request
export http::Client
export websocket#connect
```

Consumers may then use `net#request(...)`, `net::Client`, and `net#connect(...)`. Barrel modules allow explicit exports only. Wildcard exports are not supported.

The retained conversation also records `.chsm` as a source extension. The canonical notes use `.chasm` exclusively. Whether `.chsm` is an alias or has a separate role remains open.

See [Modules](docs/modules.md).

## Calls and parameter objects

Every function conceptually receives one parameter object. Ordinary parameter syntax is sugar over that model. Named call arguments initialize fields of the conceptual object:

```chasm
fn readFile(
    path: Path,
    encoding: Encoding = Utf8,
)
```

Conceptually matches:

```chasm
struct ReadFileArgs {
    path: Path
    encoding: Encoding = Utf8
}

fn readFile(args: ReadFileArgs)
```

Positional arguments are supported when their meaning is clear:

```chasm
math#max(4, 8)
```

Named arguments are part of the public API:

```chasm
std::fs#readFile(
    path: "notes.txt",
    encoding: Utf8,
)
```

A positional prefix may be followed by named arguments:

```chasm
image#resize(
    image,
    width: 800,
    height: 600,
)
```

Once a named argument appears, every remaining argument must be named. A positional argument after a named argument is invalid.

The retained conversation summary recorded a stricter rule where a call must be entirely positional or entirely named. The canonical notes take precedence, so the positional-prefix form above is the current proposal.

## Structural parameter projection

The recovered “type projection” decision is structural parameter projection.

If a value contains every field required by a function's parameter object, it may satisfy that parameter shape directly:

```chasm
struct ProjectConfig {
    path: Path
    encoding: Encoding
    retries: Int
}

fn readFile(config: {
    path: Path
    encoding: Encoding
})

let projectConfig = ProjectConfig(
    path: "notes.txt",
    encoding: Utf8,
    retries: 3,
)

std::fs#readFile(projectConfig)
```

`ProjectConfig` supplies the required `path` and `encoding` fields. `readFile` ignores its additional `retries` field.

This lets independent tools project the parts they need from one larger configuration:

```chasm
formatter#run(project)
compiler#build(project)
tests#run(project)
```

Structural projection makes parameter objects compatible by shape. It does not decide whether structural typing applies elsewhere.

See [Parameter Passing and Structural Projection](docs/parameter-passing.md).

## Value semantics and borrowing

Passing `T` gives a function an independent local value. Mutating that parameter does not mutate the caller's value:

```chasm
fn normalize(config: Config) {
    config.lineWidth = 100
}

let config = Config(
    lineWidth: 80,
)

normalize(config)
print(config.lineWidth)
// 80
```

This is an observable guarantee, not a requirement to perform a physical copy. The compiler may use moves, copy elision, uniqueness analysis, or copy-on-write storage when behavior remains unchanged.

Read-only borrowing is explicit:

```chasm
fn inspect(config: &Config)
inspect(&config)
```

Caller-visible mutation requires an explicit mutable borrow in both the function and the call:

```chasm
fn normalize(config: &mut Config) {
    config.lineWidth = 100
}

normalize(&mut config)
```

Chasm distinguishes safe language values and borrows from raw pointers:

```text
T          owned local value
&T         borrowed read-only value
&mut T     borrowed mutable value
*T         raw pointer
*mut T     raw mutable pointer
```

Raw pointers are reserved for FFI, manual memory work, unsafe code, and pointer arithmetic. Precise borrow aliasing and lifetime rules remain open.

## Memory and resources

Chasm semantics do not require a tracing garbage collector. Native and VM backends reclaim memory through scopes and regions. A garbage-collected host such as JavaScript may still perform the physical reclamation of host objects.

The compiler places values in the narrowest region that satisfies their use.

Every function conceptually receives two allocation targets:

```text
temporary region    dies when the function returns
result region       belongs to the caller
```

Locals use temporary memory. Returned or stored values are constructed in, moved into, or promoted to a wider region. Named regions represent architectural lifetimes such as a request, frame, scene, document, plugin, or application.

One possible named-region syntax is:

```chasm
region scene {
    let meshes = assets#loadScene(path: path)
    renderer#run(meshes)
}
// The scene region is released here.
```

Named-region and promotion syntax remain open. Most code does not name regions. Explicit promotion appears only when the destination lifetime cannot be inferred safely.

Promotion must not hide an unexpectedly expensive deep copy. The compiler must construct directly in the destination, perform an explicitly defined move or reconstruction, or reject the escape.

Ordinary regional memory is reclaimed in batches. Files, sockets, mutexes, GPU buffers, and foreign handles still receive deterministic cleanup. Destroying a region runs its resource finalizers before releasing its memory blocks.

A borrow may not outlive its value's region. A promoted value may not retain references into a narrower region unless those values are promoted too.

See [Regional Memory Management](docs/memory-management.md).

## Functions as values

The retained conversation records first-class function types. Functions can be stored, passed as arguments, and returned. The canonical notes also allow structs to contain callable data without gaining methods.

The exact function-type, closure, capture, and callable-value invocation syntax remains open. In particular, the language must decide whether a stored callable uses ordinary `()`, `#call(...)`, or another form.

## Interoperability and plugins

Chasm presents one typed module model over Chasm, native, JavaScript, and Lua implementations.

The initial external interfaces are:

- A C ABI for native libraries and portable embedding
- Typed JavaScript host contracts for browsers, Node.js, workers, and npm
- Typed Lua host contracts for embedded scripting and plugins

Values cross runtime boundaries as portable copies, temporary borrowed views, or explicit foreign handles such as `JsValue` and `LuaValue`. Foreign exceptions, rejected promises, Lua errors, and C status codes must become Chasm errors at the boundary. No incompatible exception or panic may unwind across an ABI.

Compiled Chasm applications may embed the VM and load Chasm bytecode plugins. JavaScript and Lua plugins use the same typed contracts. Every plugin receives explicit capabilities and owns a region or runtime context.

Untrusted plugins require process isolation and resource controls. Loading Node.js code does not make it sandboxed.

See [Interoperability](docs/interop.md) and [JavaScript Backend](docs/javascript-backend.md).

## Core grammar fragment

This fragment validates the structural syntax and call forms used in this proposal. It is not a complete grammar.

```ebnf
namespace_path = identifier, { "::", identifier } ;
callable_lookup = namespace_path, "#", identifier ;
named_callable = identifier | callable_lookup ;
data_access = expression, ".", identifier ;
call = named_callable, "(", [ arguments, [ "," ] ], ")" ;

arguments = positional_arguments, [ ",", named_arguments ]
          | named_arguments ;
positional_arguments = expression, { ",", expression } ;
named_arguments = named_argument, { ",", named_argument } ;
named_argument = identifier, ":", expression ;

read_borrow = "&", expression ;
mutable_borrow = "&mut", expression ;
```

Bare identifiers in `named_callable` refer to declared functions. Invocation through stored callable values remains open. The full grammar still needs rules for declarations, types, literals, operators, blocks, control flow, and callable values.

## Tradeoffs

- Distinct separators make relationships legible but introduce unfamiliar punctuation.
- Value semantics prevent hidden mutation but require strong copy-elision and uniqueness analysis.
- Compiler-inferred regions remove routine allocator work but make escape analysis central to correctness.
- One semantic model simplifies reasoning but limits backend-specific shortcuts.
- Filesystem modules make navigation obvious but constrain generated modules and unusual source layouts.
- Structural parameter projection reduces wrappers but makes public parameter names part of compatibility.
- Typed interop improves reach but expands the lifetime, error, resource, and sandboxing design.

## Open decisions

The language direction is established. These areas still need precise specifications:

1. Traits, generics, algebraic data types, pattern matching, and nullability.
2. Function types, closures, captures, and callable-value invocation.
3. Borrow aliasing, lifetime inference, ownership graphs, cycles, and promotion costs.
4. Errors, panics, stack traces, and propagation.
5. Async execution, cancellation, concurrency, and thread safety.
6. Imports, package boundaries, initialization, and `.chsm` behavior.
7. Native ABI, bytecode format, VM instruction set, and compile-time evaluation.
8. Reflection, attributes, package management, formatting, testing, documentation, and debugging.
9. Validation of the proposed Rust bootstrap and staged port to a self-hosted Chasm compiler.

Every decision should preserve the central promise: Chasm code makes structure, behavior, data access, ownership, and mutation visible where they happen.

## Detailed notes

- [Language design](docs/language-design.md)
- [Parameter passing and structural projection](docs/parameter-passing.md)
- [Modules](docs/modules.md)
- [Regional memory management](docs/memory-management.md)
- [JavaScript backend](docs/javascript-backend.md)
- [Interoperability](docs/interop.md)
