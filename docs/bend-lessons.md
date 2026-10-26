# What Chasm can borrow from Bend

> Status: research note, 2026-10-26

Sources are the [Bend repository][repo] at commit `295e996574573e3a88827b1cf7045b437333c8f0` (v2.0.39, 2026-10-23), its guide, the [BendTT][tt-pdf] and [BendRT][rt-pdf] papers (cited by their Typst sources), and [bend-lang.com][site]. "Bend" means Bend 2. Bend 1 programs and HVM do not carry over ([README][readme-limits]).

Borrow Bend's trust boundaries and its way of governing agents, not its type theory. These five ideas fit Chasm's compiler plan without dependent types or a prover:

1. Specifications are reviewed apart from the code that implements them.
2. Copyable data and owned resources are different kinds.
3. Code that reaches foreign, unsafe, or host operations is tracked per function, and every checked or proven claim lists it as trusted rather than checked.
4. One fast gate gives a fixed verdict, and conformance tests carry their own expected output.
5. A small, separately reviewed checking core rechecks what a larger, agent-written compiler produces.

## What Bend is

- A pure language with dependent types and an IO type for effects. A law is a type, and its proof is a definition of that type ([guide][guide-laws]).
- Values are affine by default. `is Data` types may be copied through a `+` binder, and `is Type` values have one owner. Functions, arrays, and IO handles are always `Type` ([guide][guide-kind]).
- Recursion must shrink a pattern-matched argument, and mutual recursion is banned. `@unsafe` turns the check off for one definition ([guide][guide-term]).
- A parallel let runs its calls in parallel. Purity and affinity make them independent, and the programmer promises they take equal time. The JavaScript target runs them in sequence ([guide][guide-par]).
- The toolchain is TypeScript on Bun. The human-written checker `bend.ts` has 3,882 lines, and the compiler and runtimes in `comp.ts` have 6,563 ([AGENTS.md][agents]). The compiler emits one C file that is both the CPU program and the GPU kernel, or JavaScript ([guide][guide-hood]).

## What a Bend proof covers

`bend PROOF.bend` prints `ALL PROOFS CHECK` only when three conditions hold. Every law is proven, no `?TODO` remains, and no definition outside Base relies on `@unsafe` or foreign code ([guide][guide-laws], [main.ts][main-book], [verdict][main-verdict]). `--verdict` also rechecks a translation of the proofs with BendTT, a kernel of about 4,000 lines of Lean. Its theorems state three things. No accepted definition has the empty type. Live code halts. A closed proof of `a == b` makes both sides convertible ([bendtt.lean][lean-claims]). Every proof in that file is complete, and it declares no axioms of its own ([BendTT][tt-mech]).

That result is real and narrow:

- **It proves the law as written.** In the README's demo, the agent kept "winning is impossible" by adding a wall, but "could have moved the flag, made the room kill you, or whatever" ([README][readme-demo]). A law also means what its named functions mean. The demo law calls `Game.is_won`, which lives in the agent-edited `main.bend` ([demo][demo-laws]).
- **It covers the kernel, not the toolchain.** The theorems are about `Book.check` in the Lean file. The TypeScript checker has no proof. Neither the TypeScript translation into the kernel nor the kernel's parser is verified ([README][readme-limits], [BendTT][tt-mech]). The kernel binary also trusts the Lean compiler that builds it ([AGENTS.md][agents]).
- **It covers Bend's semantics, not compiled code.** The compiler is "99% AI-written and not yet fully audited", the C runtime is unverified, and the theorems "stop at the calculus" ([README][readme-limits], [BendRT][rt-limits]). Backends can differ. F32 NaN payloads change on the JS lane, and the C lane is the reference ([WONTFIX][wontfix-nan]).
- **It trusts foreign code by its type.** For a foreign definition, "the checker reads its type, never its code". The gate therefore fails while any definition outside Base depends on foreign or `@unsafe` code ([main.ts][main-taint]).
- **Some facts sit outside the logic.** F32 operations are axioms, so nothing about floats can be proven ([README][readme-limits]). The equal-time promise behind parallel calls is unchecked, so a skewed program silently loses its parallelism ([BendRT][rt-limits]).

