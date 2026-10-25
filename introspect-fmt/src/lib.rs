//! Experimental borrowed, inspectable formatting arguments.
//!
//! `inspect_format!(|f| { ... }, "{}", expr)` accepts the usual argument *syntax*
//! of `format_args!`, but passes a scoped `Format` to a callback. Unlike
//! `format_args!`, it exposes borrowed typed primitive values and original Rust
//! type names. Unlike `format_args!`, it does not guarantee identical formatting.
//!
//! Macro arguments are borrowed once rather than evaluated twice or moved:
//! a call involving `&mut T` creates *shared* references to the resulting
//! `&mut T` value, never a second exclusive borrow.

use std::{any::type_name, fmt, marker::PhantomData};

/// Capture ordinary formatting inputs without moving their values.
/// The wrapper supplies its defining crate hygienically, including through
/// dependency aliases and downstream re-exports.
#[macro_export]
macro_rules! inspect_format {
    ($($input:tt)*) => {
        // Forward token trees so caller literals and expressions keep their spans.
        $crate::__private::__inspect_format!(($crate) $($input)*)
    };
}

mod render;
pub use render::RenderError;

/// Each argument in its original macro order, followed by any implicitly
/// captured named variables in order of first use.
#[derive(Debug)]
pub struct FormatArg<'a> {
    name: Option<&'static str>,
    expression: &'static str,
    rust_type_name: &'static str,
    value: Value<'a>,
}

impl<'a> FormatArg<'a> {
    #[doc(hidden)]
    pub fn new(
        name: Option<&'static str>,
        expression: &'static str,
        rust_type_name: &'static str,
        value: Value<'a>,
    ) -> Self {
        Self {
            name,
            expression,
            rust_type_name,
            value,
        }
    }

    pub fn name(&self) -> Option<&'static str> {
        self.name
    }
    pub fn expression(&self) -> &'static str {
        self.expression
    }
    pub fn value(&self) -> &Value<'a> {
        &self.value
    }
    pub fn type_name(&self) -> &'static str {
        self.rust_type_name
    }
}

