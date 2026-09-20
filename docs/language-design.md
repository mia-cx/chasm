# Chasm Language Design Notes

> **Status:** Draft
>
> **Chasm** is a statically typed, data-oriented programming language designed to bridge interpreted development and native deployment.
>
> **One language. One semantic model. Multiple execution backends.**

## Core Philosophy

Chasm is guided by five principles:

1. **Code should be easy to read at a glance.**
2. **Syntax should communicate semantics.**
3. **Data and behavior are separate concepts.**
4. **The runtime should be honest about memory.**
5. **Interpreted and compiled Chasm are the same language.**

## Goals

Chasm aims to be:

- Easy to read and maintain
- Statically typed
- Data-oriented
- Application-focused
- Plugin-friendly
- Interpretable and compilable
- Embeddable

Chasm intentionally avoids:

- Macro-heavy language design
- Excessive boilerplate
- Deep object-oriented inheritance
- Clever syntax that harms readability

## Runtime Model

Chasm source code is compiled into a shared, typed intermediate representation. Multiple execution backends consume that representation.

```text
Source
    ↓
Lexer
    ↓
Parser
    ↓
AST
    ↓
Type Checker
    ↓
Typed IR
    ├── Bytecode VM
    ├── Native Compiler
    ├── JavaScript Compiler
    └── WebAssembly Compiler (future)
```

The bytecode VM powers:

- Scripting
- The REPL
- Hot module replacement (HMR)
- Plugins
- Local development

Native compilation powers:

- Release binaries
- Servers
- Desktop applications
- Command-line tools

JavaScript compilation powers:

- Browser and client-side applications without WebAssembly
- Node.js applications
- JavaScript worker runtimes
- Direct DOM and npm interoperability

All backends implement the same core language and semantic model. Host APIs and low-level capabilities may differ by target.

See [JavaScript Backend](javascript-backend.md) for the detailed design notes.

## Data-Oriented Design

Chasm intentionally separates data from behavior.

Instead of attaching behavior to an object:

```java
enemy.damage(10)
```

Chasm places behavior in namespaces and passes data explicitly:

```chasm
combat#damage(enemy, amount: 10)
```

A namespace associated with a particular data type may also be used:

```chasm
Enemy#damage(enemy, amount: 10)
```

Behavior belongs to namespaces. Data belongs to structs.

### Data and Behavior in Syntax

Runtime data access:

```chasm
player.position.x
```

Behavior lookup and invocation:

```chasm
physics#integrate(world)
```

The difference is intentionally visible in the syntax.

## Syntax Semantics

Each punctuation form has one consistent meaning.

| Symbol | Meaning |
| --- | --- |
| `.` | Runtime data access |
| `::` | Namespace traversal |
| `#` | Callable lookup |
| `()` | Callable invocation |

Examples:

```chasm
player.position.x

std::fs#readText(path)

math#sqrt(value)

engine::renderer#draw(world)
```

These forms can be read as:

- `.` — look inside runtime data
- `::` — continue through namespaces
- `#` — look up a callable
- `()` — invoke the callable

## Modules

Every source file defines exactly one module, and the filesystem determines the module hierarchy.

```text
src/
    math.chasm            → math
    net/
        http.chasm        → net::http
        websocket.chasm   → net::websocket
```

Ordinary source files require no `module` declaration. A file may declare nested modules, but these create nested namespaces rather than additional top-level modules.

A directory may define a curated public API with a dedicated `module.chasm` barrel module. Barrels support explicit exports only; wildcard exports such as `export *` are not allowed.

```chasm
export http#request
export http::Client
```

This allows consumers to use `net#request(...)` and `net::Client` without exposing every declaration from the child module.

See [Modules](modules.md) for the detailed design notes.

## Structs

Structs are plain data.

```chasm
struct User {
    name: String
    age: Int
}
```

> **A struct may define a field as callable data—a value with a function type or shape—but it may never declare or implement a function.**

A callable field remains data stored by the struct; it does not become behavior owned by the struct.

Structs have:

