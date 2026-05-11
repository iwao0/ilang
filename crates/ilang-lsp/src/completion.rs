//! Completion-side helpers — keyword sets, contextual probes
//! (`in_extern_c_block`, `at_attribute_position`, `at_type_position`,
//! `preceding_kw_introduces_binder`, `brace_depth_at`), and the
//! completion-list builders (`global_completions`, `type_completions`,
//! `attribute_completions`, `push_extern_c_keywords`,
//! `push_ffi_helper_completions`, `keyword_completions`).
//!
//! `call_snippet` / `trigger_sig_help_command` were originally per-
//! item snippet hooks; both are no-op today, kept for the (likely)
//! future signature-help integration.

use ilang_ast::Span;
use tower_lsp::lsp_types::{
    Command, CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, MarkupContent,
    MarkupKind,
};

use super::builtins::ffi_helper_signature;
use super::text;
use super::Doc;

/// Read the literal token at `span` from `src` — captures hex /
/// binary / octal prefixes, underscore separators, and any
/// integer / float type suffix. Returns `None` when the span
/// doesn't resolve to a contiguous identifier-like token.
pub(crate) fn literal_token_at(src: &str, span: Span) -> Option<String> {
    let off = text::line_col_to_offset(src, span.line, span.col)?;
    let bytes = src.as_bytes();
    let mut i = off;
    if i < bytes.len() && bytes[i] == b'-' {
        i += 1;
    }
    let start = i;
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'.')
    {
        i += 1;
    }
    if i > start {
        std::str::from_utf8(&bytes[off..i]).ok().map(|s| s.to_string())
    } else {
        None
    }
}

/// Function / method completion items insert just their bare name.
/// (We used to insert `name($0)` to trigger signature help, but that
/// mangled valid uses where the user wants the name alone — passing a
/// fn as a value, referring to a method without calling it, etc.)
pub(crate) fn call_snippet(
    _name: &str,
    _kind: CompletionItemKind,
) -> (Option<String>, Option<InsertTextFormat>) {
    (None, None)
}

pub(crate) fn trigger_sig_help_command(_kind: CompletionItemKind) -> Option<Command> {
    None
}

/// ilang keywords. Each entry tags whether the keyword may appear at
/// the file's top level, inside a block (fn / method body / class
/// body / etc.), or both. The completion fallback filters by the
/// receiver's current brace depth — coarse but enough to keep
/// `init` / `return` / `break` out of top-level suggestions and
/// `fn` / `class` / `use` out of block-internal ones.
const KEYWORDS: &[(&str, KwScope)] = &[
    // Item kw (top level) and class-body-only kw stay in their scope.
    ("fn", KwScope::TopLevel),
    ("class", KwScope::TopLevel),
    ("enum", KwScope::TopLevel),
    ("use", KwScope::TopLevel),
    ("extends", KwScope::TopLevel),
    ("override", KwScope::Block),
    ("init", KwScope::Block),
    ("deinit", KwScope::Block),
    ("static", KwScope::Block),
    ("get", KwScope::Block),
    ("set", KwScope::Block),
    // Stmt / expr keywords are valid in either context — top-level
    // script-style code is a thing in ilang.
    ("let", KwScope::Both),
    ("const", KwScope::Both),
    ("if", KwScope::Both),
    ("elif", KwScope::Both),
    ("else", KwScope::Both),
    ("while", KwScope::Both),
    ("loop", KwScope::Both),
    ("for", KwScope::Both),
    ("in", KwScope::Both),
    ("match", KwScope::Both),
    ("new", KwScope::Both),
    ("as", KwScope::Both),
    ("true", KwScope::Both),
    ("false", KwScope::Both),
    ("none", KwScope::Both),
    ("some", KwScope::Both),
    // Need a surrounding fn / loop / class — but distinguishing those
    // contexts requires more than brace depth, so keep them at Block.
    ("return", KwScope::Block),
    ("break", KwScope::Block),
    ("continue", KwScope::Block),
    ("this", KwScope::Block),
    ("super", KwScope::Block),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum KwScope {
    /// Only relevant at the file's top level (depth = 0).
    TopLevel,
    /// Only inside some `{ ... }` (depth > 0).
    Block,
    /// Allowed in both contexts.
    Both,
}

/// `true` when the cursor sits inside an `@extern(C) { ... }` block.
/// Walks back across balanced braces; the first unmatched `{` is the
/// enclosing block, and we check whether `@extern(C)` precedes it
/// (with optional whitespace).
pub(crate) fn in_extern_c_block(text: &str, offset: usize) -> bool {
    let bytes = text.as_bytes();
    let end = offset.min(bytes.len());
    let mut depth: i32 = 0;
    let mut i = end;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b'}' => depth += 1,
            b'{' => {
                if depth > 0 {
                    depth -= 1;
                } else if extern_c_precedes(bytes, i) {
                    return true;
                }
                // Either way, keep walking past this `{` to inspect
                // outer enclosing braces too.
            }
            _ => {}
        }
    }
    false
}

