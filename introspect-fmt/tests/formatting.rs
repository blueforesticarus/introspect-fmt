//! Compatibility contract: compile, then reconstruct exactly without the
//! retained fmt::Arguments. Unsupported tests deliberately fail in normal runs.

#[macro_use]
mod support;
use introspect_fmt::inspect_format;
use std::fmt;

// Keep adversarial contract cases separate so new gaps remain easy to rerun.
mod adversarial;

#[test]
fn basic_usage() {
    parity!("");
    parity!("Hello, café 🦀! Price: $10; {{:.*}} {{width$}} }} {{");
    parity!("{} {} {} {}", 42, -7, 1.25, true);
    // Scalar widths share the basic contract rather than separate tiny tests.
    parity!(
        "{} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}",
        true,
        '🦀',
        -1i8,
        -2i16,
        -3i32,
        -4i64,
        -5i128,
        -6isize,
        1u8,
        2u16,
        3u32,
        4u64,
        5u128,
        6usize,
        1.25f32,
        2.5f64
    );
    parity!(
        "{} {:?} {}",
        1 + 2,
        (Some(7), vec![1, 2]),
        "  trimmed  ".trim()
    );
    parity!("{} {:?} {} {:?}", '🦀', '🦀', "a\nb", "a\nb");
    parity!("{} {}", String::from("owned"), &String::from("borrowed"));
    parity!("{:?}", vec![1, 2, 3]);
    parity!("{1} {} {0} {}", 1, 2);
    parity!("{named} {0} {named}", "positional", named = "named");
    parity!("{{{0}}} {0:} {0   }", 42,);
    parity!(r#"raw "{}""#, "value");
    parity!("nested={}", format_args!("inner={}", 42));
}

#[test]
fn primitive_radices_scientific_notation_and_debug_hex() {
    parity!("{:b} {:o} {:x} {:X}", 171, 171, 171, 171);
    parity!("{:e} {:E}", 1234.5, 1234.5);
    parity!("{:x?} {:X?}", 171, 171);
    parity!("{} {} {}", -0.0, f64::INFINITY, f64::NAN);
}

#[test]
fn implicit_captures_unicode_and_explicit_overrides() {
    let name = String::from("Ada");
    let café = 42;
    inspect_format!(
        |view| {
            support::assert_rendered(&view, &fmt::format(format_args!("{name} {café} {name:?}")));
        },
        "{name} {café} {name:?}"
    );
    inspect_format!(
        |view| {
            support::assert_rendered(&view, "override 42");
        },
        "{name} {café}",
        name = "override"
    );
    assert_eq!(name, "Ada");
}

// Separate types prove that merely compiling does not require unused traits.
struct DisplayOnly<'a>(&'a str);
impl fmt::Display for DisplayOnly<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.pad(self.0)
    }
}
#[derive(Debug)]
struct DebugOnly;
#[derive(Debug)]
struct Dual<'a>(&'a str);
impl fmt::Display for Dual<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.pad(self.0)
    }
}

#[test]
fn display_only_debug_only_and_repeated_dual_trait_values() {
    parity!(
        "{} {:?} {dual} {dual:?} {dual}",
        DisplayOnly("display"),
        DebugOnly,
        dual = Dual("both")
    );
    let owned = String::from("local");
    parity!("{}", DisplayOnly(&owned));
    let display = DisplayOnly(&owned);
    let erased: &dyn fmt::Display = &display;
    parity!("{}", erased);
}

#[test]
#[allow(clippy::useless_borrows_in_formatting)] // Compare the exact mutable-reference input type.
fn mutable_reference_and_expression_are_evaluated_once() {
    let mut value = Dual("before");
    let mut calls = 0;
    let expected = fmt::format(format_args!("{0} {0:?} {0}", &mut value));
    inspect_format!(
        |view| {
            support::assert_rendered(&view, &expected);
        },
        "{0} {0:?} {0}",
        {
            calls += 1;
            &mut value
        }
    );
    assert_eq!(calls, 1);
    value.0 = "after";
    parity!("{}", value);
}

#[test]
fn reference_only_trait_implementation() {
    struct ReferenceOnly;
    impl fmt::Display for &mut ReferenceOnly {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("reference")
        }
    }
    let mut value = ReferenceOnly;
    let reference = &mut value;
    parity!("{}", reference);
    let _still_usable = reference;
}

#[test]
fn macro_generated_templates_with_explicit_primitives() {
    parity!(concat!("value=", "{}"), 42);
    parity!(include_str!("fixtures/template.txt"), 42);
    macro_rules! template {
        () => {
            "value={}"
        };
    }
    parity!(template!(), 42);
}

#[test]
fn compiler_fixture_checks_both_compilation_and_output() {
    support::assert_compiles_and_renders(
        "supported",
        include_str!("fixtures/renamed_dependency.rs"),
        "introspect_fmt",
    );
}

