# introspect-fmt — standalone research prototype

Independent Rust workspace. **Nothing in the `tracing` repository is modified.**

The goal is an introspectible alternative to `format_args!`: capture individual
borrowed values with their original Rust types, then reconstruct the same string
without relying on a retained `fmt::Arguments`. Compatibility is a target, not
an accomplished claim; known gaps are ordinary failing tests.

## Example

```rust
use introspect_fmt::{inspect_format, Value};

let count = 12u32;
let city = String::from("Rochester");

inspect_format!(|view| {
    assert_eq!(view.template(), "{} in {}");
    for argument in view.args() {
        println!("name={:?}, expression={}, type={}",
            argument.name(), argument.expression(), argument.type_name());
        match argument.value() {
            Value::U32(number) => println!("u32: {}", number),
            Value::Str(text) => println!("str: {}", text),
            Value::Opaque(v) => println!("opaque {} ({} bytes)", v.type_name(), v.size()),
            _ => {}
        }
    }

    // Reconstruct from the template and captured values, independently.
    assert_eq!(view.render().unwrap(), "12 in Rochester");
}, "{} in {}", count, city);
```

## Design

- Two crates: `introspect-fmt` (runtime representation) and
  `introspect-fmt-macros` (syntax-only procedural macro). A public `macro_rules!`
  wrapper passes its hygienic `$crate` tokens to the hidden proc-macro entry
  point, so generated runtime paths work with renamed dependencies and re-exports.
  This uses no Cargo manifest lookup or additional dependencies. Implicit
  identifiers use the original literal's span to resolve in the caller's scope.
- A callback-scoped `Format<'_>` avoids returning a self-referential value
  borrowing expression temporaries.
- Explicit arguments are evaluated once and only *borrowed*, not moved.
  `&mut T` argument expressions are evaluated once and captured by shared
  borrows of the resulting mutable reference. No unsafe casts are needed
  for primitives.
- Each argument has a source expression, optional name, **original type name**,
  and a `Value<'_>` holding direct borrowed primitive values, or `Opaque`.
- `Value::Opaque` holds an `OpaqueValue<'_>` (`Opaque` remains an alias).
  Literal placeholders select optional, monomorphized Display and Debug hooks;
  repeated uses can select both. `display()` and `debug()` return `Option<String>`
  on demand, using default formatting rather than the placeholder's flags.
  No trait bounds are added for unused formatting traits. The erased hooks
  retain shared borrows, including for mutable-reference expressions and DSTs.
- Primitive detection uses safe, stable **autoref-based specialization** at the
  proc-macro expansion site. It does not rely on `Any`, `TypeId`, unstable
  specialization, or requiring `T: 'static`.
- Format strings with implicit captures (`"{name}"`, `"{x:>width$}"`) are
  scanned to expose the captured variables. This scanner is best-effort and
  does not validate the Rust format grammar.
- `Format::render()` rebuilds the message from its template, primitive values,
  and opaque Display/Debug hooks. It never reads the retained formatter. It
  supports basic argument selection and escaping, default Display/Debug, and
  primitive radix/scientific formats. Unsupported options return `RenderError`
  rather than silently producing different output. Fixed per-value formats
  reuse Rust's primitive formatters; the complete message is never delegated.
- `fmt::Arguments` is retained for direct literal templates, including implicit
  captures and dynamic counts. Implicit captures become explicit named operands;
  count operands remain `usize`, while a borrowed wrapper forwards the original
  value's formatting trait implementations and formatter options.
- Nonliteral templates, including nested macros such as `concat!`, retain the
  `None` fallback for the retained formatter. The independent renderer can
  still reconstruct explicit primitives from their runtime template, but custom
  values lack hooks because the proc macro cannot inspect that expansion.
  Template expressions are evaluated once, before explicit arguments;
  only inert direct literals also appear in the generated `format_args!`.

## Compatibility contract and caveats

**Target:** For each real-world `format_args!` usage, the prototype must both
compile and reconstruct an identical string without its retained formatter.
The compatibility suite covers ordinary positional/named arguments, captures,
custom formatting traits, expressions, references, counts, flags, and template
macros. Unsupported cases deliberately fail this same contract. Invalid inputs
are not required to receive compiler-identical diagnostics.

