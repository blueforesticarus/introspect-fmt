//! Owned argument capture. Templates retain grammar and primitive types once;
//! opaque values retain the exact output of each requested trait invocation.
use crate::{formatting::Formatters, spec::Spec, Format, FormatArg, RenderError, Value};

// Keep exact primitive types here: widening signed integers changes hex output,
// and widening f32 changes decimal/debug formatting even when its bits survive.
macro_rules! owned_values {
    ($($variant:ident($ty:ty)),* $(,)?) => {
        #[derive(Debug, Clone, PartialEq)]
        pub enum OwnedValue { $($variant($ty),)* Text(String) }
        impl OwnedValue {
            pub(crate) fn borrow(&self) -> FormatArg<'_> {
                // Hooks are rebuilt at the render call, where the reference slot
                // lives long enough; this helper only borrows the owned payload.
                let value = match self {
                    $(Self::$variant(value) => Value::$variant(value),)*
                    Self::Text(value) => Value::Str(value),
                };
                FormatArg { name: None, expression: "", rust_type_name: value.type_name(), value, formatting: None }
            }
        }
    };
}
owned_values! {
    Bool(bool), Char(char), I8(i8), I16(i16), I32(i32), I64(i64), I128(i128), Isize(isize),
    U8(u8), U16(u16), U32(u32), U64(u64), U128(u128), Usize(usize), F32(f32), F64(f64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateArgument {
    pub name: Option<&'static str>,
    pub kind: &'static str,
    // Opaque arguments have no primitive slot, but may have several rendered slots.
    pub slot: Option<usize>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplatePiece {
    Literal(String),
    Argument {
        index: usize,
        spec: String,
        next: usize,
        rendered: Option<usize>,
    },
}
use TemplateArgument as Argument;
use TemplatePiece as Piece;

/// A callsite-owned recipe. Counts remain references to captured usize arguments;
/// custom/pointer formatting becomes an already-rendered string per occurrence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedTemplate {
    source: &'static str,
    arguments: Vec<Argument>,
    pieces: Vec<Piece>,
    value_count: usize,
}
impl OwnedTemplate {
    /// Portable recipe access keeps serialization independent of the storage codec.
    pub fn arguments(&self) -> &[TemplateArgument] {
        &self.arguments
    }
    pub fn pieces(&self) -> &[TemplatePiece] {
        &self.pieces
    }

    /// Imported recipes must validate every index before rendering can borrow it.
    /// Grammar and value types are checked by `render` with the captured values.
    pub fn from_parts(
        source: &'static str,
        arguments: Vec<TemplateArgument>,
        pieces: Vec<TemplatePiece>,
        value_count: usize,
    ) -> Result<Self, RenderError> {
        if value_count > arguments.len().saturating_add(pieces.len())
            || arguments
                .iter()
                .any(|a| a.slot.is_some_and(|slot| slot >= value_count))
            || pieces.iter().any(|piece| match piece {
                Piece::Literal(_) => false,
                Piece::Argument {
                    index,
                    next,
                    rendered,
                    ..
                } => {
                    *index >= arguments.len()
                        || *next > arguments.len()
                        || rendered.is_some_and(|slot| slot >= value_count)
                        || (rendered.is_none() && arguments[*index].slot.is_none())
                }
            })
        {
            return Err(RenderError::InvalidTemplate);
        }
        Ok(Self {
            source,
            arguments,
            pieces,
            value_count,
        })
    }

    /// Heap capacity owned by the recipe; static macro strings are shared code data.
    pub fn heap_bytes(&self) -> usize {
        self.arguments.capacity() * std::mem::size_of::<Argument>()
            + self.pieces.capacity() * std::mem::size_of::<Piece>()
            + self
                .pieces
                .iter()
                .map(|piece| match piece {
                    Piece::Literal(text) => text.capacity(),
                    Piece::Argument { spec, .. } => spec.capacity(),
                })
                .sum::<usize>()
    }
    pub fn source(&self) -> &'static str {
        self.source
    }
    pub fn value_count(&self) -> usize {
        self.value_count
    }

    /// Compare schemas without invoking user formatting or reparsing the template.
    pub fn matches(&self, format: &Format<'_>) -> bool {
        self.source == format.template()
            && self.arguments.len() == format.args().len()
            && self.arguments.iter().zip(format.args()).all(|(a, b)| {
                a.name == b.name()
                    && a.kind == b.value().type_name()
                    && a.slot.is_none() == matches!(b.value(), Value::Opaque(_))
            })
    }

    pub fn compile(format: &Format<'_>) -> Result<Self, RenderError> {
        let mut value_count = 0;
        let arguments = format
            .args()
            .iter()
            .map(|arg| {
                let slot = if matches!(arg.value(), Value::Opaque(_)) {
                    None
                } else {
                    let slot = value_count;
                    value_count += 1;
                    Some(slot)
                };
                Argument {
                    name: arg.name(),
                    kind: arg.value().type_name(),
                    slot,
                }
            })
            .collect();
        let mut result = Self {
            source: format.template(),
            arguments,
            pieces: Vec::new(),
            value_count,
        };
        let mut next = 0;
        for piece in crate::template::pieces(format.template()) {
            match piece? {
                crate::template::Piece::Literal(text) => {
                    result.pieces.push(Piece::Literal(text.into()))
                }
                crate::template::Piece::Argument { key, spec } => {
                    let before = next;
                    let parsed = Spec::parse(spec, format.args(), &mut next)?;
                    let index = crate::spec::argument_index(key, format.args(), &mut next)?;
                    let arg = &format.args()[index];
                    // Pointer dispatch depends on the original reference type and
                    // address, so snapshot it even when capture found a primitive.
                    let rendered = if matches!(arg.value(), Value::Opaque(_))
                        || matches!(parsed.kind, crate::spec::Kind::Pointer)
                    {
                        let slot = result.value_count;
                        result.value_count += 1;
                        Some(slot)
                    } else {
                        None
                    };
                    result.pieces.push(Piece::Argument {
                        index,
                        spec: spec.into(),
                        next: before,
                        rendered,
                    });
                }
            }
        }
        Ok(result)
    }

    pub fn capture(&self, format: &Format<'_>) -> Result<Vec<OwnedValue>, RenderError> {
        if !self.matches(format) {
            return Err(RenderError::InvalidTemplate);
        }
        let mut values = vec![OwnedValue::Text(String::new()); self.value_count];
        for (argument, schema) in format.args().iter().zip(&self.arguments) {
            if let Some(slot) = schema.slot {
                macro_rules! copy_values {
                    ($($variant:ident),*) => { match argument.value() {
                        $(Value::$variant(value) => OwnedValue::$variant(**value),)*
                        Value::Str(value) => OwnedValue::Text((*value).into()),
                        Value::Opaque(_) => unreachable!(),
                    } };
                }
                values[slot] = copy_values!(
                    Bool, Char, I8, I16, I32, I64, I128, Isize, U8, U16, U32, U64, U128, Usize,
                    F32, F64
                );
            }
        }
        for piece in &self.pieces {
            if let Piece::Argument {
                index,
                spec,
                next,
                rendered: Some(slot),
            } = piece
            {
                let mut next = *next;
                let parsed = Spec::parse(spec, format.args(), &mut next)?;
                let mut text = String::new();
                crate::render::render_argument(&format.args()[*index], &parsed, &mut text)?;
                values[*slot] = OwnedValue::Text(text);
            }
        }
        Ok(values)
    }

    /// Reconstruct with no borrowed user values or retained fmt::Arguments.
    pub fn render(&self, values: &[OwnedValue]) -> Result<String, RenderError> {
        if values.len() != self.value_count {
            return Err(RenderError::InvalidTemplate);
        }
        let arguments: Vec<_> = self
            .arguments
            .iter()
            .map(|schema| {
                let mut arg = match schema.slot {
                    Some(slot) => values[slot].borrow(),
                    None => OwnedValue::borrow(&EMPTY),
                };
                arg.name = schema.name;
                arg
            })
            .collect();
        // No borrowed formatting hooks survive capture. Primitive hooks are
        // recreated from owned values inside render_primitive for each use.
        let mut output = String::new();
        for piece in &self.pieces {
            match piece {
                Piece::Literal(text) => output.push_str(text),
                Piece::Argument {
                    index,
                    spec,
                    next,
                    rendered,
                } => {
                    if let Some(slot) = rendered {
                        let OwnedValue::Text(text) = &values[*slot] else {
                            return Err(RenderError::InvalidTemplate);
                        };
                        output.push_str(text);
                    } else {
                        let mut next = *next;
                        let parsed = Spec::parse(spec, &arguments, &mut next)?;
                        let slot = self.arguments[*index]
                            .slot
                            .ok_or(RenderError::InvalidTemplate)?;
                        render_primitive(&values[slot], &parsed, &mut output)?;
                    }
                }
            }
        }
        Ok(output)
    }
}
static EMPTY: OwnedValue = OwnedValue::Bool(false);

fn render_primitive(
    value: &OwnedValue,
    spec: &Spec,
    output: &mut String,
) -> Result<(), RenderError> {
    macro_rules! render {
        ($value:expr, $($trait:ident),*) => {{
            let borrowed = $value;
            let formatting = Formatters::new(&borrowed)$(.$trait())*.finish();
            if !formatting.render(spec, output)? { return Err(RenderError::UnsupportedSpecifier(spec.literal())); }
        }};
    }
    macro_rules! integers {
        ($($variant:ident),*) => { match value {
            $(OwnedValue::$variant(value) => render!(value, display, debug, binary, octal, lower_hex, upper_hex, lower_exp, upper_exp),)*
            OwnedValue::F32(value) => render!(value, display, debug, lower_exp, upper_exp),
            OwnedValue::F64(value) => render!(value, display, debug, lower_exp, upper_exp),
            OwnedValue::Bool(value) => render!(value, display, debug),
            OwnedValue::Char(value) => render!(value, display, debug),
            OwnedValue::Text(value) => render!(value, display, debug),
        } };
    }
    integers!(I8, I16, I32, I64, I128, Isize, U8, U16, U32, U64, U128, Usize);
    Ok(())
}
