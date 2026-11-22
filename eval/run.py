#!/usr/bin/env python3
"""Agent token-cost eval: the same tasks in Chasm, Lua and Rust.

An agent gets a task, replies with a whole program, and sees the compiler
error or the first wrong output line until it passes or runs out of attempts.
Every API token in that loop counts, including the language reference.
"""

import argparse
import concurrent.futures
import datetime
import json
import os
import pathlib
import re
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
TASKS = ROOT / "eval" / "tasks"
CHASM = ROOT / "target" / "release" / "chasm"
API = "https://api.anthropic.com/v1"
LANGS = ("chasm", "lua", "rust")
EXT = {"chasm": "chasm", "lua": "lua", "rust": "rs"}

COMMON = (
    "You write complete single-file programs. The program reads no input and "
    "writes to standard output. Reply with exactly one fenced code block that "
    "holds the whole program, and nothing else."
)


def system_prompt(lang):
    if lang == "lua":
        return COMMON + "\n\nWrite Lua 5.4. It runs with `lua program.lua`."
    if lang == "rust":
        return (
            COMMON + "\n\nWrite Rust (edition 2021, standard library only). It "
            "builds with `rustc --edition 2021 program.rs` and then runs."
        )
    alpha = (ROOT / "docs" / "alpha.md").read_text()
    reference = alpha[alpha.index("## Language") : alpha.index("## Implementation")]
    example = (ROOT / "examples" / "fizzbuzz.chasm").read_text()
    return (
        COMMON + "\n\nWrite Chasm, a new language. It runs with "
        "`chasm run program.chasm`. This is its complete reference.\n\n"
        + reference
        + "\n## Example\n\n```chasm\n"
        + example
        + "```"
    )


def api(path, body, key):
    request = urllib.request.Request(
        f"{API}/{path}",
        data=json.dumps(body).encode(),
        headers={
            "x-api-key": key,
            "anthropic-version": "2023-06-01",
            "content-type": "application/json",
        },
    )
    for attempt in range(8):
        try:
            with urllib.request.urlopen(request, timeout=300) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            if error.code not in (429, 500, 502, 503, 529) or attempt == 7:
                raise RuntimeError(f"{error.code}: {error.read().decode()}") from error
        except (urllib.error.URLError, TimeoutError):
            if attempt == 7:
                raise
        time.sleep(2**attempt)


def reference_tokens(lang, model, key):
    """Input tokens the system prompt adds to each request."""
    messages = [{"role": "user", "content": "x"}]
    bare = api("messages/count_tokens", {"model": model, "messages": messages}, key)
    full = api(
        "messages/count_tokens",
        {"model": model, "system": system_prompt(lang), "messages": messages},
        key,
    )
    return full["input_tokens"] - bare["input_tokens"]


def extract(reply):
    blocks = re.findall(r"```[\w+-]*\n(.*?)```", reply, re.S)
    return blocks[-1] if blocks else reply


# Generated programs run without the API key in their environment.
SANDBOX_ENV = {k: v for k, v in os.environ.items() if k != "ANTHROPIC_API_KEY"}


def run(lang, source, timeout=10):
    """Return (exit status, stdout, stderr) for one program."""
    with tempfile.TemporaryDirectory() as directory:
        path = pathlib.Path(directory) / f"program.{EXT[lang]}"
        path.write_text(source)
        if lang == "chasm":
            command = [str(CHASM), "run", str(path)]
        elif lang == "lua":
            command = ["lua", str(path)]
        else:
            binary = pathlib.Path(directory) / "program"
            try:
                built = subprocess.run(
                    ["rustc", "--edition", "2021", "-o", str(binary), str(path)],
                    capture_output=True, text=True, timeout=120, env=SANDBOX_ENV,
                )
            except subprocess.TimeoutExpired:
                return None, "", "rustc timed out after 120 seconds"
            if built.returncode != 0:
                return built.returncode, "", built.stderr
            command = [str(binary)]
        try:
            result = subprocess.run(
                command, capture_output=True, text=True, timeout=timeout, cwd=directory,
                env=SANDBOX_ENV,
            )
        except subprocess.TimeoutExpired:
            return None, "", f"timed out after {timeout} seconds"
        return result.returncode, result.stdout, result.stderr.replace(directory + "/", "")


def judge(lang, source, expected):
    """Return None when the program passes, else feedback for the agent."""
    status, stdout, stderr = run(lang, source)
    if status != 0:
        return f"The program failed (exit status {status}):\n```\n{stderr[-2000:]}\n```"
    got = stdout.rstrip().splitlines()
    want = expected.rstrip().splitlines()
    if [line.rstrip() for line in got] == want:
        return None
    for index, line in enumerate(want):
        if index >= len(got):
            return f"The output stops after line {index}; line {index + 1} should be `{line}`."
        if got[index].rstrip() != line:
            return f"Output line {index + 1} is `{got[index]}`; it should be `{line}`."
    return f"The output has {len(got)} lines; it should have {len(want)}."


