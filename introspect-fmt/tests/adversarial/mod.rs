//! Valid Rust usages that stress parsing and statement-scoped temporaries.
//! Each compiler fixture runs Rust's oracle first; any future regressions stay red.

use crate::support;

#[test]
fn closing_brace_as_fill() {
    // A closing brace can be the fill character when followed by alignment.
    parity!("[{:}>8}]", "x");
    // Check all alignments, trait selection, and the next field's boundary.
    parity!("[{:}<8}] [{:}^8x}] [{:}>8}] {}", "x", 42, "x", "tail");
    parity!("{{{:}>8}}}", "x");
}

mod nightly_required {
    #[test]
    fn closing_brace_fill_preserves_custom_traits_and_counts() {
        struct HexOnly(u32);
        impl std::fmt::LowerHex for HexOnly {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                // Forward options to verify both scanners retain the whole spec.
                std::fmt::LowerHex::fmt(&self.0, f)
            }
        }
        parity!("{0:}>#width$x} {0:}<width$x}", HexOnly(42), width = 8);
        #[derive(Debug)]
        struct DebugOnly;
        parity!("{:}>8?}", DebugOnly);
        let width = 8;
        // A hidden count capture must be discovered past the brace fill too.
        introspect_fmt::inspect_format!(
            |view| {
                crate::support::assert_rendered(
                    &view,
                    &format!("{:}>width$.precision$}", "abcdef", precision = 3),
                );
            },
            "{:}>width$.precision$}",
            "abcdef",
            precision = 3
        );
        parity!("{:}>.*}", 3, "abcdef");
    }
}

#[test]
fn opening_brace_as_fill() {
    // A brace inside the specifier is data, rather than another placeholder.
    parity!("[{:{<8}]", "x");
}

#[test]
fn borrowed_method_result_of_temporary() {
    // The temporary owner survives the original formatting statement.
    support::assert_compiles_and_renders(
        "temporary_method_borrow",
        r#"fn main() { CASE!("{}", String::from("temporary").as_str()); }"#,
        "introspect_fmt",
    );
}

#[test]
fn nested_arguments_borrow_local_value() {
    // Nonconstant inner formatting cannot use the builtin's literal folding.
    support::assert_compiles_and_renders(
        "nested_local_arguments",
        r#"fn main() {
            let value = String::from("local");
            CASE!("{}", format_args!("inner={}", value));
        }"#,
        "introspect_fmt",
    );
}

#[test]
fn iterator_borrows_temporary_owner() {
    // Method chains can return a reference into a temporary container too.
    support::assert_compiles_and_renders(
        "temporary_iterator_borrow",
        r#"fn main() {
            CASE!("{}", vec![String::from("first")].iter().next().unwrap());
        }"#,
        "introspect_fmt",
    );
}

#[test]
fn temporary_receiver_drop_timing() {
    // Dropping a temporary receiver early is observable even when its method
    // returns an owned value and therefore passes the borrow checker.
    support::assert_compiles_and_renders(
        "temporary_receiver_drop_timing",
        r#"
        use std::{cell::Cell, fmt};
        struct Guard<'a>(&'a Cell<bool>);
        impl Guard<'_> {
            fn value(&self) -> u32 { 42 }
        }
        impl Drop for Guard<'_> {
            fn drop(&mut self) { self.0.set(true); }
        }
        struct Observer<'a>(&'a Cell<bool>);
        impl fmt::Display for Observer<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "dropped={}", self.0.get())
            }
        }
        fn main() {
            let dropped = Cell::new(false);
            CASE!("{} {}", Guard(&dropped).value(), Observer(&dropped));
            // The receiver must drop once the formatting statement completes.
            assert!(dropped.get());
        }
        "#,
        "introspect_fmt",
    );
}

#[test]
fn generated_binding_names_do_not_shadow_arguments() {
    // These names intentionally resemble macro internals to exercise hygiene.
    support::assert_compiles_and_renders(
        "generated_binding_hygiene",
        r#"fn main() {
            let __introspect_fmt_template = "outer template";
            let __introspect_fmt_values = "outer values";
            let __introspect_fmt_arg_0 = "outer argument";
            CASE!("{} {} {}", __introspect_fmt_template,
                __introspect_fmt_values, __introspect_fmt_arg_0);
        }"#,
        "introspect_fmt",
    );
}
