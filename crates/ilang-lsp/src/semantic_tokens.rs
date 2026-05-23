//! `textDocument/semanticTokens/full` provider.
//!
//! Classifies identifiers from the live buffer into LSP semantic
//! token types (class / function / method / parameter / ...).
//! Keywords, operators, numbers, and strings stay with the
//! TextMate grammar — semantic tokens layer on top to disambiguate
//! identifier uses the grammar can't tell apart by syntax alone
//! (e.g. `foo` is a function call vs `foo` is a local variable).

use std::collections::HashMap;

use ilang_lexer::{tokenize, TokenKind};
use tower_lsp::lsp_types::{SemanticToken, SemanticTokenModifier, SemanticTokenType};

use crate::types::Doc;
use crate::RefEntry;

/// Token type list, ordered so the index matches the `u32` we send
/// over the wire. The LSP `semanticTokensProvider.legend.tokenTypes`
/// capability advertises this same order.
pub(crate) const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::CLASS,
    SemanticTokenType::INTERFACE,
    SemanticTokenType::ENUM,
    SemanticTokenType::ENUM_MEMBER,
    SemanticTokenType::STRUCT,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::METHOD,
    SemanticTokenType::PROPERTY,
    SemanticTokenType::PARAMETER,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::NAMESPACE,
    SemanticTokenType::TYPE,
];

pub(crate) const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,
    SemanticTokenModifier::STATIC,
    SemanticTokenModifier::READONLY,
];

const TY_CLASS: u32 = 0;
const TY_INTERFACE: u32 = 1;
const TY_ENUM: u32 = 2;
const TY_ENUM_MEMBER: u32 = 3;
const TY_STRUCT: u32 = 4;
const TY_FUNCTION: u32 = 5;
const TY_METHOD: u32 = 6;
const TY_PROPERTY: u32 = 7;
const TY_PARAMETER: u32 = 8;
const TY_VARIABLE: u32 = 9;
const TY_NAMESPACE: u32 = 10;
#[allow(dead_code)]
const TY_TYPE: u32 = 11;

const MOD_DECLARATION: u32 = 1 << 0;
const MOD_STATIC: u32 = 1 << 1;
const MOD_READONLY: u32 = 1 << 2;

/// Map a `RefEntry.signature` prefix to (token_type, modifier_bits).
/// Returns `None` for signatures we don't classify (e.g. `this:` —
/// the `this` keyword is handled by the TextMate grammar).
fn classify_signature(sig: &str) -> Option<(u32, u32)> {
    // Order matters: longer / more specific prefixes first so
    // `(static method)` doesn't get caught by `(method)`.
    let table: &[(&str, u32, u32)] = &[
        ("(static method)", TY_METHOD, MOD_STATIC),
        ("(static getter)", TY_PROPERTY, MOD_STATIC),
        ("(static setter)", TY_PROPERTY, MOD_STATIC),
        ("(static const)", TY_PROPERTY, MOD_STATIC | MOD_READONLY),
        ("(static property)", TY_PROPERTY, MOD_STATIC),
        ("(method)", TY_METHOD, 0),
        ("(getter)", TY_PROPERTY, 0),
        ("(setter)", TY_PROPERTY, 0),
        ("(property)", TY_PROPERTY, 0),
        ("(variant)", TY_ENUM_MEMBER, 0),
        ("(parameter)", TY_PARAMETER, 0),
        ("(for-binding)", TY_VARIABLE, 0),
        ("(pattern)", TY_VARIABLE, 0),
        ("(module)", TY_NAMESPACE, 0),
        ("(import)", TY_VARIABLE, 0),
        ("class ", TY_CLASS, 0),
        ("struct ", TY_STRUCT, 0),
        ("union ", TY_STRUCT, 0),
        ("interface ", TY_INTERFACE, 0),
        ("enum ", TY_ENUM, 0),
        ("fn ", TY_FUNCTION, 0),
        ("@objc fn ", TY_FUNCTION, 0),
        ("@extern fn ", TY_FUNCTION, 0),
        ("let ", TY_VARIABLE, 0),
        ("const ", TY_VARIABLE, MOD_READONLY),
    ];
    for (prefix, ty, modifiers) in table.iter() {
        if sig.starts_with(prefix) {
            return Some((*ty, *modifiers));
        }
    }
    None
}

