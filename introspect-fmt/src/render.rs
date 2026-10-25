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
                        Some('}') => break,
                        Some(ch) => field.push(ch),
                        None => return Err(RenderError::InvalidTemplate),
                    }
                }
                let (head, spec) = field.split_once(':').unwrap_or((&field, ""));
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
                output.push_str(&render_value(argument.value(), spec.trim_end())?);
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
    match spec {
        "e" => Ok(format!("{value:e}")),
        "E" => Ok(format!("{value:E}")),
        _ => basic(value, spec),
    }
}

fn render_value(value: &Value<'_>, spec: &str) -> Result<String, RenderError> {
    match value {
        Value::Bool(value) => basic(value, spec),
        Value::Char(value) => basic(value, spec),
        Value::Str(value) => basic(value, spec),
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