#[test]
fn raw_identifier_names() {
    parity!("{type}", r#type = 42);
}

#[test]
fn keyword_named_arguments_compile() {
    support::assert_compiles_and_renders(
        "keywords",
        include_str!("fixtures/keyword_named.rs"),
        "introspect_fmt",
    );
}

#[test]
fn implicit_receiver_capture() {
    support::assert_compiles_and_renders(
        "receiver",
        include_str!("fixtures/receiver_capture.rs"),
        "introspect_fmt",
    );
}

#[test]
fn renamed_dependency_compiles() {
    support::assert_compiles_and_renders(
        "renamed",
        include_str!("fixtures/renamed_dependency.rs"),
        "renamed_fmt",
    );
}

#[test]
fn no_std_caller_with_std_enabled_runtime_compiles() {
    // A dependency can be no_std while feature unification enables the runtime.
    support::assert_compiles_and_renders(
        "no_std_caller",
        include_str!("fixtures/no_std_caller.rs"),
        "introspect_fmt",
    );
}

#[test]
fn reexported_macro_without_direct_runtime_dependency() {
    support::assert_reexported_compiles_and_renders(
        "reexported",
        include_str!("fixtures/renamed_dependency.rs"),
    );
}

#[test]
fn explicit_sign() {
    parity!("{:+} {:+} {:-}", 42, -42, 42);
}

#[test]
fn alternate_radix_prefixes() {
    parity!("{:#x} {:#X} {:#b} {:#o}", 42, 42, 42, 42);
}

#[test]
fn float_precision_and_rounding() {
    parity!("{:.1} {:.1}", 1.25, 1.75);
}

#[test]
fn string_precision_counts_unicode_characters() {
    parity!("{:.3}", "é🦀abc");
}

#[test]
fn forwarded_literal_retains_custom_formatters() {
    // macro_rules! forwarding wraps a literal in an invisible token group.
    macro_rules! forward {
        ($template:literal, $value:expr) => {
            inspect_format!(
                |view| {
                    support::assert_rendered(&view, "custom");
                },
                $template,
                $value
            )
        };
    }
    let value = DisplayOnly("custom");
    assert_eq!(fmt::format(format_args!("{}", value)), "custom");
    forward!("{}", value);
}

#[test]
fn literal_width_and_alignment() {
    parity!("[{:8}] [{:<8}] [{:^8}] [{:>8}]", "x", "x", "x", "x");
}

#[test]
fn unicode_fill() {
    parity!("{:🦀^8}", "é");
}

#[test]
fn sign_aware_zero_padding() {
    parity!("{:08} {:08}", 42, -42);
    // Padding must follow signs/prefixes and retain Rust's primitive semantics.
    parity!("{:+08} {:#08x} {:#08X} {:#012b} {:#06o}", 42, 42, 42, 5, 9);
    parity!("{:08.2} {:08.2} {:08}", -1.25, -0.0, f64::INFINITY);
    parity!("{:🦀<08} {:^08}", -42, 42);
}

#[test]
fn primitive_padding_combines_with_precision_and_counts_characters() {
    // Odd center padding puts the extra fill on the right; precision precedes width.
    parity!("[{:🦀^8.2}] [{:>6.3}] [{:2}]", "é🦀abc", "é🦀abc", "long");
    parity!("[{:8}] [{:<8}] [{:^8}] [{:>8}]", -42, -42, -42, -42);
    parity!("[{:8}] [{:🦀^8}] [{:08}]", true, 'é', "x");
    parity!("[{:10.2}] [{:🦀<10.2}] [{:#10x}]", 1.25, -1.25, 42);
}

// Shared capabilities use identical parity assertions with either backend.
mod traits_and_counts {
    use super::*;

    #[test]
    fn value_and_count_expressions_are_evaluated_once() {
        let mut value_calls = 0;
        let mut count_calls = 0;
        inspect_format!(
            |view| support::assert_rendered(&view, "     abc"),
            "{:>width$.3}",
            {
                value_calls += 1;
                DisplayOnly("abcdef")
            },
            width = {
                count_calls += 1;
                8
            }
        );
        assert_eq!((value_calls, count_calls), (1, 1));
    }

    #[test]
    fn custom_observes_absent_and_explicit_zero_counts() {
        struct Counts;
        impl fmt::Display for Counts {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                // Passing zero for an absent option would change custom behavior.
                let counts = (formatter.width(), formatter.precision(), formatter.align());
                write!(formatter, "{counts:?}")
            }
        }
        parity!("{0} {0:.0} {0:0$} {0:>.0}", 0usize);
        parity!("{0} {0:.0} {0:width$} {0:>.0}", Counts, width = 0);
    }

    #[test]
    fn custom_observes_simple_sign_and_alternate_flags() {
        struct Flags;
        impl fmt::Display for Flags {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                // Verify forwarding flags rather than assuming numeric output.
                let flags = (
                    formatter.sign_plus(),
                    formatter.sign_minus(),
                    formatter.alternate(),
                );
                write!(formatter, "{flags:?}")
            }
        }
        parity!("{0} {0:+} {0:-} {0:#} {0:+#} {0:-#}", Flags);
    }

    #[test]
    fn dynamic_width_positional() {
        parity!("{1:0$}", 8, "x");
    }
    #[test]
    fn dynamic_width_named_with_flags() {
        parity!("{:+#0width$x}", 42, width = 8);
    }
    #[test]
    fn dynamic_width_and_precision_captured() {
        let value = 1.25;
        let width = 8;
        let précision = 2;
        inspect_format!(
            |view| {
                support::assert_rendered(
                    &view,
                    &fmt::format(format_args!("{value:width$.précision$}")),
                );
            },
            "{value:width$.précision$}"
        );
    }
    #[test]
    fn dynamic_precision_positional() {
        parity!("{1:.0$}", 2, 1.25);
    }
    #[test]
    fn star_precision_consumes_before_implicit_value() {
        parity!("{:.*} {:.*}", 2, 1.25, 3, 2.5);
    }
    #[test]
    fn star_precision_with_explicit_value_preserves_next_index() {
        parity!("{2:.*} {}", 2, "next", 1.25);
    }
    #[test]
    fn alternate_pretty_debug() {
        parity!("{:#?}", vec![1, 2]);
    }
    #[test]
    fn custom_debug_receives_hex_flags() {
        parity!("{:x?}", vec![171]);
    }
    #[test]
    fn raw_pointer_address() {
        let value = 42;
        parity!("{:p}", &value as *const i32);
    }
    #[test]
    fn shared_and_mutable_pointer_addresses() {
        let mut value = 42;
        parity!("{:p}", &value);
        parity!("{:p}", &mut value);
    }
    #[test]
    fn custom_display_receives_width_and_precision() {
        parity!("{:>8.3}", DisplayOnly("abcdef"));
    }
    // Each trait-only type is independent: no Display or Debug implementation
    // can accidentally make these pass via a fallback to a different trait.
    macro_rules! trait_only_case {
        ($test:ident, $trait:ident, $template:tt) => {
            #[test]
            fn $test() {
                struct Only;
                impl fmt::$trait for Only {
                    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                        formatter.write_str("custom")
                    }
                }
                parity!($template, Only);
            }
        };
    }
    trait_only_case!(custom_binary, Binary, "{:b}");
    trait_only_case!(custom_octal, Octal, "{:o}");
    trait_only_case!(custom_lower_hex, LowerHex, "{:x}");
    trait_only_case!(custom_upper_hex, UpperHex, "{:X}");
    trait_only_case!(custom_lower_exp, LowerExp, "{:e}");
    trait_only_case!(custom_upper_exp, UpperExp, "{:E}");
    trait_only_case!(custom_pointer, Pointer, "{:p}");
}

