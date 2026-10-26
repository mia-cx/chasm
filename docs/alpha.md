# Chasm alpha

> **Status:** `0.1.0-alpha.1`, an early alpha of the stage-zero host in `host/`.
> It implements a scalar slice of the [bootstrap language](bootstrap-language.md),
> roughly slices 1 and 2 of the [compiler plan](compiler-plan.md). It is not the
> bootstrap subset, it cannot compile itself, and it has no borrows, regions, or
> resources.

This document is the contract for what the alpha accepts and how programs behave.
Anything it does not list is rejected with a diagnostic.

## Commands

```text
chasm check PATH                 check a program; print nothing on success
chasm run PATH                   check, then interpret the program
chasm emit c PATH                check, then print the generated C to stdout
chasm build PATH -o OUTPUT       check, generate C, and compile it to a native executable
chasm help                       print usage (also -h, --help)
chasm --version                  print the version (also -V)
```

`PATH` names one `.chasm` source file; directories and multi-file packages are not
supported. The extension is not enforced. Run from the repository with
`cargo run -- <command> ...`.

`build` passes the generated C to the C compiler on standard input, so no
temporary C file is written. The compiler is the program named by the `CC`
environment variable, or `cc` when `CC` is unset or empty. `CC` names one
program, run directly without a shell; it is not split into arguments. The
command line is `$CC -std=c11 -O2 -x c - -o OUTPUT`. GCC and Clang are the
supported compilers, because the generated code uses `__builtin_*_overflow`.
`build` refuses an `OUTPUT` that is the source file itself, including through a
symlink or hard link.

### Exit status

| Status | Meaning |
| --- | --- |
| 0 | success |
| 1 | the program has an error, a file cannot be read, or the C compiler failed |
| 2 | invalid command line |
| 70 | internal compiler error: the IR validator rejected the compiler's own output |
| 101 | the program panicked (`run`, and native executables) |

### Diagnostics

Compile errors go to stderr, one per line:

```text
PATH:LINE:COLUMN: error: MESSAGE
```

Lines and columns start at 1. Columns count Unicode scalar values, so a tab is
one column. The lexer and parser stop at the first error in the file. The checker
reports the first error in each function, in source order.

## Language

### Source text

- A source file is UTF-8 text.
- `//` starts a comment that runs to the end of the line. There are no block
  comments.
