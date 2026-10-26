# Behavior contracts

> Status: open design proposal, 2026-10-26. No syntax or enforcement policy adopted.

Mia proposes making Chasm itself test-driven: describe behavior before supplying
its implementation. A separate Opus review recommends a module contract with signatures,
executable examples, and properties. Documentation and tooling consume that
contract. Contract checks run separately from application execution.

## Proposed development model

1. Write a module's observable behavior beside its signatures.
2. Supply implementations incrementally. A missing body is an explicit hole.
   A check that encounters one reports pending; an assertion mismatch reports failure.
3. Rerun affected checks as code changes. Pending work and failing checks remain
   visible during development. Runtime behavior stays consistent across build modes.
4. Proposed: a release gate over selected contract checks. No policy is adopted.
   The exact enforcement boundary remains open.

A codec illustrates the model: examples for encoding can pass while decoding
is unfinished. A round-trip property exercises both exports and remains pending
until both implementations exist. One behavior may cover several exports.

## Useful enforcement

The proposed unit is a module's exported contract. A quota of tests per function
encourages tests that merely call a function. Coverage records execution;
it cannot establish that the assertions express useful behavior.

Effect-free checks can use pinned seeds and reproducible inputs. Cache keys must
include the contract, its dependency closure, and the toolchain. Host checks need
explicit capabilities and a recorded environment; they cannot inherit the same
cache assumptions.

Portable contracts should agree across the reference evaluator, native output,
and JavaScript. Host-specific contracts need their declared target environment.
Requiring every application to run on every backend would conflict with Chasm's
explicit target capabilities. Backend selection and the release gate need a
precise policy before implementation.

## Keeping the contract meaningful

A passing example establishes the observed result for that example. A property
run reports the inputs tried, seed, and counterexamples. Neither establishes a
universal proof. A proof, if Chasm adds proofs later, covers only its stated
proposition under its trusted assumptions. Trivial contracts can pass trivially.

Changing expected output, narrowing generated inputs, or changing a helper that
decides success changes the contract. Review must include those dependencies.
The [Bend assessment](bend-lessons.md) explains why protecting a law file alone
does not protect its meaning. Recommended: review contract changes separately
from implementation changes. A compiler cannot establish that tests capture intent.

## Questions before adoption

- Which exports must have declared behavior, and what does the release gate check?
- How do holes propagate through checks, and how are skipped paths reported so
  pending work cannot conceal an assertion failure?
- How do effectful checks declare their environment, capabilities, and time bounds?
- What invalidates cached results across dependencies, generated inputs, and backends?
- Which parts belong in the language, and which belong in its standard build tool?

This is a later design track alongside the [compiler plan](compiler-plan.md).
The first compiler slice needs ordinary conformance checks; first-class application
contracts need the execution, error, and effect rules they will exercise.
