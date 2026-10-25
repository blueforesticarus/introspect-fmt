//! Syntax-directed prototype, leaving Rust's built-in formatter in charge of
//! validating/rendering only when doing so cannot alter argument capture.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use std::collections::{HashMap, HashSet};
use syn::{
    ext::IdentExt,
    parenthesized,
    parse::{Parse, ParseStream, Parser},
    parse_macro_input, Expr, ExprClosure, ExprLit, Ident, Lit, LitStr, Result, Token,
};

enum InputArg {
    Positional(Expr),
    Named(Ident, Expr),
}
struct Invocation {
    runtime: TokenStream2,
    callback: ExprClosure,
    format: Expr,
    args: Vec<InputArg>,
}
impl Parse for Invocation {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        // The public wrapper supplies ($crate). Keep these hygienic tokens,
        // rather than printing/reparsing a path or consulting Cargo manifests.
        let runtime_group;
        parenthesized!(runtime_group in input);
        let runtime = runtime_group.parse()?;
        let callback = input.parse::<ExprClosure>()?;
        input.parse::<Token![,]>()?;
        let format = input.parse::<Expr>()?;
        let mut args = Vec::new();
        while !input.is_empty() {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            // Rust's formatting grammar allows keyword argument names too.
            // Parse the label before interpreting any token as an expression.
            // A positional comparison such as `true == false` also starts
            // with identifier-like tokens; only a single '=' introduces a name.
            if input.peek(Ident::peek_any)
                && input.peek2(Token![=])
                && !input.peek2(Token![==])
                && !input.peek2(Token![=>])
            {
                let name = input.call(Ident::parse_any)?;
                input.parse::<Token![=]>()?;
                args.push(InputArg::Named(name, input.parse()?));
            } else {
                args.push(InputArg::Positional(input.parse()?));
            }
        }
        Ok(Self {
            runtime,
            callback,
            format,
            args,
        })
    }
}

// Accumulate requirements per argument, rather than per placeholder, so a
// repeated argument can retain both hooks without another evaluation.
#[derive(Hash, PartialEq, Eq)]
enum ArgumentKey {
    Position(usize),
    Name(String),
}
#[derive(Default)]
struct RequiredTraits {
    display: bool,
    debug: bool,
}

// Parse trait selection and count operands together, after skipping fill and
// flags. Searching for '$' or '.*' in raw text confuses escapes, fill, and names.
struct SpecInfo {
    traits: RequiredTraits,
    counts: Vec<ArgumentKey>,
    star_precision: bool,
}
fn analyze_spec(spec: &str) -> SpecInfo {
    let mut info = SpecInfo {
        traits: RequiredTraits::default(),
        counts: Vec::new(),
        star_precision: false,
    };
    let mut spec = spec.trim_end();
    let mut chars = spec.char_indices();
    let first = chars.next();
    let second = chars.next();
    if second.is_some_and(|(_, ch)| matches!(ch, '<' | '^' | '>')) {
        let (offset, ch) = second.unwrap();
        spec = &spec[offset + ch.len_utf8()..];
    } else if first.is_some_and(|(_, ch)| matches!(ch, '<' | '^' | '>')) {
        spec = &spec[1..];
    }
    if spec.starts_with(['+', '-']) {
        spec = &spec[1..];
    }
    if spec.starts_with('#') {
        spec = &spec[1..];
    }
    // `0$` is argument zero, while `0width$` starts with the padding flag.
    if spec.starts_with('0') && !spec.starts_with("0$") {
        spec = &spec[1..];
    }
    fn skip_count<'a>(spec: &'a str, counts: &mut Vec<ArgumentKey>) -> &'a str {
        if let Some((count, rest)) = spec.split_once('$') {
            let key = if let Ok(index) = count.parse() {
                Some(ArgumentKey::Position(index))
            } else if valid_ident(count) {
                Some(ArgumentKey::Name(count.to_owned()))
            } else {
                None
            };
            if let Some(key) = key {
                counts.push(key);
                return rest;
            }
        }
        spec.trim_start_matches(|ch: char| ch.is_ascii_digit())
    }
    spec = skip_count(spec, &mut info.counts);
    if let Some(precision) = spec.strip_prefix('.') {
        if let Some(rest) = precision.strip_prefix('*') {
            info.star_precision = true;
            spec = rest;
        } else {
            spec = skip_count(precision, &mut info.counts);
        }
    }
    info.traits = RequiredTraits {
        display: spec.is_empty(),
        debug: matches!(spec, "?" | "x?" | "X?"),
    };
    info
}