- Spaces, tabs, and carriage returns separate tokens. Newlines end statements
  (see [Statement ends](#statement-ends)).
- Identifiers match `[A-Za-z_][A-Za-z0-9_]*`. Identifiers are ASCII only;
  non-ASCII text may appear only in strings and comments.
- Keywords: `fn let if else while return true false and or not`.
- Reserved words, rejected wherever they appear with "this alpha does not
  support": `struct enum match for in break continue try import export async
  await region trait mut`.

### Statement ends

A newline ends a statement when it follows a token that can end one: an
identifier, an integer or string literal, `true`, `false`, `return`, `)`, or `}`.
Newlines inside parentheses never end a statement, so argument and parameter
lists may span lines. Other newlines are ignored, so a line ending in `+`, `,`,
`(`, `{`, or `=` continues on the next line.

Consequences:

- `else` must be on the same line as the `}` before it.
- A line that starts with a binary operator does not continue the line above.
- Semicolons are not used and are rejected.

### Literals

- **Integers** are decimal digits with no sign, separators, or leading zeros
  (`0` itself is fine). A literal must fit in `Int`. A unary `-` applied to a
  literal belongs to the literal, so `-2147483648` is valid while `2147483648`
  alone, or after a binary `-` as in `1 - 2147483648`, is not.
- **Booleans** are `true` and `false`.
- **Strings** are written in double quotes and cannot span lines. Escapes:
  `\n` newline, `\t` tab, `\r` carriage return, `\\` backslash, `\"` quote. Any
  other backslash sequence is an error. A raw NUL character is an error. Every
  other character, including tabs and non-ASCII UTF-8, stands for itself.

### Types

| Type | Values |
| --- | --- |
| `Int` | signed 32-bit integers, -2147483648 to 2147483647 |
| `Bool` | `true`, `false` |
| `String` | immutable UTF-8 text from string literals |

There are no implicit conversions. A function without `-> Type` returns no value;
calls to it may only be used as statements. Other type names, including `I64`,
`U8`, `Unit`, `Array`, `Option`, and `Result`, are rejected.

### Grammar

```ebnf
file      = { fn_decl } ;
fn_decl   = "fn" IDENT "(" [ params ] ")" [ "->" type ] block ;
params    = param { "," param } [ "," ] ;
param     = IDENT ":" type ;
type      = "Int" | "Bool" | "String" ;

block     = "{" { stmt END } "}" ;          (* END: a statement-ending newline, or before "}" *)
stmt      = let | assign | if | while | return | expr ;
let       = "let" IDENT [ ":" type ] "=" expr ;
assign    = IDENT "=" expr ;
if        = "if" expr block [ "else" ( block | if ) ] ;
while     = "while" expr block ;
return    = "return" [ expr ] ;

expr      = or ;
or        = and { "or" and } ;
and       = not { "and" not } ;
not       = "not" not | compare ;
compare   = sum [ ( "==" | "!=" | "<" | "<=" | ">" | ">=" ) sum ] ;
sum       = product { ( "+" | "-" ) product } ;
product   = unary { ( "*" | "/" | "%" ) unary } ;
unary     = "-" unary | primary ;
primary   = INT | STRING | "true" | "false" | IDENT | call | "(" expr ")" ;
call      = IDENT "(" [ args ] ")" ;
args      = arg { "," arg } [ "," ] ;
arg       = [ IDENT ":" ] expr ;
```

Precedence runs from loosest to tightest: `or`, `and`, `not`, comparisons,
`+ -`, `* / %`, unary `-`, then calls and parentheses. Binary arithmetic is
left-associative. Comparisons do not chain: `a < b < c` is an error.

Nesting is limited to 200 levels. Each block, `else if`, parenthesized
expression, call argument, condition, unary `-` or `not`, and each binary
operator in a chain such as `1 + 2 + 3` adds a level while it is open, so a
chain of more than about 200 operators in one expression is rejected. Split
long expressions with `let`.

### Declarations and names

- A file is a list of functions. Nothing else may appear at the top level.
- A program needs `fn main()`, with no parameters and no return type.
- Function names are unique. `print` is a prelude function and cannot be
  redefined.
- Functions may call each other in any order, including recursively.
- Functions and variables have separate namespaces; a call always names a function.
- Parameters and `let` bindings are variables, and every variable is mutable.
  A `let` is visible from the next statement to the end of its block.
- A variable name cannot be declared while another variable with that name is
  visible. Sibling blocks may reuse a name.
- A `let` without a type takes the type of its value.

### Statements

- `let name = value` and `let name: Type = value` declare and initialize a
  variable.
- `name = value` assigns a variable of the same type.
- An expression statement evaluates its expression and discards the value.
- `if` and `while` conditions must be `Bool`. Each `if` and `while` body is its own
  block scope.
- `return value` returns from a function with a return type; `return` alone
  returns from a function without one. A function with a return type must
  return on every path. The check is structural: a path returns when it ends
  in `return`, or in an `if` whose branches all return. A `while` loop never
  counts, so `while true { return 1 }` still needs a `return` after it.

### Expressions

| Operators | Operands | Result |
| --- | --- | --- |
| `+ - * / %` | `Int`, `Int` | `Int` |
| unary `-` | `Int` | `Int` |
| `< <= > >=` | `Int`, `Int` | `Bool` |
| `== !=` | two `Int` or two `Bool` | `Bool` |
| `and or` | `Bool`, `Bool` | `Bool` |
| `not` | `Bool` | `Bool` |

Strings support no operators; they can be bound, passed, returned, and printed.

Arithmetic is checked. `+`, `-`, `*`, and unary `-` panic with `integer overflow`
when the true result does not fit in `Int`. `/` and `%` truncate toward zero; a
zero divisor panics with `division by zero`. `-2147483648 / -1` panics with
`integer overflow`, and `-2147483648 % -1` is `0`. Arithmetic on literals is not
folded at compile time, so `2147483647 + 1` compiles and panics when it runs.

### Calls

A call names a function and gives one argument per parameter. Arguments are
positional, named (`name: value`), or a positional prefix followed by named
arguments. Positional arguments fill parameters in order; each named argument
fills the parameter with that name. Every parameter must be filled exactly once.
Default values are not supported.

`print(value)` is the only prelude function. It takes one `Int`, `Bool`, or
`String` argument (named `value` when passed by name), returns no value, and
writes the value followed by `\n` to stdout. Integers print in decimal with a
leading `-` when negative, booleans print as `true` or `false`, and strings
print their bytes unchanged.

### Evaluation order

Evaluation runs left to right, always:

- Binary operands evaluate left, then right, then the operator applies.
- Call arguments evaluate in the order written, whether positional or named.
  Then the call runs.
- `and` evaluates its right side only when the left is `true`; `or` only when
  the left is `false`.
- An assignment or `let` evaluates its value, then stores it.
- A `while` condition evaluates before every iteration.

## Runtime behavior

A panic prints its message and the source location of the operator or call
that caused it to stderr, then exits with status 101:

```text
panic: integer overflow
  at examples/overflow.chasm:2:22
```

The location's path is the `PATH` given to `run` or `build`. Output printed
before the panic is written to stdout first.

| Panic message | Cause |
| --- | --- |
| `integer overflow` | `+ - *`, unary `-`, or `/` produced a result outside `Int` |
| `division by zero` | `/` or `%` with a zero divisor |
| `call depth limit exceeded` | a call would make more than 10000 calls active at once, counting `main` |

The interpreter and native executables apply the same call depth limit, so deep
recursion panics identically on both. Native frames must still fit the
platform's stack; with the usual 8 MiB main-thread stack they do. Infinite
loops run forever on both lanes.

A program that returns from `main` exits with status 0. Write errors on stdout
are not part of the portable contract: the interpreter reports one as an error
with status 1, while native executables ignore them.

## Implementation

`host/` is a Rust crate with no dependencies, pinned to Rust 1.97.0.

```text
source → lexer → parser → checker + lowering → typed IR → IR validator
                                                            ├── interpreter (run)
                                                            └── C emitter → cc (emit c, build)
```

- The IR is structured: blocks, `if`, `loop`, `break`, and `return`. Every
  operand is a variable or a constant, and every operation stores into a
  variable, so evaluation order is fixed in the IR rather than left to C.
- The validator independently rechecks types, variable and string indices,
  `break` placement, and that value-returning functions return on every path. It
  catches some lowering bugs; it is not a soundness proof.
- The interpreter is the reference lane. The C backend must match its stdout,
  stderr, and exit status on every program.
- Generated C uses `__builtin_*_overflow` for checked arithmetic and never uses
  program text as a format string. String literals and the source path are
  emitted with octal escapes for every byte outside printable ASCII and for
  `"`, `\`, and `?`.

## Not in this alpha

Structs, enums, `match`, arrays, maps, `Option`, `Result`, `try`, explicit `panic`,
`for`, `break`, `continue`, default arguments, string operations, `I64`, `U8`,
floats, modules and `::` or `#` paths, borrows (`&`, `&mut`), regions, resources,
closures, generics, async, the JavaScript backend, and token, AST, or IR dumps.
Each is rejected with a diagnostic where the lexer or parser can recognize it.
