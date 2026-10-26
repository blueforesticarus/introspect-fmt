CODEX GENERATED: CODE MOSTLY UNREVIEWED

# introspect-fmt — standalone research prototype

Independent Rust workspace. The parent project's vendored `tracing` crate now
consumes this prototype through a private, callback-scoped formatting bridge.

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
- `OwnedTemplate::compile(&view)` creates a reusable callsite recipe;
  `capture(&view)` produces `OwnedValue` arguments that outlive the callback.
  Primitives retain their original types. Custom trait calls and pointer output
  are captured as strings per placeholder, with its actual options, so repeated
  Display/Debug uses can differ. `render(&values)` reconstructs the field without
  retaining user values or invoking their formatters again. Borrowed rendering
  and owned recipes share literal/placeholder parsing and count resolution.
- Explicit arguments are evaluated once and only *borrowed*, not moved.
  `&mut T` argument expressions are evaluated once and captured by shared
  borrows of the resulting mutable reference. One match scrutinee borrows all
  operands in source order, keeping temporary owners alive through the callback,
  including receivers of methods returning borrowed values. No unsafe casts are
  needed for primitives.
- Each argument has a source expression, optional name, **original type name**,
  and a `Value<'_>` holding direct borrowed primitive values, or `Opaque`.
- `Value::Opaque` holds an `OpaqueValue<'_>` (`Opaque` remains an alias).
  Literal placeholders select optional, monomorphized Display and Debug hooks;
  repeated uses can select both. `display()` and `debug()` return `Option<String>`
  on demand, using default formatting rather than the placeholder's flags.
  No trait bounds are added for unused formatting traits. The erased hooks
  retain shared borrows, including for mutable-reference expressions and DSTs.
  These methods and reconstruction borrow one shared nine-trait dispatch table;
  no separate Display/Debug erased-pointer machinery is retained.
- Stable primitive detection uses safe **autoref-based specialization** at the
  proc-macro expansion site. With `nightly`, minimal specialization also supplies
  an opaque default for by-value probes, allowing types inferred later (such as
  `FromStr` errors) and preserving primitive capture through generic functions.
  Neither path requires `Any`, `TypeId`, or `T: 'static`.
- Format strings with implicit captures (`"{name}"`, `"{x:>width$}"`) are
  scanned to expose the captured variables. This scanner is best-effort and
  does not validate the Rust format grammar.
- Each `FormatArg` additionally holds optional hooks for the original value's
  requested formatting traits, including primitives and references. A shared
  borrowed reference slot preserves DST metadata and original Pointer dispatch.
  Repeated placeholders reuse these hooks with different options; unused traits
  introduce no bounds. Hook capture allocates nothing and never formats eagerly.
- `Format::render_to(&mut impl fmt::Write)` independently handles literals,
  escaping, argument selection, and positional/named/star counts. It never reads
  the retained formatter.
  With the opt-in `nightly` feature, `formatting_options` constructs a real
  `Formatter` per placeholder and calls the original trait implementation,
  writing directly into the output buffer. Without it, stable format calls cover
  default traits, common Debug flags, and ordinary width/precision/alignment;
  a small primitive fallback handles additional options. Unsupported combinations
  return `RenderError`; the complete message is never delegated. `render()` is a
  convenience wrapper that collects the streamed output into a `String`. Writer
  and user formatter errors propagate immediately, and errors can leave partial
  output. The default tracing visitor uses the streaming path directly.
- `fmt::Arguments` is retained for analyzed literal templates, including implicit
  captures and dynamic counts. Implicit captures become explicit named operands;
  count operands remain `usize`, while a borrowed wrapper forwards the original
  value's formatting trait implementations and formatter options.
- With `nightly`, `proc_macro_expand` expands template macros such as `concat!`,
  `include_str!`, and user macros into literals before analysis. The expanded
  literal is reused for code generation, preserving custom hooks, trait selection,
  and count inference without expanding the template twice.
- Without `nightly`, macro-generated templates retain no `fmt::Arguments` or
  custom hooks; the renderer can still reconstruct explicit primitives. Runtime
  template expressions keep this behavior in both modes and are evaluated once,
  before explicit arguments. Only analyzed literals reach `format_args!`.

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
2. Retaining `fmt::Arguments` depends on recognizing a literal template, either
   directly or through nightly macro expansion.
   Literal reconstruction forwards formatting to the original value type,
   including its `Pointer` implementation; it does not add a reference layer
   to trait dispatch.
3. Without `nightly`, implicit captures in macro-generated templates cannot be
   discovered by the separate parser; explicit arguments are still captured.
4. Literal templates infer detected dynamic count operands as `usize`, including
   unsuffixed literals. Without `nightly`, macro-generated templates cannot be
   scanned, so those counts may instead be recorded as `i32`.
5. Parsing of placeholders is best-effort rather than a full compiler parser.
   Names accept identifiers and keywords; raw identifier labels are normalized
   to the same format name, and keyword captures retain the Rust spelling
   needed to borrow their source variables.