def attempt_task(task, lang, trial, args, key):
    prompt = (TASKS / task / "task.md").read_text()
    expected = (TASKS / task / "expected.txt").read_text()
    messages = [{"role": "user", "content": prompt}]
    record = {"task": task, "lang": lang, "trial": trial, "passed": False,
              "attempts": 0, "input_tokens": 0, "output_tokens": 0, "feedback": []}
    for _ in range(args.max_attempts):
        response = api(
            "messages",
            {"model": args.model, "max_tokens": 4096, "temperature": args.temperature,
             "system": system_prompt(lang), "messages": messages},
            key,
        )
        record["attempts"] += 1
        record["input_tokens"] += response["usage"]["input_tokens"]
        record["output_tokens"] += response["usage"]["output_tokens"]
        reply = "".join(part.get("text", "") for part in response["content"])
        source = extract(reply)
        record["source"] = source
        feedback = judge(lang, source, expected)
        if feedback is None:
            record["passed"] = True
            break
        record["feedback"].append(feedback)
        messages += [
            {"role": "assistant", "content": reply},
            {"role": "user", "content": feedback + "\n\nReply with the whole corrected program."},
        ]
    return record


def summarize(records, ref_tokens, args):
    lines = [
        f"Model `{args.model}`, temperature {args.temperature}, {args.trials} trials, "
        f"up to {args.max_attempts} attempts per run.",
        "",
        "| Language | Pass rate | First-try pass | Tokens per correct change "
        "| Without reference | Output tokens per correct change | Reference tokens |",
        "| --- | --- | --- | --- | --- | --- | --- |",
    ]
    for lang in LANGS:
        rows = [r for r in records if r["lang"] == lang]
        if not rows:
            continue
        passed = sum(r["passed"] for r in rows)
        first = sum(r["passed"] and r["attempts"] == 1 for r in rows)
        total = sum(r["input_tokens"] + r["output_tokens"] for r in rows)
        reference = sum(r["attempts"] for r in rows) * ref_tokens[lang]
        output = sum(r["output_tokens"] for r in rows)
        per = lambda value: f"{value / passed:,.0f}" if passed else "n/a"
        lines.append(
            f"| {lang} | {passed}/{len(rows)} | {first}/{len(rows)} | {per(total)} "
            f"| {per(total - reference)} | {per(output)} | {ref_tokens[lang]:,} |"
        )
    failures = [r for r in records if not r["passed"]]
    if failures:
        lines += ["", "Failed runs:", ""]
        for r in sorted(failures, key=lambda r: (r["lang"], r["task"], r["trial"])):
            lines.append(f"- {r['lang']} `{r['task']}` trial {r['trial']}: {r['feedback'][-1].splitlines()[0]}")
    return "\n".join(lines) + "\n"


def self_test(tasks, langs):
    ok = True
    for task in tasks:
        expected = (TASKS / task / "expected.txt").read_text()
        for lang in langs:
            feedback = judge(lang, (TASKS / task / f"solution.{EXT[lang]}").read_text(), expected)
            ok &= feedback is None
            print(f"{'ok  ' if feedback is None else 'FAIL'} {lang} {task}" + (f": {feedback}" if feedback else ""))
    return ok


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", default="claude-sonnet-4-5")
    parser.add_argument("--langs", default=",".join(LANGS))
    parser.add_argument("--tasks", default="", help="comma-separated task names; default all")
    parser.add_argument("--trials", type=int, default=3)
    parser.add_argument("--max-attempts", type=int, default=3)
    parser.add_argument("--temperature", type=float, default=1.0)
    parser.add_argument("--jobs", type=int, default=8)
    parser.add_argument("--self-test", action="store_true",
                        help="check the reference solutions; no API calls")
    args = parser.parse_args()

    langs = [lang for lang in args.langs.split(",") if lang]
    tasks = args.tasks.split(",") if args.tasks else sorted(p.name for p in TASKS.iterdir() if p.is_dir())
    subprocess.run(["cargo", "build", "--release", "--quiet"], cwd=ROOT, check=True)
    if args.self_test:
        raise SystemExit(0 if self_test(tasks, langs) else 1)

    key = os.environ.get("ANTHROPIC_API_KEY")
    if not key:
        raise SystemExit("Set ANTHROPIC_API_KEY.")
    ref_tokens = {lang: reference_tokens(lang, args.model, key) for lang in langs}
    jobs = [(t, l, n) for t in tasks for l in langs for n in range(1, args.trials + 1)]
    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    out = ROOT / "eval" / "results" / f"{stamp}-{args.model}"
    out.mkdir(parents=True)
    records, skipped = [], 0
    with concurrent.futures.ThreadPoolExecutor(args.jobs) as pool, \
            open(out / "runs.jsonl", "w") as file:
        futures = {pool.submit(attempt_task, *job, args, key): job for job in jobs}
        for future in concurrent.futures.as_completed(futures):
            try:
                record = future.result()
            except Exception as error:
                print(f"skipped {futures[future]}: {error}", file=sys.stderr)
                skipped += 1
                continue
            records.append(record)
            file.write(json.dumps(record) + "\n")
            file.flush()
    summary = summarize(records, ref_tokens, args)
    if skipped:
        summary += f"\n{skipped} runs errored before finishing and are not counted.\n"
    (out / "summary.md").write_text(summary)
    print(summary)
    print(f"Wrote {out.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