// Best-effort literal scanner: track captures, count operands, and required
// traits. Rust's builtin remains responsible for full format validation.
struct FormatInfo {
    implicit_names: Vec<String>,
    count_names: HashSet<String>,
    count_positions: HashSet<usize>,
    traits: HashMap<ArgumentKey, RequiredTraits>,
}
fn analyze_format(fmt: &str) -> FormatInfo {
    let mut info = FormatInfo {
        implicit_names: Vec::new(),
        count_names: HashSet::new(),
        count_positions: HashSet::new(),
        traits: HashMap::new(),
    };
    let mut seen = HashSet::new();
    let mut chars = fmt.char_indices().peekable();
    let mut next_positional = 0usize;
    while let Some((_, ch)) = chars.next() {
        if ch != '{' {
            continue;
        }
        if chars.peek().map(|(_, c)| *c) == Some('{') {
            chars.next();
            continue;
        }
        let mut inside = String::new();
        let mut done = false;
        for (_, c) in chars.by_ref() {
            if c == '}' {
                done = true;
                break;
            }
            inside.push(c);
        }
        if !done {
            break;
        }
        let (head, spec) = inside.split_once(':').unwrap_or((&inside, ""));
        let head = head.trim();
        if valid_ident(head) && seen.insert(head.to_owned()) {
            info.implicit_names.push(head.to_owned());
        }
        // A `.*` precision consumes the next positional *before* an implicit
        // value placeholder consumes its position.
        let spec = analyze_spec(spec);
        if spec.star_precision {
            info.count_positions.insert(next_positional);
            next_positional += 1;
        }
        let key = if head.is_empty() {
            let index = next_positional;
            next_positional += 1;
            Some(ArgumentKey::Position(index))
        } else if let Ok(index) = head.parse() {
            Some(ArgumentKey::Position(index))
        } else if valid_ident(head) {
            Some(ArgumentKey::Name(head.to_owned()))
        } else {
            None
        };
        if let Some(key) = key {
            let entry = info.traits.entry(key).or_default();
            entry.display |= spec.traits.display;
            entry.debug |= spec.traits.debug;
        }

        // Dynamic counts are exact usize operands. Identifiers are kept whole,
        // including Unicode, and a leading zero padding flag is already gone.
        for count in spec.counts {
            match count {
                ArgumentKey::Position(index) => {
                    info.count_positions.insert(index);
                }
                ArgumentKey::Name(name) => {
                    info.count_names.insert(name.clone());
                    if seen.insert(name.clone()) {
                        info.implicit_names.push(name);
                    }
                }
            }
        }
    }
    info
}

// Format names are IDENTIFIER_OR_KEYWORD, except bare `_`. Use the same
// parser for invocation labels, placeholder names, and dynamic count names.
fn parse_format_ident(name: &str) -> Option<Ident> {
    if name.is_empty() || name == "_" {
        return None;
    }
    Ident::parse_any.parse_str(name).ok()
}
fn valid_ident(name: &str) -> bool {
    parse_format_ident(name).is_some()
}
fn argument_name(ident: &Ident) -> String {
    // `type = ...` and `r#type = ...` both bind the format name "type".
    ident.unraw().to_string()
}

