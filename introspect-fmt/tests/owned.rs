//! Owned recipes must render after original values and trait hooks are gone.
use introspect_fmt::{inspect_format, OwnedTemplate, OwnedValue};

#[test]
fn primitive_capture_outlives_inputs_and_preserves_argument_order() {
    let (template, values) = {
        let text = String::from("héllo");
        inspect_format!(
            |view| {
                let template = OwnedTemplate::compile(&view).unwrap();
                let values = template.capture(&view).unwrap();
                (template, values)
            },
            "{text:?} {}{:x} {3:.*}",
            4usize,
            -1i8,
            4usize,
            1.234567f32
        )
    };
    assert!(matches!(values[0], OwnedValue::Usize(4)));
    assert!(matches!(values[1], OwnedValue::I8(-1)));
    assert!(matches!(values[3], OwnedValue::F32(_)));
    assert_eq!(
        template.render(&values).unwrap(),
        format!("{:?} {}{:x} {:.4}", "héllo", 4, -1i8, 1.234567f32)
    );
}

#[cfg(feature = "nightly")]
#[test]
fn custom_traits_are_rendered_once_per_occurrence_with_actual_options() {
    use std::{cell::Cell, fmt};
    struct Custom<'a>(&'a Cell<usize>);
    impl fmt::Display for Custom<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.set(self.0.get() + 1);
            f.pad("display")
        }
    }
    impl fmt::Debug for Custom<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.set(self.0.get() + 1);
            if f.alternate() {
                f.write_str("alternate debug")
            } else {
                f.write_str("debug")
            }
        }
    }
    let calls = Cell::new(0);
    let (template, values) = {
        let custom = Custom(&calls);
        inspect_format!(
            |view| {
                let template = OwnedTemplate::compile(&view).unwrap();
                let values = template.capture(&view).unwrap();
                (template, values)
            },
            "{0:*>width$.precision$}|{0:#?}{0:?}",
            custom,
            width = 10,
            precision = 4
        )
    };
    assert_eq!(calls.get(), 3);
    assert_eq!(
        template.render(&values).unwrap(),
        "******disp|alternate debugdebug"
    );
    assert_eq!(
        template.render(&values).unwrap(),
        "******disp|alternate debugdebug"
    );
    assert_eq!(calls.get(), 3); // Reading stored values never invokes user code.
}

#[cfg(feature = "nightly")]
#[test]
fn dynamic_counts_change_without_recompiling_and_brace_fill_survives() {
    let template = inspect_format!(
        |view| OwnedTemplate::compile(&view).unwrap(),
        "{{{value:}>width$.precision$}}} {value:x?}",
        value = "héllo",
        width = 10,
        precision = 2
    );
    inspect_format!(
        |view| {
            assert!(template.matches(&view));
            let values = template.capture(&view).unwrap();
            assert_eq!(
                template.render(&values).unwrap(),
                format!(
                    "{{{value:}>width$.precision$}}} {value:x?}",
                    value = "héllo",
                    width = 7,
                    precision = 3
                )
            );
        },
        "{{{value:}>width$.precision$}}} {value:x?}",
        value = "héllo",
        width = 7,
        precision = 3
    );
}

#[cfg(not(feature = "nightly"))]
#[test]
fn opaque_generic_primitives_do_not_reuse_a_typed_schema() {
    // Method selection in a generic function can produce Opaque(u32), whose
    // Rust type name equals the name of an inspectable primitive u32.
    fn generic<T: std::fmt::Display>(value: T, typed: &OwnedTemplate) -> String {
        inspect_format!(
            |view| {
                assert!(!typed.matches(&view));
                let opaque = OwnedTemplate::compile(&view).unwrap();
                opaque.render(&opaque.capture(&view).unwrap()).unwrap()
            },
            "{value}"
        )
    }
    let value = 42u32;
    let typed = inspect_format!(|view| OwnedTemplate::compile(&view).unwrap(), "{value}");
    assert_eq!(generic(value, &typed), "42");
}

#[cfg(feature = "nightly")]
#[test]
fn capture_allows_type_inference_from_a_later_parse_result() {
    fn parse() -> Result<std::net::Ipv4Addr, ()> {
        let value = "127.0.0.1".parse();
        let value = value.or_else(|why| {
            inspect_format!(
                |view| {
                    let _ = view.render().unwrap();
                },
                "parse failed: {}",
                why
            );
            Err(())
        })?;
        Ok(value)
    }
    assert_eq!(parse().unwrap(), std::net::Ipv4Addr::LOCALHOST);
}
