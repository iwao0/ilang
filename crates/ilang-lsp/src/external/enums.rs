//! Enum-variant registration helpers — used during external module
//! harvest to record each variant's `(variant) EnumName.VariantName`
//! signature (plus source location for F12) into the cross-doc
//! `external_signatures` / `external_sources` maps.

use std::collections::HashMap;
use std::path::Path;

use ilang_ast::{Span, Symbol as AstSymbol};

use crate::text;
use crate::ExternalSources;
use super::ExternalLoc;

pub(crate) fn register_enum_variants(
    e: &ilang_ast::EnumDecl,
    enum_key: &str,
    out: &mut HashMap<AstSymbol, String>,
    src: Option<&str>,
) {
    let mut auto: i64 = 0;
    for v in e.variants.iter() {
        // Hover blurb for one variant. The displayed value is either
        // the integer discriminant (auto-numbered or explicit) or
        // the literal string for `: string`-repr enums.
        let val_int: Option<i64> = match &v.discriminant {
            Some(ilang_ast::DiscriminantLit::Int(d)) => {
                auto = d + 1;
                Some(*d)
            }
            Some(ilang_ast::DiscriminantLit::Str(_)) => None,
            None => {
                let cur = auto;
                auto += 1;
                Some(cur)
            }
        };
        let key = format!("{enum_key}.{}", v.name);
        // Prefer the literal text the user wrote (`0x40000000` rather
        // than `1073741824`, or `"some string"` rather than the auto
        // value) when source is available and the variant has an
        // explicit discriminant. Fall back to the integer form.
        let val_text: String = match (src, &v.discriminant) {
            (Some(s), Some(_)) => discriminant_literal_text(s, v.span)
                .unwrap_or_else(|| val_int.map(|n| n.to_string()).unwrap_or_default()),
            _ => val_int.map(|n| n.to_string()).unwrap_or_default(),
        };
        let sig = match &v.payload {
            ilang_ast::VariantPayload::Unit => {
                format!("(variant) {enum_key}.{} = {val_text}", v.name)
            }
            ilang_ast::VariantPayload::Tuple(_) => {
                format!("(variant) {enum_key}.{}(...)", v.name)
            }
            ilang_ast::VariantPayload::Struct(_) => {
                format!("(variant) {enum_key}.{} {{ ... }}", v.name)
            }
        };
        out.insert(AstSymbol::intern(&key), sig);
    }
}

/// Read the literal token for an enum variant's `= value` from
/// source — preserves hex / binary / underscore-separated forms
/// the parser collapses into an `i64`. Returns `None` when no
/// `= literal` is found at the variant's span.
pub(crate) fn discriminant_literal_text(src: &str, v_span: Span) -> Option<String> {
    let off = text::line_col_to_offset(src, v_span.line, v_span.col)?;
    let bytes = src.as_bytes();
    let mut i = off;
    // Skip the variant identifier itself.
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
    {
        i += 1;
    }
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'=' {
        return None;
    }
    i += 1;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    // String discriminant: `= "literal"` for `: string`-repr
    // enums. Capture the entire quoted span (including the
    // surrounding quotes) so hover shows `= "SDL_AUDIO"`
    // verbatim.
    if i < bytes.len() && bytes[i] == b'"' {
        let start = i;
        i += 1;
        while i < bytes.len() && bytes[i] != b'"' {
            if bytes[i] == b'\\' && i + 1 < bytes.len() {
                i += 2;
            } else {
                i += 1;
            }
        }
        if i < bytes.len() && bytes[i] == b'"' {
            i += 1;
            return std::str::from_utf8(&bytes[start..i]).ok().map(|s| s.to_string());
        }
        return None;
    }
    let start = i;
    if i < bytes.len() && bytes[i] == b'-' {
        i += 1;
    }
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
    {
        i += 1;
    }
    if i > start {
        std::str::from_utf8(&bytes[start..i]).ok().map(|s| s.to_string())
    } else {
        None
    }
}

