# Chasm Design Notes: Regional Memory Management

> **Status:** Draft
>
> Chasm uses compiler-inferred memory regions, chooses the narrowest valid lifetime by default, and requires explicit promotion only when a lifetime boundary cannot be inferred safely. Ordinary memory is reclaimed in batches; resources still receive deterministic cleanup.

## Central Rule

> **Values live in the narrowest region that satisfies their use. Returning, storing, or explicitly promoting a value widens its required region. Whenever possible, Chasm constructs the value directly in its destination region.**

This model aims to provide arena-style allocation and batch reclamation without requiring routine allocator plumbing in application code.

## Default Function Regions

Every function conceptually receives two allocation targets:

```text
temporary region    dies when the function returns
result region       supplied by and belongs to the caller
```

Ordinary local allocations use the temporary region:

```chasm
fn parse(path: Path) -> Project {
    let source = fs#readFile(path: path)
    let tokens = parser#tokenize(source)

    return parser#parse(tokens)
}
```

Conceptually:

- `source` and `tokens` use function-local memory.
- The returned `Project` is constructed in, moved into, or promoted to the caller's result region.
- The temporary region resets once when the function exits.
- The programmer writes no allocator code.

These are logical allocation targets, not a requirement that every call create two physical arenas. The compiler may coalesce, inline, stack-allocate, or eliminate regions when doing so preserves behavior.

## Destination-Directed Construction

Promotion should not imply that Chasm blindly relocates an already-built object graph. Memory containing internal pointers generally cannot be moved safely without fixups or reconstruction.

When the destination lifetime is statically known, the compiler should construct the value directly in the destination region:

```chasm
app.projects#append(project)
```

If `app.projects` belongs to the application region, the required lifetime of `project` is at least that long. The compiler can propagate this destination backward into the expression that creates `project`.

If a value has already been materialized in a narrower region, Chasm must do one of the following explicitly defined operations:

1. Move it without relocation when its backing storage is already compatible.
2. Reconstruct or copy the value and its owned graph into the wider region.
3. Reject the escape when safe promotion cannot be proven.

The language must not hide an unexpectedly expensive deep promotion behind a call that appears to be a constant-time move.

## Explicit Promotion

Most code should not name regions. Explicit allocation is reserved for ambiguous lifetime boundaries and architectural ownership.

One possible call-site form is:

```chasm
let project = parser#parse(
    source: source,
    in: app.memory,
)
```

A declaration-oriented form may be cleaner:

```chasm
let project in app.memory = parser#parse(source: source)
```

The exact syntax remains undecided. In either form, the destination region is visible where the value crosses into longer-lived state.

## Region Hierarchy

A typical widening of lifetimes is:

```text
expression scratch
    ↓
function temporary region
    ↓
caller/result region
    ↓
request, frame, scene, document, or plugin region
    ↓
application region
```

The named architectural regions are examples, not a single mandatory global hierarchy. Programs define regions where grouped lifetimes are meaningful.

```chasm
fn handleRequest(request: &Request) -> Response {
    // Temporary allocations use function- or request-lifetime memory.
}
```

```chasm
region scene {
    let meshes = assets#loadScene(path: path)
    renderer#run(meshes)
}
// The entire scene region is released here.
```

## Value Semantics and Backing Storage

Value semantics and allocation regions answer different questions:

- Value semantics determine whether mutating a parameter can affect the caller.
- Regions determine where dynamically allocated backing storage lives and when it is reclaimed.

Passing `String`, `Array`, or another container by value must not accidentally create shared mutable state merely because its backing storage is regional. Chasm still guarantees independent observable value semantics unless the program uses an explicit borrow.

The implementation may use moves, copy-on-write, uniqueness analysis, or copy elision to avoid unnecessary deep copies, provided those optimizations do not change observable behavior.

## Deterministic Resource Cleanup

Resetting an arena can discard ordinary memory in one operation, but some values own resources that cannot simply vanish:

- Files
- Sockets
- Mutexes
- GPU buffers
- Foreign handles

These resources still receive deterministic cleanup:

```chasm
fn process() {
    let file = fs#open(path: path)
    // file closes on every exit from this scope
}
```

Chasm tracks finalizable resources separately from ordinary arena allocations. Destroying a region performs two phases:

1. Run registered resource finalizers in reverse registration order.
2. Reset or release the region's memory blocks.

Strings, arrays, syntax nodes, and ordinary records need no individual destruction when they contain no finalizable resources. They disappear with their region.

Moving or promoting a resource must transfer its cleanup registration to the destination scope or region exactly once. Cleanup must also occur on early returns and error propagation; panic and process-abort behavior remains to be specified.

## Safe References Across Regions

A borrow may not outlive the region containing the borrowed value. A value promoted into a wider region may not retain references into a narrower region unless those referenced values are promoted as well.

This constraint is necessary to prevent examples such as an application-lifetime `Project` retaining a pointer into a function's temporary region. The compiler's escape and borrow analysis must validate the complete reachable ownership graph, not only the outer struct.

Raw pointers remain an unsafe escape hatch and do not receive these guarantees.

## No Garbage Collector

The proposed model does not use tracing garbage collection. Reclamation comes from:

- Stack and scope exit
- Function temporary-region reset
- Named region destruction
- Ownership transfer and destination-directed construction
- Deterministic cleanup for finalizable resources

Programs that create cyclic owned graphs will need a separate explicit representation or policy; regions alone reclaim such graphs only when the containing region dies.

## Comparison with Rust and Odin

### Rust

Rust associates initialized values with drop scopes. When a value leaves its drop scope, Rust runs its destructor when required; it does not normally clear the value's memory bytes. Destruction may recursively release owned allocations, while plain stack data with no drop work may compile to no cleanup at all.

Many independently allocated owned values can still require individual destructor and deallocation work.

### Odin

Odin is manually memory-managed and exposes allocator selection through explicit parameters and its implicit `context`. Its context distinguishes a general allocator from a temporary allocator, making arena and grouped-allocation patterns practical while leaving lifetime discipline and release/reset operations to the programmer.

### Chasm

```text
compiler-inferred regions
shortest valid lifetime by default
automatic batch reset
explicit promotion at architectural boundaries
deterministic cleanup only where resources require it
no tracing garbage collector
```

Chasm's intended middle ground is automatic regional allocation with explicit, visible lifetime widening rather than per-value allocator selection or pervasive per-value destruction.

## Open Semantic Questions

This direction still requires precise answers for:

- The syntax for named regions and explicit promotion
- Whether promotion may ever perform an implicit deep copy
- Region inference across separate compilation and dynamic dispatch
- Ownership graphs, cycles, and shared immutable storage
- Borrow aliasing and lifetime rules
- Finalizer declaration, ordering, failure, and panic behavior
- Interaction with async suspension, generators, and closures
- ABI representation of hidden temporary and result-region capabilities
- VM and native-backend equivalence

## References

- [Destructors — The Rust Reference](https://doc.rust-lang.org/reference/destructors.html)
- [Allocators — Odin Language Overview](https://odin-lang.org/docs/overview/#allocators)