The site advertises broader guarantees than these. Chasm documents should state each guarantee with its scope.

## Borrow now

Every item is a recommendation, not adopted semantics or project governance. Items marked "language decision" would change Chasm's specification. The [compiler plan](compiler-plan.md) proceeds on its stated defaults until Mia decides.

### 1. Specifications are reviewed apart from implementations

Bend splits laws from proofs. A human writes `LAWS.bend`, and "the AI does not touch it". The agent writes `PROOF.bend` and the code ([guide][guide-laws]). The toolchain only checks that `PROOF.bend` imports `LAWS.bend` ([main.ts][main-book]). Nothing in it detects an edited law. Bend runs its own repository the same way. Agents must not edit the human-written checker, and CI enforces size caps that only Taelin may change ([AGENTS.md][agents], [repo gate][repo-gate]).

Recommended for the compiler project:

- Changes to the language specification, conformance expectations, and later program laws arrive separately from implementation changes, so each gets its own review.
- Weakening counts as a contract change: a narrower input domain, an added precondition, fewer samples, or a skipped test.
- A law's observers belong to the law. If an agent made Bend's `is_won` return `False`, the `you_cant_win` law would hold without any edit to `LAWS.bend`.

This needs no language change. Whether CI enforces the split is Mia's choice.

### 2. Copyable data and owned resources are different kinds (language decision)

Every Bend type has a kind. `Data` may be copied, and `Type` has one owner. A list is as copyable as its elements, and a type built from two others takes the smaller quantity ([guide][guide-kind]). Handles are affine and opaque, so "no program can forge or reuse one" ([guide][guide-io]). The runtime uses this directly. A match frees the node it consumes, there is no garbage collector, and "a compiler may drop a copy the source spelled, never add one" ([guide][guide-hood], [BendRT][rt-copy]). Copying a `Data` value costs a reference count, emitted at the binder's second use ([BendRT][rt-own]).

Recommended: qualify Chasm's value promise the same way, as the [compiler audit](compiler-audit.md) also does:

- Plain data keeps independent value semantics and may be copied.
- Files, sockets, GPU buffers, foreign handles, and any struct, container, or closure that holds one are resources. Passing one by value moves it, use after move fails to compile, and its cleanup registration transfers exactly once.
- A closure that captures a resource or a `&mut` borrow is not copyable.
- Chasm's borrows spare it Bend's handle threading. A `&mut` borrow replaces handing the handle back beside every result.

Leave out Bend's stricter rule that no function value may be copied. Bend needs it because Girard's and Curry's paradoxes both copy a function ([BendTT][tt-abstract]). Without a prover, Chasm only needs a closure to take the kind of what it captures.

### 3. Trust boundaries are tracked per function

Bend treats `@unsafe` and foreign effects as promises. The gate walks the reference graph from the user's definitions, floods each promise back along it, and names every definition that depends on one ([main.ts][main-taint]). Effects run only on the event loop, "so proofs, termination and the GPU never touch host code" ([guide][guide-effects]).

Chasm can draw this boundary without an IO monad. The compiler records, per function in typed IR, whether it reaches host effects, foreign calls, or raw pointers, and propagates that through calls. Every check, test, and future proof reports which functions it trusted rather than checked. Assume `&mut` borrows are exclusive. Then a function that reaches none of these depends only on its inputs, and its results and final `&mut` state are reproducible. Laws, compile-time evaluation, and parallelism can later rely on that class.

Three parts are language decisions: whether module-level mutable state exists, whether a function type records its effect class (otherwise calls through function values count as effects), and whether a public function's class is part of its API.

### 4. One fast gate with a fixed vocabulary

Bend's gate prints one of two fixed lines and exits with 0 or 1 ([main.ts][main-strings], [verdict][main-verdict]). An open `?TODO` fails it ([main.ts][main-book]). When the TypeScript checker accepts what the kernel rejects, the gate prints a mismatch notice, never a pass ([main.ts][main-verdict]). The `--verdict` CI gate sorts each file into agree, relies on unsafe code, rejected, out of scope, false reject, timeout, or infrastructure error ([verdict gate][safe-gate]). A test is a Bend file that ends in the lines its run must print. It passes only when the checker's interpreter, the JS build, and the C build all print them ([test gate][test-gate]).

