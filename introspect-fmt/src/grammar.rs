//! Format-string grammar shared by compile-time recipes and runtime parsing.
//! The proc macro includes this file, so both sides parse identically.
//! Placeholders reference arguments by index; names and spec text are resolved
//! here, so a parsed template never consults them while capturing or rendering.

/// The formatting trait a placeholder selects; also indexes the hook table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FormatTrait {
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

/// A literal width/precision, or the index of a usize argument read when used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Count {
    Literal(u16),
    Argument(u16),
}

/// One placeholder's options. Flags are packed because recipes are emitted
/// into every caller's binary; see the associated bit constants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatSpec {
    pub fill: char,
    pub flags: u8,
    pub kind: FormatTrait,
    pub width: Option<Count>,
    pub precision: Option<Count>,
}

impl FormatSpec {
    pub const ALIGN_LEFT: u8 = 1;
    pub const ALIGN_CENTER: u8 = 2;
    pub const ALIGN_RIGHT: u8 = 3;
    const ALIGN_MASK: u8 = 3;
    pub const SIGN_PLUS: u8 = 1 << 2;
    pub const SIGN_MINUS: u8 = 2 << 2;
    const SIGN_MASK: u8 = 3 << 2;
    pub const ALTERNATE: u8 = 1 << 4;
    pub const ZERO: u8 = 1 << 5;
    pub const DEBUG_LOWER_HEX: u8 = 1 << 6;
    pub const DEBUG_UPPER_HEX: u8 = 2 << 6;
    const DEBUG_HEX_MASK: u8 = 3 << 6;

    pub fn align(&self) -> Option<char> {
        match self.flags & Self::ALIGN_MASK {
            Self::ALIGN_LEFT => Some('<'),
            Self::ALIGN_CENTER => Some('^'),
            Self::ALIGN_RIGHT => Some('>'),
            _ => None,
        }
    }
    pub fn sign(&self) -> Option<char> {
        match self.flags & Self::SIGN_MASK {
            Self::SIGN_PLUS => Some('+'),
            Self::SIGN_MINUS => Some('-'),
            _ => None,
        }
    }
    pub fn alternate(&self) -> bool {
        self.flags & Self::ALTERNATE != 0
    }
    pub fn zero(&self) -> bool {
        self.flags & Self::ZERO != 0
    }
    /// `Some(true)` for `X?`, `Some(false)` for `x?`.
    pub fn debug_hex(&self) -> Option<bool> {
        match self.flags & Self::DEBUG_HEX_MASK {
            Self::DEBUG_LOWER_HEX => Some(false),
            Self::DEBUG_UPPER_HEX => Some(true),
            _ => None,
        }
    }
}

/// A `{...}` field at `start..end` of the source; the literal text before it
/// is the source between the previous placeholder's end and `start`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placeholder {
    pub start: u16,
    pub end: u16,
    pub argument: u16,
    pub spec: FormatSpec,
}

#[derive(Debug, PartialEq, Eq)]
pub enum GrammarError {
    Invalid,
    MissingArgument(String),
    Unsupported(String),
}

/// Placeholders in source order. Literal braces are validated while scanning;
/// after any error the iterator is exhausted.
pub struct Parser<'s, R> {
    source: &'s str,
    at: usize,
    next: usize,
    arguments: usize,
    resolve: R,
}

impl<'s, R: Fn(&str) -> Option<usize>> Parser<'s, R> {
    /// `resolve` maps an argument name to its index among `arguments`.
    pub fn new(source: &'s str, arguments: usize, resolve: R) -> Self {
        Self {
            source,
            at: 0,
            next: 0,
            arguments,
            resolve,
        }
    }

    fn placeholder(
        &mut self,
        start: usize,
        end: usize,
        key: &str,
        spec: &str,
    ) -> Result<Placeholder, GrammarError> {
        // Offsets are compact; text after the last placeholder is unbounded.
        let (Ok(start), Ok(end)) = (u16::try_from(start), u16::try_from(end)) else {
            return Err(GrammarError::Unsupported(
                "placeholder beyond 65535 bytes".into(),
            ));
        };
        // A `.*` precision consumes the next positional before the value does.
        let spec = self.spec(spec)?;
        let argument = self.argument(key)?;
        Ok(Placeholder {
            start,
            end,
            argument,
            spec,
        })
    }

    fn index(&self, key: &str) -> Result<u16, GrammarError> {
        let index = match key.parse::<usize>() {
            Ok(index) => Some(index),
            Err(_) => (self.resolve)(key),
        };
        index
            .filter(|index| *index < self.arguments)
            .map(|index| index as u16)
            .ok_or_else(|| GrammarError::MissingArgument(key.into()))
    }

    // Implicit positions count every `{}` and `.*`, even past the argument list.
    fn position(&mut self) -> Result<u16, GrammarError> {
        let index = self.next;
        self.next += 1;
        if index < self.arguments {
            Ok(index as u16)
        } else {
            Err(GrammarError::MissingArgument(index.to_string()))
        }
    }

    fn argument(&mut self, key: &str) -> Result<u16, GrammarError> {
        let key = key.trim();
        if key.is_empty() {
            return self.position();
        }
        self.index(key)
    }