fn captured_value(
    runtime: &TokenStream2,
    temp: &Ident,
    required: Option<&RequiredTraits>,
) -> TokenStream2 {
    let display = required
        .filter(|traits| traits.display)
        .map(|_| quote! { .display() });
    let debug = required
        .filter(|traits| traits.debug)
        .map(|_| quote! { .debug() });
    quote! {
        #runtime::__private::Formatters::new(&#temp)
            #display #debug
            .attach(#runtime::__private::Probe::new(#temp).__introspect_fmt_capture())
    }
}

#[proc_macro]
pub fn inspect_format(tokens: TokenStream) -> TokenStream {
    let Invocation {
        runtime,
        callback,
        format,
        args,
    } = parse_macro_input!(tokens as Invocation);
    let (literal, capture_span) = match &format {
        Expr::Lit(ExprLit {
            lit: Lit::Str(s), ..
        }) => (Some(s.value()), s.span()),
        _ => (None, Span::call_site()),
    };
    let info = literal.as_ref().map(|text| analyze_format(text));
    let mut bound = Vec::new();
    let mut descriptors = Vec::new();
    let mut fmt_args = Vec::new();
    let mut explicitly_named = HashSet::new();

    for (i, arg) in args.iter().enumerate() {
        let temp = format_ident!("__introspect_fmt_arg_{}", i, span = Span::mixed_site());
        let (name, expr) = match arg {
            InputArg::Positional(expr) => (None, expr),
            InputArg::Named(ident, expr) => {
                explicitly_named.insert(argument_name(ident));
                (Some(ident), expr)
            }
        };
        // Borrow the expression exactly once. In particular, we do not MOVE
        // non-Copy values, and the same (&mut T) is not constructed twice.
        // Counts are required to be usize by Rust's formatting language.
        // Named explicit arguments also have positional indices in Rust.
        // Union both spellings when the same value is referenced either way.
        let mut required = RequiredTraits::default();
        let mut is_count = false;
        if let Some(info) = &info {
            let positional = info.traits.get(&ArgumentKey::Position(i));
            let named =
                name.and_then(|name| info.traits.get(&ArgumentKey::Name(argument_name(name))));
            for traits in positional.into_iter().chain(named) {
                required.display |= traits.display;
                required.debug |= traits.debug;
            }
            is_count = info.count_positions.contains(&i)
                || name.is_some_and(|name| info.count_names.contains(&argument_name(name)));
        }
        let captured = captured_value(&runtime, &temp, Some(&required));
        if is_count {
            bound.push(quote! { let #temp: &usize = &(#expr); });
        } else {
            bound.push(quote! { let #temp = &(#expr); });
        }
        let text = LitStr::new(&quote!(#expr).to_string(), Span::call_site());
        let label = match name {
            Some(name) => {
                let label = LitStr::new(&argument_name(name), Span::call_site());
                quote! { Some(#label) }
            }
            None => quote! { None },
        };
        descriptors.push(quote! {
            #runtime::FormatArg::new(
                #label, #text,
                #runtime::__private::rust_type_name(#temp),
                #captured
            )
        });
        // Counts stay usize operands; values use explicit trait forwarding to
        // preserve their original impls, pointer addresses, and DST support.
        let fmt_value = if is_count {
            quote! { *#temp }
        } else {
            quote! { #runtime::__private::Render::new(#temp) }
        };
        fmt_args.push(match name {
            Some(name) => quote! { #name = #fmt_value },
            None => fmt_value,
        });
    }

    // Turn implicit captures into explicit named formatting operands. This
    // reuses the captured borrow and avoids relying on implicit macro hygiene.
    if let Some(ref info) = info {
        for (i, name) in info.implicit_names.iter().enumerate() {
            if explicitly_named.contains(name) {
                continue;
            }
            let Some(mut ident) = parse_format_ident(name) else {
                continue;
            };
            // A captured keyword variable is spelled r#keyword in Rust code;
            // receiver/path keywords cannot be raw and retain their spelling.
            if syn::parse_str::<Ident>(name).is_err()
                && !matches!(name.as_str(), "self" | "Self" | "crate" | "super")
            {
                ident = Ident::new_raw(name, capture_span);
            }
            // The proc-macro call site is inside the wrapper. The literal's
            // original span instead resolves captures in the user's scope.
            ident.set_span(capture_span);
            let temp = format_ident!("__introspect_fmt_implicit_{}", i, span = Span::mixed_site());
            if info.count_names.contains(name) {
                bound.push(quote! { let #temp: &usize = &#ident; });
            } else {
                bound.push(quote! { let #temp = &#ident; });
            }
            let fmt_value = if info.count_names.contains(name) {
                quote! { *#temp }
            } else {
                quote! { #runtime::__private::Render::new(#temp) }
            };
            fmt_args.push(quote! { #ident = #fmt_value });
            let label = LitStr::new(name, Span::call_site());
            let captured = captured_value(
                &runtime,
                &temp,
                info.traits.get(&ArgumentKey::Name(name.clone())),
            );
            descriptors.push(quote! {
                #runtime::FormatArg::new(
                    Some(#label), #label,
                    #runtime::__private::rust_type_name(#temp),
                    #captured
                )
            });
        }
    }

    // Only a direct literal can be analyzed before expansion. Dollar signs,
    // escaped braces, implicit captures, and dynamic counts are all handled by
    // Rust's builtin now that operands retain their original types.
    let rendered = if literal.is_some() {
        let args = fmt_args;
        quote! { Some(::std::format_args!(#format #(, #args)*)) }
    } else {
        quote! { None }
    };

    let expanded = quote! {{
        use #runtime::__private::Capture as _;
        // Nonliteral templates are evaluated here exactly once; only inert
        // direct literals are also passed to the compiler's format_args!.
        let __introspect_fmt_template: &'static str = #format;
        #(#bound)*
        let __introspect_fmt_values = [#(#descriptors),*];
        #runtime::__private::invoke(#runtime::Format::new(
            __introspect_fmt_template,
            &__introspect_fmt_values,
            #rendered,
        ), #callback)
    }};
    expanded.into()
}