For Chasm:

- One check command with stable diagnostic codes and a machine-readable form, so agents act on facts instead of parsing prose.
- Separate verdicts for "the program is wrong", "the tool cannot decide", and "the tool failed". Unsupported input fails closed.
- Portable conformance tests that carry their expected output and pass only when the interpreter, JS, and native lanes all match it. Target-specific tests run on their declared targets. This gives the conformance suite in the [JavaScript backend notes](javascript-backend.md) a cheap format.

Bend's checker is fast, and its errors precise, partly because it infers almost nothing ([guide][guide-affine]). Chasm can take a milder form: explicit function signatures, with inference only inside bodies (language decision).

### 5. A small checking core, separate from the compiler

Bend keeps its trusted parts small and human-owned. The checker has 3,882 lines and the Lean kernel with its proofs about 4,000, while the larger compiler is mostly agent-written ([AGENTS.md][agents], [README][readme-limits]). `--verdict` passes only when both checkers accept. Whatever the kernel cannot express fails, along with every definition that names it ([safe.ts][safe-scope]). CI caps the size of core files, and an oversized file "is made smaller by simplification, never by moving code out" ([repo gate][repo-gate]).

Chasm's version is an independent validator for typed IR. It rechecks types, moves, borrows, and region escapes on the exact IR each backend consumes. The front end and optimizer can change fast and be agent-written. When the validator rejects what the front end accepted, that is a compiler bug, and the build stops. A size budget for the validator is optional. The validator cross-checks IR invariants. It does not show that lowering preserved source meaning, and the docs say so.

## Stage later

**Laws as property tests.** This is the near-term path to laws, and it claims no proofs. Once the interpreter and a test runner exist, a law can be an ordinary Chasm predicate over generated inputs, kept in a module reviewed apart from the code it tests. [Behavior contracts](behavior-contracts.md) holds the broader open proposal. It may name only functions that reach no host, foreign, or unsafe code, so every backend can reproduce a run. A result is a pass count with its seed, or a shrunk counterexample, never "proven". Because it runs compiled code, it catches backend divergence that Bend's source-level proofs cannot see. Keep laws out of function bodies for now. An assertion that runs in development and vanishes in release would break Chasm's rule of one semantics for every build ([README](../README.md)). In-body contracts wait for error and panic semantics (language decision).

**A total fragment, not a total program.** Bend requires termination per definition. A server loop counts down a fuel argument or is marked `@unsafe`, and the gate suggests moving unused unsafe definitions out of the proof's imports ([guide][guide-term], [main.ts][main-verdict]). Chasm can do the same without restricting loops. The compiler infers which functions terminate by construction, such as iteration over a finite collection or recursion on a smaller part. Future proofs may name only those functions, while application shells keep their `while` loops, servers, and event loops. Termination matters for proofs, because "a function that never returns could otherwise prove anything", and for compile-time evaluation ([guide][guide-term]). Property tests need only a timeout, so build this when proofs or compile-time evaluation arrive.

**Structured fork-join.** Bend's parallel calls are safe because "a forked task owns its arguments" and pure code reaches no effects ([BendRT][rt-abstract]). Chasm could offer the same over effect-free functions once borrow aliasing rules exist. JS would run the forks in sequence and get the same result. This waits for the async and concurrency design.

## Leave out

- Dependent types, `Type : Type`, proofs as programs, and a mechanized kernel. They would remake the language, and Chasm has not chosen formal verification as a goal. Revisit only by explicit decision.
- The IO monad and `do` blocks. Chasm is imperative, and idea 3 draws the same boundary without them.
- Affine-by-default values, single-use closures, and `+` copy markers. Chasm's value semantics already covers plain data.
- Restrictions that come from Bend's checker: no mutual recursion, no `if`, and no match on a computed value ([README][readme-limits]).
- Bend's runtime design: one heap shared with the GPU, the fixed task grid, and the equal-time contract ([BendRT][rt-abstract]). Chasm's targets are applications, plugins, and embedding.

## Sequencing in the compiler plan

