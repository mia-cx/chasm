# Chasm Design Notes: JavaScript Backend

> **Status:** Draft
>
> JavaScript is a first-class Chasm compilation target. It allows Chasm applications and libraries to run directly in browsers, Node.js, and JavaScript workers without requiring WebAssembly.

## Backend Architecture

The JavaScript compiler consumes the same typed intermediate representation as the VM and native backends:

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
    └── WebAssembly Compiler
```

The core language semantics remain the same across backends. Host APIs and low-level capabilities may differ by target.

## Target and Platform

The output target and host platform are separate concepts:

```bash
chasm build --target javascript --platform browser
chasm build --target javascript --platform node
chasm build --target javascript --platform worker
```

The profiles provide different host capabilities:

- `browser` exposes browser and DOM APIs.
- `node` exposes Node.js and npm integration.
- `worker` exposes the APIs available to the selected worker runtime.

A program that uses unavailable host capabilities should fail at compile time when possible.

## ECMAScript Modules

Each Chasm module compiles to an ECMAScript module:

```text
src/
    app.chasm               → dist/app.js
    net/
        http.chasm          → dist/net/http.js
        module.chasm        → dist/net.js
```

Chasm's filesystem modules and explicit barrel exports map naturally to ESM imports and exports. Generated output should remain compatible with standard JavaScript runtimes, bundlers, and tree-shaking tools.

## Output Artifacts

A JavaScript build should be able to emit:

```text
module.js
module.js.map
module.d.ts
```

- JavaScript contains executable ESM output.
- Source maps preserve Chasm locations in browser and Node debuggers.
- TypeScript declarations allow JavaScript and TypeScript applications to consume exported Chasm APIs.

Readable stack traces and reliable source maps are part of the backend's correctness requirements.

## Direct JavaScript and npm Interop

On the JavaScript backend, imported JavaScript modules should compile to direct ESM imports whenever possible.

Conceptually:

```chasm
extern "js" module npm::"date-fns" as dates

let result = dates#format(
    date: now,
    format: "yyyy-MM-dd",
)
```

The compiler delegates package resolution to the host toolchain rather than implementing an npm resolver.

A mixed project may retain the ordinary JavaScript ecosystem files:

```text
project/
    chasm.toml
    package.json
    pnpm-lock.yaml

    src/
        main.chasm
```

npm, pnpm, or another supported JavaScript package manager remains responsible for installation and lockfiles.

TypeScript declaration files can provide the starting point for generated Chasm foreign-module contracts. Complex TypeScript features may require generated approximations, manual binding overrides, or a dynamic `JsValue` escape hatch.

## Browser APIs

Browser APIs may be described by generated contracts derived from Web IDL or TypeScript's DOM declarations.

Conceptually:

```chasm
let element = dom::document#querySelector(
    selector: "#app",
)
```

Chasm should not build a separate DOM abstraction when direct, typed bindings can represent the platform accurately.

## Preserving Value Semantics

JavaScript objects have reference semantics; Chasm values do not. Generated JavaScript must preserve Chasm's observable value semantics.

```chasm
let second = first
second.name = "changed"
```

This must not mutate `first` unless an explicit mutable borrow or foreign reference-semantic handle is involved.

The backend may use:

- Copy elision
- Uniqueness analysis
- Copy-on-write storage
- Immutable internal representations
- Explicit copying when required

A foreign `JsValue` retains JavaScript reference semantics and remains distinct from an ordinary Chasm value.

## Regional Memory on a Garbage-Collected Host

JavaScript cannot guarantee immediate physical reclamation of ordinary object memory. The JavaScript backend therefore preserves Chasm's lifetime and resource semantics while delegating physical object reclamation to the host garbage collector.

At region destruction, generated code:

1. Runs deterministic Chasm resource finalizers.
2. Unregisters region-owned browser or host resources.
3. Releases references owned by the region.
4. Allows the JavaScript garbage collector to reclaim ordinary object memory later.

Useful region-owned browser resources include:

- Event listeners
- Timers
- Observers
- Object URLs
- WebSocket connections
- WebGL or WebGPU resources

Chasm must not claim that a JavaScript build can force immediate physical memory reclamation.

## Layout-Sensitive Operations

Ordinary JavaScript objects do not have a stable native memory layout. Layout-sensitive Chasm features therefore require explicit target capabilities.

A hybrid representation is preferred:

- Ordinary structs compile to efficient JavaScript representations.
- Explicitly layout-sensitive data uses `ArrayBuffer`, `DataView`, or typed arrays.
- Raw pointers are unavailable for ordinary JavaScript objects.
- FFI-layout data may retain a declared layout when backed by linear memory.

The final capability and diagnostic model remains to be designed.

## Numerical Semantics

Generated JavaScript must match the VM and native backends:

- `I32` uses explicit signed 32-bit operations.
- `U32` uses explicit unsigned normalization.
- `F32` uses `Math.fround` where required.
- `F64` uses JavaScript numbers.
- `I64` and `U64` use `BigInt` or a specified runtime representation.
- Overflow, division, conversion, and comparison behavior remain backend-independent.

These rules require shared backend conformance tests.

## Async Interop

The JavaScript backend should eventually map Chasm asynchronous functions to JavaScript promises and allow declared promise-returning JavaScript functions to participate in Chasm async code.

The exact async model remains undecided. It must account for:

- Promise rejection and Chasm errors
- Region lifetimes across suspension
- Cancellation
- Browser and Node event loops
- Async callbacks entering Chasm

## Toolchain Integration

Chasm emits standards-based ESM and delegates bundling, minification, package resolution, and development serving to mature JavaScript tools:

```text
Chasm compiler
    ↓
ESM JavaScript + source maps + TypeScript declarations
    ↓
Vite, esbuild, Rollup, Node.js, or a browser
```

Chasm should not implement its own JavaScript bundler.

## Backend Conformance

The VM is the initial semantic reference. A shared conformance suite should run equivalent programs through the VM, JavaScript, and native backends and compare observable behavior.

The suite must cover:

- Evaluation order
- Integer and floating-point behavior
- Value copying and mutation
- Structural projection
- Borrow validation
- Region and resource cleanup
- Errors and panics
- Callable values
- Module initialization

## Proposed Implementation Order

1. Establish the VM or interpreter as the semantic reference.
2. Emit standards-based ESM from typed IR.
3. Add source maps and browser debugging.
4. Implement Chasm value semantics in generated JavaScript.
5. Add direct JavaScript module bindings.
6. Generate TypeScript declarations for Chasm exports.
7. Consume straightforward TypeScript declarations for imports.
8. Add DOM and npm workflows.
9. Add async interoperability after Chasm's async semantics are defined.
10. Add layout-sensitive JavaScript representations where required.
