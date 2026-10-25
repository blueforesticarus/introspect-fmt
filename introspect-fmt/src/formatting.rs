//! Erased trait calls preserve the original argument type and borrow metadata.
//! Only Formatter construction depends on nightly; capture works on stable.

use crate::{
    spec::{Kind, Spec},
    RenderError,
};
use std::{fmt, marker::PhantomData};

type Hook = unsafe fn(*const (), &mut fmt::Formatter<'_>) -> fmt::Result;

#[doc(hidden)]
#[derive(Debug)]
pub struct Formatting<'a> {
    // Address of a live &T slot, never the metadata-erased address of a DST.
    address: *const (),
    hooks: [Option<Hook>; 9],
    _borrow: PhantomData<&'a ()>,
}

#[doc(hidden)]
pub struct Formatters<'a, T: ?Sized> {
    formatting: Formatting<'a>,
    _value: PhantomData<&'a T>,
}

// Explicit trait calls avoid an extra blanket reference implementation, which
// would change Pointer output. Each method instantiates only its own bound.
macro_rules! hooks {
    ($($method:ident => $trait:ident),* $(,)?) => {$(
        pub fn $method(mut self) -> Self where T: fmt::$trait {
            unsafe fn call<T: ?Sized + fmt::$trait>(address: *const (), formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                // SAFETY: the builder pairs this thunk with an &'a T reference
                // slot of the exact same type, which outlives the returned hooks.
                let slot = unsafe { &*address.cast::<&T>() };
                fmt::$trait::fmt(*slot, formatter)
            }
            self.formatting.hooks[Kind::$trait as usize] = Some(call::<T>);
            self
        }
    )*};
}

impl<'a, T: ?Sized> Formatters<'a, T> {
    pub fn new(value: &'a &'a T) -> Self {
        Self {
            formatting: Formatting {
                address: value as *const &T as *const (),
                hooks: [None; 9],
                _borrow: PhantomData,
            },
            _value: PhantomData,
        }
    }

    hooks! {
        display => Display, debug => Debug, binary => Binary, octal => Octal,
        lower_hex => LowerHex, upper_hex => UpperHex, lower_exp => LowerExp,
        upper_exp => UpperExp, pointer => Pointer,
    }

    pub fn finish(self) -> Formatting<'a> {
        self.formatting
    }
}

// This proxy also lets stable Rust provide a few common option combinations
// without building a runtime Formatter or reimplementing custom formatting.
#[cfg(not(feature = "nightly"))]
struct Call<'a> {
    address: *const (),
    hook: Hook,
    _borrow: PhantomData<&'a ()>,
}
#[cfg(not(feature = "nightly"))]
impl fmt::Display for Call<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // SAFETY: only Formatting::render constructs Call, from a paired hook.
        unsafe { (self.hook)(self.address, formatter) }
    }
}
#[cfg(not(feature = "nightly"))]
impl fmt::Debug for Call<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl Formatting<'_> {
    pub(crate) fn render(&self, spec: &Spec, output: &mut String) -> Result<bool, RenderError> {
        let Some(hook) = self.hooks[spec.kind as usize] else {
            return Ok(false);
        };
        #[cfg(feature = "nightly")]
        {
            use fmt::{Alignment, DebugAsHex, FormattingOptions, Sign};
            let mut options = FormattingOptions::new();
            options
                .fill(spec.fill)
                .align(spec.align.map(|align| match align {
                    '<' => Alignment::Left,
                    '^' => Alignment::Center,
                    _ => Alignment::Right,
                }))
                .sign(
                    spec.sign
                        .map(|sign| if sign == '+' { Sign::Plus } else { Sign::Minus }),
                )
                .alternate(spec.alternate)
                .sign_aware_zero_pad(spec.zero)
                .debug_as_hex(spec.debug_hex.map(|upper| {
                    if upper {
                        DebugAsHex::Upper
                    } else {
                        DebugAsHex::Lower
                    }
                }));
            // This unstable API currently accepts u16 counts. Never truncate a
            // captured usize; oversized values remain explicit render errors.
            let narrow = |value: usize| {
                u16::try_from(value).map_err(|_| RenderError::UnsupportedSpecifier(spec.literal()))
            };
            options
                .width(spec.width.map(narrow).transpose()?)
                .precision(spec.precision.map(narrow).transpose()?);
            let mut formatter = options.create_formatter(output);
            // SAFETY: the builder installed this exact hook and borrowed slot.
            unsafe { hook(self.address, &mut formatter) }
                .map_err(|_| RenderError::FormattingFailed)?;
            Ok(true)
        }
        #[cfg(not(feature = "nightly"))]
        {
            use fmt::Write;
            let call = Call {
                address: self.address,
                hook,
                _borrow: PhantomData,
            };
            // Stable format macros already support dynamic counts. A small set
            // of literal alignments covers ordinary custom width/precision too,
            // preserving None versus Some(0) for implementations inspecting it.
            if spec.fill == ' '
                && spec.sign.is_none()
                && !spec.alternate
                && !spec.zero
                && spec.debug_hex.is_none()
            {
                macro_rules! aligned {
                    ($align:literal) => {
                        match (spec.width, spec.precision) {
                            (None, None) => {
                                write!(output, concat!("{call:", $align, "}"), call = call)
                            }
                            (Some(width), None) => write!(
                                output,
                                concat!("{call:", $align, "width$}"),
                                call = call,
                                width = width
                            ),
                            (None, Some(precision)) => write!(
                                output,
                                concat!("{call:", $align, ".precision$}"),
                                call = call,
                                precision = precision
                            ),
                            (Some(width), Some(precision)) => write!(
                                output,
                                concat!("{call:", $align, "width$.precision$}"),
                                call = call,
                                width = width,
                                precision = precision
                            ),
                        }
                    };
                }
                let result = match spec.align {
                    None => aligned!(""),
                    Some('<') => aligned!("<"),
                    Some('^') => aligned!("^"),
                    Some(_) => aligned!(">"),
                };
                result.map_err(|_| RenderError::FormattingFailed)?;
                return Ok(true);
            }
            // Keep the fallback small: default traits and common Debug flags.
            // More complex primitive options use the existing best-effort path.
            if spec.align.is_some() || spec.zero || spec.width.is_some() || spec.precision.is_some()
            {
                return Ok(false);
            }
            // Signs alone are cheap to forward on stable too. Combined hex
            // Debug flags stay outside this deliberately small dispatch table.
            if let Some(sign) = spec.sign {
                if spec.debug_hex.is_some() {
                    return Ok(false);
                }
                let result = match (sign, spec.alternate) {
                    ('+', false) => write!(output, "{call:+}"),
                    ('+', true) => write!(output, "{call:+#}"),
                    (_, false) => write!(output, "{call:-}"),
                    (_, true) => write!(output, "{call:-#}"),
                };
                result.map_err(|_| RenderError::FormattingFailed)?;
                return Ok(true);
            }
            let result = match (spec.alternate, spec.debug_hex) {
                (false, None) => write!(output, "{call}"),
                (true, None) => write!(output, "{call:#}"),
                (false, Some(false)) => write!(output, "{call:x?}"),
                (false, Some(true)) => write!(output, "{call:X?}"),
                (true, Some(false)) => write!(output, "{call:#x?}"),
                (true, Some(true)) => write!(output, "{call:#X?}"),
            };
            result.map_err(|_| RenderError::FormattingFailed)?;
            Ok(true)
        }
    }
}