**Limits of this first prototype:**

1. The callback syntax adds one leading argument:
   `inspect_format!(|view| { ... }, "...", args...)`.
2. Retaining `fmt::Arguments` depends on recognizing a direct literal template.
   Literal reconstruction forwards formatting to the original value type,
   including its `Pointer` implementation; it does not add a reference layer
   to trait dispatch.
3. For format strings created with macros, implicit captures cannot be
   discovered by the separate parser; explicit arguments are still captured.
4. Literal templates infer detected dynamic count operands as `usize`, including
   unsuffixed literals. Macro-generated templates cannot be scanned, so those
   counts may instead be recorded as `i32`.
5. Parsing of placeholders is best-effort rather than a full compiler parser.
   Names accept identifiers and keywords; raw identifier labels are normalized
   to the same format name, and keyword captures retain the Rust spelling
   needed to borrow their source variables.
6. `OpaqueValue` exposes an address and size for *diagnostic purposes only*;
   these do not enable safe downcasting. Its Display/Debug hooks exist only for
   traits requested in a scanned literal template. Other formatting traits and
   macro-generated templates have no hooks. Each hook call allocates a string
   and invokes user formatting code; it does not cache output or retain flags.
7. An explicit argument is bound as `&(expression)` to preserve non-moving
   behavior. Context-sensitive lifetime and inference differences from the
   compiler builtin remain possible and must be tested before claiming full
   `format_args!` compatibility.

## Tests

The compatibility matrix is in [formatting.rs](introspect-fmt/tests/formatting.rs).
Basic working cases share `basic_usage`; complex usages have focused tests.
Every output assertion compares against Rust's `format_args!` oracle, after
constructing a view with its retained formatter set to `None`. It calls the
actual `Format::render()` implementation, rather than a test-only renderer.

The `unsupported` module contains intentionally failing tests. They assert
successful compilation and exact output, just like supported cases. They are
neither ignored nor marked `should_panic`; errors and mismatches stay red until
the feature works. Cases rejected by the prototype's macro are compiled in
subprocesses so they do not prevent the other tests from running. The harness
first compiles/runs the Rust oracle, then the prototype, and compares output.
Alias and facade fixtures also exercise implicit/keyword captures, custom trait
hooks, mutable borrows, single evaluation, and calls through another wrapper.
An internal runtime test checks `$crate` resolution within its defining crate.

| Area | Current independent reconstruction |
| --- | --- |
| Literals, escapes, basic primitives, names, positions, repeats | Supported |
| Primitive radix/scientific formats and hex Debug | Supported |
| Default custom Display/Debug, dual traits, mutable references | Supported |
| Macro-generated templates with explicit primitives | Supported |
| Width, fill, alignment, signs, prefixes, zero padding, precision | Failing tests |
| Dynamic counts and star-precision indexing | Failing tests |
| Custom formatter flags and non-Display/Debug hooks | Failing tests |
| Pointer output | Failing tests |
| Custom hooks with macro-generated or forwarded literals | Failing tests |
| Keyword argument names, raw identifier names, and receiver captures | Supported |
| Renamed dependencies and facade re-exports | Supported |

[introspection.rs](introspect-fmt/tests/introspection.rs) separately consolidates
primitive specialization, metadata, source order, count inference, hook laziness,
and borrowing. Nonliteral expressions and unsized values accepted as prototype
extensions are kept here, separate from valid Rust formatting syntax.

Run the entire matrix, including known failures and metadata tests:

```sh
cargo test --workspace --locked --no-fail-fast
```

A supported-only smoke check is available separately; it does not establish
full compatibility:

```sh
cargo test --workspace --locked -- --skip unsupported::
```

The Nix flake supplies Cargo, Rust, rustfmt, Clippy, and rust-analyzer. Run the
full matrix inside the pinned environment with:

```sh
nix develop -c cargo test --workspace --locked --no-fail-fast
```

The cases follow the [standard library formatting documentation](https://doc.rust-lang.org/std/fmt/).
Add a focused parity test whenever a new usage pattern is found; a passing
introspection assertion or retained formatter is not evidence of reconstruction.
