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
    ("interface", KwScope::TopLevel),
    ("enum", KwScope::TopLevel),
    ("use", KwScope::TopLevel),
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
/// If `offset` sits inside the body of a `use M { ... }` selective
/// import (between the opening `{` and a matching `}` that hasn't yet
/// been typed), returns the imported module name. Used by completion
/// to swap the global candidate list for the target module's own
/// exports — typing `N` after `use cocoa {` should offer `NSObject`,
/// not the buffer-local fn names that `global_completions` would
/// surface.
pub(crate) fn enclosing_use_module(text: &str, offset: usize) -> Option<String> {
    let bytes = text.as_bytes();
    if offset > bytes.len() {
        return None;
    }
    // Scan backward to find an unmatched `{`. Bail on `}` (balanced
    // close) and on `;` / `\n\n` boundaries the parser would treat as
    // a hard statement break — those can't sit inside a use list.
    let mut depth = 0i32;
    let mut i = offset;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b'}' => depth += 1,
            b'{' => {
                if depth == 0 {
                    // Found the candidate opener. Look at what
                    // precedes it: skip whitespace, then an
                    // identifier (and optional `as _` alias / `as
                    // <name>`), then the `use` keyword.
                    let mut j = i;
                    while j > 0 && matches!(bytes[j - 1], b' ' | b'\t') {
                        j -= 1;
                    }
                    // Optional `as _` / `as <ident>`.
                    let mut after_alias = j;
                    if j >= 1 && (bytes[j - 1] == b'_' || bytes[j - 1].is_ascii_alphanumeric()) {
                        let alias_end = j;
                        let mut k = j;
                        while k > 0 && (bytes[k - 1].is_ascii_alphanumeric() || bytes[k - 1] == b'_')
                        {
                            k -= 1;
                        }
                        let alias = &bytes[k..alias_end];
                        // Need a preceding `as` token to treat this
                        // as the alias rather than the module ident.
                        let mut a = k;
                        while a > 0 && matches!(bytes[a - 1], b' ' | b'\t') {
                            a -= 1;
                        }
                        if a >= 2 && &bytes[a - 2..a] == b"as" {
                            let before_as = a - 2;
                            let prev_is_boundary = before_as == 0
                                || !(bytes[before_as - 1].is_ascii_alphanumeric()
                                    || bytes[before_as - 1] == b'_');
                            if prev_is_boundary {
                                after_alias = before_as;
                                let _ = alias;
                            }
                        }
                    }
                    let mut j = after_alias;
                    while j > 0 && matches!(bytes[j - 1], b' ' | b'\t') {
                        j -= 1;
                    }
                    // Module ident.
                    if j == 0 {
                        return None;
                    }
                    let ident_end = j;
                    while j > 0 && (bytes[j - 1].is_ascii_alphanumeric() || bytes[j - 1] == b'_') {
                        j -= 1;
                    }
                    if j == ident_end {
                        return None;
                    }
                    let module = std::str::from_utf8(&bytes[j..ident_end]).ok()?.to_string();
                    let mut k = j;
                    while k > 0 && matches!(bytes[k - 1], b' ' | b'\t') {
                        k -= 1;
                    }
                    // `use` keyword (3 chars), preceded by a token
                    // boundary so we don't match e.g. `disuse`.
                    if k < 3 || &bytes[k - 3..k] != b"use" {
                        return None;
                    }
                    let before_use = k - 3;
                    if before_use > 0
                        && (bytes[before_use - 1].is_ascii_alphanumeric()
                            || bytes[before_use - 1] == b'_')
                    {
                        return None;
                    }
                    return Some(module);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// If `offset` sits inside the body of a `class Foo { ... }` (or
/// `pub class Foo : Parent { ... }`) declaration, returns the
/// outermost enclosing class name. Used by completion to map a bare
/// `this.` receiver to the class whose fields / methods should be
/// listed.
///
/// Implementation: forward scan with brace-tracking. Each open brace
/// pushes the most-recently-seen `class Name` token onto a stack
/// (or `None` if the brace came from a non-class construct); each
/// close pops. At the end, the first `Some` on the stack from the
/// outside in is the enclosing class.
pub(crate) fn enclosing_class(text: &str, offset: usize) -> Option<String> {
    let bytes = text.as_bytes();
    let end = offset.min(bytes.len());
    let mut stack: Vec<Option<String>> = Vec::new();
    let mut pending_class: Option<String> = None;
    let mut i = 0;
    let mut in_line_comment = false;
    let mut block_depth: u32 = 0;
    let mut in_string = false;
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
            if b == b'\\' && i + 1 < end {
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
            i += 1;
            continue;
        }
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            let mut j = i;
            while j < end && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            let prev_boundary = start == 0
                || !(bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_');
            if prev_boundary && &bytes[start..j] == b"class" {
                let mut k = j;
                while k < end && matches!(bytes[k], b' ' | b'\t') {
                    k += 1;
                }
                let name_start = k;
                while k < end && (bytes[k].is_ascii_alphanumeric() || bytes[k] == b'_') {
                    k += 1;
                }
                if k > name_start {
                    if let Ok(name) = std::str::from_utf8(&bytes[name_start..k]) {
                        pending_class = Some(name.to_string());
                    }
                }
                i = k;
                continue;
            }
            i = j;
            continue;
        }
        match b {
            b'{' => {
                stack.push(pending_class.take());
            }
            b'}' => {
                stack.pop();
                pending_class = None;
            }
            _ => {}
        }
        i += 1;
    }
    for entry in &stack {
        if let Some(name) = entry {
            return Some(name.clone());
        }
    }
    None
}

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
    // SIMD vector types. Listed under `simd.<elem><N>` so typing
    // `simd.` filters the completion list to just these entries.
    // `NewSimd` lowers via a stack slot (store-each-lane + vector
    // load), so element / lane combinations that hit cranelift
    // arm64's `scalar_to_vector` ISLE-TODO (notably `f32x2`) are
    // OK to expose — the path bypasses that instruction entirely.
    for name in &[
        "simd.f32x2",
        "simd.f32x4",
        "simd.f64x2",
        "simd.i8x16",
        "simd.i16x8",
        "simd.i32x4",
        "simd.i64x2",
    ] {
        out.push(CompletionItem {
            label: (*name).to_string(),
            kind: Some(CompletionItemKind::STRUCT),
            detail: Some(format!("SIMD vector — assign from a {}-element array literal",
                match *name {
                    "simd.f32x4" | "simd.i32x4" => 4,
                    "simd.f32x2" | "simd.f64x2" | "simd.i64x2" => 2,
                    "simd.i8x16" => 16,
                    "simd.i16x8" => 8,
                    _ => 0,
                })),
            ..CompletionItem::default()
        });
    }
    let is_type_sig = |sig: &str| -> bool {
        sig.starts_with("class ")
            || sig.starts_with("struct ")
            || sig.starts_with("union ")
            || sig.starts_with("enum ")
            || sig.starts_with("interface ")
            || sig.starts_with("@objc interface ")
    };
    let is_interface_sig = |sig: &str| -> bool {
        sig.starts_with("interface ") || sig.starts_with("@objc interface ")
    };
    for (name, sym) in doc.symbols.iter() {
        if !is_type_sig(&sym.signature) {
            continue;
        }
        let kind = if is_interface_sig(&sym.signature) {
            CompletionItemKind::INTERFACE
        } else {
            CompletionItemKind::CLASS
        };
        out.push(CompletionItem {
            label: name.as_str().to_string(),
            kind: Some(kind),
            detail: Some(sym.signature.clone()),
            ..CompletionItem::default()
        });
    }
    // Imported types brought in via `use module` show as
    // `module.TypeName`.
    for (name, sig) in doc.external_signatures.iter() {
        if !is_type_sig(sig) {
            continue;
        }
        let kind = if is_interface_sig(sig) {
            CompletionItemKind::INTERFACE
        } else {
            CompletionItemKind::CLASS
        };
        out.push(CompletionItem {
            label: name.as_str().to_string(),
            kind: Some(kind),
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
    // Names brought into the buffer's bare namespace via a selective
    // (`use M { X, Y }`) or wildcard (`use M { * }`) import. The
    // harvest pass keys those entries under the bare name in
    // `external_signatures`; everything containing a `.` is a
    // module-qualified entry that surfaces through the module-name
    // listing further down instead.
    for (name, sig) in doc.external_signatures.iter() {
        let s = name.as_str();
        if s.contains('.') {
            continue;
        }
        if doc.symbols.contains_key(name) || doc.var_types.contains_key(name) {
            continue;
        }
        // Hide @objc desugar internals that the wildcard / selective
        // harvest may have re-keyed as bare entries — they live in
        // the module's namespace but aren't user-callable surface.
        if crate::symbols::is_synthesized_objc_helper(s) {
            continue;
        }
        let kind = if sig.starts_with("class ")
            || sig.starts_with("struct ")
            || sig.starts_with("union ")
        {
            CompletionItemKind::CLASS
        } else if sig.starts_with("enum ") {
            CompletionItemKind::ENUM
        } else if sig.starts_with("const ") {
            CompletionItemKind::CONSTANT
        } else {
            CompletionItemKind::FUNCTION
        };
        let (insert_text, fmt) = call_snippet(s, kind);
        let command = trigger_sig_help_command(kind);
        out.push(CompletionItem {
            label: s.to_string(),
            kind: Some(kind),
            detail: Some(sig.clone()),
            documentation: doc.external_docs.get(name).cloned().map(|d| {
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

#[cfg(test)]
mod use_completion_tests {
    use super::enclosing_use_module;

    #[test]
    fn inside_single_line_use_brace() {
        let src = "use cocoa { N";
        assert_eq!(enclosing_use_module(src, src.len()).as_deref(), Some("cocoa"));
    }

    #[test]
    fn inside_multiline_use_brace() {
        let src = "use cocoa {\n    NSObject\n    N";
        assert_eq!(enclosing_use_module(src, src.len()).as_deref(), Some("cocoa"));
    }

    #[test]
    fn inside_use_with_alias_discard() {
        let src = "use cocoa as _ { N";
        assert_eq!(enclosing_use_module(src, src.len()).as_deref(), Some("cocoa"));
    }

    #[test]
    fn outside_use_brace_returns_none() {
        let src = "let x = { 1 + ";
        assert!(enclosing_use_module(src, src.len()).is_none());
    }

    #[test]
    fn after_closed_use_brace_returns_none() {
        let src = "use cocoa { NSObject }\nlet x = ";
        assert!(enclosing_use_module(src, src.len()).is_none());
    }
}

#[cfg(test)]
mod enclosing_class_tests {
    use super::enclosing_class;

    #[test]
    fn inside_method_body_simple() {
        let src = "\
pub class Foo {
    pub init() {
        ";
        assert_eq!(enclosing_class(src, src.len()).as_deref(), Some("Foo"));
    }

    #[test]
    fn inside_method_with_inheritance() {
        let src = "\
pub class Bar : Parent, Iface {
    pub run() {
        ";
        assert_eq!(enclosing_class(src, src.len()).as_deref(), Some("Bar"));
    }

    #[test]
    fn between_class_decls_returns_none() {
        let src = "class A { fn a() {} }\nclass B { fn b() {} }\n";
        assert!(enclosing_class(src, src.len()).is_none());
    }

    #[test]
    fn inside_method_after_block_statement() {
        let src = "\
class Foo {
    pub run() {
        if x == 0 { return }
        ";
        assert_eq!(enclosing_class(src, src.len()).as_deref(), Some("Foo"));
    }
}
