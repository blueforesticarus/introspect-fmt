//! Independent per-argument reconstruction; template.rs walks literals and
//! placeholders. Format one value at a time; never delegate the full message to Rust's
//! formatter or read the optional retained formatting arguments.

use crate::{
    spec::{Kind, Spec},
    FormatArg, Value,
};
use std::fmt;

/// Information the prototype cannot currently reconstruct.
#[derive(Debug, PartialEq, Eq)]
pub enum RenderError {
    InvalidTemplate,
    MissingArgument(String),
    UnsupportedSpecifier(String),
    MissingFormatter(&'static str),
    FormattingFailed,
}

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTemplate => {
                formatter.write_str("unmatched brace, or captured values do not fit the template")
            }
            Self::MissingArgument(name) => write!(formatter, "argument {name:?} was not captured"),
            Self::UnsupportedSpecifier(spec) => {
                write!(formatter, "unsupported format specifier {spec:?}")
            }
            Self::MissingFormatter(name) => write!(formatter, "opaque value has no {name} hook"),
            Self::FormattingFailed => {
                formatter.write_str("writer or argument formatter returned fmt::Error")
            }
        }
    }
}
impl std::error::Error for RenderError {}

fn formatting<'a>(argument: &'a FormatArg<'_>) -> Option<&'a crate::formatting::Formatting<'a>> {
    argument.formatting.or_else(|| match argument.value() {
        Value::Opaque(value) => value.formatting,
        _ => None,
    })
}

// Shared by borrowed rendering and owned capture so custom trait options agree.
pub(crate) fn render_argument(
    argument: &FormatArg<'_>,
    spec: &Spec,
    output: &mut dyn fmt::Write,
) -> Result<(), RenderError> {
    if !formatting(argument)
        .map(|f| f.render(spec, output))
        .transpose()?
        .unwrap_or(false)
    {
        render_value(argument.value(), spec, output)?;
    }
    Ok(())
}

// Fixed per-value formats reuse Rust's primitive implementations. The message's
// argument selection, literal pieces, and escaping are reconstructed in template.rs.
fn basic<T: ?Sized + fmt::Display + fmt::Debug>(
    value: &T,
    spec: &str,
) -> Result<String, RenderError> {
    match spec {
        "" => Ok(value.to_string()),
        "?" => Ok(format!("{value:?}")),
        "x?" => Ok(format!("{value:x?}")),
        "X?" => Ok(format!("{value:X?}")),
        _ => Err(RenderError::UnsupportedSpecifier(spec.to_owned())),
    }
}

fn integer<T>(value: &T, spec: &str) -> Result<String, RenderError>
where
    T: fmt::Display
        + fmt::Debug
        + fmt::Binary
        + fmt::Octal
        + fmt::LowerHex
        + fmt::UpperHex
        + fmt::LowerExp
        + fmt::UpperExp,
{
    match spec {
        // Delegate sign and radix prefix semantics to the primitive formatter.
        "+" => Ok(format!("{value:+}")),
        "-" => Ok(format!("{value:-}")),
        "#b" => Ok(format!("{value:#b}")),
        "#o" => Ok(format!("{value:#o}")),
        "#x" => Ok(format!("{value:#x}")),
        "#X" => Ok(format!("{value:#X}")),
        "+#b" => Ok(format!("{value:+#b}")),
        "+#o" => Ok(format!("{value:+#o}")),
        "+#x" => Ok(format!("{value:+#x}")),
        "+#X" => Ok(format!("{value:+#X}")),
        "b" => Ok(format!("{value:b}")),
        "o" => Ok(format!("{value:o}")),
        "x" => Ok(format!("{value:x}")),
        "X" => Ok(format!("{value:X}")),
        "e" => Ok(format!("{value:e}")),
        "E" => Ok(format!("{value:E}")),
        _ => basic(value, spec),
    }
}

fn float<T: fmt::Display + fmt::Debug + fmt::LowerExp + fmt::UpperExp>(
    value: &T,
    spec: &str,
    precision: Option<usize>,
) -> Result<String, RenderError> {
    // Rust handles rounding before rendering, including ties and special values.
    if let Some(precision) = precision {
        return Ok(format!("{value:.precision$}"));
    }
    match spec {
        "+" => Ok(format!("{value:+}")),
        "-" => Ok(format!("{value:-}")),
        "e" => Ok(format!("{value:e}")),
        "E" => Ok(format!("{value:E}")),
        _ => basic(value, spec),
    }
}

