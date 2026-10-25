//! Supplementary metadata and borrowing checks. Output parity is tested in
//! formatting.rs; these tests protect the prototype's other public promises.
use introspect_fmt::{inspect_format, OpaqueValue, Value};
use std::{cell::Cell, fmt};

#[test]
fn all_primitive_variants_preserve_types_and_borrow_identity() {
    // Exercise all three generated specializations without duplicating a test
    // for each numeric width. Original types differ; primitive values do not.
    macro_rules! primitive {
        ($variant:ident, $value:expr, $ty:ty) => {{
            let mut value: $ty = $value;
            inspect_format!(|view| {
                let arg = &view.args()[0];
                assert_eq!(arg.type_name(), stringify!($ty));
                assert_eq!(arg.value().type_name(), stringify!($ty));
                assert!(matches!(arg.value(), Value::$variant(v) if std::ptr::eq(*v, &value)));
            }, "{:?}", value);
            inspect_format!(|view| {
                assert_eq!(view.args()[0].type_name(), std::any::type_name::<&$ty>());
                assert!(matches!(view.args()[0].value(), Value::$variant(v) if **v == $value));
            }, "{:?}", &value);
            inspect_format!(|view| {
                assert_eq!(view.args()[0].type_name(), std::any::type_name::<&mut $ty>());
                assert!(matches!(view.args()[0].value(), Value::$variant(v) if **v == $value));
            }, "{:?}", &mut value);
        }};
    }
    primitive!(Bool, true, bool);
    primitive!(Char, '🦀', char);
    primitive!(I8, -1, i8);
    primitive!(I16, -2, i16);
    primitive!(I32, -3, i32);
    primitive!(I64, -4, i64);
    primitive!(I128, -5, i128);
    primitive!(Isize, -6, isize);
    primitive!(U8, 1, u8);
    primitive!(U16, 2, u16);
    primitive!(U32, 3, u32);
    primitive!(U64, 4, u64);
    primitive!(U128, 5, u128);
    primitive!(Usize, 6, usize);
    primitive!(F32, 1.25, f32);
    primitive!(F64, 2.5, f64);
}

#[test]
fn strings_and_non_static_opaque_values_preserve_metadata() {
    let owned = String::from("borrowed");
    let slice = owned.as_str();
    inspect_format!(
        |view| {
            assert_eq!(view.template(), "{} {} {}");
            for arg in view.args() {
                assert!(matches!(arg.value(), Value::Str(value) if *value == slice));
                assert_eq!(arg.value().type_name(), "str");
            }
            assert_eq!(view.args()[0].expression(), "owned");
            assert_eq!(view.args()[0].type_name(), std::any::type_name::<String>());
            assert_eq!(view.args()[1].type_name(), std::any::type_name::<&String>());
        },
        "{} {} {}",
        owned,
        &owned,
        slice
    );
    #[derive(Debug)]
    struct Borrowed<'a>(&'a str);
    let custom = Borrowed(slice);
    inspect_format!(
        |view| {
            let arg = &view.args()[0];
            let Value::Opaque(value): &Value<'_> = arg.value() else {
                panic!("expected opaque")
            };
            let value: &OpaqueValue<'_> = value;
            let _: &introspect_fmt::Opaque<'_> = value;
            assert_eq!(value.address(), &custom as *const Borrowed<'_> as *const ());
            assert_eq!(value.size(), std::mem::size_of_val(&custom));
            assert_eq!(value.type_name(), arg.type_name());
            assert_eq!(arg.value().type_name(), arg.type_name());
            assert_eq!(value.display(), None);
            assert_eq!(value.debug().as_deref(), Some("Borrowed(\"borrowed\")"));
        },
        "{:?}",
        custom
    );
    assert_eq!(custom.0, slice);
}

#[test]
fn names_source_order_deduplication_and_count_inference() {
    let value = 1.25;
    let précision = 2;
    inspect_format!(
        |view| {
            assert_eq!(
                view.args().iter().map(|arg| arg.name()).collect::<Vec<_>>(),
                [Some("width"), Some("value"), Some("précision")]
            );
            assert_eq!(view.args()[1].expression(), "value");
            assert!(matches!(view.args()[0].value(), Value::Usize(value) if **value == 8));
            assert!(matches!(view.args()[2].value(), Value::Usize(value) if **value == 2));
        },
        "{value:width$.précision$} {value}",
        width = 8
    );
    inspect_format!(
        |view| {
            assert!(matches!(view.args()[0].value(), Value::Usize(value) if **value == 2));
            assert!(matches!(view.args()[2].value(), Value::Usize(value) if **value == 3));
        },
        "{:.*} {:.*}",
        2,
        1.25,
        3,
        2.5
    );
    let first = 1;
    let second = 2;
    inspect_format!(
        |view| {
            assert_eq!(
                view.args().iter().map(|arg| arg.name()).collect::<Vec<_>>(),
                [Some("second"), Some("first")]
            );
        },
        "{{ignored}} {second} {first} {second}"
    );
}