- No hidden methods
- No function declarations or implementations
- No inheritance
- No virtual dispatch

## Traits

Behavior sharing is achieved through traits or interfaces.

Traits describe behavior. Structs describe data.

The exact trait syntax and dispatch model remain to be designed.

## Memory Model

Chasm should stay close to actual memory layout. Struct layout should be predictable and inspectable.

The language should expose operations such as:

```chasm
sizeOf(Type)

alignOf(Type)

offsetOf(Type.field)
```

Potential future layout support includes:

- Array of Structures (AoS)
- Structure of Arrays (SoA)

Chasm uses implicit regional memory with explicit promotion. The compiler places values in the narrowest region that satisfies their use.

Every function conceptually receives a temporary region and a caller-owned result region. Local allocations use temporary memory that is reset once at function exit. Returned or stored values are constructed in, moved into, or explicitly promoted to a wider region. Named regions represent architectural lifetimes such as a request, frame, scene, document, plugin, or application.

```text
expression scratch
    ↓
function temporary region
    ↓
caller/result region
    ↓
named architectural region
    ↓
application region
```

Chasm distinguishes values, safe borrows, and raw pointers:

```text
T          owned local value
&T         borrowed read-only value
&mut T     borrowed mutable value
*T         raw pointer
*mut T     raw mutable pointer
```

Values have independent semantics by default. Read-only and mutable borrows are explicit at the declaration and call site. A borrow may not outlive the region containing its value. Raw pointers are reserved for operations such as FFI, manual memory management, unsafe code, and pointer arithmetic.

Ordinary regional memory is reclaimed in batches without per-value destruction. Files, sockets, mutexes, GPU buffers, foreign handles, and other finalizable resources still receive deterministic cleanup before their region is reset.

The model has no tracing garbage collector. Precise promotion, aliasing, resource-finalization, async, and ABI rules remain to be designed.

See [Regional Memory Management](memory-management.md) for the detailed design notes.

## Function Calls

Chasm strongly encourages named parameters when they improve readability.

Prefer:

```chasm
std::fs#readFile(
    path: "notes.txt",
)
```

Over:

```chasm
std::fs#readFile("notes.txt")
```

### Positional Arguments

Positional arguments remain supported where their meaning is clear:

```chasm
math#max(4, 8)
```

Once a named argument appears, every following argument must also be named.

Valid:

```chasm
image#resize(
    image,
    width: 800,
    height: 600,
)
```

Invalid:

```chasm
image#resize(
    image,
    width: 800,
    600,
)
```

### Named Parameters as API

Parameter names are part of a function's public API. Renaming a public parameter from `timeout` to `duration`, for example, is a breaking change.

### One Conceptual Argument

Internally, every function conceptually takes exactly one argument: a parameter object.

For example:

```chasm
fn readFile(
    path: Path,
    encoding: Encoding = Utf8,
)
```

is equivalent to:

```chasm
struct ReadFileArgs {
    path: Path
    encoding: Encoding = Utf8
}

fn readFile(args: ReadFileArgs)
```

The compiler omits the explicit object-construction syntax at ordinary call sites.

### Structural Parameter Projection

Function parameter objects are structurally typed. If a value contains every field required by a function's parameter type, it may be passed directly.

Given:

```chasm
struct ProjectConfig {
    path: Path
    encoding: Encoding
    retries: Int
}
```

And:

```chasm
fn readFile(config: {
    path: Path
    encoding: Encoding
})
```

This call is valid:

```chasm
std::fs#readFile(projectConfig)
```

`ProjectConfig` satisfies the required shape. Its extra `retries` field is ignored by `readFile`. This allows one project configuration to be consumed by independent tools, each of which projects only the fields it declares.

Structural parameter projection removes many common reasons for function overloading.

### Overloads

Overloads should be uncommon. Named parameters and default values cover most overload use cases.

Instead of defining:

```text
open(path)
open(path, encoding)
open(path, encoding, retries)
```

Prefer one function called in different ways:

```chasm
open(
    path: "...",
)

open(
    path: "...",
    encoding: Utf8,
)

open(
    path: "...",
    retries: 3,
)
```