/// `true` if `@extern(C)` (with optional whitespace) appears
/// immediately before byte index `at`.
fn extern_c_precedes(bytes: &[u8], at: usize) -> bool {
    let mut j = at;
    while j > 0 && matches!(bytes[j - 1], b' ' | b'\t' | b'\r' | b'\n') {
        j -= 1;
    }
    if j == 0 || bytes[j - 1] != b')' {
        return false;
    }
    let mut k = j - 1;
    while k > 0 && matches!(bytes[k - 1], b' ' | b'\t') {
        k -= 1;
    }
    if k == 0 || bytes[k - 1] != b'C' {
        return false;
    }
    k -= 1;
    while k > 0 && matches!(bytes[k - 1], b' ' | b'\t') {
        k -= 1;
    }
    if k == 0 || bytes[k - 1] != b'(' {
        return false;
    }
    k -= 1;
    while k > 0 && matches!(bytes[k - 1], b' ' | b'\t') {
        k -= 1;
    }
    if k < 6 || &bytes[k - 6..k] != b"extern" {
        return false;
    }
    let kk = k - 6;
    kk >= 1 && bytes[kk - 1] == b'@'
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

/// `true` when the cursor sits in attribute syntax — i.e. an `@`
/// (followed by an in-progress identifier) is the previous non-ident
/// character on the line.
pub(crate) fn at_attribute_position(text: &str, offset: usize) -> bool {
    let bytes = text.as_bytes();
    let end = offset.min(bytes.len());
    let mut i = end;
    while i > 0 {
        let b = bytes[i - 1];
        if b.is_ascii_alphanumeric() || b == b'_' {
            i -= 1;
        } else {
            break;
        }
    }
    i > 0 && bytes[i - 1] == b'@'
}

/// ilang attributes for completion. `(args)` snippets are inserted
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

/// `true` when the cursor follows a `:` (with optional whitespace and
/// a partial ident underway). That's the type slot of `let x: T`,
/// `const x: T`, `fn f(x: T)`, `field: T` etc.
pub(crate) fn at_type_position(text: &str, offset: usize) -> bool {
    let bytes = text.as_bytes();
    let end = offset.min(bytes.len());
    let mut i = end;
    while i > 0 {
        let b = bytes[i - 1];
        if b.is_ascii_alphanumeric() || b == b'_' {
            i -= 1;
        } else {
            break;
        }
    }
    while i > 0 && matches!(bytes[i - 1], b' ' | b'\t') {
        i -= 1;
    }
    i > 0 && bytes[i - 1] == b':'
}

const PRIMITIVE_TYPES: &[&str] = &[
    "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64", "bool", "string",
];

/// Identifiers valid in a type position: primitive types, user classes
/// / enums (incl. structs / unions), and `module.X` names imported via
/// `use`.
pub(crate) fn type_completions(doc: &Doc) -> Vec<CompletionItem> {
    let mut out: Vec<CompletionItem> = Vec::new();
    for t in PRIMITIVE_TYPES {
        out.push(CompletionItem {
            label: (*t).to_string(),
            kind: Some(CompletionItemKind::TYPE_PARAMETER),
            ..CompletionItem::default()
        });
    }
    for (name, sym) in doc.symbols.iter() {
        let is_type = sym.signature.starts_with("class ")
            || sym.signature.starts_with("struct ")
            || sym.signature.starts_with("union ")
            || sym.signature.starts_with("enum ");
        if !is_type {
            continue;
        }
        out.push(CompletionItem {
            label: name.as_str().to_string(),
            kind: Some(CompletionItemKind::CLASS),
            detail: Some(sym.signature.clone()),
            ..CompletionItem::default()
        });
    }
    // Imported types brought in via `use module` show as
    // `module.TypeName`.
    for (name, sig) in doc.external_signatures.iter() {
        let is_type = sig.starts_with("class ")
            || sig.starts_with("struct ")
            || sig.starts_with("union ")
            || sig.starts_with("enum ");
        if !is_type {
            continue;
        }
        out.push(CompletionItem {
            label: name.as_str().to_string(),
            kind: Some(CompletionItemKind::CLASS),
            detail: Some(sig.clone()),
            ..CompletionItem::default()
        });
    }
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}

/// `true` when the cursor is right after a `let` / `const` keyword
/// (with optional whitespace and possibly a partial ident underway).
/// Used to suppress completion at the binder position — anything we
/// suggest there would shadow / overwrite the new name.
pub(crate) fn preceding_kw_introduces_binder(text: &str, offset: usize) -> bool {
    let bytes = text.as_bytes();
    let end = offset.min(bytes.len());
    // Skip the in-progress ident the user is typing.
    let mut i = end;
    while i > 0 {
        let b = bytes[i - 1];
        if b.is_ascii_alphanumeric() || b == b'_' {
            i -= 1;
        } else {
            break;
        }
    }
    while i > 0 && matches!(bytes[i - 1], b' ' | b'\t') {
        i -= 1;
    }
    for kw in ["let", "const"] {
        let n = kw.len();
        if i >= n && &bytes[i - n..i] == kw.as_bytes() {
            let prev = if i > n { Some(bytes[i - n - 1]) } else { None };
            let boundary = prev
                .map(|c| !c.is_ascii_alphanumeric() && c != b'_')
                .unwrap_or(true);
            if boundary {
                return true;
            }
        }
    }
    false
}

/// Brace depth of `text` at byte offset `offset`. Counts `{` and `}`
/// outside string / char / line / block comments. Used by completion
/// to filter keywords by context.
pub(crate) fn brace_depth_at(text: &str, offset: usize) -> i32 {
    let bytes = text.as_bytes();
    let end = offset.min(bytes.len());
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut in_line_comment = false;
    let mut block_depth: i32 = 0;
    let mut i = 0;
    while i < end {
        let b = bytes[i];
        if in_line_comment {
            if b == b'\n' {
                in_line_comment = false;
            }
            i += 1;
            continue;
        }
        if block_depth > 0 {
            if b == b'/' && i + 1 < end && bytes[i + 1] == b'*' {
                block_depth += 1;
                i += 2;
                continue;
            }
            if b == b'*' && i + 1 < end && bytes[i + 1] == b'/' {
                block_depth -= 1;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if in_string {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if b == b'/' && i + 1 < end {
            if bytes[i + 1] == b'/' {
                in_line_comment = true;
                i += 2;
                continue;
            }
            if bytes[i + 1] == b'*' {
                block_depth = 1;
                i += 2;
                continue;
            }
        }
        if b == b'"' {
            in_string = true;
        } else if b == b'{' {
            depth += 1;
        } else if b == b'}' {
            depth -= 1;
        }
        i += 1;
    }
    depth
}

/// Append keyword completions matching the cursor's brace context.
fn keyword_completions(at_top_level: bool, out: &mut Vec<CompletionItem>) {
    for (label, scope) in KEYWORDS {
        let allowed = match scope {
            KwScope::TopLevel => at_top_level,
            KwScope::Block => !at_top_level,
            KwScope::Both => true,
        };
        if allowed {
            out.push(CompletionItem {
                label: (*label).to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                ..CompletionItem::default()
            });
        }
    }
}

/// Top-level identifiers visible in `doc`, used as completion fallback
/// when the user is just typing a name (no receiver). Only the bare
/// names appear — `use module` namespaces show up as the module name
/// itself, not as `module.member` (those land in the `module.`
/// completion list).
pub(crate) fn global_completions(doc: &Doc, at_top_level: bool) -> Vec<CompletionItem> {
    let mut out: Vec<CompletionItem> = Vec::new();
    for (name, sym) in doc.symbols.iter() {
        let kind = if sym.signature.starts_with("class ")
            || sym.signature.starts_with("struct ")
            || sym.signature.starts_with("union ")
        {
            CompletionItemKind::CLASS
        } else if sym.signature.starts_with("enum ") {
            CompletionItemKind::ENUM
        } else if sym.signature.starts_with("const ") {
            CompletionItemKind::CONSTANT
        } else {
            CompletionItemKind::FUNCTION
        };
        let (insert_text, fmt) = call_snippet(name.as_str(), kind);
        let command = trigger_sig_help_command(kind);
        out.push(CompletionItem {
            label: name.as_str().to_string(),
            kind: Some(kind),
            detail: Some(sym.signature.clone()),
            documentation: sym.doc.clone().map(|d| {
                Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: d,
                })
            }),
            insert_text,
            insert_text_format: fmt,
            command,
            ..CompletionItem::default()
        });
    }
    // Local variables / params seen anywhere in the file. Last-write-
    // wins so the type info attached is approximate when names recur
    // across scopes.
    for (name, ty) in doc.var_types.iter() {
        if doc.symbols.contains_key(name) {
            continue;
        }
        out.push(CompletionItem {
            label: name.as_str().to_string(),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some(format!("{name}: {ty}")),
            ..CompletionItem::default()
        });
    }
    let mut modules: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for key in doc.external_signatures.keys() {
        if let Some((m, _)) = key.as_str().split_once('.') {
            modules.insert(m.to_string());
        }
    }
    for m in modules {
        out.push(CompletionItem {
            label: m.clone(),
            kind: Some(CompletionItemKind::MODULE),
            detail: Some(format!("(module) {m}")),
            ..CompletionItem::default()
        });
    }
    // Built-in singleton — always available.
    out.push(CompletionItem {
        label: "console".to_string(),
        kind: Some(CompletionItemKind::VARIABLE),
        detail: Some("(builtin) console: Console".to_string()),
        ..CompletionItem::default()
    });
    keyword_completions(at_top_level, &mut out);
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}
