//! Shared, allocation-free literal/placeholder traversal for borrowed rendering
//! and callsite-owned recipes. Counts and argument selection live in spec.rs.
use crate::RenderError;

pub(crate) enum Piece<'a> {
    Literal(&'a str),
    Argument { key: &'a str, spec: &'a str },
}
pub(crate) struct Pieces<'a>(&'a str);
pub(crate) fn pieces(template: &str) -> Pieces<'_> {
    Pieces(template)
}
impl<'a> Iterator for Pieces<'a> {
    type Item = Result<Piece<'a>, RenderError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.0.is_empty() {
            return None;
        }
        let text = self.0;
        let brace = text.find(['{', '}']).unwrap_or(text.len());
        if brace > 0 {
            self.0 = &text[brace..];
            return Some(Ok(Piece::Literal(&text[..brace])));
        }
        if text.starts_with("{{") || text.starts_with("}}") {
            self.0 = &text[2..];
            return Some(Ok(Piece::Literal(&text[..1])));
        }
        // On any error, consume the tail so iteration cannot repeat the error.
        self.0 = "";
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
                    self.0 = &text[at + 2..];
                    let (key, spec) = field.split_once(':').unwrap_or((field, ""));
                    return Some(Ok(Piece::Argument { key, spec }));
                }
            }
        }
        Some(Err(RenderError::InvalidTemplate))
    }
}
