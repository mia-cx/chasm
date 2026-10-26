# Chasm design notes: bootstrap language

> **Status:** Draft proposal, 2026-10-26
>
> The bootstrap subset is the smallest coherent Chasm that can express its own compiler. Every program in it has the same specified observations on the interpreter, native, and JavaScript lanes, within stated resource limits. Everything outside it waits behind a named gate.

The labels match the [compiler plan](compiler-plan.md). **Recommended** is the default, **Open** is Mia's product decision with a working default, **Gated** waits for a condition, and **Hypothesis** is unshown. All syntax here is proposed. The [compiler audit](compiler-audit.md) supplies the failure cases these rules answer. The subset restricts the full design in the other notes for the bootstrap; it does not replace that design.

## A first look

```chasm
enum Expr {
    Number(value: Int)
    Add(left: Expr, right: Expr)
    Negate(inner: Expr)
}

fn eval(expr: &Expr) -> Int {
    return match expr {
        Expr::Number(value) => value
        Expr::Add(left, right) => eval(&left) + eval(&right)
        Expr::Negate(inner) => -eval(&inner)
    }
}

fn main() {
    let expr = Expr::Add(
        left: Expr::Number(value: 40),
        right: Expr::Number(value: 2),
    )
    print(eval(&expr))
}
```

Every borrow is visible at the call, `match` must cover every variant, and `+` traps on overflow.

## What first, what later

| Area | In the subset | Later, behind a gate |
| --- | --- | --- |
| Data | `struct`, `enum` with payloads, recursive enums | anonymous record types with structural projection |
| Scalars | `Bool`, `Int`, `I64`, `U8` | floats, once cross-lane formatting is specified; other widths; bitwise and wrapping operators |
| Containers | built-in `String`, `Array<T>`, `Map<K, V>`, `Option<T>`, `Result<T, E>` | user-defined generic types |
| Behavior | free functions in modules, named and positional arguments, defaults | user generics and traits; closures and function values |
| Ownership | copies, moves at last use, second-class `&` and `&mut` | borrow-returning functions; resources with finalizers |
| Memory | inferred call regions | named regions and explicit promotion |
| Errors | `Result`, `try`, `panic` | panic behavior once resources exist |
| Modules | file and directory modules, absolute paths, `std` | imports, visibility, `module.chasm` barrels, packages |
| State | module-level literal constants | mutable globals, recommended never |
| Outside world | `print`, file read and write, process arguments, exit status | raw pointers, C ABI, JavaScript and Lua interop, async |
| Testing | conformance files outside the language | first-class [behavior contracts](behavior-contracts.md), an open design track |

An item leaves the right column only with three things: a written rule, proof cases passing on every lane, and a concrete program that needs it.

## Values, copies, and moves

**Recommended.**

