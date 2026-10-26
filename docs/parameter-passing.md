# Chasm Design Notes: Parameter Passing and Structural Projection

> **Status:** Draft
>
> Chasm models every function as accepting one parameter object. Ordinary parameter syntax is ergonomic sugar over that model. Values are passed independently by default; borrowing and caller-visible mutation must be explicit.

## One Conceptual Argument

Although functions use ordinary parameter syntax:

```chasm
fn readFile(
    path: Path,
    encoding: Encoding = Utf8,
)
```

they are conceptually equivalent to:

```chasm
struct ReadFileArgs {
    path: Path
    encoding: Encoding = Utf8
}

fn readFile(args: ReadFileArgs)
```

The compiler omits the explicit parameter-object construction syntax. This gives every function a consistent underlying model while preserving pleasant call-site ergonomics.

The model is conceptual and applies once per call. Lowering never wraps a parameter object in another, and a struct argument is never spread across separate parameters, so each call has exactly one meaning.

## Named Arguments

Named arguments are preferred when they make a call easier to understand:

```chasm
std::fs#readFile(
    path: "notes.txt",
)
```

Instead of:

```chasm
std::fs#readFile("notes.txt")
```

Parameter names are part of a function's public API. Renaming a public parameter is therefore a breaking change.

## Positional Arguments

Positional arguments remain supported where their meaning is clear:

```chasm
math#max(4, 8)
```

Once a named argument appears, every remaining argument must also be named.

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

## Structural Parameter Projection

Function parameter objects are structurally compatible. If a value contains every field required by a function's parameter type, it may be passed directly.

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

`ProjectConfig` satisfies the function's required shape. Its additional `retries` field is ignored by `readFile`.

This makes it practical to maintain one project configuration that is consumed by many independent tools:

```chasm
formatter#run(project)

compiler#build(project)

tests#run(project)
```

Each tool projects only the fields it declares.

The [bootstrap subset](bootstrap-language.md#structural-projection) proposes the precise rule: projection targets only anonymous record parameter types, copies exactly the selected fields with no layout cast, and leaves unused fields with their owner. Named structs stay nominal.

## Value Semantics by Default

Passing a value normally gives the function an independent local value:

```chasm
fn normalize(config: Config) {
    config.lineWidth = 100
}
```

```chasm
let config = Config(
    lineWidth: 80,
)

normalize(config)

print(config.lineWidth)
// 80
```

Mutating a value parameter affects only the function's local value. It does not mutate the caller's value.

This is a semantic guarantee, not a requirement to perform a physical copy. The compiler may use moves, copy elision, or other optimizations whenever observable behavior remains identical.

A proposed refinement limits the guarantee to ordinary copyable data and gives resources move-only ownership. The [bootstrap subset](bootstrap-language.md#borrows-and-exclusivity) also defines when a value argument is snapshotted relative to borrowed arguments.

## Explicit Read-Only Borrowing

A function can borrow existing data without taking ownership:

```chasm
fn inspect(config: &Config)
```

The caller must borrow the value explicitly:

```chasm
inspect(&config)
```

A read-only borrow cannot mutate the borrowed value.

## Explicit Mutable Borrowing

Caller-visible mutation requires explicit opt-in from both the function and the caller:

```chasm
fn normalize(config: &mut Config) {
    config.lineWidth = 100
}
```

```chasm
normalize(&mut config)
```

This syntax communicates that:

- The function mutates caller-owned data.
- The function operates directly on the borrowed value.
- No independent local value exists for that parameter.

## Parameter and Pointer Forms

Chasm distinguishes safe values and borrows from raw pointers:

```text
T          owned local value
&T         borrowed read-only value
&mut T     borrowed mutable value
*T         raw pointer
*mut T     raw mutable pointer
```

`&T` and `&mut T` are safe language concepts. They carry guarantees that raw pointers do not.

Raw pointers exist separately for:

- Foreign function interfaces (FFI)
- Manual memory management
- Unsafe code
- Pointer arithmetic

The precise lifetime and aliasing rules for borrows remain to be designed.

## Guiding Philosophy

Mutation should always be visible.

```chasm
process(config)
```

This call does not mutate the caller's value.

```chasm
process(&mut config)
```

This call may mutate the caller's value.

Chasm should have no hidden reference semantics and no accidental object mutation. The syntax should communicate ownership and intent directly.