// Strip only padding options for the primitive fallback, using the shared
// parsed spec rather than maintaining a second format grammar here.
fn fallback_text(value: &Value<'_>, spec: &Spec) -> Result<String, RenderError> {
    // Preserve opaque error details instead of stripping unsupported options.
    if matches!(value, Value::Opaque(_)) {
        return render_unpadded(value, spec);
    }
    if matches!(spec.kind, Kind::Debug)
        && (spec.width.unwrap_or(0) != 0 || spec.align.is_some() || spec.zero)
    {
        return Err(RenderError::UnsupportedSpecifier(spec.literal()));
    }
    let core = Spec {
        fill: ' ',
        align: None,
        zero: false,
        width: None,
        ..*spec
    };
    render_unpadded(value, &core)
}

// Stream padding separately; only the best-effort stable primitive fallback
// allocates a per-value string. Captured hooks stream their original trait call.
fn render_value(
    value: &Value<'_>,
    spec: &Spec,
    output: &mut dyn fmt::Write,
) -> Result<(), RenderError> {
    let text = fallback_text(value, spec)?;
    let numeric = !matches!(value, Value::Bool(_) | Value::Char(_) | Value::Str(_));
    let padding = spec.width.unwrap_or(0).saturating_sub(text.chars().count());
    if spec.zero && numeric {
        // Sign-aware zero padding overrides alignment and follows radix prefixes.
        let sign = usize::from(text.starts_with(['+', '-']));
        let prefix = if ["0x", "0o", "0b"]
            .iter()
            .any(|prefix| text[sign..].starts_with(prefix))
        {
            2
        } else {
            0
        };
        let split = sign + prefix;
        output
            .write_str(&text[..split])
            .map_err(|_| RenderError::FormattingFailed)?;
        pad(output, '0', padding)?;
        return output
            .write_str(&text[split..])
            .map_err(|_| RenderError::FormattingFailed);
    }
    let alignment = spec.align.unwrap_or(if numeric { '>' } else { '<' });
    let left = match alignment {
        '>' => padding,
        '^' => padding / 2,
        _ => 0,
    };
    pad(output, spec.fill, left)?;
    output
        .write_str(&text)
        .map_err(|_| RenderError::FormattingFailed)?;
    pad(output, spec.fill, padding - left)
}

// Avoid both a padding allocation and newer std::iter::repeat_n requirements.
fn pad(output: &mut dyn fmt::Write, fill: char, count: usize) -> Result<(), RenderError> {
    for _ in 0..count {
        output
            .write_char(fill)
            .map_err(|_| RenderError::FormattingFailed)?;
    }
    Ok(())
}

fn render_unpadded(value: &Value<'_>, parsed: &Spec) -> Result<String, RenderError> {
    let literal = parsed.literal();
    let spec = literal.as_str();
    // Precision-only support remains intentionally narrow on the stable path.
    let precision =
        if parsed.sign.is_none() && !parsed.alternate && matches!(parsed.kind, Kind::Display) {
            parsed.precision
        } else {
            None
        };
    match value {
        Value::Bool(value) => basic(value, spec),
        Value::Char(value) => basic(value, spec),
        Value::Str(value) => {
            // String precision counts Unicode scalar values rather than bytes.
            if let Some(precision) = precision {
                Ok(format!("{value:.precision$}"))
            } else {
                basic(value, spec)
            }
        }
        Value::I8(value) => integer(value, spec),
        Value::I16(value) => integer(value, spec),
        Value::I32(value) => integer(value, spec),
        Value::I64(value) => integer(value, spec),
        Value::I128(value) => integer(value, spec),
        Value::Isize(value) => integer(value, spec),
        Value::U8(value) => integer(value, spec),
        Value::U16(value) => integer(value, spec),
        Value::U32(value) => integer(value, spec),
        Value::U64(value) => integer(value, spec),
        Value::U128(value) => integer(value, spec),
        Value::Usize(value) => integer(value, spec),
        Value::F32(value) => float(value, spec, precision),
        Value::F64(value) => float(value, spec, precision),
        // Opaque values always use the shared hook table above.
        Value::Opaque(_) => Err(match spec {
            "" => RenderError::MissingFormatter("Display"),
            "?" => RenderError::MissingFormatter("Debug"),
            _ => RenderError::UnsupportedSpecifier(spec.to_owned()),
        }),
    }
}
