//! Parsed templates. A literal template's placeholders become a `static Parsed`
//! at its macro invocation, paired with the source at runtime; other sources parse at runtime into an `OwnedTemplate`. Templates
//! are independent of argument types; `Recipe` pairs one with the argument kinds
//! that select the value slot layout.
//!
//! Slot layout: every primitive argument in argument order, then one rendered
//! string per placeholder whose argument is opaque or which formats a pointer.
use crate::{
    formatting::Formatters,
    grammar::{self, Count, FormatTrait, GrammarError, Parser, Placeholder},
    spec::Spec,
    ArgumentKind, Format, FormatArg, RenderError, Value,
};
use std::fmt;

impl From<GrammarError> for RenderError {
    fn from(error: GrammarError) -> Self {
        match error {
            GrammarError::Invalid => Self::InvalidTemplate,
            GrammarError::MissingArgument(key) => Self::MissingArgument(key),
            GrammarError::Unsupported(spec) => Self::UnsupportedSpecifier(spec),
        }
    }
}

/// A parsed format string: the source plus its placeholders by argument index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Template<'a> {
    source: &'a str,
    placeholders: &'a [Placeholder],
    arguments: u16,
}

/// A literal template's placeholders, emitted as one `static` per macro
/// invocation. It omits the source, which `Format::template` already carries,
/// so the static holds a single pointer.
#[derive(Debug, PartialEq, Eq)]
pub struct Parsed {
    placeholders: &'static [Placeholder],
    arguments: u16,
}

impl Parsed {
    // Macro-emitted only: offsets and indices are trusted here, but capture and
    // rendering still report InvalidTemplate rather than panic if they are wrong.
    #[doc(hidden)]
    pub const fn new_unchecked(placeholders: &'static [Placeholder], arguments: u16) -> Self {
        Self {
            placeholders,
            arguments,
        }
    }
    /// Pair with the invocation's source, which must be the same literal.
    pub(crate) fn template(&'static self, source: &'static str) -> Template<'static> {
        Template {
            source,
            placeholders: self.placeholders,
            arguments: self.arguments,
        }
    }
}

/// Receives captured slots in layout order.
pub trait CaptureSink {
    /// A primitive argument; never `Value::Opaque`.
    fn value(&mut self, value: &Value<'_>);
    /// A new rendered slot; the returned buffer receives its text.
    fn text(&mut self) -> &mut String;
}

/// Reads captured slots by layout index; rendered slots are `Value::Str`.
pub trait Slots {
    fn slot(&self, index: usize) -> Option<Value<'_>>;
}

fn rendered(placeholder: &Placeholder, kind: ArgumentKind) -> bool {
    kind == ArgumentKind::Opaque || placeholder.spec.kind == FormatTrait::Pointer
}

// Primitive slots precede rendered ones and keep argument order.
fn primitive_slot(kinds: &[ArgumentKind], index: u16) -> Option<usize> {
    let index = usize::from(index);
    (kinds.get(index)? != &ArgumentKind::Opaque).then(|| {
        kinds[..index]
            .iter()
            .filter(|kind| **kind != ArgumentKind::Opaque)
            .count()
    })
}

fn count_value(value: Option<&Value<'_>>, index: u16) -> Result<usize, RenderError> {
    match value {
        Some(Value::Usize(value)) => Ok(**value),
        // Nonliteral templates do not provide the macro's usize inference.
        // Report the limitation rather than silently accepting another type.
        _ => Err(RenderError::UnsupportedSpecifier(format!(
            "count {index} must be usize"
        ))),
    }
}

fn borrowed_count(arguments: &[FormatArg<'_>], index: u16) -> Result<usize, RenderError> {
    count_value(
        arguments.get(usize::from(index)).map(FormatArg::value),
        index,
    )
}

// Literal segments come from the source between placeholders.
pub(crate) fn render_pieces(
    source: &str,
    placeholders: impl Iterator<Item = Result<Placeholder, RenderError>>,
    output: &mut dyn fmt::Write,
    mut argument: impl FnMut(&Placeholder, &mut dyn fmt::Write) -> Result<(), RenderError>,
) -> Result<(), RenderError> {
    let literal = |text: Option<&str>, output: &mut dyn fmt::Write| {
        let text = text.ok_or(RenderError::InvalidTemplate)?;
        grammar::unescape(text, |text| output.write_str(text))
            .map_err(|_| RenderError::FormattingFailed)
    };
    let mut at = 0;
    for placeholder in placeholders {
        let placeholder = placeholder?;
        literal(source.get(at..usize::from(placeholder.start)), output)?;
        argument(&placeholder, output)?;
        at = usize::from(placeholder.end);
    }
    literal(source.get(at..), output)
}

// Borrowed rendering, shared by Format::render_to with or without a recipe.
pub(crate) fn render_borrowed(
    source: &str,
    placeholders: impl Iterator<Item = Result<Placeholder, RenderError>>,
    arguments: &[FormatArg<'_>],
    output: &mut dyn fmt::Write,
) -> Result<(), RenderError> {
    render_pieces(source, placeholders, output, |placeholder, output| {
        let spec = Spec::resolve(&placeholder.spec, |index| borrowed_count(arguments, index))?;
        let argument = arguments
            .get(usize::from(placeholder.argument))
            .ok_or(RenderError::InvalidTemplate)?;
        crate::render::render_argument(argument, &spec, output)
    })
}

pub(crate) fn parse<'s>(
    source: &'s str,
    arguments: &'s [FormatArg<'_>],
) -> impl Iterator<Item = Result<Placeholder, RenderError>> + 's {
    Parser::new(source, arguments.len(), move |key| {
        arguments.iter().position(|arg| arg.name() == Some(key))
    })
    .map(|placeholder| placeholder.map_err(RenderError::from))
}