#[test]
fn opaque_hooks_are_lazy_selective_and_can_retain_both_traits() {
    // Each formatter records its own calls; attachment itself must invoke none.
    struct Counted<'a>(&'a Cell<usize>, &'a Cell<usize>);
    impl fmt::Display for Counted<'_> {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.set(self.0.get() + 1);
            formatter.write_str("display")
        }
    }
    impl fmt::Debug for Counted<'_> {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.1.set(self.1.get() + 1);
            formatter.write_str("debug")
        }
    }
    let display_calls = Cell::new(0);
    let debug_calls = Cell::new(0);
    let counted = Counted(&display_calls, &debug_calls);
    inspect_format!(
        |view| {
            assert_eq!(view.args().len(), 1);
            assert_eq!((display_calls.get(), debug_calls.get()), (0, 0));
            let Value::Opaque(value) = view.args()[0].value() else {
                panic!("expected opaque")
            };
            assert_eq!(value.display().as_deref(), Some("display"));
            assert_eq!((display_calls.get(), debug_calls.get()), (1, 0));
            assert_eq!(value.debug().as_deref(), Some("debug"));
            assert_eq!((display_calls.get(), debug_calls.get()), (1, 1));
            assert_eq!(value.display().as_deref(), Some("display"));
            assert_eq!((display_calls.get(), debug_calls.get()), (2, 1));
        },
        "{0} {0:?} {0}",
        counted
    );
    inspect_format!(
        |view| {
            let Value::Opaque(value) = view.args()[0].value() else {
                panic!("expected opaque")
            };
            assert_eq!(value.debug(), None);
        },
        "{}",
        counted
    );
    inspect_format!(
        |view| {
            let Value::Opaque(value) = view.args()[0].value() else {
                panic!("expected opaque")
            };
            assert_eq!(value.display(), None);
        },
        "{:?}",
        counted
    );
}

#[test]
fn extension_templates_and_unsized_values_keep_borrowing_semantics() {
    // Expressions are an introspection extension, not valid format_args! syntax.
    let mut template_calls = 0;
    let mut value_calls = 0;
    let result = inspect_format!(
        |view| {
            assert_eq!(view.render().unwrap(), "value=42");
            view.args().len()
        },
        {
            template_calls += 1;
            "value={}"
        },
        {
            value_calls += 1;
            42u8
        }
    );
    assert_eq!((template_calls, value_calls, result), (1, 1, 1));
    struct NoTraits;
    inspect_format!(
        |view| {
            assert!(matches!(view.args()[0].value(), Value::Opaque(_)));
        },
        concat!("{", "}"),
        NoTraits
    );
    let owned = String::from("dynamic");
    let erased: &dyn fmt::Display = &owned;
    inspect_format!(
        |view| {
            assert_eq!(view.args()[0].type_name(), "dyn core::fmt::Display");
            assert_eq!(view.render().unwrap(), "dynamic");
        },
        "{}",
        *erased
    );
}

#[test]
fn keyword_names_preserve_hooks_borrows_and_count_inference() {
    // A keyword label is metadata, not a Rust expression. Only its value is
    // evaluated, once, and raw labels have the same canonical name.
    struct DisplayOnly<'a>(&'a str);
    impl fmt::Display for DisplayOnly<'_> {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(self.0)
        }
    }
    #[derive(Debug)]
    struct DebugOnly;
    let owned = String::from("borrowed");
    let mut value = DisplayOnly(&owned);
    let mut calls = 0;
    inspect_format!(|view| {
        assert_eq!(view.args().iter().map(|arg| arg.name()).collect::<Vec<_>>(),
            [Some("type"), Some("self")]);
        assert_eq!(view.args()[0].type_name(), std::any::type_name::<&mut DisplayOnly<'_>>());
        let Value::Opaque(display) = view.args()[0].value() else { panic!("expected opaque") };
        let Value::Opaque(debug) = view.args()[1].value() else { panic!("expected opaque") };
        assert_eq!(display.display().as_deref(), Some("borrowed"));
        assert_eq!(display.debug(), None);
        assert_eq!(debug.display(), None);
        assert_eq!(debug.debug().as_deref(), Some("DebugOnly"));
        let independent = introspect_fmt::Format::new(view.template(), view.args(), None);
        assert_eq!(independent.render().unwrap(), "borrowed DebugOnly borrowed");
    }, "{type} {self:?} {type}", type = { calls += 1; &mut value }, self = DebugOnly);
    assert_eq!(calls, 1);
    assert_eq!(value.0, owned);

    // Keyword count operands keep usize inference, including raw spellings.
    // Width/precision rendering is a separate, still unsupported capability.
    inspect_format!(|view| {
        assert_eq!(view.args().iter().map(|arg| arg.name()).collect::<Vec<_>>(),
            [Some("type"), Some("async"), Some("value")]);
        assert!(matches!(view.args()[0].value(), Value::Usize(value) if **value == 8));
        assert!(matches!(view.args()[1].value(), Value::Usize(value) if **value == 2));
    }, "{value:type$.async$}", r#type = 8, async = 2, value = 1.25);
}
