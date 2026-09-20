# Chasm Design Notes: Interoperability

> **Status:** Draft
>
> Chasm should provide one typed module surface over Chasm, native, JavaScript, and Lua implementations. Module and invocation boundaries should feel lightweight while type, ownership, resource, and runtime boundaries remain explicit.

## Guiding Rule

> **Blur module and invocation boundaries, but keep type, ownership, and runtime boundaries honest.**

Interop should feel like ordinary Chasm code after a foreign interface has been declared. The declaration is responsible for describing conversions and boundary behavior.

## Interop Layers

Chasm should distinguish three external interfaces:

1. **C ABI** for native libraries and portable embedding.
2. **JavaScript host interface** for browsers, Node.js, workers, and npm.
3. **Lua host interface** for lightweight embedded scripting and plugins.

The internal Chasm native ABI and bytecode module ABI remain separate from these external interfaces.

## C ABI Foundation

The C ABI is the native interoperability baseline. It supports importing and exporting functions built from explicitly ABI-safe types:

- Fixed-width integers and floats
- Explicitly represented booleans
- Raw pointers
- Function pointers
- Opaque handles
- C-layout structs
- Pointer-and-length records

Ordinary Chasm strings, arrays, traits, closures, structural parameter objects, and errors do not cross the C boundary without an explicit representation.

C++, Rust, and other native languages should normally interoperate through C-compatible shims rather than unstable language-specific ABIs.

## Typed JavaScript and Lua Modules

Foreign modules may expose typed Chasm contracts.

Conceptually:

```chasm
extern "js" module npm::"sharp" as sharp {
    fn resize(args: {
        image: Bytes
        width: Int
        height: Int
    }) -> Promise<Bytes>
}
```

```chasm
extern "lua" module "./plugin.lua" as plugin {
    fn update(args: {
        world: &mut World
        delta: Float
    })
}
```

The exact declaration syntax remains undecided.

Once declared, calls use normal Chasm namespace and argument syntax:

```chasm
sharp#resize(
    image: image,
    width: 800,
    height: 600,
)

plugin#update(
    world: &mut world,
    delta: delta,
)
```

## Structural Argument Mapping

Chasm's conceptual single-argument model maps naturally to JavaScript objects and Lua tables:

```chasm
compiler#build(
    path: path,
    optimize: true,
)
```

```js
compiler.build({
  path,
  optimize: true,
});
```

```lua
compiler.build {
    path = path,
    optimize = true,
}
```

A JavaScript object or Lua table may satisfy a declared Chasm parameter shape when it contains compatible required fields. Extra fields are ignored by the structural projection.

Dynamic values entering typed Chasm code require validation or a trusted binding contract.

## Crossing Values

Interop distinguishes three categories of values.

### Portable Values

Values copied or converted between runtimes:

- Booleans
- Integers and floats
- Strings
- Byte buffers
- Arrays
- Plain records
- Options and results

### Borrowed Views

Storage shared temporarily without copying:

- Byte buffers
- Strings
- Numeric arrays
- Read-only records

A borrowed view normally expires when the foreign call returns unless a wider lifetime is explicit.

### Foreign Handles

Objects that retain their original runtime identity:

```chasm
JsValue
LuaValue
```

A `JsValue` is owned by the JavaScript garbage collector. A `LuaValue` may be represented by a rooted Lua registry reference. Chasm regions own the foreign roots or handles, not the underlying objects directly.

Destroying the Chasm region releases those roots and lets the foreign runtime reclaim the object according to its own rules.

## Callbacks

Plain foreign function pointers are the simplest callback form. Closures additionally contain captured environments and require explicit lifetime management.

A callback retained by foreign code must keep its Chasm code and captured environment alive. Unloading a plugin or destroying its region must invalidate or unregister callbacks before releasing their storage.

JavaScript's hidden `this` receiver and Lua's colon-call receiver become explicit in Chasm bindings rather than introducing hidden receiver semantics into Chasm itself.

## Errors

Foreign exceptions, rejected promises, Lua errors, and C status codes must be translated at the boundary. No native exception or panic may unwind through an incompatible ABI.

Bindings must define how failures become Chasm errors and how Chasm failures are reported when foreign code calls Chasm.

## JavaScript and npm

Node.js should be a first-class Chasm host for maximum npm compatibility:

```text
Node.js
    ├── Resolves and loads npm packages
    ├── Hosts the Chasm VM or generated JavaScript
    ├── Registers JavaScript modules with Chasm
    └── Calls exported Chasm functions
```

Chasm delegates npm package installation, lockfiles, export-condition resolution, and lifecycle behavior to established JavaScript tooling.

TypeScript declaration files may generate initial Chasm contracts. Complex TypeScript types require binding overrides or dynamic `JsValue` access.

Chasm modules should also be publishable as npm packages containing JavaScript or native/WASM glue, TypeScript declarations, and source maps.

## Lua

Lua interop should use the Lua C API and support both directions:

- Chasm applications embed Lua for plugins or scripting.
- Lua applications load Chasm modules.
- Chasm and Lua functions call each other through typed contracts.

Lua tables map naturally to structural parameter records. Metatables, coroutines, multiple return values, and `nil` require explicit binding rules.

## Plugin Model

A Chasm application may host plugins implemented in multiple languages:

```text
Chasm application
    ├── Chasm bytecode plugins
    ├── JavaScript plugins
    └── Lua plugins
```

Each plugin should receive an explicit capability-oriented host API and own a region or runtime context. Unloading a plugin should:

1. Prevent new calls into it.
2. Invalidate or unregister callbacks.
3. Release foreign-runtime roots.
4. Run Chasm resource finalizers.
5. Reset the plugin region.
6. Destroy or recycle its foreign runtime context.

Untrusted Node.js code is not sandboxed merely because it is loaded as a plugin. Untrusted plugins require process isolation, explicit capabilities, resource limits, and controlled filesystem, network, and environment access.

## Stable Embedding API

The Chasm VM should eventually expose a stable C embedding API that allows a host to:

- Create and destroy a VM
- Load bytecode modules
- Register host functions
- Call Chasm exports
- Exchange explicitly represented values
- Create and destroy plugin regions
- Collect diagnostics
- Unload modules safely

Language-specific host bindings should be built over this stable embedding surface where practical.

## ABI Separation

Chasm distinguishes:

1. The stable external C ABI
2. The initially unstable Chasm native ABI
3. The versioned bytecode and VM ABI
4. The typed JavaScript and Lua host contracts

The native Chasm ABI should not be frozen until parameter lowering, result regions, errors, finalization, and async behavior are stable.