impl<'a> Template<'a> {
    pub fn source(&self) -> &'a str {
        self.source
    }
    pub fn placeholders(&self) -> &'a [Placeholder] {
        self.placeholders
    }
    pub fn argument_count(&self) -> usize {
        usize::from(self.arguments)
    }

    pub(crate) fn slot_kinds<'k>(
        &self,
        kinds: &'k [ArgumentKind],
    ) -> impl Iterator<Item = ArgumentKind> + use<'a, 'k> {
        let placeholders = self.placeholders;
        let primitive = kinds.iter().copied().filter(|k| *k != ArgumentKind::Opaque);
        let text = placeholders.iter().filter_map(move |placeholder| {
            let kind = *kinds.get(usize::from(placeholder.argument))?;
            rendered(placeholder, kind).then_some(ArgumentKind::Str)
        });
        primitive.chain(text)
    }
    pub(crate) fn capture(
        &self,
        format: &Format<'_>,
        sink: &mut impl CaptureSink,
    ) -> Result<(), RenderError> {
        let arguments = format.args();
        if arguments.len() != self.argument_count() {
            return Err(RenderError::InvalidTemplate);
        }
        // Reject non-usize counts before writing, so nothing unrenderable is stored.
        // Literal templates cannot fail here: the macro binds counts as &usize.
        for placeholder in self.placeholders {
            for count in [placeholder.spec.width, placeholder.spec.precision] {
                if let Some(Count::Argument(index)) = count {
                    borrowed_count(arguments, index)?;
                }
            }
        }
        for argument in arguments {
            if !matches!(argument.value(), Value::Opaque(_)) {
                sink.value(argument.value());
            }
        }
        for placeholder in self.placeholders {
            let argument = arguments
                .get(usize::from(placeholder.argument))
                .ok_or(RenderError::InvalidTemplate)?;
            if rendered(placeholder, argument.value().kind()) {
                let spec =
                    Spec::resolve(&placeholder.spec, |index| borrowed_count(arguments, index))?;
                crate::render::render_argument(argument, &spec, sink.text())?;
            }
        }
        Ok(())
    }

    pub(crate) fn render_to(
        &self,
        kinds: &[ArgumentKind],
        slots: &(impl Slots + ?Sized),
        output: &mut impl fmt::Write,
    ) -> Result<(), RenderError> {
        if kinds.len() != self.argument_count() {
            return Err(RenderError::InvalidTemplate);
        }
        let mut text = kinds.iter().filter(|k| **k != ArgumentKind::Opaque).count();
        let placeholders = self.placeholders.iter().copied().map(Ok);
        render_pieces(self.source, placeholders, output, |placeholder, output| {
            let kind = *kinds
                .get(usize::from(placeholder.argument))
                .ok_or(RenderError::InvalidTemplate)?;
            if rendered(placeholder, kind) {
                let Some(Value::Str(value)) = slots.slot(text) else {
                    return Err(RenderError::InvalidTemplate);
                };
                text += 1;
                return output
                    .write_str(value)
                    .map_err(|_| RenderError::FormattingFailed);
            }
            let spec = Spec::resolve(&placeholder.spec, |index| {
                let slot = primitive_slot(kinds, index).and_then(|slot| slots.slot(slot));
                count_value(slot.as_ref(), index)
            })?;
            let value = primitive_slot(kinds, placeholder.argument)
                .and_then(|slot| slots.slot(slot))
                .filter(|value| value.kind() == kind)
                .ok_or(RenderError::InvalidTemplate)?;
            render_primitive(&value, &spec, output)
        })
    }
}