/// Same as `register_enum_variants`, but also records each variant's
/// source location in `sources` (so F12 jumps to the variant line).
pub(crate) fn register_enum_variants_with_sources(
    e: &ilang_ast::EnumDecl,
    enum_key: &str,
    out: &mut HashMap<AstSymbol, String>,
    sources: &mut ExternalSources,
    module_path: &Path,
    src: &str,
) {
    register_enum_variants(e, enum_key, out, Some(src));
    for v in e.variants.iter() {
        let key = AstSymbol::intern(&format!("{enum_key}.{}", v.name));
        sources.insert(
            key,
            ExternalLoc {
                path: module_path.to_path_buf(),
                span: v.span,
                name_len: v.name.as_str().len() as u32,
            },
        );
    }
}

/// Inject the type-checker-only built-ins (`Result<T, E>`, `Map<K, V>`,
/// `Promise<T>`, `ObjCBlock<F>`) into the external signatures table so
/// hover / completion on the bare type name (and `Result.ok` / `.err`)
/// works without the user importing or declaring anything. Mirrors what
/// `register_enum_variants` would produce for a user-written enum.
pub(crate) fn register_builtin_enums(out: &mut HashMap<AstSymbol, String>) {
    for (name, sig) in [
        ("Result", "enum Result<T, E>"),
        ("Map", "class Map<K, V>"),
        ("Promise", "class Promise<T>"),
        ("ObjCBlock", "class ObjCBlock<F>"),
    ] {
        out.entry(AstSymbol::intern(name))
            .or_insert_with(|| sig.to_string());
    }
    out.entry(AstSymbol::intern("Result.ok"))
        .or_insert_with(|| "(variant) Result.ok(...)".to_string());
    out.entry(AstSymbol::intern("Result.err"))
        .or_insert_with(|| "(variant) Result.err(...)".to_string());
}

#[cfg(test)]
mod tests {
    use super::discriminant_literal_text;
    use ilang_ast::{Item, Span};
    use ilang_lexer::tokenize;
    use ilang_parser::parse;

    fn span_of_first_variant(src: &str) -> Span {
        let toks = tokenize(src).expect("lex");
        let prog = parse(&toks).expect("parse");
        for it in &prog.items {
            if let Item::Enum(e) = it {
                return e.variants[0].span;
            }
        }
        panic!("no enum");
    }

    #[test]
    fn integer_literal() {
        let src = "enum X: i32 { foo = 0x10 }";
        let span = span_of_first_variant(src);
        assert_eq!(discriminant_literal_text(src, span).unwrap(), "0x10");
    }

    #[test]
    fn integer_underscore_separator() {
        let src = "enum X: i64 { foo = 1_000 }";
        let span = span_of_first_variant(src);
        assert_eq!(discriminant_literal_text(src, span).unwrap(), "1_000");
    }

    #[test]
    fn negative_integer() {
        let src = "enum X: i32 { foo = -1 }";
        let span = span_of_first_variant(src);
        assert_eq!(discriminant_literal_text(src, span).unwrap(), "-1");
    }

    #[test]
    fn string_literal() {
        let src = "enum X: string { foo = \"SDL_HINT_AUDIO\" }";
        let span = span_of_first_variant(src);
        assert_eq!(
            discriminant_literal_text(src, span).unwrap(),
            "\"SDL_HINT_AUDIO\""
        );
    }

    #[test]
    fn string_literal_with_long_alignment_spaces() {
        let src = "enum X: string {\n    audioResamplingMode               = \"SDL_AUDIO_RESAMPLING_MODE\"\n}\n";
        let span = span_of_first_variant(src);
        assert_eq!(
            discriminant_literal_text(src, span).unwrap(),
            "\"SDL_AUDIO_RESAMPLING_MODE\""
        );
    }

    #[test]
    fn no_explicit_discriminant() {
        let src = "enum X { foo, bar }";
        let span = span_of_first_variant(src);
        assert_eq!(discriminant_literal_text(src, span), None);
    }
}
