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
    Command, CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, MarkupContent,
    MarkupKind,
};

use super::builtins::ffi_helper_signature;
use super::helpers::sig_body_skip_attrs;
use super::Doc;

mod context;
mod handler;
pub(crate) use context::{
    at_attribute_position, at_type_position, brace_depth_at, enclosing_class,
    enclosing_use_module, in_extern_c_block, literal_token_at,
    preceding_kw_introduces_binder,
};
pub(crate) use handler::{handle_completion, resolve_receiver_class};

const PRIMITIVE_TYPES: &[&str] = &[
    "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64", "bool", "string",
];

/// Built-in generic types the type checker pre-registers but no source
/// file declares. Surfaced as type-position completions so `let a: M`
/// suggests `Map`. The last field is the generic-argument count —
/// drives the snippet insertion (`Result<$1, $2>`) so accepting the
/// completion drops the cursor straight into the first slot and
/// fires the `<...>` signature-help overlay.
const BUILTIN_GENERIC_TYPES: &[(&str, &str, CompletionItemKind, usize)] = &[
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
    // `super` shows up two ways:
    //   - top-level `use super.M { ... }` — walk up the dep tree
    //   - class-body `super.method()` / `super(args)`
    // Tag as `Both` so completion offers it in either context.
    ("super", KwScope::Both),
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

/// `true` when the cursor follows a `:` (with optional whitespace and
/// a partial ident underway). That's the type slot of `let x: T`,
/// `const x: T`, `fn f(x: T)`, `field: T` etc.
/// If `offset` sits inside the body of a `use M { ... }` selective
/// import (between the opening `{` and a matching `}` that hasn't yet

pub(crate) fn type_completions(doc: &Doc) -> Vec<CompletionItem> {
    let mut out: Vec<CompletionItem> = Vec::new();
    for t in PRIMITIVE_TYPES {
        out.push(CompletionItem {
            label: (*t).to_string(),
            kind: Some(CompletionItemKind::TYPE_PARAMETER),
            ..CompletionItem::default()
        });
    }
    for (name, detail, kind, generic_count) in BUILTIN_GENERIC_TYPES {
        // Build `Name<$1, $2, ...>` so accepting the completion
        // leaves the cursor on the first generic slot and fires the
        // `<...>` signature-help overlay — without this, the user
        // gets the bare name with no hint that more typing is
        // needed.
        let slots = (1..=*generic_count)
            .map(|i| format!("${i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let snippet = format!("{name}<{slots}>");
        out.push(CompletionItem {
            label: (*name).to_string(),
            kind: Some(*kind),
            detail: Some((*detail).to_string()),
            insert_text: Some(snippet),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            command: Some(Command {
                title: String::new(),
                command: "editor.action.triggerParameterHints".to_string(),
                arguments: None,
            }),
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
        let body = sig_body_skip_attrs(sig);
        body.starts_with("class ")
            || body.starts_with("struct ")
            || body.starts_with("union ")
            || body.starts_with("enum ")
            || body.starts_with("interface ")
            || body.starts_with("@objc interface ")
    };
    let is_interface_sig = |sig: &str| -> bool {
        let body = sig_body_skip_attrs(sig);
        body.starts_with("interface ") || body.starts_with("@objc interface ")
    };
    for (name, sym) in doc.symbols.iter() {
        if !is_type_sig(&sym.signature) {
            continue;
        }
        // Hide every `__`-prefixed type — synthesised @objc desugar
        // helpers (`__objc_b*_sel_cache` etc.) plus any other
        // internal-by-convention name.
        if name.as_str().starts_with("__") {
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
        // Strip the module prefix before testing — `__`-prefixed
        // suffixes are internal regardless of which module they're
        // re-exported from.
        let bare = name
            .as_str()
            .rsplit_once('.')
            .map(|(_, t)| t)
            .unwrap_or(name.as_str());
        if bare.starts_with("__") {
            continue;
        }
        let kind = if is_interface_sig(sig) {
            CompletionItemKind::INTERFACE
        } else {
            CompletionItemKind::CLASS
        };
        // Label depends on whether the bare name is already imported
        // (`use cocoa { NSApplicationDelegate }`): if yes, show bare
        // (matches how the user will reference it); if no, show the
        // module-qualified form so it's obvious the `cocoa.` prefix
        // is part of the inserted text. The completion handler stamps
        // a synthetic `filter_text` based on the typed prefix, so the
        // dotted label still surfaces under `app`-style queries.
        let full = name.as_str().to_string();
        let already_imported = doc
            .selective_use_names
            .contains(&crate::AstSymbol::intern(bare));
        let label = if already_imported || full == bare {
            bare.to_string()
        } else {
            full.clone()
        };
        out.push(CompletionItem {
            label,
            kind: Some(kind),
            detail: Some(sig.clone()),
            ..CompletionItem::default()
        });
    }
    // Per bare suffix + kind, keep only the entry with the shortest
    // dotted prefix. Umbrella modules re-export through chains
    // (`cocoa` → `appkit` → `controls`), so a type defined in
    // `controls.il` ends up registered under three keys
    // (`cocoa.X`, `cocoa.appkit.X`, `cocoa.appkit.controls.X`). All
    // three forms used to surface as separate completion items and
    // the editor's fuzzy matcher would happily pick the deepest one,
    // inserting `cocoa.appkit.controls.X` where the user meant the
    // umbrella `cocoa.X`. Collapse to the shortest path — that's the
    // canonical one the umbrella exists to provide.
    let bare_suffix = |s: &str| -> String {
        s.rsplit_once('.').map(|(_, t)| t).unwrap_or(s).to_string()
    };
    // Sort so that within each bare-suffix group the entry with the
    // fewest dots comes first, then dedup_by keeps it. Kind is
    // intentionally NOT part of the key — `Result` from
    // BUILTIN_GENERIC_TYPES (ENUM) and `Result` from the buffer's
    // class table (CLASS) point at the same type, and listing both
    // is just noise.
    out.sort_by(|a, b| {
        (
            bare_suffix(&a.label),
            a.label.matches('.').count(),
            a.label.clone(),
        )
            .cmp(&(
                bare_suffix(&b.label),
                b.label.matches('.').count(),
                b.label.clone(),
            ))
    });
    out.dedup_by(|a, b| bare_suffix(&a.label) == bare_suffix(&b.label));
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}

/// `true` when the cursor is right after a `let` / `const` keyword
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
        // Hide every `__`-prefixed name. Covers synthesized @objc
        // desugar helpers (`__objc_*`, `__super_*`) plus C-ABI
        // doubles like `__memcpy` that VSCode would otherwise
        // surface for harmless prefixes like `me`.
        if name.as_str().starts_with("__") {
            continue;
        }
        let body = sig_body_skip_attrs(&sym.signature);
        let kind = if body.starts_with("class ")
            || body.starts_with("struct ")
            || body.starts_with("union ")
        {
            CompletionItemKind::CLASS
        } else if body.starts_with("enum ") {
            CompletionItemKind::ENUM
        } else if body.starts_with("const ") {
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
        if name.as_str().starts_with("__") {
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
        // Hide every `__`-prefixed bare import — @objc desugar
        // internals plus C-ABI doubles that VSCode would otherwise
        // surface for short prefixes.
        if s.starts_with("__") {
            continue;
        }
        // Module entries (`(module) cocoa`) come back from the harvest
        // under their bare key alongside `cocoa.NSObject` etc. The
        // MODULE listing further down already surfaces them; emitting
        // them here too would push a second `cocoa` item classified
        // as FUNCTION (the default fallthrough below since the
        // signature doesn't begin with `class`/`enum`/...).
        let body = sig_body_skip_attrs(sig);
        if body.starts_with("(module) ") {
            continue;
        }
        let kind = if body.starts_with("class ")
            || body.starts_with("struct ")
            || body.starts_with("union ")
        {
            CompletionItemKind::CLASS
        } else if body.starts_with("enum ") {
            CompletionItemKind::ENUM
        } else if body.starts_with("const ") {
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
    // Selective imports (`use cocoa { NSWindowStyleMask, makeWindow }`)
    // register names only in `selective_use_names`; the signature
    // lives under the dotted key in `external_signatures`. The bare
    // loop above misses them because dotted keys are filtered. Add a
    // bare-label completion for each selectively imported name by
    // walking the dotted external map for a suffix match.
    for bare_name in doc.selective_use_names.iter() {
        let bare = bare_name.as_str();
        if bare.starts_with("__") { continue; }
        if doc.symbols.contains_key(bare_name) { continue; }
        if doc.var_types.contains_key(bare_name) { continue; }
        if doc.external_signatures.contains_key(bare_name) { continue; }
        let Some(sig) = doc.external_signatures.iter().find_map(|(k, v)| {
            (k.as_str().rsplit_once('.').map(|(_, t)| t) == Some(bare)).then(|| v.clone())
        }) else { continue };
        let body = sig_body_skip_attrs(&sig);
        let kind = if body.starts_with("class ")
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
        } else {
            CompletionItemKind::FUNCTION
        };
        let (insert_text, fmt) = call_snippet(bare, kind);
        let command = trigger_sig_help_command(kind);
        out.push(CompletionItem {
            label: bare.to_string(),
            kind: Some(kind),
            detail: Some(sig),
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
        // `NSWindowStyleMask.titled` etc. live in `external_signatures`
        // as dotted keys (variant accesses), so the split above would
        // happily pick `NSWindowStyleMask` as a "module" and produce
        // a phantom MODULE entry next to the real ENUM one. Skip any
        // prefix that's actually a known type/enum/interface, plus
        // the type-checker's pre-registered built-in generics
        // (`Result`, `Map`, ...) which the user can reach as
        // `Result.ok(...)` even though they don't appear in any of
        // the per-doc maps.
        let m_key = crate::AstSymbol::intern(&m);
        let is_builtin_generic = BUILTIN_GENERIC_TYPES
            .iter()
            .any(|(n, _, _, _)| *n == m);
        let is_known_type = doc.local_enums.contains_key(&m_key)
            || doc.external_enums.contains_key(&m_key)
            || doc.classes.contains_key(&m_key)
            || doc.local_interfaces.contains_key(&m_key)
            || doc.external_interfaces.contains_key(&m_key)
            || is_builtin_generic;
        if is_known_type {
            continue;
        }
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

