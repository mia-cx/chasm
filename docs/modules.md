# Chasm Design Notes: Modules

> **Status:** Draft
>
> Chasm derives its module hierarchy from the filesystem. Each source file defines one module, while explicit barrel modules control directory-level public APIs.

## One File, One Module

Every Chasm source file defines exactly one module. The filesystem determines the module hierarchy.

```text
src/
    math.chasm            → math
    fs.chasm              → fs
    json.chasm            → json

    net/
        http.chasm        → net::http
        websocket.chasm   → net::websocket
```

Ordinary source files do not require a `module` declaration.

This keeps:

- Navigation obvious
- Imports predictable
- Hot module replacement granular
- Incremental compilation straightforward
- Tooling simple

## Nested Modules

A source file may optionally declare nested modules:

```chasm
module testing {
    ...
}
```

A nested module creates a nested namespace inside the source file's module. It does not define an additional top-level module.

## Barrel Modules

A directory may expose a curated public API through a dedicated `module.chasm` barrel module.

```text
net/
    http.chasm
    websocket.chasm
    module.chasm
```

Unlike an ordinary source filename, `module.chasm` represents the enclosing directory's module. In this example, it defines the public surface of `net` rather than a `net::module` namespace.

```chasm
export http#request
export http::Client

export websocket#connect
```

Consumers can use the flattened public API:

```chasm
net#request(...)
net::Client

net#connect(...)
```

Without the barrel exports, consumers address the child modules directly:

```chasm
net::http#request(...)
net::http::Client

net::websocket#connect(...)
```

A barrel changes the directory module's public surface. It does not change where the exported declaration is implemented.

## Explicit Exports Only

Barrel modules support only explicit exports.

Allowed:

```chasm
export http#request
export http::Client
```

Not allowed:

```chasm
export *
```

Requiring explicit exports improves:

- Discoverability
- API stability
- Tooling
- Protection against accidental public APIs

## Guiding Philosophy

Filesystem layout should closely match module structure. Reading a project tree should immediately reveal the namespace hierarchy without requiring additional language syntax.
