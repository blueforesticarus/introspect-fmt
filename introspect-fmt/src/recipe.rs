//! Owned capture recipes: a parsed template plus the argument kinds that select
//! its slot layout. Literal templates use the invocation's static placeholders;
//! other sources are parsed once at runtime and shared by reference count.
use crate::{
    template::OwnedTemplate, ArgumentKind, CaptureSink, Format, Parsed, RenderError, Slots,
    Template,
};
use smallvec::SmallVec;
use std::{fmt, sync::Arc};

#[derive(Debug, Clone)]
enum Source {
    // The source is the invocation's literal; the static holds only placeholders.
    Static(&'static str, &'static Parsed),
    Owned(Arc<OwnedTemplate>),
}

/// Everything needed to capture a `Format` and render it again later.
#[derive(Debug, Clone)]
pub struct Recipe {
    source: Source,
    kinds: SmallVec<[ArgumentKind; 8]>,
}

impl PartialEq for Recipe {
    fn eq(&self, other: &Self) -> bool {
        let same = match (&self.source, &other.source) {
            (Source::Static(_, a), Source::Static(_, b)) => std::ptr::eq(*a, *b),
            (Source::Owned(a), Source::Owned(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        self.kinds == other.kinds && (same || self.template() == other.template())
    }
}
impl Eq for Recipe {}

impl Recipe {
    /// Literal templates are never parsed here; others parse once.
    pub fn new(format: &Format<'_>) -> Result<Self, RenderError> {
        let source = match format.parsed() {
            Some(parsed) => Source::Static(format.template(), parsed),
            None => Source::Owned(Arc::new(OwnedTemplate::compile(format)?)),
        };
        Ok(Self {
            source,
            kinds: format.kinds().collect(),
        })
    }
    /// Rebuild a recipe from its portable parts: source, every argument's
    /// name in order, and every argument's kind.
    pub fn parse(
        source: &str,
        names: &[Option<&str>],
        kinds: &[ArgumentKind],
    ) -> Result<Self, RenderError> {
        if names.len() != kinds.len() {
            return Err(RenderError::InvalidTemplate);
        }
        Ok(Self {
            source: Source::Owned(Arc::new(OwnedTemplate::parse(source, names)?)),
            kinds: kinds.into(),
        })
    }
    /// Whether `format` can be captured with this recipe. Literal templates
    /// compare by static address; runtime ones by source and argument names.
    pub fn matches(&self, format: &Format<'_>) -> bool {
        let same = match (&self.source, format.parsed()) {
            (Source::Static(_, a), Some(b)) => std::ptr::eq(*a, b),
            (Source::Owned(a), None) => a.matches(format),
            _ => false,
        };
        // Generic callsites share one template across differently typed instances.
        same && self.kinds.iter().copied().eq(format.kinds())
    }

    pub fn template(&self) -> Template<'_> {
        match &self.source {
            Source::Static(source, parsed) => parsed.template(source),
            Source::Owned(template) => template.template(),
        }
    }
    pub fn kinds(&self) -> &[ArgumentKind] {
        &self.kinds
    }
    /// Captured value kinds per slot; rendered slots are `Str`.
    pub fn slot_kinds(&self) -> impl Iterator<Item = ArgumentKind> + '_ {
        let template = self.template();
        template.slot_kinds(&self.kinds)
    }
    pub fn value_count(&self) -> usize {
        self.slot_kinds().count()
    }
    /// Heap capacity excludes this inline value; static templates are code data.
    pub fn heap_bytes(&self) -> usize {
        let kinds = if self.kinds.spilled() {
            self.kinds.capacity() * size_of::<ArgumentKind>()
        } else {
            0
        };
        kinds
            + match &self.source {
                Source::Static(..) => 0,
                Source::Owned(template) => {
                    2 * size_of::<usize>() + size_of::<OwnedTemplate>() + template.heap_bytes()
                }
            }
    }

    /// Copy primitives and render opaque/pointer placeholders into `sink`.
    /// User formatting runs here, once per rendered placeholder.
    pub fn capture(
        &self,
        format: &Format<'_>,
        sink: &mut impl CaptureSink,
    ) -> Result<(), RenderError> {
        if !self.kinds.iter().copied().eq(format.kinds()) {
            return Err(RenderError::InvalidTemplate);
        }
        self.template().capture(format, sink)
    }
    /// Reconstruct from captured slots with no user values or trait hooks.
    pub fn render_to(
        &self,
        slots: &(impl Slots + ?Sized),
        output: &mut impl fmt::Write,
    ) -> Result<(), RenderError> {
        self.template().render_to(&self.kinds, slots, output)
    }
    pub fn render(&self, slots: &(impl Slots + ?Sized)) -> Result<String, RenderError> {
        let mut output = String::new();
        self.render_to(slots, &mut output)?;
        Ok(output)
    }
}
