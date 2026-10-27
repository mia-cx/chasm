# Agent authoring

Chasm is designed primarily for agents to write and read. People must still be
able to understand the source. Token efficiency must preserve unambiguous syntax,
explicit mutation, and ownership contracts.

## What to measure

Measure tokens per correct change across the complete task. Include source read
and written, retrieved interfaces, tool output, diagnostics, and repair attempts.
Record correctness and repair counts alongside tokens. Shorter source alone
does not establish a better result.

Compare equivalent tasks against Java and compact implementations in TypeScript,
Python, and Rust. Pin each model, tokenizer, task, and available tools.
Any claim about token savings needs measurements; none have been performed.

## Candidates

- [File-local imports](modules.md#imports) shorten repeated module paths. An
  explicit `as` alias handles name conflicts.
- [Same-name shorthand](parameter-passing.md#same-name-shorthand) proposes
  `name:` for `name: name`, including named constructor fields.
- Infer local types while keeping function parameter and return types explicit.
- Use `try` to propagate failures from `Result`-returning functions. This does
  not define an error-returning `main` or implicit process-exit convention.
- Prefer direct data and function APIs. Explicit barrel exports and structural
  parameter projection can reduce forwarding wrappers, subject to their existing gates.

Expected-type lookup for short enum variant names remains gated in the
[bootstrap proposal](bootstrap-language.md#modules-names-and-syntax).
The prelude already supplies names such as `Ok` and `Err` in that proposal.

## Whitespace and formatting

Choose a canonical formatter with predictable line breaks and indentation.
Measure indentation widths, blank lines, and multiline calls with the target tokenizers.
Tokenizers can encode runs of spaces and newlines compactly; character savings
need not produce token savings. Formatting must preserve the language's newline
and statement-termination rules. No indentation-width change is adopted here.

## Illustrative HTTP server

This is a proposal example, not a program the scalar alpha can run.
The HTTP library, imports, resources, and argument shorthand are unimplemented.
`http#respond` returns `Result<Int, http::Error>` here, with the sent status on success.
The exact HTTP API and resource transfers remain subject to their lifetime and ownership gates.

```chasm
import std::net::http

fn route(request: &http::Request) -> http::Response {
    if request.path == "/hello" {
        return http#text(status: 200, body: "Hello from Chasm!")
    }
    return http#text(status: 404, body: "Not found")
}

fn handle(server: &mut http::Server) -> Result<Int, http::Error> {
    let request = try http#accept(&mut server)
    let response = route(&request)
    return http#respond(request, response:)
}

fn main() {
    let server = match http#listen(host: "127.0.0.1", port: 8080) {
        Ok(server) => server
        Err(error) => panic(message: error.message)
    }

    while true {
        match handle(&mut server) {
            Ok(status) => print(status)
            Err(error) => print(error.message)
        }
    }
}
```

`response:` expands to `response: response`. The borrow markers stay explicit.
`try` propagates an `accept` failure from `handle`; `main` reports per-connection errors.

This sketch gives `accept` an `&mut Server` parameter, so `handle` must receive
mutable access from its caller. If `accept` only needs `&Server`, the built-in
lint should flag unnecessary mutable access in `handle` once all its uses allow read access.
Doing I/O alone does not determine the required borrow type.