/// A runtime-parsed template, for nonliteral sources or imported recipes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OwnedTemplate {
    source: Box<str>,
    placeholders: Box<[Placeholder]>,
    arguments: u16,
    // Placeholders resolved names to indices; reuse must see the same names.
    names: Box<[Option<Box<str>>]>,
}

impl OwnedTemplate {
    /// `names` lists every argument in order, as `Format::args` would.
    pub fn parse(source: &str, names: &[Option<&str>]) -> Result<Self, RenderError> {
        let arguments = u16::try_from(names.len())
            .map_err(|_| RenderError::UnsupportedSpecifier("over 65535 arguments".into()))?;
        let placeholders = Parser::new(source, names.len(), |key| {
            names.iter().position(|name| *name == Some(key))
        })
        .collect::<Result<_, _>>()?;
        Ok(Self {
            source: source.into(),
            placeholders,
            arguments,
            names: names.iter().map(|name| name.map(Box::from)).collect(),
        })
    }
    pub fn compile(format: &Format<'_>) -> Result<Self, RenderError> {
        let names: Vec<_> = format.args().iter().map(FormatArg::name).collect();
        Self::parse(format.template(), &names)
    }
    /// Whether `format` has this source and the same argument names in order.
    pub fn matches(&self, format: &Format<'_>) -> bool {
        *self.source == *format.template()
            && self.names.len() == format.args().len()
            && self
                .names
                .iter()
                .zip(format.args())
                .all(|(name, arg)| name.as_deref() == arg.name())
    }
    pub fn template(&self) -> Template<'_> {
        Template {
            source: &self.source,
            placeholders: &self.placeholders,
            arguments: self.arguments,
        }
    }
    /// Heap capacity owned by the recipe.
    pub fn heap_bytes(&self) -> usize {
        self.source.len()
            + std::mem::size_of_val(&*self.placeholders)
            + std::mem::size_of_val(&*self.names)
            + self
                .names
                .iter()
                .flatten()
                .map(|name| name.len())
                .sum::<usize>()
    }
}

// Hooks are recreated over the captured value for each use, so primitive
// output matches the borrowed path's trait dispatch exactly.
fn render_primitive(
    value: &Value<'_>,
    spec: &Spec,
    output: &mut dyn fmt::Write,
) -> Result<(), RenderError> {
    macro_rules! render {
        ($value:expr, $($trait:ident),*) => {{
            let borrowed = $value;
            let formatting = Formatters::new(&borrowed)$(.$trait())*.finish();
            if !formatting.render(spec, output)? {
                return Err(RenderError::UnsupportedSpecifier(spec.literal()));
            }
        }};
    }
    macro_rules! integers {
        ($($variant:ident),*) => { match *value {
            $(Value::$variant(value) => render!(value, display, debug, binary, octal, lower_hex, upper_hex, lower_exp, upper_exp),)*
            Value::F32(value) => render!(value, display, debug, lower_exp, upper_exp),
            Value::F64(value) => render!(value, display, debug, lower_exp, upper_exp),
            Value::Bool(value) => render!(value, display, debug),
            Value::Char(value) => render!(value, display, debug),
            Value::Str(value) => render!(value, display, debug),
            Value::Opaque(_) => return Err(RenderError::InvalidTemplate),
        } };
    }
    integers!(I8, I16, I32, I64, I128, Isize, U8, U16, U32, U64, U128, Usize);
    Ok(())
}
