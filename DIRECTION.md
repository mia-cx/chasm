# Chasm

Chasm is a readable, statically typed, data-oriented language for applications,
with one core semantic model across interpreted, native, and JavaScript execution.
It separates data from behavior and makes borrowing and caller-visible mutation
explicit. Native execution should reclaim memory through inferred regions and
deterministic resource cleanup, without a tracing collector.

## Current state

The repository contains a Rust stage-zero scalar alpha (`0.1.0-alpha.1`), an
interpreter over typed IR, and a C backend with native builds through GCC or
Clang. Functions, mutable locals, branches, loops, recursion, checked Int32,
booleans, and literal strings work in single-file programs. Independent CLI
tests compare interpreter/native results and exercise diagnostics, evaluation
order, arithmetic traps, build safety, and optimized/UBSan C output.

The [alpha contract](docs/alpha.md) defines implemented behavior. The wider
language proposal remains in the [README](README.md) and design notes.
Borrowing, resources, regions, JavaScript, and self-hosting are unimplemented;
memory feasibility and performance gates remain unperformed.

## Immediate priority

Extend the working scalar core toward the bootstrap subset in thin slices.
Resolve copying versus resource ownership, borrow validity during storage changes,
region escapes, cleanup dependencies, and unambiguous call elaboration before
expanding the language. Use conservative rejection where a rule is unresolved.

Stage zero uses Rust, C emission for initial native execution, and an interpreter
over typed IR as the semantic reference.
Rust's checked borrowing supports the indexed compiler design; a cheap port to
Chasm remains a hypothesis. Trial one parser/lowering pass before broad host
investment. Stable F# is the fallback if Rust needs excessive scaffolding.

Check JavaScript against the same portable behavior. Explicitly model invalid
storage in the interpreter so host reachability cannot conceal native lifetime
bugs. Require measured copying and peak-memory growth limits before the port.
Per-call arenas do not yet establish bounded retention for long-lived state.

## Design constraints

- Ordinary copyable data has independent value semantics. Resource ownership
  requires a separate, nonduplicable contract, including nested resources.
- Region lifetime is only one condition for safe borrowing. Moves, replacement,
  reallocation, and incompatible aliases also constrain validity.
- Destination-directed construction should avoid hidden graph reconstruction.
  Expensive promotion needs a defined, visible cost contract.
- Backend parity covers observable arithmetic, mutation, errors, and cleanup.
  Host capabilities may differ explicitly.
- Embedding, plugins, HMR, async, and parallel execution remain goals. Each needs
  its own lifetime and conformance gates after the synchronous core works.

## Self-hosting and verification

Choose the emitted runtime independently of the bootstrap host. Self-host only
after the required language subset passes conformance and memory feasibility
gates. Native-compiler self-hosting precedes porting the reference interpreter
and JavaScript emitter. Preserve a rebuildable stage zero and compare later
stages under a pinned toolchain. Bootstrap equality establishes a fixed point
for those inputs; it does not establish compiler correctness.

The [compiler plan](docs/compiler-plan.md) defines the milestones. Its additional
subset rules remain subject to language design review. The scalar alpha has
conformance evidence; ownership, memory, and self-hosting gates have not run.

Bend informs the separation of copyable data and affine resources, explicit
effects, and independently checked contracts. Formal proofs are a later,
scoped extension; tests do not establish universal laws.

Mia is exploring a test-driven language: module behavior contracts expressed
before implementation. [Behavior contracts](docs/behavior-contracts.md) records
the proposal and open enforcement questions; it is not yet adopted semantics.
