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

### Same-name shorthand

**Proposal.** A named argument may omit its value when a visible variable has the same name:

```chasm
http#respond(request, response:)
```

This expands to `http#respond(request, response: response)`. The same rule applies
to named fields in constructor calls. A shorthand argument ends at `,` or `)`.
An unresolved name is an error. Ordinary name resolution, type checking, and
left-to-right evaluation still apply.

A bare argument remains positional. Shorthand does not insert a borrow;
`config:` passes `config` by value. A mutable borrow still needs
`config: &mut config`. The scalar alpha does not implement this shorthand.

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

- The function may mutate caller-owned data.
- The function operates directly on the borrowed value.
- No independent local value exists for that parameter.

### Permission and reborrowing

The caller grants access; the callee's parameter type declares what access it requires.
Access is part of the type:

- `T` gives the callee an owned local value. Ordinary copyable data remains independent of the caller's value.
- `&T` permits reading the caller's value.
- `&mut T` permits mutating the caller's value, subject to exclusivity and lifetime rules.

Local values and by-value parameters are mutable without a separate `mut` declaration.
Resources follow the proposed move-only ownership rules when passed by value.
A read-only borrow cannot be upgraded to a mutable borrow, even for one call.
The compiler rejects passing `&T` to a parameter requiring `&mut T`.

A borrowed parameter names the borrowed place. Passing `&mut parameter` onward
temporarily reborrows that same place; it does not create a reference to a reference.
The reborrow can forward only access already granted by the caller.
An owned local value can also be borrowed mutably for a call.
How many calls need mutation does not change the required parameter type.
See the [bootstrap borrow rules](bootstrap-language.md#borrows-and-exclusivity) for access duration and exclusivity.

### Unnecessary mutable borrow lint

Built-in linting should suggest `&T` when an `&mut T` parameter only needs read access.
Direct writes, writes to fields or elements, and forwarding a mutable borrow count as mutable uses.
The callee's signature determines whether a forwarded borrow needs mutable access;
performing I/O alone does not imply that requirement.
Changing an independent local copy does not count as mutating the borrowed value.
This lint awaits borrow support in the compiler.

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