/// One classified token before encoding to LSP wire form. Sorted by
/// (line, col) and deduplicated by exact (line, col, length) before
/// `to_delta_encoded` runs.
#[derive(Clone, Copy)]
struct ClassifiedToken {
    line:      u32, // 0-based
    col:       u32, // 0-based
    length:    u32,
    ty:        u32,
    modifiers: u32,
}

/// Build the full-document token list for `doc.text`.
pub(crate) fn build_tokens(doc: &Doc) -> Vec<SemanticToken> {
    let text = &doc.text;
    let Ok(tokens) = tokenize(text) else {
        return Vec::new();
    };

    // Index refs by (1-based line, 1-based start_col) for O(1)
    // lookup. Multiple refs at the same position pick the first
    // — they'd carry the same target identity by construction.
    let mut by_pos: HashMap<(u32, u32), &RefEntry> = HashMap::new();
    for r in &doc.refs {
        by_pos.entry((r.line, r.start_col)).or_insert(r);
    }

    let mut out: Vec<ClassifiedToken> = Vec::new();

    for tok in tokens {
        // Keyword tokens (`none`, `true`, `class`, …) can stand in
        // for identifiers — most commonly as enum variant names or
        // after a `.`. Pick up their source spelling via
        // `keyword_str` so the position lookup below still finds
        // the matching RefEntry. Pure Ident tokens take the same
        // code path.
        let (name_borrowed, name_owned): (Option<&str>, Option<String>) = match &tok.kind {
            TokenKind::Ident(n) => (Some(n.as_str()), None),
            other => match other.keyword_str() {
                Some(s) => (None, Some(s.to_string())),
                None => continue,
            },
        };
        let name: &str = name_borrowed.unwrap_or_else(|| name_owned.as_deref().unwrap());
        let line1 = tok.span.line;
        let col1 = tok.span.col;
        let len = name.len() as u32;

        let (ty, modifiers) = if let Some(r) = by_pos.get(&(line1, col1)) {
            // `this`-style entries — leave to the TextMate grammar.
            if r.signature.starts_with("this:") {
                continue;
            }
            match classify_signature(&r.signature) {
                Some(x) => x,
                None => continue,
            }
        } else if let Some(sym) = doc
            .symbols
            .get(&ilang_ast::Symbol::intern(name))
        {
            // Bare name lookup catches decl sites that don't have a
            // self-RefEntry (the most common case for non-recursive
            // fns / classes). Mark `declaration` so the editor can
            // theme decls distinctly from uses if desired.
            let Some((t, m)) = classify_signature(&sym.signature) else {
                continue;
            };
            (t, m | MOD_DECLARATION)
        } else if doc
            .classes
            .contains_key(&ilang_ast::Symbol::intern(name))
        {
            (TY_CLASS, 0)
        } else {
            continue;
        };

        out.push(ClassifiedToken {
            line: line1.saturating_sub(1),
            col: col1.saturating_sub(1),
            length: len,
            ty,
            modifiers,
        });
    }

    out.sort_by(|a, b| (a.line, a.col).cmp(&(b.line, b.col)));
    out.dedup_by(|a, b| a.line == b.line && a.col == b.col && a.length == b.length);

    to_delta_encoded(&out)
}

/// Encode the absolute (line, col) tokens as the LSP delta wire
/// format: each row is `[deltaLine, deltaStart, length, type,
/// modifiers]`, where `deltaStart` is relative to the previous
/// token on the same line (and to column 0 when `deltaLine > 0`).
fn to_delta_encoded(tokens: &[ClassifiedToken]) -> Vec<SemanticToken> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut prev_line: u32 = 0;
    let mut prev_col: u32 = 0;
    for t in tokens {
        let delta_line = t.line - prev_line;
        let delta_start = if delta_line == 0 {
            t.col - prev_col
        } else {
            t.col
        };
        out.push(SemanticToken {
            delta_line,
            delta_start,
            length: t.length,
            token_type: t.ty,
            token_modifiers_bitset: t.modifiers,
        });
        prev_line = t.line;
        prev_col = t.col;
    }
    out
}