6. `OpaqueValue` exposes an address and size for *diagnostic purposes only*;
   these do not enable safe downcasting. Its Display/Debug hooks exist only for
   traits requested in a scanned literal template. These introspection methods
   allocate default-formatted strings on demand. Reconstruction uses the same
   per-argument hooks for all nine formatting traits, without caching output.
   Macro-generated templates acquire hooks only with `nightly`.
7. Explicit arguments are borrowed as `&(expression)` in one match scrutinee
   to preserve non-moving behavior and statement-scoped temporary lifetimes.
   Context-sensitive lifetime and inference differences from the compiler builtin
   remain possible and must be tested before claiming full `format_args!`
   compatibility.
8. The nightly `FormattingOptions` API currently accepts `u16` width/precision.
   Larger counts return an explicit reconstruction error rather than truncating.
   The pinned standard formatter also rejects counts beyond this limit.
   The nightly toolchain and APIs are pinned through the Fenix flake input.

## Features

No features are enabled by default, so ordinary consumers can use stable Rust.
Enable exact runtime formatter options and template macro expansion with a
nightly compiler:

```toml
# The feature is on the runtime crate; downstream callers need no feature attributes.
introspect-fmt = { path = "introspect-fmt", features = ["nightly"] }
```

The feature gates runtime Formatter construction and proc-macro template
expansion. After expansion, capture, primitive introspection, trait selection,
and count resolution use the same implementation as direct literals.
The default flake shell supplies Fenix nightly; `nix develop .#stable` supplies
Fenix stable for checking that the fallback has no unstable API dependency.
The primitive padding path avoids post-1.71 standard-library APIs; the nightly
feature requirement is separate from that compatibility constraint.

## Tests

The compatibility matrix is in [formatting.rs](introspect-fmt/tests/formatting.rs).
Basic working cases share `basic_usage`; complex usages have focused tests.
Every output assertion compares against Rust's `format_args!` oracle, after
constructing a view with its retained formatter set to `None`. It calls the
actual `Format::render()` implementation, rather than a test-only renderer.

The same tests run in both feature modes. The `nightly_required` module asserts
exact output and passes with `--features nightly`; it deliberately fails without
that feature. These assertions use neither `cfg` to hide tests, `ignore`,
`should_panic`, nor expected errors as success. Cases rejected by the prototype's
macro are compiled in
subprocesses so they do not prevent the other tests from running. The harness
first compiles/runs the Rust oracle, then the prototype, and compares output.
Alias and facade fixtures also exercise implicit/keyword captures, custom trait
hooks, mutable borrows, single evaluation, and calls through another wrapper.
An internal runtime test checks `$crate` resolution within its defining crate.

| Area | Default / stable | `nightly` feature |
| --- | --- | --- |
| Literals, escapes, basic primitives, names, positions, repeats | Supported | Supported |
| Primitive radix/scientific formats and hex Debug | Supported | Supported |
| Custom trait-only types, mutable references, dual traits | Supported | Supported |
| Dynamic counts and star-precision indexing | Supported | Supported |
| Primitive width, Unicode fill, alignment, zero padding | Supported | Supported |
| Custom width/precision with space fill, common Debug flags | Supported | Supported |
| Arbitrary custom fill, signs, flags, and count combinations | Best effort; failing tests | Supported within count limits |
| Original Pointer output | Supported | Supported |
| Forwarded literals, keyword names, renamed crates, re-exports | Supported | Supported |
| Macro-generated templates with explicit primitives | Best effort | Supported |
| Custom hooks and counts with macro-generated templates | Failing tests | Supported |

[introspection.rs](introspect-fmt/tests/introspection.rs) separately consolidates
primitive specialization, metadata, source order, count inference, hook laziness,
and borrowing. Nonliteral expressions and unsized values accepted as prototype
extensions are kept here, separate from valid Rust formatting syntax.

Run the entire matrix, including intentionally red gaps, in both modes:

```sh
nix develop -c cargo test --workspace --locked --features nightly --no-fail-fast
nix develop .#stable -c cargo test --workspace --locked --no-fail-fast
```

The nightly run should pass; the default/stable run should fail only
`nightly_required::`. These are parity assertions, not tests expecting errors.
The adversarial cases cover brace fill, borrows into temporary receivers, and
temporary drop timing. Rerun them with
`cargo test --features nightly --test formatting adversarial`.
A smoke check excluding the known stable gaps is available separately; it does
not establish full compatibility:

```sh
nix develop .#stable -c cargo test --workspace --locked -- --skip nightly_required::
```

The flake lock pins Fenix, Cargo, Rust, rustfmt, Clippy, rust-analyzer, and matching
standard-library sources. Compiler fixtures match the active compiler/features
and support both traditional Cargo artifacts and nightly's separate metadata.

The cases follow the [standard library formatting documentation](https://doc.rust-lang.org/std/fmt/).
Add a focused parity test whenever a new usage pattern is found; a passing
introspection assertion or retained formatter is not evidence of reconstruction.
