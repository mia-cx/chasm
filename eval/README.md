# Agent token-cost eval

Measures what it costs an agent to write the same programs in Chasm, Lua and
Rust, as [agent authoring](../docs/agent-authoring.md) defines it: tokens per
correct change, including context, diagnostics and repair attempts.

## Method

Each of the ten [tasks](tasks/) asks for a short program that prints fixed
output, using only what the [alpha](../docs/alpha.md) can express: `Int`,
`Bool`, string literals, functions, `if`, `while` and recursion.

For every task, language and trial, the model gets the task and replies with a
whole program. The harness runs it (`chasm run`, `lua`, or `rustc` then the
binary) and compares stdout with `expected.txt`. On failure the model sees the
error output or the first wrong line and tries again, up to three attempts.

The Chasm system prompt carries the alpha's language reference and one example,
because no model knows Chasm. Lua and Rust get one line naming the toolchain.
Every token of every request and reply counts.

| Column | Meaning |
| --- | --- |
| Pass rate | runs that passed within the attempt limit |
| First-try pass | runs that passed on the first attempt |
| Tokens per correct change | all input and output tokens, divided by passing runs |
| Without reference | the same, minus the system prompt's tokens on every request |
| Output tokens per correct change | tokens the model wrote, divided by passing runs |
| Reference tokens | tokens the system prompt adds to each request |

## Run

Needs Python 3, Lua 5.4, GCC or Clang, and an Anthropic API key.

```sh
python3 eval/run.py --self-test                 # reference solutions pass; no API calls
ANTHROPIC_API_KEY=... python3 eval/run.py       # all tasks, 3 trials each
python3 eval/run.py --model claude-haiku-4-5 --tasks fib,isqrt --trials 5
```

Results go to `eval/results/<time>-<model>/`: `summary.md` and one JSON line
per run, with its final source and feedback.

## Adding a task

Add `tasks/<name>/` with `task.md`, `expected.txt`, and a reference solution in
each language (`solution.chasm`, `solution.lua`, `solution.rs`), then run
`--self-test`.