// Run these in BOTH configurations: exact parity passes with nightly and stays
// visibly red without it. Never cfg away, ignore, or expect a failure as success.
mod nightly_required {
    use super::*;

    #[test]
    fn macro_generated_template_retains_custom_formatters() {
        // All these templates must retain hooks without evaluating user formatting.
        parity!(concat!("value=", "{}"), DisplayOnly("custom"));
        parity!(include_str!("fixtures/template.txt"), DisplayOnly("custom"));
        parity!(concat!("{0}", " {0:?}"), Dual("custom"));
    }

    #[test]
    fn macro_generated_template_preserves_trait_selection_and_counts() {
        struct HexOnly(u32);
        impl fmt::LowerHex for HexOnly {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                // A Display or Debug requirement would reject this argument.
                fmt::LowerHex::fmt(&self.0, formatter)
            }
        }
        macro_rules! template {
            () => {
                concat!("{0:🦀>+#width$x}", " {0:0width$x}")
            };
        }
        // Unsuffixed counts must be inferred as usize after expansion.
        parity!(template!(), HexOnly(171), width = 12);
        parity!(concat!("{:.*}", " {}"), 3, DisplayOnly("abcdef"), "tail");
    }

    #[test]
    fn custom_display_observes_all_options() {
        struct Options;
        impl fmt::Display for Options {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                // Inspect flags rather than just padding a constant string.
                let options = (
                    formatter.fill(),
                    formatter.align(),
                    formatter.sign_plus(),
                    formatter.sign_minus(),
                    formatter.alternate(),
                    formatter.sign_aware_zero_pad(),
                    formatter.width(),
                    formatter.precision(),
                );
                write!(formatter, "{options:?}")
            }
        }
        parity!("{:🦀^+#012.3}", Options);
        parity!(
            "{:🦀>-#width$.precision$}",
            Options,
            width = 11,
            precision = 0
        );
    }

    #[test]
    fn custom_radix_receives_dynamic_counts_and_flags() {
        // No Display or Debug implementation: only the requested trait may be required.
        struct HexOnly(u32);
        impl fmt::LowerHex for HexOnly {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::LowerHex::fmt(&self.0, formatter)
            }
        }
        parity!("{0:🦀>+#width$x} {0:🦀<#width$x}", HexOnly(171), width = 12);
    }

    #[test]
    fn debug_receives_combined_options() {
        parity!("{:🦀>+#012.3x?}", vec![171, 42]);
        parity!(
            "{:🦀^#width$.precision$X?}",
            Some(171),
            width = 12,
            precision = 3
        );
    }
}
