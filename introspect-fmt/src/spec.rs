//! Resolved placeholder options for both rendering backends. Parsing lives in
//! grammar.rs; this only substitutes dynamic counts into a parsed spec.
//! Keep counts as usize until a backend explicitly validates its API limits.

use crate::{
    grammar::{Count, FormatSpec},
    RenderError,
};

pub(crate) use crate::grammar::FormatTrait as Kind;

#[derive(Clone, Copy, Debug)]
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

impl Spec {
    /// `count` reads a usize argument by index; literal counts need no lookup.
    pub fn resolve(
        spec: &FormatSpec,
        count: impl Fn(u16) -> Result<usize, RenderError>,
    ) -> Result<Self, RenderError> {
        let resolve = |value: Option<Count>| match value {
            None => Ok(None),
            Some(Count::Literal(value)) => Ok(Some(usize::from(value))),
            Some(Count::Argument(index)) => count(index).map(Some),
        };
        Ok(Self {
            fill: spec.fill,
            align: spec.align(),
            sign: spec.sign(),
            alternate: spec.alternate(),
            zero: spec.zero(),
            width: resolve(spec.width)?,
            precision: resolve(spec.precision)?,
            kind: spec.kind,
            debug_hex: spec.debug_hex(),
        })
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
