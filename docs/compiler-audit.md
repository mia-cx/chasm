# Compiler feasibility audit

**Recommendation.** Use Rust for the bootstrap compiler, provisionally validated
by one indexed parser/lowering pass. Stable F# is the fallback if Rust requires
disproportionate scaffolding. Start with an executable semantic model, then C
and JavaScript emission from shared typed IR. The [concrete-plan review](#review-of-the-concrete-bootstrap-plan)
explains this recommendation and the necessary fixes. The main uncertainty is
the ownership model, not the implementation language.

**Observed.** The six existing documents contain proposals, examples, and acknowledged open questions.
They demonstrate no compiler, native performance, memory-safety proof, or backend equivalence.
This audit covers [language design](language-design.md), [memory](memory-management.md), [parameters](parameter-passing.md),
[modules](modules.md), [interop](interop.md), and [JavaScript](javascript-backend.md).
The failures below are counterexamples to insufficient rules, not reports of implementation bugs.
All acceptance gates remain unperformed. Recommendations remain proposals until adopted.

## Ranked blockers

### 1. Independent values need a precise boundary

**Resource duplication.** Copy a record containing an open file, then finalize each copy.
Copying its handle can close the same resource twice. Sharing ownership changes cleanup and identity semantics.
Duplicating the external resource is a separate operation with possible failure and observable effects.
The universal promise that every value becomes an independent local value therefore needs qualification.
**Close when** every supported type has defined copy, transfer, borrow, and cleanup behavior, including nested containers.
Recommend independently copyable ordinary data and noncopyable resource ownership. Reject unsupported copying before lowering.
Moving a resource consumes its previous ownership and transfers its cleanup obligation exactly once.
Treating that operation as copy elision requires proof that later reads, borrows, and cleanup cannot distinguish it.

**Mutable aliases.** Copy an array of records, borrow an element mutably, then write through that borrow.
A shallow copy changes the original. Detaching only the outer array can still share a nested array.
A borrow obtained before a later copy can also bypass a copy-on-write write barrier.
**Close when** nested mutation tests preserve independence and overlapping incompatible borrows fail statically.
Include reallocation while an element is borrowed, overlapping slices, and reborrowing through a projected field.
Use eager copying as the reference behavior. Add copy-on-write only after its equivalence and lifetime conditions hold.
Copy-on-write ownership counts also need accounting; ordinary region reset does not automatically maintain them.

**Hidden capabilities.** A copied record can contain a callable that captures a mutable borrow of caller state.
Invoking it changes that state despite passing the record without a mutable-borrow marker.
Foreign handles likewise preserve external identity, as the interop notes already acknowledge.
**Close when** the independence promise explicitly identifies its owned data graph and exceptions for captured or foreign state.
Exclusive borrows and resource owners must not become duplicable by nesting them inside records or closures.

### 2. Region inference needs ownership edges and storage validity

**Returning references.** A returned project borrows token text stored in the function's temporary region.
Moving the project header to the result region leaves its text dangling.
Copying the borrowed text instead changes reference identity and mutation relationships; it is not automatically equivalent.
**Close when** inference distinguishes owned edges, safe borrows, and foreign handles throughout reachable data.
Promotion traverses only edges whose semantics permit reconstruction. Borrowed storage must remain valid through its last use.
Test direct returns, nested aggregates, closures, and conditional returns borrowing from different inputs.

**Owned graphs.** Graph reconstruction can loop on cycles or duplicate shared nodes without a defined policy.
A copied value can outlive its source region while still sharing copy-on-write backing storage scheduled for reset.
**Close when** sharing preserves backing-storage lifetime, or copying or rejection prevents that escape.
Specify cycle handling before accepting cyclic owned graphs. Recommend rejecting them in the initial subset.
Test repeated edges and shared backing across region exits, not just trees of independently allocated values.

**Relocation and mutation.** A borrowed element can become invalid when its array reallocates inside a still-live region.
A promoted self-referential object can still point into its previous allocation.
The rule "the region outlives the borrow" is necessary but insufficient.
**Close when** storage identity, initialization, replacement, and outstanding borrows constrain moves and mutation.
Reject unsupported self-references and relocation while borrowed in the first implementation.
Do not infer safe aliasing merely because both references belong to the same region.

**Retention.** A server repeatedly replaces a small application value while allocating each replacement in the application region.
Only the latest value remains useful, but every previous allocation survives until shutdown.
Sharing one retained leaf can similarly keep an entire temporary arena alive.
**Close when** repeated replacement and retained-leaf experiments measure peak bytes, retained bytes, and bytes copied.
Specify an explicit lifetime boundary for reclaiming such storage; never equate memory safety with bounded retention.
Sibling architectural lifetimes also need constraints, not an assumption that every region has one narrowest common destination.
Reject unresolved escapes rather than silently widening everything to application lifetime.

### 3. Deterministic cleanup must survive transfer, failure, and suspension

**Ordering.** A buffer finalizer uses a device. Moving both registrations can reverse their dependency order.
Reverse registration order alone does not prove that every finalizer sees its dependencies alive.
The interop unload sequence also releases foreign roots before running Chasm finalizers.
A finalizer using a rooted Lua callback would then access an already-released dependency.
**Close when** cleanup traces preserve dependencies after promotion and release required roots after their last finalizer use.
Specify scope cleanup separately from region storage reset. Coalescing arenas must not delay observable resource release.
Cover partial construction, replacement, early return, errors, and cleanup that itself fails.
Define panic and abort behavior explicitly; an unconditional "every exit" promise cannot cover process termination.
Prevent resurrection or registration into a region already undergoing teardown unless a precise rule supports it.

**Async and closures.** A suspended task retains a borrow after its caller's scope ends.
Cancelling it may release resources while a callback still uses them, or omit cleanup entirely.
A copied closure environment may also duplicate a unique resource obligation.
**Close when** capture summaries and task ownership keep borrowed storage and cleanup dependencies alive until the final use.
Before supporting suspension, test cancellation before execution, at each suspension, and during foreign completion.
Recommend rejecting escaping borrowed closures and suspension in the initial subset until these rules exist.
Plugin unload must also account for calls already in progress; preventing new calls does not establish quiescence.
Test a retained callback during unload. Either complete existing calls safely or reject that unload.

### 4. Callable contracts must survive projection and separate compilation

**Argument ambiguity.** The parameter notes describe both a whole parameter object and one ordinary record-valued parameter.
A record can match the whole argument shape and the first positional parameter simultaneously.
The conceptual desugaring can also introduce another argument-object layer if applied repeatedly.
**Close when** one elaboration rule assigns every call exactly one meaning, or rejects ambiguity without declaration-order dependence.
Test whole-record calls, single-record parameters, mixed arguments, defaults, and overload candidates together.
Set evaluation order before lowering. Each supplied expression and each selected default must execute exactly once.

**Projection ownership.** A large configuration contains a finalizable extra field that the callee does not request.
Moving the whole configuration can consume or close that field despite projection supposedly ignoring it.
Borrowing a narrower shape can also violate exclusivity when two projected views overlap.
**Close when** projection selects fields before their copy, transfer, or borrow rules apply.
Test that unused fields retain ownership and that disjoint mutable projections differ from overlapping ones.

**Separate compilation.** A library returns a reference into an argument, but its exported type omits that relationship.
A caller compiled without the library body cannot decide whether its temporary argument may die.
Trait calls, generic code, and function-valued fields create the same problem.
**Close when** module metadata carries sufficient lifetime, capture, ownership, and calling-convention information.
Public parameter names and default behavior must survive this boundary too.
Compile callers using metadata alone. Check results against whole-program compilation, including incompatible library replacements.
Pin module identity, initialization order, and collisions between a file and a same-named directory barrel.
One file per module does not itself settle initialization cycles or safe hot replacement.
Serialized bytecode can claim false types or invalid cleanup edges. Validate these before execution, or restrict loading to trusted artifacts.
Close that gate with malformed-module rejection tests before claiming safe loading of untrusted plugins.

### 5. Shared typed IR does not establish backend equivalence

**Arithmetic.** Naive JavaScript multiplication can round before a later 32-bit truncation.
For wrapping U32 arithmetic, squaring 4,294,967,295 must produce 1, not the rounded Number result's low bits.
JavaScript Number uses binary64; `Math.imul` specifies multiplication modulo 2^32. [ECMAScript Number](https://tc39.es/ecma262/multipage/ecmascript-data-types-and-values.html#sec-ecmascript-language-types-number-type), [Math.imul](https://tc39.es/ecma262/multipage/numbers-and-dates.html#sec-math.imul).
This example requires that result only if Chasm chooses wrapping arithmetic; checked arithmetic must reject overflow instead.
**Close when** widths, overflow, shifts, division, remainder, and conversions have target-independent rules, including constant folding.
Compare I64 values beyond exact Number precision, minimum signed integer divided by minus one, and division by zero.
Define F32 rounding points, signed zero, NaNs, and any permitted variation in mathematical library functions.
Include text encoding, indexing, equality, and observable collection iteration in the portable contract.

**Managed reference execution.** A reference interpreter can keep an expired Chasm object alive through its host collector.
It then accepts execution that would access freed native storage.
**Close when** the semantic model explicitly invalidates logical storage and checks resource state independently of host reachability.
Compare output, errors, mutation, and cleanup traces across backends. Allocation instrumentation separately checks retention assumptions.
Preserve ownership and cleanup information until each backend has lowered it into equivalent operations.

**Foreign calls.** A foreign library retains a temporary buffer or reenters Chasm during an exclusive borrow.
A declared signature alone cannot make either operation safe.
JavaScript getters and Lua metatables can also execute code while a binding validates or copies a record.
**Close when** binding contracts cover retention, reentrancy, initialization, ownership, and error translation.
Test retained callbacks, wrong dynamic types, partially converted resources, and incompatible unwinding.
Unsafe trusted bindings remain explicit assumptions. Validation cannot prove arbitrary foreign code follows its contract.
Keep ABI-safe records distinct from ordinary structural values; lifetime safety does not establish layout compatibility.

## Bootstrap language assessment

This comparison concerns the temporary compiler and semantic interpreter, not the eventual embedded VM.
No existing implementation creates a sunk-cost reason to retain C#.
The recommendation values straightforward compiler data structures and a small toolchain dependency set.

| Host | Concrete advantage | Cost for this bootstrap | Assessment |
| --- | --- | --- | --- |
| C# | Managed graphs and pattern matching support an evolving compiler. Current documentation also describes closed unions. [GC](https://learn.microsoft.com/en-us/dotnet/standard/garbage-collection/fundamentals), [unions](https://learn.microsoft.com/en-us/dotnet/csharp/language-reference/builtin-types/union). | The compiler needs .NET. C# 15 unions require the still-prerelease .NET 11 SDK on this review date. Exhaustive handling must work on the pinned SDK. | Credible alternative; managed graphs need a representation the Chasm subset can express. |
| F# | Recursive discriminated unions directly describe ASTs and IR. Incomplete matches receive a diagnostic. [Unions](https://learn.microsoft.com/en-us/dotnet/fsharp/language-reference/discriminated-unions), [FS0025](https://learn.microsoft.com/en-us/dotnet/fsharp/language-reference/compiler-messages/fs0025). | Shares the .NET deployment dependency. The indexed design still needs a concrete port check. | Strongest fallback if the bounded Rust trial exposes excessive host-specific scaffolding. |
| Rust | Enums support exhaustive matching, and ownership checks protect the compiler's own memory. [Match](https://doc.rust-lang.org/book/ch06-02-match.html), [ownership](https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html). | Mutable compiler graphs need ownership planning or indexed storage. Those checks do not validate Chasm lifetimes. | Provisional default for the proposed indexed, value-oriented compiler. Validate portability through one pass before broad investment. |
| OCaml | Variants suit compiler trees; the toolchain emits standalone native executables. [Types](https://ocaml.org/docs/basic-data-types), [native compiler](https://ocaml.org/manual/latest/native.html). | Adds its ecosystem; native interfaces require its runtime's GC rules. [C interface](https://ocaml.org/manual/latest/intfc.html). | Technically strong. Existing maintainer fluency could outweigh C#'s draft preference. |
| Go | GC simplifies compiler graphs, and the language keeps implementation choices limited. [FAQ](https://go.dev/doc/faq#garbage_collection). | No variant types; compiler-node coverage requires discipline beyond ordinary type switches. Native bindings add cgo constraints. [Variants](https://go.dev/doc/faq#variant_types), [cgo](https://pkg.go.dev/cmd/cgo). | Viable, with less direct help for evolving IR variants. |
| TypeScript | Discriminated unions support exhaustive checks; existing JavaScript tooling is directly usable. [Narrowing](https://www.typescriptlang.org/docs/handbook/2/narrowing.html#exhaustiveness-checking). | Types erase at runtime; integer semantics and mutable aliases need explicit treatment in the interpreter. [Erasure](https://www.typescriptlang.org/docs/handbook/2/basic-types.html#erased-types). | Credible if browser-hosted compilation is a first requirement. It can also emit native code through C. |
| Zig | Tagged unions, explicit allocators, and integrated cross-target tooling are relevant strengths. [Language reference](https://ziglang.org/documentation/master/#Tagged-union), [cross-compilation](https://ziglang.org/learn/overview/#cross-compiling-is-a-first-class-use-case). | The compiler itself takes on allocation and lifetime discipline. [Memory](https://ziglang.org/documentation/master/#Memory). | Better justified by an immediate native-runtime requirement than a temporary frontend. |

**Decision check.** In the first compiler slice, add one IR case and require every relevant consumer to handle it.
Use compiler diagnostics as errors where available; prohibit silent catch-all behavior in semantic passes.
If C# needs substantial machinery for this, reconsider F# before expanding the compiler.
Maintainer fluency is unknown here. This assessment is an engineering recommendation, not a measured productivity result.

The bootstrap host's collector does not imply a collector in emitted Chasm binaries.
Generate native programs against a separate region/resource runtime. Keep the managed interpreter a semantic reference.
The generated native runtime must use regions and explicit resource cleanup, with no tracing collector.
A production native embedded VM requires its own implementation decision; a reference interpreter does not satisfy that deliverable.
.NET Native AOT packages required runtime libraries and restricts dynamic loading; it does not establish Chasm's memory model. [Native AOT](https://learn.microsoft.com/en-us/dotnet/core/deploying/native-aot/).
Compiling the managed interpreter with Native AOT retains its managed memory model.

## Native backend and toolchain checks

**Recommend C emission first.** It permits inspecting the lowered program and using an external optimizing compiler.
This avoids committing the frontend to one native code-generation library while the semantics are still changing.
Keep C lowering behind the shared typed IR. Treat source locations, checked operations, and cleanup edges as required inputs.

**Failure.** A signed addition overflows before generated code checks its result.
C undefined behavior can invalidate that check; aliasing violations can also miscompile apparently correct memory access.
These are documented compiler constraints. [Clang overflow checks](https://clang.llvm.org/docs/UndefinedBehaviorSanitizer.html#available-checks), [GCC aliasing](https://gcc.gnu.org/onlinedocs/gcc/Optimize-Options.html#index-fstrict-aliasing).
**Close when** lowering implements the chosen arithmetic using defined operations and validates before invalid division or shifts.
Sequence side effects explicitly. Handle integer promotions, conversions, alignment, padding, bounds, and effective types deliberately.
Do not compare ordinary values using padding bytes or derive target layout from the bootstrap host's layout.
Verify exported sizes, alignments, offsets, and calling conventions against a C fixture on every claimed ABI.
Run native conformance with optimization enabled and disabled, with two C compilers, and with relevant sanitizers.
Sanitizer success is evidence for executed paths, not proof that generated C is always defined.

Direct LLVM is a later option if measured requirements justify it.
LLVM also has poison and undefined behavior; changing backend does not remove the semantic obligation. [LLVM reference](https://llvm.org/docs/LangRef.html#poison-values).
Avoid promising Rust-level performance from C or LLVM emission alone.
Compare equivalent workloads against Rust, including parsing, container mutation, retained state, promotion, and resource-heavy execution.
Report time, peak memory, copied bytes, and allocation behavior. Keep unsupported performance claims as goals.

Cross-compiling Chasm output differs from cross-compiling the bootstrap compiler itself.
.NET Native AOT does not support cross-OS compilation through its normal workflow. [Microsoft guidance](https://learn.microsoft.com/en-us/dotnet/core/deploying/native-aot/cross-compile).
A C# process can nevertheless emit C for another target and invoke that target's compiler.
Clang still needs suitable headers, libraries, linker, and ABI configuration. [Clang cross-compilation](https://clang.llvm.org/docs/CrossCompilation.html).
Rust likewise selects a target linker; cgo requires a C cross-compiler. [rustc](https://doc.rust-lang.org/rustc/codegen-options/index.html#linker), [cgo](https://pkg.go.dev/cmd/cgo).
Zig can supply cross-target tooling without becoming Chasm's host language. Check the selected target and dependencies explicitly.
Close this gate by running a cross-built fixture on its destination target, not merely producing an object file.

## Internal proof obligations and acceptance gates

The minimal semantic model needs typed storage, ownership, borrows, region validity, and resource state.
For each transition, show that live references remain valid, incompatible accesses remain excluded, and ownership obligations stay unique.
Show that permitted copying preserves observable independence and that cleanup cannot use already-finalized dependencies.
Progress and preservation are useful proof goals for a defined safe core; their scope must include permitted errors.
[PLFA](https://plfa.github.io/Properties/) demonstrates these properties for a different, smaller calculus; it proves nothing about Chasm.
Compiler correctness additionally needs behavior preservation through lowering. Typed IR alone does not provide it.
[CompCert](https://compcert.org/man/manual001.html) makes this distinction concrete through explicit semantic-preservation proofs and stated boundaries.
Recommend small executable-model checks first. Claim formal verification only for propositions actually proved about the implemented system.

1. **Resolve the counterexamples.** Write expected acceptance, rejection, or observable behavior for every ranked example above.
   Completion requires one coherent copy/borrow/region/resource model, plus unambiguous call elaboration and arithmetic rules.
   Record rejected features explicitly. An undecided behavior cannot enter the supported subset.
2. **Execute the same small core in the reference model and native output.** Include aggregates, calls, mutation, promotion, and cleanup.
   Completion requires matching traces, negative borrow tests, defined-C checks, and measured retention under repeated replacement.
   Start with eager copies and conservative escapes. Optimization must preserve this baseline's behavior.
3. **Run that core through JavaScript.** Exercise arithmetic boundaries, nested copies, errors, and resource cleanup.
   Completion requires matching portable traces and diagnostics, with explicit target restrictions for layout and raw pointers.
   JavaScript physical reclamation remains host-controlled, as the existing backend notes require.
4. **Validate each extension before admitting it.** Separate compilation must work using exported metadata alone.
   FFI must pass retention and reentrancy cases. Escaping closures, async, and unload must pass their lifetime tests.
   These gates apply when each feature enters scope; they need not all precede a useful synchronous compiler.
5. **Bootstrap after the compiler's required subset works.** Let C0 compile Chasm compiler source S into C1.
   Compile the same S with C1 to obtain C2, then with C2 to obtain C3.
   Require matching conformance results and bit-identical C2/C3 artifacts under a pinned, deterministic build environment.
   Include the runtime, libraries, backend compiler, linker, flags, paths, and generated inputs in that environment.
   Canonical IR comparisons help diagnose differences but do not replace artifact reproducibility. [Reproducible-build definition](https://reproducible-builds.org/docs/definition/).

Self-hosting demonstrates that the supported language can express and build this compiler.
Identical stages establish a fixed point for those inputs; both stages can preserve the same miscompilation.
Rust's bootstrap documentation likewise describes later-stage comparison as a sanity check. [Bootstrap stages](https://rustc-dev-guide.rust-lang.org/building/bootstrapping/what-bootstrapping-does.html).
Retain a rebuildable stage zero and a documented source-to-binary chain. A frozen bootstrap must still accept its designated source.
Bootstrap equality does not prove safety, performance, or absence of a compromised compiler.
Diverse double-compilation addresses a separate trust question under additional assumptions. [Wheeler's DDC research](https://dwheeler.com/trusting-trust/).

## Choices that need Mia

1. **The value promise.** Approve noncopyable resource ownership and an explicit boundary around foreign or captured state.
   This qualifies the current universal wording. The implementation should not decide that product contract silently.
2. **Visible promotion cost.** Decide whether an inferred escape may perform an arbitrarily large deep copy.
   Recommend explicit acknowledgement for graph reconstruction; ordinary destination-directed construction can remain inferred.
3. **Initial product emphasis.** Native application execution and production native embedding have different first deliverables.
   Recommend the synchronous native compiler first. If embedding comes first, revisit managed-VM duplication before fixing the host.

Host syntax, helper layout, eager-copy implementation, and conservative rejection can remain implementation defaults.
Challenge any implementation plan that assumes COW solves ownership, arena lifetimes prove borrow safety, or typed IR proves backend parity.
Also challenge immediate async/HMR support, universal native-speed claims, and self-hosting used as a correctness argument.

## Review of the concrete bootstrap plan

Reviewed the compiler plan, bootstrap subset, README, direction, Bend assessment, and behavior-contract proposal.
These are design findings, not observed compiler failures. No gate below has been performed.
For this subset, recommend **Rust emitting C**, with the bounded host check below.
The necessary fixes follow in priority order. Semantic changes remain proposals.

1. **Define snapshots and borrow activation before fixing the ABI.**

   The subset requires overlapping value arguments to be copied, but its ABI describes only callee-local mutation.
   In `f(saved: T, target: &mut T)`, let the body set `target.n = 2`, then print `saved.n`.
   With `x.n = 1`, `f(x, &mut x)` must print `1`, although `saved` is never mutated.
   Caller snapshots could satisfy this; the ABI must state their obligation explicitly.
   Snapshot each value when its argument expression evaluates, including before later arguments mutate its source.
   Thus `f(x, mutateAndReturn(&mut x))` also needs independence, despite no borrowed parameter reaching `f`.
   Propose reserving borrowed places during argument evaluation and activating access at invocation.
   Allow reads needed by `Array#push(&mut xs, xs[0])`; reject later argument evaluation that mutates a reserved root.
   This conservatively rejects `f(&mut xs[0], grow(&mut xs))`, which otherwise can retain an invalid element address.
   Moves must respect outstanding borrows, including borrows reserved by earlier arguments.
   Gate on both argument orders, nested data, and later mutating argument expressions.
   See [exclusivity](bootstrap-language.md#borrows-and-exclusivity), [evaluation order](bootstrap-language.md#evaluation-order), and [ABI](bootstrap-language.md#runtime-abi).

2. **Turn memory measurements into a feasibility gate before the compiler port.**

   The arena rule retains every replaced heap value in a long-lived map. Calling a helper does not reclaim those values.
   A recursive parser's fallback copy-out can also copy successively larger subtrees unless destination propagation avoids it.
   Being a batch compiler therefore does not establish acceptable peak memory or copying cost.
   Propose parser/checker/emitter workloads of N, 2N, and 4N nodes, below each lane's stack limit.
   Require native peak arena bytes and total copied bytes to grow at most 3× per doubling before the port proceeds.
   Include a skewed expression tree and a growing symbol map; record absolute peaks on a pinned reference machine.
   Before claiming long-lived-state support, test fixed-size map values overwritten 10,000 and 20,000 times.
   Propose allowing at most one live map's footprint of additional retained storage in the second run.
   The current arena-only rule fails that requirement by design; name the limitation until reclamation changes.
   Resources can remain outside the bootstrap language, but run a small synthetic-resource semantics experiment before the port.
   Check nested moves, replacement, partial construction, early errors, and finalizer dependencies with expected traces.
   This need not introduce resource syntax or real handles. Reconcile Bend's proposed early resource enforcement with the later rollout.
   See [retention gate](compiler-plan.md#conformance-and-acceptance-gates) and [region limits](bootstrap-language.md#regions-and-promotion).

3. **Correct the claims about independent checking and defined C.**

   The argument against an AST interpreter is backwards when that interpreter supplies an independent oracle.
   A shared lowering can reverse two prints while producing well-typed IR; the IR interpreter and both emitters then agree wrongly.
   The planned handwritten expectations help. Keep them, and include source-order and snapshot cases that exercise lowering.
   Keeping only an IR interpreter is reasonable; a second full interpreter is optional, not a prerequisite.
   The validator checks IR invariants, not preservation of source meaning. Checked execution covers executed paths.
   Track logical storage invalidation on overwrite and reallocation, as well as moves and region resets.
   For `xs[i] = growAndReturn(&mut xs)`, the proposed assignment rule requires resolving the element address after the RHS.
   An address cached before growth may point into retired backing storage even while its arena remains live.
   Inject this malformed lowering to check the validator/runtime; poisoning only reset arenas cannot reliably expose it.
   Omit C `restrict` initially. Later use requires a proof about actual C accesses, including aggregate views and nested backing.
   The snapshot example above violates that promise if read and mutable pointers still designate the same modified object.
   Call the runtime C11 plus GCC/Clang overflow extensions; `__builtin_*_overflow` is not an ISO C11 facility.
   “Conformance carries correctness” overstates the evidence. Tests, sanitizers, and fixed points establish only their stated checks.
   See [interpreter](compiler-plan.md#typed-ir-and-the-reference-interpreter), [C backend](compiler-plan.md#native-backend-c-first), and [self-host test](compiler-plan.md#self-host-completion-test).

4. **Separate native self-hosting from replacement of every host component.**

   Milestone 2 ports the frontend, validator, C emitter, and driver. The interpreter and JS emitter wait until milestone 3.
   Consequently, “chasm3 passes ... on every lane” does not identify which implementation produces each result.
   Label milestone 2 native-compiler self-hosting. Keep the host interpreter and JS backend operational as explicit reference lanes.
   Compare stage3 native results against those references; host-only runs do not exercise the new frontend through those backends.
   If that coverage is required before milestone 3, define a validated IR interchange that the retained backends actually consume.
   By milestone 3, build and run the Chasm interpreter and JS emitter with the new compiler, then rerun the complete suite.
   Record compiler/frontend/backend identity per lane. Preserve all supported behavior and commands before freezing the host.
   Keep the existing stage equality checks, but distinguish the native fixed point from completion of those ports.

5. **Align portable guarantees and proposed governance across documents.**

   Scope backend parity to specified observations, supported capabilities, and execution within stated resource limits.
   The subset's universal parity claim conflicts with its exhaustion exceptions and Node-specific process-exit panic contract.
   “Region choice never changes behavior” needs valid placements, unobserved allocation counters, and those resource-limit exclusions.
   A wrong placement is something checking aims to detect; no current validator or execution check guarantees detection.
   Cleanup accounting must return to the caller's baseline after a call, retaining live result storage.
   A `break` inside an active function does not require its call region, or every caller's region, to disappear.
   The behavior-contract note correctly leaves syntax, enforcement, capabilities, and test-versus-proof distinctions open.
   Preserve that status when reconciling Bend's “adopt now” CI restrictions and the plan's expectation-change approval requirement.
   A browser contract using DOM capabilities cannot run unchanged under Node or a native host without those capabilities.
   Separate portable contracts from target contracts; unsupported, pending, and failed checks must remain distinct.
   Human ownership and approval policies are proposals, not already-adopted language semantics or project governance.

**Host decision.** Rust is a reasonable provisional default because enums and checked host borrowing support the proposed indexed design.
Managed object graphs would need a different representation in this subset.
C# 15's documented unions are real, but its .NET 11 toolchain is prerelease on this review date.
F# already supplies stable discriminated unions and can use the same arrays, indices, and free-function style.
TypeScript has `BigInt`, used by the proposed JS lane itself; OCaml has `Int64`.
Their integer ergonomics differ, but the host table must not treat 64-bit support as absent.
Rust borrowing does not enforce Chasm's implicit-copy semantics, allocation placement, or every stricter borrow restriction.
A mechanical port is therefore a hypothesis. No-crates and no-closures rules are implementation preferences, not correctness gates.
Before broad host investment, trial one indexed parser/lowering pass with nested values and borrowed mutation.
Add an IR variant and require every semantic consumer to diagnose its omission; account for copies in a proposed Chasm translation.
Use stable F# if Rust needs disproportionate host-specific scaffolding. If the target subset cannot express the pass, fix that subset first.
