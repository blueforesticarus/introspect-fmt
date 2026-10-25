//! Independent reconstruction from the public template and captured values.
//! Format one value at a time; never delegate the full message to Rust's
//! formatter or read the optional retained formatting arguments.

use crate::{FormatArg, Value};
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
            Self::InvalidTemplate => formatter.write_str("unmatched brace in template"),
            Self::MissingArgument(name) => write!(formatter, "argument {name:?} was not captured"),
            Self::UnsupportedSpecifier(spec) => {
                write!(formatter, "unsupported format specifier {spec:?}")
            }
            Self::MissingFormatter(name) => write!(formatter, "opaque value has no {name} hook"),
            Self::FormattingFailed => formatter.write_str("argument formatter returned fmt::Error"),
        }
    }
}
impl std::error::Error for RenderError {}

// This interface deliberately cannot access Format's retained formatter.
// Fail explicitly when options cannot be reproduced; never silently drop them.
pub(crate) fn render(template: &str, arguments: &[FormatArg<'_>]) -> Result<String, RenderError> {
    let mut output = String::new();
    let mut chars = template.chars().peekable();
    let mut next = 0usize;
    while let Some(ch) = chars.next() {
        match ch {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                output.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                output.push('}');
            }
            '{' => {
                let mut field = String::new();
                loop {
                    match chars.next() {
                        // Immediately after ':', a brace followed by alignment
                        // is fill. Match the proc-macro's field boundary rule.
                        Some('}')
                            if !(field.ends_with(':')
                                && chars
                                    .peek()
                                    .is_some_and(|next| matches!(next, '<' | '^' | '>'))) =>
                        {
                            break;
                        }
                        Some(ch) => field.push(ch),
                        None => return Err(RenderError::InvalidTemplate),
                    }
                }
                let (head, spec) = field.split_once(':').unwrap_or((&field, ""));
                // Star precision consumes its count before the implicit value.
                // The same parser/count semantics feed both feature modes.
                let spec = crate::spec::Spec::parse(spec, arguments, &mut next)?;
                let head = head.trim();
                let argument = if head.is_empty() {
                    let index = next;
                    next += 1;
                    arguments.get(index)
                } else if let Ok(index) = head.parse::<usize>() {
                    arguments.get(index)
                } else {
                    arguments
                        .iter()
                        .find(|argument| argument.name() == Some(head))
                }
                .ok_or_else(|| RenderError::MissingArgument(head.to_owned()))?;
                let rendered = match &argument.formatting {
                    Some(formatting) => formatting.render(&spec, &mut output)?,
                    None => false,
                };
                if !rendered {
                    // Macro-generated templates may have no captured hooks.
                    // Retain the small primitive fallback in either feature mode.
                    output.push_str(&render_value(argument.value(), &spec.literal())?);
                }
            }
            '}' => return Err(RenderError::InvalidTemplate),
            ch => output.push(ch),
        }
    }
    Ok(output)
}

// Fixed per-value formats reuse Rust's primitive implementations. The message's
// argument selection, literal pieces, and escaping are reconstructed above.
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
) -> Result<String, RenderError> {
    // Rust handles rounding before rendering, including ties and special values.
    if let Some(precision) = literal_precision(spec) {
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

// Only a complete literal precision is supported here; flags stay explicit errors.
fn literal_precision(spec: &str) -> Option<usize> {
    let digits = spec.strip_prefix('.')?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

// Separate padding from primitive rendering so Unicode fill stays independent
// of numeric signs, prefixes, and Rust's precision/rounding implementations.
fn render_value(value: &Value<'_>, spec: &str) -> Result<String, RenderError> {
    if matches!(value, Value::Opaque(_)) {
        return render_unpadded(value, spec);
    }
    let unsupported = || RenderError::UnsupportedSpecifier(spec.to_owned());
    let mut rest = spec;
    let mut fill = ' ';
    let mut alignment = None;
    let mut chars = rest.chars();
    let first = chars.next();
    let second = chars.next();
    if second.is_some_and(|ch| matches!(ch, '<' | '^' | '>')) {
        fill = first.unwrap();
        alignment = second;
        rest = &rest[fill.len_utf8() + 1..];
    } else if first.is_some_and(|ch| matches!(ch, '<' | '^' | '>')) {
        alignment = first;
        rest = &rest[1..];
    }
    let flag_end = rest
        .char_indices()
        .find(|(_, ch)| !matches!(ch, '+' | '-' | '#'))
        .map_or(rest.len(), |(index, _)| index);
    let flags = &rest[..flag_end];
    rest = &rest[flag_end..];
    let zero = rest.starts_with('0');
    if zero {
        rest = &rest[1..];
    }
    let width_end = rest.bytes().take_while(u8::is_ascii_digit).count();
    let width = if width_end == 0 {
        0
    } else {
        rest[..width_end]
            .parse::<usize>()
            .map_err(|_| unsupported())?
    };
    rest = &rest[width_end..];
    let core = format!("{flags}{rest}");
    let numeric = !matches!(value, Value::Bool(_) | Value::Char(_) | Value::Str(_));
    // Debug implementations have type-specific padding semantics. Leave these
    // explicit errors until options can be passed to their original formatter.
    if rest.ends_with('?') && (width != 0 || alignment.is_some() || zero) {
        return Err(unsupported());
    }
    let text = render_unpadded(value, &core)?;
    let padding = width.saturating_sub(text.chars().count());
    if zero && numeric {
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
        return Ok(format!(
            "{}{}{}",
            &text[..split],
            "0".repeat(padding),
            &text[split..]
        ));
    }
    let alignment = alignment.unwrap_or(if numeric { '>' } else { '<' });
    let left = match alignment {
        '>' => padding,
        '^' => padding / 2,
        _ => 0,
    };
    let pad = |count| std::iter::repeat_n(fill, count).collect::<String>();
    Ok(format!("{}{text}{}", pad(left), pad(padding - left)))
}

fn render_unpadded(value: &Value<'_>, spec: &str) -> Result<String, RenderError> {
    match value {
        Value::Bool(value) => basic(value, spec),
        Value::Char(value) => basic(value, spec),
        Value::Str(value) => {
            // String precision counts Unicode scalar values rather than bytes.
            if let Some(precision) = literal_precision(spec) {
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
        Value::F32(value) => float(value, spec),
        Value::F64(value) => float(value, spec),
        Value::Opaque(value) => match spec {
            "" => value
                .display()
                .ok_or(RenderError::MissingFormatter("Display")),
            "?" => value.debug().ok_or(RenderError::MissingFormatter("Debug")),
            _ => Err(RenderError::UnsupportedSpecifier(spec.to_owned())),
        },
    }
}