- Every subset type is plain copyable data. Assignment, passing, returning, and storing produce an independent value. Limiting that promise to copyable data, with resources separate, is a proposal ([open decision 2](#open-decisions-for-mia)).
- Values are trees. The subset has no references inside values, so it cannot build sharing or cycles.
- The compiler turns a copy into a move when the source local has no later use. No read can see the difference.
- Every local is mutable inside its function. Value semantics keeps that mutation invisible elsewhere.

**Gated: resources.** A type that owns a file, socket, foreign handle, or similar is a resource. Resources move and never copy, and use after move fails to compile. A struct, container, or closure holding a resource is a resource too. Scope exit finalizes unmoved resources, and a move transfers that duty exactly once. Finalizers run in an order that keeps each one's dependencies alive. Reverse registration order alone does not establish that once moves reorder registrations. The plan's [Resources gate](compiler-plan.md#feasibility-gates) checks these semantics with a synthetic resource before the port. Source syntax waits for the audit's cleanup cases, with matching finalizer traces on every lane.

## Borrows and exclusivity

**Recommended: borrows are second-class.**

- `&T` and `&mut T` appear only as parameter types. `&place` and `&mut place` appear only as call arguments, as the target of `for`, and after `match`.
- `match` bindings borrow parts of the matched place. They are read-only unless the code says `match &mut place`.
- No variable, field, or container can hold a borrow, and no function returns one. So no borrow outlives its call or block, and no lifetime syntax exists. The [mutable value semantics paper](https://www.jot.fm/issues/issue_2022_02/article2.pdf) listed by the Hylo project calls this the strictest form, where references "are only created implicitly, at function boundaries, and cannot be stored in variables or object fields."
- A borrow binding names the borrowed place. Reading it copies the value, and assigning through a `&mut` binding writes the place. Passing it on needs `&` or `&mut` again, so every caller-visible mutation shows `&mut` at the call.

**Recommended: exclusivity.** Two accesses to one place may overlap only when both read. This is Swift's [Law of Exclusivity](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0176-enforce-exclusive-access-to-memory.md), checked statically here:

- In a call, a `&mut` argument may not overlap any other borrowed argument.
- A value argument is a snapshot taken when its expression evaluates. Later arguments and the callee's borrows cannot change it.
- A borrowed argument reserves its place when it evaluates, and its access starts when the call runs. Later argument expressions may read from a reserved root, but may not mutate or move it.
- A reserved or active borrow counts as a later use of its root, so no move can consume that root.
- Places overlap when one is a prefix of the other. Different fields of one struct are disjoint. Two elements of one array always count as overlapping, and `Array#swap` covers the common case.
- While a `for` or `match` borrow is live, its root is reachable only through that borrow. So no array can reallocate under a live element borrow.
- No mutable globals exist, so a callee reaches caller state only through its parameters.

**Gated:** functions that return a borrow derived from one parameter, such as element accessors. The gate is a real need in the compiler source, plus a rule that works across separate compilation.

## Regions and promotion

**Recommended for the subset.**

- No region syntax exists. A compiler pass puts each local root in the call's temporary region, or in the caller's result region when its value can reach the return.
- A place lives in its root's region. A `&mut` borrow carries that region, so a store through it allocates where the borrowed value lives.
- When a value must cross regions and the compiler could not build it in place, it is copied. A copy costs time proportional to the value's size. Values are trees, so no graph reconstruction ever happens.
- `chasm explain copies` (proposed) lists every copy the compiler inserted, with its source span.
- Borrows cannot escape. With valid placements, region choice changes memory use and no specified observation. The validator and checked mode aim to catch a wrong placement; neither guarantees detection.
- Signatures carry no region information. Every function places its results in the caller's result region, so separate compilation needs nothing more.

**Known limit.** Arenas free nothing individually. In `while running { state = step(state) }`, each old `state` stays allocated until the enclosing call returns. A call is the subset's explicit lifetime boundary, so moving a loop body into a function bounds its temporaries. Replaced values still accumulate in the caller's region, so a long-lived map overwritten repeatedly retains every old value. A recursive parser's fallback copy-out can also copy successively larger subtrees unless destination propagation avoids it. That a batch compiler stays within budget is a **Hypothesis**, tested by the plan's [feasibility gates](compiler-plan.md#feasibility-gates) before the port. MLKit, a region-inference compiler, lets [tracing collection augment its regions](https://github.com/melsman/mlkit) for better memory behavior. Chasm excludes tracing collection, so it needs another answer.

**Gated: named regions and promotion.** This covers `region frame { ... }` blocks and explicit promotion syntax. The gate is the plan's Long-lived state measurements, the audit's sibling-region cases, and Mia's choice of syntax. Mia also decides how a deep promotion is acknowledged in source.

## Errors and panics

**Recommended.**

- `Result<T, E>` carries recoverable failure. `try expr` yields the `Ok` value or returns the `Err` from the enclosing function. The error types must match exactly.
- A panic is unrecoverable. Explicit `panic(message:)`, arithmetic traps, out-of-bounds indexing, and unwrapping `None` all panic.
- A panic prints `panic: <message>` and its source location to stderr, then exits with status 101. The interpreter, native code, and JavaScript under Node behave the same. Hosts without processes, such as browsers, report panics through a target contract on the divergence list.
- A panic runs no cleanup. The subset has no resources, so nothing observable is skipped.

**Gated:** panic behavior once resources exist. The choice is abort without finalizers, or unwind and finalize. Plugin isolation depends on it.

## Arithmetic

**Recommended.**

- `Int` is a signed 32-bit integer, an **Open** default explained below. `I64` and `U8` complete the subset.
- `+`, `-`, `*`, and unary `-` trap on overflow, on every lane and in every build. Swift [traps on overflow](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/advancedoperators/) the same way.
- `/` and `%` truncate toward zero. Division by zero and `MIN / -1` trap.
- A unary minus directly before a literal belongs to the literal, so `-2147483648` is a valid `Int`.
- A literal takes its expected type, otherwise `Int`. An out-of-range literal fails to compile.
- No implicit conversions exist. Conversion functions trap when the value does not fit, and mixed-type operands fail to compile.
- `checkedAdd` and its siblings return `Option` for code that handles overflow itself, such as the compiler's literal parser.
- Constant folding never changes behavior. An expression that would trap is left to trap at run time.

The JavaScript lane computes `Int` in doubles plus one range check. The double product of two `Int` values is exact whenever the true product fits, so the check is exact. `I64` uses `BigInt`.

The boundary table is a proof case on every lane:

| Expression | Type | Result |
| --- | --- | --- |
| `2147483647 + 1` | `Int` | panic: integer overflow |
| `-2147483648 - 1` | `Int` | panic: integer overflow |
| `-2147483648 / -1` | `Int` | panic: integer overflow |
| `46341 * 46341` | `Int` | panic: integer overflow |
| `46340 * 46340` | `Int` | `2147395600` |
| `7 / 0` and `7 % 0` | `Int` | panic: division by zero |
| `-7 / 2` | `Int` | `-3` |
| `-7 % 2` | `Int` | `-1` |
| `9223372036854775807 + 1` | `I64` | panic: integer overflow |
| `255 + 1` | `U8` | panic: integer overflow |

**Open: `Int` width.** The default is 32-bit. A 64-bit `Int` makes every default integer on the JavaScript lane a `BigInt` or an emulated pair. Dart's web integers [lose precision past 53 bits](https://dart.dev/resources/language/number-representation) instead, a divergence Chasm's one-semantics rule forbids. A 32-bit `Int` caps counts and indices at 2,147,483,647, and large offsets need explicit `I64`.

## Evaluation order

**Recommended: left to right, always.**

- Operands evaluate left to right.
- Call arguments evaluate in the order written, named or positional. Defaults for omitted parameters evaluate next, in declaration order. Then the call runs.
- Assignment evaluates the target's index expressions, then the right side, then resolves the element address, checks bounds, and stores. The store's access starts after the right side finishes, so the right side may borrow the target's root, and a right side that grows the array stores into its new backing storage.
- `and` and `or` short-circuit.
- Each supplied expression and each selected default evaluates exactly once.

## Generics and traits

**Recommended.**

- Only the built-in generic types exist: `Array`, `Map`, `Option`, and `Result`. Go likewise had only built-in generic slices and maps until [user generics arrived in Go 1.18](https://go.dev/doc/go1.18).
- `==` works on integers, `Bool`, `String`, and enums without payloads. Other comparisons are ordinary functions.
- `Map` keys are integers or `String`. Maps iterate in insertion order, so output never depends on hashing.

**Gated: user generics and traits.** The default plan monomorphizes for native and JavaScript, starts with static dispatch, and keeps dynamic dispatch on explicit callable fields. The gate is duplicated code in the compiler source that generics would remove, plus a trait syntax Mia approves.

## Structural projection

**Recommended rule. Gated feature.**

- Projection applies only where a parameter's type is an anonymous record type, such as `{ path: Path, encoding: Encoding }`. A value with at least those fields, under the same names and types, converts by copying exactly those fields. No layout cast occurs.
- Named structs stay nominal. A call never spreads a struct across separate parameters, so each call has exactly one meaning.
- The `readFile(projectConfig)` example in [Parameter passing](parameter-passing.md) works as written under this rule.
- Projection selects fields before ownership rules apply, so fields the callee never names keep their owner.
- A projected `&mut {...}` parameter may use copy-in copy-out, as Swift specifies for [in-out parameters](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/declarations/#In-Out-Parameters). Exclusivity hides the copy while a panic still ends the process.

The compiler needs no projection, so the gate is self-hosting plus the audit's projection cases.

## Modules, names, and syntax

**Recommended.**

- A file is a module and a directory is a namespace, as in [Modules](modules.md). The package root is the directory given to `chasm`.
- Paths are absolute from the package root, such as `net::http#request`. `std` is reserved. The subset has no `import`, and every top-level declaration is visible across the package.
- Unqualified names resolve to locals, then the current module, then the prelude: `print`, `Some`, `None`, `Ok`, `Err`.
- Variants are qualified, as in `Encoding::Utf8`. The bare `Utf8` shorthand in the existing notes is **Gated**, since it needs expected-type lookup.
- Module-level `let` holds literal constants only. No module runs initialization code, so module cycles are harmless and initialization order never arises.
- Construction uses call syntax on the type or variant: `Config(lineWidth: 80)`, `Expr::Number(value: 40)`.
- Newlines end statements and declarations by [Go's rule](https://go.dev/ref/spec#Semicolons). A terminator follows a line's last token when that token can end a statement. The multi-line examples in the existing notes end lines with `(`, `{`, `,`, or a complete item, so they parse unchanged.
- `and`, `or`, and `not` are the logical operators, which leaves `&` to borrows alone.

## Diagnostics

**Recommended.**

- Each diagnostic has a stable code, a primary span with file, line, and column, a message, and optional notes.
- Output comes as human text or JSON (proposed `--diagnostics json`).
- Three verdicts stay distinct: the program is wrong, the input is not supported yet, and the compiler failed. Unsupported input fails closed, as in [Bend lessons](bend-lessons.md).
- The checker reports every error in a module. The parser stops at the first syntax error in a file, and recovery comes later.
- The host and the Chasm compiler must produce identical diagnostics on the corpus.

## Runtime ABI

**Recommended. Private and unstable.**

- Scalars pass by value. Aggregates pass as pointers. The caller passes a snapshot when the source could change before the call returns: it overlaps a `&mut` argument, or a later argument expression mutates it. Otherwise the pointer names caller storage, and the callee copies on entry only if it mutates the parameter itself.
- `&T` lowers to a pointer. `&mut T` lowers to a pointer plus its root's region.
- Every call that can return heap data receives the caller's result region.
- Symbol names derive deterministically from the module path and the name.
- A panic calls one runtime entry point, which prints and exits with status 101.
- The external C ABI in [Interoperability](interop.md) stays separate and is **Gated** with raw pointers.

## JavaScript lane

**Recommended.** The semantics stay identical, while storage differs.

- Region transfers become no-ops. Semantic copies stay, through generated per-type copy functions.
- Structured IR emits as structured JavaScript with no control-flow reconstruction.
- `String` means UTF-8 text on every lane. The JavaScript runtime may hold a native string and convert for byte-level operations. That representation is internal.
- Deep recursion can exhaust any lane's stack. Stack exhaustion sits on the divergence list, outside the portable contract.

## Proof cases

Each case is a conformance test with hand-written expectations, run on every lane.

| Case | Program sketch | Expected |
| --- | --- | --- |
| Copy | `let b = a`, `b.name = "x"`, `print(a.name)` | the original name |
| Value parameter | `normalize(config)` mutates its parameter, then `print(config.lineWidth)` | `80` |
| Mutable borrow | `normalize(&mut config)`, then print | `100` |
| Nested copy | copy an `Array<Array<Int>>`, push into an inner array of the copy, print the original inner length | unchanged |
| Overlap snapshot | `x.n = 1`, then `f(saved: x, target: &mut x)`, whose body sets `target.n = 2` and prints `saved.n` | `1` |
| Later-argument snapshot | `f(x, mutateAndReturn(&mut x))` | `f` receives `x` as it was before the mutation |
| Reserved read | `Array#push(&mut xs, xs[0])` | allowed; pushes the old `xs[0]` |
| Reserved mutation | `f(&mut xs[0], grow(&mut xs))` | exclusivity error |
| Store after growth | `xs[i] = growAndReturn(&mut xs)` | stores into the grown array; bounds use the new length |
| Overlap | `f(&mut a, &a)` | exclusivity error |
| Disjoint fields | `f(&mut a.x, &mut a.y)` | allowed |
| Two elements | `f(&mut xs[i], &mut xs[j])` | exclusivity error |
| Borrowed root | `for x in &mut xs { Array#push(&mut xs, 1) }` | exclusivity error |
| Escaping borrow | `let r = &x`, a field of type `&T`, a return type `&T` | three distinct errors |
| Order | `f(b: log("b"), a: log("a"))`, where defaults also log | `b`, `a`, then defaults in declaration order |
| Exhaustive | a `match` missing one variant | error naming the variant |
| Early exit | `try` fails inside a loop inside a call | the `Err` returns, and the live-region count drops back to the caller's baseline plus live returned error storage |
| Recursion | build a recursive `Expr` below every lane's stack limit and evaluate it | the same value everywhere |
| Arithmetic | the boundary table | as listed |

## Open decisions for Mia

1. **`Int` width.** The default is 32-bit, with the tradeoff stated under [Arithmetic](#arithmetic).
2. **Copyable data versus resources.** The default copies plain data and moves resources. This narrows the existing notes' promise that every value is independent. It remains a proposal until Mia adopts it.
3. **Second-class borrows.** The default is yes. Ordinary code then needs no lifetime syntax, while borrow-returning accessors wait for their gate.
4. **Behavior contracts.** [Behavior contracts](behavior-contracts.md) holds the open proposal. This document reserves no syntax or policy for it.
