//! Shared format grammar and count resolution for both rendering backends.
//! Keep counts as usize until a backend explicitly validates its API limits.

use crate::{FormatArg, RenderError, Value};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Kind {
    Display,
    Debug,
    Binary,
    Octal,
    LowerHex,
    UpperHex,
    LowerExp,
    UpperExp,
    Pointer,
}

#[derive(Debug)]
pub(crate) struct Spec {
    pub fill: char,
    pub align: Option<char>,
    pub sign: Option<char>,
    pub alternate: bool,
    pub zero: bool,
    pub width: Option<usize>,
    pub precision: Option<usize>,
    pub kind: Kind,
    pub debug_hex: Option<bool>,
}

pub(crate) fn argument<'a>(
    key: &str,
    arguments: &'a [FormatArg<'_>],
) -> Result<&'a FormatArg<'a>, RenderError> {
    let found = if let Ok(index) = key.parse::<usize>() {
        arguments.get(index)
    } else {
        arguments
            .iter()
            .find(|argument| argument.name() == Some(key))
    };
    found.ok_or_else(|| RenderError::MissingArgument(key.to_owned()))
}

fn count_value(key: &str, arguments: &[FormatArg<'_>]) -> Result<usize, RenderError> {
    match argument(key, arguments)?.value() {
        Value::Usize(value) => Ok(**value),
        // Nonliteral templates do not provide the macro's usize inference.
        // Report the limitation rather than silently accepting another type.
        _ => Err(RenderError::UnsupportedSpecifier(format!(
            "count {key} must be usize"
        ))),
    }
}

fn count(rest: &mut &str, arguments: &[FormatArg<'_>]) -> Result<Option<usize>, RenderError> {
    // A count ending in '$' may be a Unicode name or a positional index.
    if let Some(end) = rest.find('$') {
        let key = &rest[..end];
        if !key.is_empty() && !key.contains('.') {
            let value = count_value(key, arguments)?;
            *rest = &rest[end + 1..];
            return Ok(Some(value));
        }
    }
    let end = rest.bytes().take_while(u8::is_ascii_digit).count();
    if end == 0 {
        return Ok(None);
    }
    let value = rest[..end]
        .parse()
        .map_err(|_| RenderError::UnsupportedSpecifier((*rest).to_owned()))?;
    *rest = &rest[end..];
    Ok(Some(value))
}

impl Spec {
    pub fn parse(
        spec: &str,
        arguments: &[FormatArg<'_>],
        next: &mut usize,
    ) -> Result<Self, RenderError> {
        let invalid = || RenderError::UnsupportedSpecifier(spec.to_owned());
        let mut rest = spec.trim_end();
        let mut result = Self {
            fill: ' ',
            align: None,
            sign: None,
            alternate: false,
            zero: false,
            width: None,
            precision: None,
            kind: Kind::Display,
            debug_hex: None,
        };
        let mut chars = rest.chars();
        let first = chars.next();
        let second = chars.next();
        if second.is_some_and(|ch| matches!(ch, '<' | '^' | '>')) {
            result.fill = first.unwrap();
            result.align = second;
            rest = &rest[result.fill.len_utf8() + 1..];
        } else if first.is_some_and(|ch| matches!(ch, '<' | '^' | '>')) {
            result.align = first;
            rest = &rest[1..];
        }
        if rest.starts_with(['+', '-']) {
            result.sign = rest.chars().next();
            rest = &rest[1..];
        }
        if let Some(tail) = rest.strip_prefix('#') {
            result.alternate = true;
            rest = tail;
        }
        // `0$` references argument zero; `08` and `0width$` start with a flag.
        if rest.starts_with('0') && !rest.starts_with("0$") {
            result.zero = true;
            rest = &rest[1..];
        }
        result.width = count(&mut rest, arguments)?;
        if let Some(tail) = rest.strip_prefix('.') {
            rest = tail;
            result.precision = if let Some(tail) = rest.strip_prefix('*') {
                rest = tail;
                let value = count_value(&next.to_string(), arguments)?;
                *next += 1;
                Some(value)
            } else {
                Some(count(&mut rest, arguments)?.ok_or_else(invalid)?)
            };
        }
        result.kind = match rest {
            "" => Kind::Display,
            "?" => Kind::Debug,
            "x?" | "X?" => {
                result.debug_hex = Some(rest == "X?");
                Kind::Debug
            }
            "b" => Kind::Binary,
            "o" => Kind::Octal,
            "x" => Kind::LowerHex,
            "X" => Kind::UpperHex,
            "e" => Kind::LowerExp,
            "E" => Kind::UpperExp,
            "p" => Kind::Pointer,
            _ => return Err(invalid()),
        };
        Ok(result)
    }

    pub fn suffix(&self) -> &'static str {
        match self.kind {
            Kind::Display => "",
            Kind::Debug => match self.debug_hex {
                Some(false) => "x?",
                Some(true) => "X?",
                None => "?",
            },
            Kind::Binary => "b",
            Kind::Octal => "o",
            Kind::LowerHex => "x",
            Kind::UpperHex => "X",
            Kind::LowerExp => "e",
            Kind::UpperExp => "E",
            Kind::Pointer => "p",
        }
    }

    pub fn literal(&self) -> String {
        use std::fmt::Write;
        // Reuse the small stable renderer after substituting dynamic counts.
        let mut text = String::new();
        if let Some(align) = self.align {
            text.push(self.fill);
            text.push(align);
        }
        if let Some(sign) = self.sign {
            text.push(sign);
        }
        if self.alternate {
            text.push('#');
        }
        if self.zero {
            text.push('0');
        }
        if let Some(width) = self.width {
            write!(text, "{width}").unwrap();
        }
        if let Some(precision) = self.precision {
            write!(text, ".{precision}").unwrap();
        }
        text.push_str(self.suffix());
        text
    }
}