### Configuration Objects

Named arguments are intended for one-off configuration. Larger, reusable configurations should still use named structs.

```chasm
struct ServerConfig {
    port: Int
    host: String
    compression: Bool
}
```

```chasm
server#listen(config)
```

### Value Semantics and Borrowing

Passing a value gives the function an independent local value by default. Mutating that parameter does not mutate the caller's value.

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

The compiler may optimize away physical copies through moves, copy elision, or similar transformations whenever observable behavior remains unchanged.

Read-only borrowing is explicit:

```chasm
fn inspect(config: &Config)

inspect(&config)
```

Caller-visible mutation requires an explicit mutable borrow from both the function and caller:

```chasm
fn normalize(config: &mut Config) {
    config.lineWidth = 100
}

normalize(&mut config)
```

Mutation should always be visible: `process(config)` does not mutate the caller's value, while `process(&mut config)` may do so.

See [Parameter Passing and Structural Projection](parameter-passing.md) for the detailed design notes.

## Readability

Chasm optimizes for code that explains itself.

Prefer:

```chasm
image#resize(
    image,
    width: 800,
    height: 600,
    filter: Lanczos,
)
```

Over:

```c
resize(image, 800, 600, 2, true, false);
```

## Object-Oriented Programming

Chasm intentionally avoids traditional object-oriented programming.

Chasm does not have:

- Inheritance
- A hidden `this` parameter
- Deep class hierarchies

Instead, it uses:

- Structs
- Traits
- Free functions
- Namespaces

## Interoperability

Chasm should expose one typed module surface over Chasm, native, JavaScript, and Lua implementations.

The initial interoperability layers are:

- A C ABI for native libraries and portable embedding
- Typed JavaScript integration for browsers, Node.js, workers, and npm
- Typed Lua integration for lightweight embedded scripting and plugins

Chasm's structural parameter objects map naturally to JavaScript objects and Lua tables. Portable values may be converted across runtimes, temporary storage may cross as an explicit borrowed view, and objects that retain foreign identity remain typed foreign handles such as `JsValue` or `LuaValue`.

Language boundaries should be syntactically lightweight but remain explicit about type validation, ownership, garbage-collected foreign objects, callbacks, errors, and resource lifetimes.

See [Interoperability](interop.md) for the detailed design notes.

## Plugin System

Compiled Chasm applications should be able to embed the Chasm VM. Applications can then execute Chasm bytecode for:

- Plugins
- Scripting
- Automation
- Modding
- Hot module replacement

The same language used to build an application is also used to extend it.

## Development Workflow

During development:

```text
Source
    ↓
Bytecode
    ↓
VM
```

For production, source may target several backends:

```text
Source
    ├── Native executable
    ├── ECMAScript modules
    └── WebAssembly module (future)
```

There are no core semantic differences between these workflows. Host APIs and low-level capabilities may differ by target.

## Self-Hosting

The initial Chasm compiler will be implemented in C#:

```text
C#
    ↓
Chasm compiler
```

The compiler can eventually be rewritten in Chasm:

```text
Chasm
    ↓
Chasm compiler
```

The proposed bootstrap process is:

1. Build the initial compiler in C#.
2. Rewrite the compiler in Chasm.
3. Use the Chasm compiler to compile itself.
4. Retain the C# compiler only as a bootstrap implementation.

## Design Inspirations

### C#

- Readability
- Tooling
- Developer experience

### Odin

- Data-oriented design
- Predictable memory
- Minimal object orientation

### Rust

- Namespaces
- Traits
- Strong typing

Chasm does not aim to adopt Rust's macro system or syntax density.

### TypeScript

- Structural parameter objects
- Ergonomic APIs

### Lua

- Embeddability

## Open Design Questions

The following topics remain undecided:

- Async model
- Generic implementation
- Reflection
- Package manager
- Error handling
- Borrowing model
- Native ABI
- VM instruction set
- Compile-time evaluation
- Attribute system
- Formatting conventions