/// Safe borrowed access to supported primitives, with a type-erased fallback.
/// Only formatting traits requested by the template are required; there are no
/// blanket `Debug`, `Display`, `Any`, or `'static` bounds on user types.
#[derive(Debug)]
pub enum Value<'a> {
    Bool(&'a bool),
    Char(&'a char),
    I8(&'a i8),
    I16(&'a i16),
    I32(&'a i32),
    I64(&'a i64),
    I128(&'a i128),
    Isize(&'a isize),
    U8(&'a u8),
    U16(&'a u16),
    U32(&'a u32),
    U64(&'a u64),
    U128(&'a u128),
    Usize(&'a usize),
    F32(&'a f32),
    F64(&'a f64),
    Str(&'a str),
    Opaque(OpaqueValue<'a>),
}

impl Value<'_> {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Bool(_) => "bool",
            Self::Char(_) => "char",
            Self::I8(_) => "i8",
            Self::I16(_) => "i16",
            Self::I32(_) => "i32",
            Self::I64(_) => "i64",
            Self::I128(_) => "i128",
            Self::Isize(_) => "isize",
            Self::U8(_) => "u8",
            Self::U16(_) => "u16",
            Self::U32(_) => "u32",
            Self::U64(_) => "u64",
            Self::U128(_) => "u128",
            Self::Usize(_) => "usize",
            Self::F32(_) => "f32",
            Self::F64(_) => "f64",
            Self::Str(_) => "str",
            Self::Opaque(v) => v.type_name,
        }
    }
}

/// Non-primitive borrowed value. A raw address and size are exposed for
/// experiments; neither provides safe dynamic downcasting on its own.
/// Optional hooks render default Display/Debug output on demand. Their erased
/// pointer borrows the macro's reference slot for the duration of the callback.
#[derive(Debug)]
pub struct OpaqueValue<'a> {
    type_name: &'static str,
    address: *const (),
    size: usize,
    // This points at a borrowed reference slot, preserving even DST metadata.
    // Only Formatters::attach can pair it with the monomorphized functions.
    formatting_address: *const (),
    display: Option<unsafe fn(*const ()) -> String>,
    debug: Option<unsafe fn(*const ()) -> String>,
    _lifetime: PhantomData<&'a ()>,
}
impl OpaqueValue<'_> {
    pub fn type_name(&self) -> &'static str {
        self.type_name
    }
    pub fn address(&self) -> *const () {
        self.address
    }
    pub fn size(&self) -> usize {
        self.size
    }
    /// Format with default Display options, if requested by a literal template.
    /// Placeholder width, precision, and flags are not retained here.
    pub fn display(&self) -> Option<String> {
        self.display.map(|format| {
            // SAFETY: attach pairs this function with its original borrowed T.
            unsafe { format(self.formatting_address) }
        })
    }
    /// Format with default Debug options, if requested by a literal template.
    pub fn debug(&self) -> Option<String> {
        self.debug.map(|format| {
            // SAFETY: attach pairs this function with its original borrowed T.
            unsafe { format(self.formatting_address) }
        })
    }
}

/// Backwards-compatible name for the original opaque representation.
pub type Opaque<'a> = OpaqueValue<'a>;

/// A short-lived borrowed view. It cannot safely escape the callback that
/// `inspect_format!` constructs.
#[derive(Debug)]
pub struct Format<'a> {
    template: &'static str,
    arguments: &'a [FormatArg<'a>],
    rendered: Option<fmt::Arguments<'a>>,
}
impl<'a> Format<'a> {
    #[doc(hidden)]
    pub fn new(
        template: &'static str,
        arguments: &'a [FormatArg<'a>],
        rendered: Option<fmt::Arguments<'a>>,
    ) -> Self {
        Self {
            template,
            arguments,
            rendered,
        }
    }
    pub fn template(&self) -> &'static str {
        self.template
    }
    pub fn args(&self) -> &[FormatArg<'a>] {
        self.arguments
    }
    /// Reconstruct from the template, typed primitives, and opaque hooks.
    /// Does not read retained `fmt::Arguments`. Unsupported options return an
    /// error rather than producing a string with different formatting.
    pub fn render(&self) -> Result<String, RenderError> {
        render::render(self.template, self.arguments)
    }
    /// `None` means this prototype deliberately declined to reconstruct
    /// standard formatting, not that any argument was omitted.
    pub fn formatted(&self) -> Option<&fmt::Arguments<'a>> {
        self.rendered.as_ref()
    }
}

/// Hidden macro implementation details; exported so an invocation from another
/// crate can select primitive-specialized methods at its own monomorphized site.
#[doc(hidden)]
pub mod __private {
    use super::*;

    // Public only because downstream wrapper expansions must reach this entry.
    #[doc(hidden)]
    pub use introspect_fmt_macros::inspect_format as __inspect_format;

    // A function bound supplies the closure's input type before Rust checks its
    // body; immediately calling an untyped closure does not provide that context.
    pub fn invoke<'a, R>(view: Format<'a>, callback: impl FnOnce(Format<'a>) -> R) -> R {
        callback(view)
    }

    /// A wrapper with its own type lets stable method lookup select concrete
    /// primitive types first, then add one autoref for the universal fallback.
    /// The wrapper is never retained and does not move the borrowed value.
    pub struct Probe<'a, T: ?Sized> {
        value: &'a T,
    }
    impl<'a, T: ?Sized> Probe<'a, T> {
        pub fn new(value: &'a T) -> Self {
            Self { value }
        }
    }
    pub fn rust_type_name<T: ?Sized>(_: &T) -> &'static str {
        type_name::<T>()
    }

    // Forward explicitly to T's trait impl rather than the blanket impl for
    // &T (notably Pointer). This sized wrapper also keeps the prototype's
    // existing support for borrowed unsized values, without copying or moving.
    pub struct Render<'a, T: ?Sized>(&'a T);
    impl<'a, T: ?Sized> Render<'a, T> {
        pub fn new(value: &'a T) -> Self {
            Self(value)
        }
    }
    macro_rules! forward_formatting {
        ($($trait:ident),* $(,)?) => {$(
            impl<T: ?Sized + fmt::$trait> fmt::$trait for Render<'_, T> {
                fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                    // Forward the original Formatter so flags and counts survive.
                    fmt::$trait::fmt(self.0, formatter)
                }
            }
        )*};
    }
    forward_formatting! {
        Display, Debug, Binary, Octal, LowerHex, UpperHex, LowerExp, UpperExp, Pointer
    }

    // The macro passes a reference to its stable local &T binding. Using a
    // sized reference slot avoids erasing the metadata of unsized user values.
    pub struct Formatters<'a, T> {
        value: &'a T,
        display: Option<unsafe fn(*const ()) -> String>,
        debug: Option<unsafe fn(*const ()) -> String>,
    }
    impl<'a, T> Formatters<'a, T> {
        pub fn new(value: &'a T) -> Self {
            Self {
                value,
                display: None,
                debug: None,
            }
        }
        pub fn display(mut self) -> Self
        where
            T: fmt::Display,
        {
            // Only instantiate this thunk when the template requires Display.
            unsafe fn render<T: fmt::Display>(pointer: *const ()) -> String {
                // SAFETY: attach stores a live, aligned &T with this exact thunk.
                let value = unsafe { &*pointer.cast::<T>() };
                format!("{}", value)
            }
            self.display = Some(render::<T>);
            self
        }
        pub fn debug(mut self) -> Self
        where
            T: fmt::Debug,
        {
            // Debug-only arguments never instantiate the Display thunk.
            unsafe fn render<T: fmt::Debug>(pointer: *const ()) -> String {
                // SAFETY: attach stores a live, aligned &T with this exact thunk.
                let value = unsafe { &*pointer.cast::<T>() };
                format!("{:?}", value)
            }
            self.debug = Some(render::<T>);
            self
        }
        pub fn attach(self, mut value: Value<'a>) -> Value<'a> {
            if let Value::Opaque(opaque) = &mut value {
                // Set pointer and both functions together so safe callers cannot
                // combine a thunk for one type with a pointer to another type.
                opaque.formatting_address = self.value as *const T as *const ();
                opaque.display = self.display;
                opaque.debug = self.debug;
            }
            value
        }
    }

    pub trait Capture<'a> {
        fn __introspect_fmt_capture(self) -> Value<'a>;
    }

    macro_rules! primitives {
        ($($name:ident => $type:ty),* $(,)?) => {
            $(
                impl<'a> Capture<'a> for Probe<'a, $type> {
                    fn __introspect_fmt_capture(self) -> Value<'a> {
                        Value::$name(self.value)
                    }
                }
                // Even an expression of type `&T` remains introspectible.
                impl<'a, 'b: 'a> Capture<'a> for Probe<'a, &'b $type> {
                    fn __introspect_fmt_capture(self) -> Value<'a> {
                        Value::$name(*self.value)
                    }
                }
                impl<'a, 'b: 'a> Capture<'a> for Probe<'a, &'b mut $type> {
                    fn __introspect_fmt_capture(self) -> Value<'a> {
                        Value::$name(&**self.value)
                    }
                }
            )*
        };
    }

    primitives! {
        Bool => bool, Char => char,
        I8 => i8, I16 => i16, I32 => i32, I64 => i64,
        I128 => i128, Isize => isize,
        U8 => u8, U16 => u16, U32 => u32, U64 => u64,
        U128 => u128, Usize => usize,
        F32 => f32, F64 => f64
    }

    impl<'a> Capture<'a> for Probe<'a, str> {
        fn __introspect_fmt_capture(self) -> Value<'a> {
            Value::Str(self.value)
        }
    }
    impl<'a, 'b: 'a> Capture<'a> for Probe<'a, &'b str> {
        fn __introspect_fmt_capture(self) -> Value<'a> {
            Value::Str(self.value)
        }
    }
    impl<'a> Capture<'a> for Probe<'a, String> {
        fn __introspect_fmt_capture(self) -> Value<'a> {
            Value::Str(self.value.as_str())
        }
    }
    impl<'a, 'b: 'a> Capture<'a> for Probe<'a, &'b String> {
        fn __introspect_fmt_capture(self) -> Value<'a> {
            Value::Str(self.value.as_str())
        }
    }

    // No bounds whatsoever on T. The lifetime of Value::Opaque is tied to
    // the original borrowed argument, NOT to the short-lived Probe wrapper.
    impl<'a, T: ?Sized> Capture<'a> for &Probe<'a, T> {
        fn __introspect_fmt_capture(self) -> Value<'a> {
            let value = self.value;
            Value::Opaque(OpaqueValue {
                type_name: type_name::<T>(),
                address: value as *const T as *const (),
                size: std::mem::size_of_val(value),
                formatting_address: std::ptr::null(),
                display: None,
                debug: None,
                _lifetime: PhantomData,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn macro_resolves_its_defining_crate_for_internal_calls() {
        let captured = String::from("internal");
        crate::inspect_format!(
            |view| {
                assert!(
                    matches!(view.args()[1].value(), crate::Value::Str(value) if *value == "internal")
                );
                let independent = crate::Format::new(view.template(), view.args(), None);
                assert_eq!(independent.render().unwrap(), "internal 42");
            },
            "{captured} {}",
            42
        );
    }
}