    fn count_spec(&self, rest: &mut &str) -> Result<Option<Count>, GrammarError> {
        // A count ending in '$' may be a Unicode name or a positional index.
        if let Some(end) = rest.find('$') {
            let key = &rest[..end];
            if !key.is_empty() && !key.contains('.') {
                let index = self.index(key)?;
                *rest = &rest[end + 1..];
                return Ok(Some(Count::Argument(index)));
            }
        }
        let end = rest.bytes().take_while(u8::is_ascii_digit).count();
        if end == 0 {
            return Ok(None);
        }
        // Rust itself rejects literal counts above u16::MAX.
        let value = rest[..end]
            .parse()
            .map_err(|_| GrammarError::Unsupported((*rest).to_owned()))?;
        *rest = &rest[end..];
        Ok(Some(Count::Literal(value)))
    }

    fn spec(&mut self, text: &str) -> Result<FormatSpec, GrammarError> {
        let invalid = || GrammarError::Unsupported(text.to_owned());
        let mut rest = text.trim_end();
        let mut spec = FormatSpec {
            fill: ' ',
            flags: 0,
            kind: FormatTrait::Display,
            width: None,
            precision: None,
        };
        let align = |ch: char| match ch {
            '<' => Some(FormatSpec::ALIGN_LEFT),
            '^' => Some(FormatSpec::ALIGN_CENTER),
            '>' => Some(FormatSpec::ALIGN_RIGHT),
            _ => None,
        };
        let mut chars = rest.chars();
        let first = chars.next();
        if let Some(flag) = chars.next().and_then(align) {
            let fill = first.unwrap();
            spec.fill = fill;
            spec.flags |= flag;
            rest = &rest[fill.len_utf8() + 1..];
        } else if let Some(flag) = first.and_then(align) {
            spec.flags |= flag;
            rest = &rest[1..];
        }
        if let Some(tail) = rest.strip_prefix('+') {
            spec.flags |= FormatSpec::SIGN_PLUS;
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix('-') {
            spec.flags |= FormatSpec::SIGN_MINUS;
            rest = tail;
        }
        if let Some(tail) = rest.strip_prefix('#') {
            spec.flags |= FormatSpec::ALTERNATE;
            rest = tail;
        }
        // `0$` references argument zero; `08` and `0width$` start with a flag.
        if rest.starts_with('0') && !rest.starts_with("0$") {
            spec.flags |= FormatSpec::ZERO;
            rest = &rest[1..];
        }
        spec.width = self.count_spec(&mut rest)?;
        if let Some(tail) = rest.strip_prefix('.') {
            rest = tail;
            spec.precision = Some(if let Some(tail) = rest.strip_prefix('*') {
                rest = tail;
                Count::Argument(self.position()?)
            } else {
                self.count_spec(&mut rest)?.ok_or_else(invalid)?
            });
        }
        spec.kind = match rest {
            "" => FormatTrait::Display,
            "?" => FormatTrait::Debug,
            "x?" => {
                spec.flags |= FormatSpec::DEBUG_LOWER_HEX;
                FormatTrait::Debug
            }
            "X?" => {
                spec.flags |= FormatSpec::DEBUG_UPPER_HEX;
                FormatTrait::Debug
            }
            "b" => FormatTrait::Binary,
            "o" => FormatTrait::Octal,
            "x" => FormatTrait::LowerHex,
            "X" => FormatTrait::UpperHex,
            "e" => FormatTrait::LowerExp,
            "E" => FormatTrait::UpperExp,
            "p" => FormatTrait::Pointer,
            _ => return Err(invalid()),
        };
        Ok(spec)
    }
}

impl<R: Fn(&str) -> Option<usize>> Iterator for Parser<'_, R> {
    type Item = Result<Placeholder, GrammarError>;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let Some(offset) = self.source[self.at..].find(['{', '}']) else {
                self.at = self.source.len();
                return None;
            };
            let start = self.at + offset;
            let text = &self.source[start..];
            if text.starts_with("{{") || text.starts_with("}}") {
                self.at = start + 2;
                continue;
            }
            // On any error, consume the tail so iteration cannot repeat it.
            self.at = self.source.len();
            if text.starts_with('{') {
                let mut chars = text[1..].char_indices().peekable();
                while let Some((at, ch)) = chars.next() {
                    if ch == '}' {
                        let field = &text[1..at + 1];
                        // A brace immediately after ':' followed by alignment is
                        // a fill character, matching the macro's field boundaries.
                        if field.ends_with(':')
                            && chars
                                .peek()
                                .is_some_and(|(_, next)| matches!(next, '<' | '^' | '>'))
                        {
                            continue;
                        }
                        let end = start + at + 2;
                        self.at = end;
                        let (key, spec) = field.split_once(':').unwrap_or((field, ""));
                        return Some(self.placeholder(start, end, key, spec));
                    }
                }
            }
            return Some(Err(GrammarError::Invalid));
        }
    }
}

/// Write a literal segment between placeholders, collapsing `{{` and `}}`.
/// Parsed sources contain only doubled braces here.
pub fn unescape<E>(text: &str, mut write: impl FnMut(&str) -> Result<(), E>) -> Result<(), E> {
    let mut rest = text;
    while let Some(at) = rest.find(['{', '}']) {
        write(&rest[..=at])?;
        rest = rest.get(at + 2..).unwrap_or("");
    }
    write(rest)
}
