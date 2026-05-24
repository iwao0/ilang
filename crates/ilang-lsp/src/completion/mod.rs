//! `CompletionItem` builders — keyword sets, attribute completions,
//! global / type / FFI-helper completions. Pure builders: each
//! returns the items to surface for a given context decision made
//! by `completion::context` (positional probes) and orchestrated
//! by `completion::handler` (`handle_completion`).
//!
//! `call_snippet` / `trigger_sig_help_command` were originally per-
//! item snippet hooks; both are no-op today, kept for the (likely)
//! future signature-help integration.

use tower_lsp::lsp_types::{
    Command, CompletionItem, CompletionItemKind, InsertTextFormat,
};

use super::builtins::ffi_helper_signature;
use super::helpers::sig_body_skip_attrs;

mod context;
mod globals;
mod handler;
mod types;
pub(crate) use context::{
    at_attribute_position, at_type_position, brace_depth_at, enclosing_class,
    enclosing_use_module, in_extern_c_block, in_extern_objc_block, literal_token_at,
    preceding_kw_introduces_binder,
};
pub(crate) use globals::global_completions;
pub(crate) use handler::{handle_completion, resolve_receiver_class};
pub(crate) use types::type_completions;

pub(super) const PRIMITIVE_TYPES: &[&str] = &[
    "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64", "bool", "string",
];

/// Built-in generic types the type checker pre-registers but no source
/// file declares. Surfaced as type-position completions so `let a: M`
/// suggests `Map`. The last field is the generic-argument count —
/// drives the snippet insertion (`Result<$1, $2>`) so accepting the
/// completion drops the cursor straight into the first slot and
/// fires the `<...>` signature-help overlay.
pub(super) const BUILTIN_GENERIC_TYPES: &[(&str, &str, CompletionItemKind, usize)] = &[
    ("Map", "class Map<K, V>", CompletionItemKind::CLASS, 2),
    ("Promise", "class Promise<T>", CompletionItemKind::CLASS, 1),
    ("Result", "enum Result<T, E>", CompletionItemKind::ENUM, 2),
    ("ObjCBlock", "class ObjCBlock<F>", CompletionItemKind::CLASS, 1),
];

/// Both default to no-op today, retained as a placeholder for future
/// per-item snippet logic (auto-call wrap for fn names, opting out
/// when referring to a method without calling it, etc.).
pub(crate) fn call_snippet(
    _name: &str,
    _kind: CompletionItemKind,
) -> (Option<String>, Option<InsertTextFormat>) {
    (None, None)
}

pub(crate) fn trigger_sig_help_command(_kind: CompletionItemKind) -> Option<Command> {
    None
}

/// Map a buffer-side signature string (`class Foo`, `enum Bar`,
/// `@flags\nenum Bar`, `(module) m`, `interface I`, …) to the
/// LSP `CompletionItemKind` the editor should render. Used by
/// every completion path that turns a stored signature into a
/// completion item so the kind classifier stays in one place.
/// Unknown / function-shaped signatures fall through to FUNCTION.
pub(crate) fn classify_signature_kind(sig: &str) -> CompletionItemKind {
    let body = sig_body_skip_attrs(sig);
    if body.starts_with("class ")
        || body.starts_with("struct ")
        || body.starts_with("union ")
    {
        CompletionItemKind::CLASS
    } else if body.starts_with("enum ") {
        CompletionItemKind::ENUM
    } else if body.starts_with("interface ") || body.starts_with("@objc interface ") {
        CompletionItemKind::INTERFACE
    } else if body.starts_with("const ") {
        CompletionItemKind::CONSTANT
    } else if body.starts_with("(module) ") {
        CompletionItemKind::MODULE
    } else {
        CompletionItemKind::FUNCTION
    }
}

/// `true` when `kind` represents a type-ish entry (class / enum /
/// struct / union / interface) — used to filter the type-position
/// completion list to types only.
pub(crate) fn kind_is_type(kind: CompletionItemKind) -> bool {
    matches!(
        kind,
        CompletionItemKind::CLASS
            | CompletionItemKind::ENUM
            | CompletionItemKind::INTERFACE
            | CompletionItemKind::STRUCT
    )
}

/// Item-introducer keywords that are valid inside `@extern(C) { }`.
/// `static` is already covered by the generic block-scope keyword
/// list, so it's omitted here to avoid duplicates.
pub(crate) fn push_extern_c_keywords(out: &mut Vec<CompletionItem>) {
    for kw in ["fn", "struct", "union", "class"] {
        out.push(CompletionItem {
            label: kw.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            ..CompletionItem::default()
        });
    }
}

pub(crate) fn push_ffi_helper_completions(out: &mut Vec<CompletionItem>) {
    for name in [
        "stringFromCstr",
        "cstrFromString",
        "freeCstr",
        "bytesFromBuffer",
        "readI8",
        "readI16",
        "readI32",
        "readI64",
        "readU8",
        "readU16",
        "readU32",
        "readU64",
        "readF32",
        "readF64",
        "writeI8",
        "writeI16",
        "writeI32",
        "writeI64",
        "writeU8",
        "writeU16",
        "writeU32",
        "writeU64",
        "writeF32",
        "writeF64",
        "fnAddr",
        "arrayFromCArray",
        "cstrArrayToStrings",
        "errnoCheck",
        "errnoCheckI64",
    ] {
        if let Some(sig) = ffi_helper_signature(name) {
            out.push(CompletionItem {
                label: name.to_string(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(sig.to_string()),
                ..CompletionItem::default()
            });
        }
    }
}

/// where the attribute typically takes arguments.
pub(crate) fn attribute_completions() -> Vec<CompletionItem> {
    let entries: &[(&str, Option<&str>, &str)] = &[
        ("extern", Some("extern(C)"), "@extern(C) { ... }"),
        ("lib", Some("lib(\"$1\")"), "@lib(\"libname\")"),
        ("optional", None, "@optional"),
        ("symbol", Some("symbol(\"$1\")"), "@symbol(\"name\")"),
        ("packed", None, "@packed"),
        ("bits", Some("bits($1)"), "@bits(N)"),
        ("flags", None, "@flags"),
        ("override", None, "@override"),
        ("requires", Some("requires($1)"), "@requires(cap)"),
        ("deprecated", Some("deprecated($1)"), "@deprecated(reason)"),
        ("since", Some("since(\"$1\")"), "@since(\"version\")"),
    ];
    entries
        .iter()
        .map(|(label, snippet, detail)| CompletionItem {
            label: (*label).to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some((*detail).to_string()),
            insert_text: snippet.map(|s| (*s).to_string()),
            insert_text_format: snippet.map(|_| InsertTextFormat::SNIPPET),
            ..CompletionItem::default()
        })
        .collect()
}