1. **Before the first compiler slice.** Write down the trust tiers: checked safe code; trusted foreign, unsafe, and host code; trusted compiler, runtime, C compiler, and JS engine. Proceed on the recommended copyable-data-versus-resources default.
2. **With the front end and reference interpreter.** Carry kinds and effect classes in typed IR. The plan's [Resources gate](compiler-plan.md#feasibility-gates) checks resource moves and cleanup with a synthetic resource before the port, so ownership feasibility is tested early while resource syntax waits. Start the conformance suite in the expected-output format, with the fixed verdicts.
3. **With the first backends.** Put the IR validator in front of C and JS emission. Run every portable conformance test on all lanes. The divergence list holds only explicit platform, capability, and resource-exhaustion exceptions, each with a test, as Bend's [WONTFIX][wontfix] file does. It never relaxes arithmetic, copies, or portable parity.
4. **Once a test runner exists.** Add property-test laws over effect-free functions, in modules reviewed apart from the code they test. First-class behavior tests stay an open track in [Behavior contracts](behavior-contracts.md).
5. **When proofs, compile-time evaluation, or parallelism enter scope.** Add totality inference, then structured fork-join.

## Host note

Bend shows that a TypeScript host can carry a checker plus a compiler that emits C and JS at this size, shipped as a Bun executable ([AGENTS.md][agents], [flake.nix][flake]). It also shows the costs. Native builds wait on clang, there is no separate compilation, and the README recommends the JS backend for fast development ([README][readme-limits]). Its JS lane overflows the host stack on deep non-tail recursion, which any JS backend has to plan for ([WONTFIX][wontfix-js]). None of this settles Chasm's host. The transferable lesson does not depend on the host. Keep the checker small and separately reviewed, and put the larger, agent-written compiler behind gates.

[repo]: https://github.com/bendlang/bend/tree/295e996574573e3a88827b1cf7045b437333c8f0
[site]: https://bend-lang.com
[tt-pdf]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/paper/BendTT.pdf
[rt-pdf]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/paper/BendRT.pdf
[readme-demo]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/README.md?plain=1#L60-L64
[readme-limits]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/README.md?plain=1#L220-L260
[agents]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/AGENTS.md?plain=1#L3-L35
[guide-affine]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/guide/GUIDE.md?plain=1#L55-L65
[guide-term]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/guide/GUIDE.md?plain=1#L105-L121
[guide-par]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/guide/GUIDE.md?plain=1#L147-L170
[guide-kind]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/guide/GUIDE.md?plain=1#L219-L255
[guide-laws]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/guide/GUIDE.md?plain=1#L306-L341
[guide-io]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/guide/GUIDE.md?plain=1#L371-L377
[guide-effects]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/guide/GUIDE.md?plain=1#L408-L411
[guide-hood]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/guide/GUIDE.md?plain=1#L638-L661
[demo-laws]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/demos/app_win_is_bug_2d/LAWS.bend#L3-L11
[main-strings]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/bend2/main.ts#L79-L90
[main-verdict]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/bend2/main.ts#L887-L919
[main-taint]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/bend2/main.ts#L921-L967
[main-book]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/bend2/main.ts#L1025-L1033
[safe-scope]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/bend2/safe.ts#L1-L47
[lean-claims]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/bend2/bendtt.lean#L1704-L1745
[repo-gate]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/gates/repo.ts#L1-L11
[safe-gate]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/gates/safe.ts#L1-L12
[test-gate]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/gates/test.ts#L1-L15
[wontfix]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/WONTFIX.txt
[wontfix-nan]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/WONTFIX.txt#L106-L113
[wontfix-js]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/WONTFIX.txt#L147-L150
[tt-abstract]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/bend2/docs/BendTT/main.typ#L135-L149
[tt-mech]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/bend2/docs/BendTT/main.typ#L1065-L1108
[rt-abstract]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/bend2/docs/BendRT/main.typ#L101-L120
[rt-copy]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/bend2/docs/BendRT/main.typ#L186-L191
[rt-own]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/bend2/docs/BendRT/main.typ#L304-L327
[rt-limits]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/bend2/docs/BendRT/main.typ#L616-L626
[flake]: https://github.com/bendlang/bend/blob/295e996574573e3a88827b1cf7045b437333c8f0/flake.nix#L1-L12
