//! Bare-name completion fallback when no `.` is in front of the
//! cursor. Surfaces every visible identifier (local decls, imports,
//! variables, keywords) plus the module-name list synthesised from
//! `external_signatures`. Pure builder over `Doc`.

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, MarkupContent, MarkupKind,
};

use super::{
    call_snippet, classify_signature_kind, trigger_sig_help_command,
    BUILTIN_GENERIC_TYPES,
};
use crate::Doc;

/// File-level / block-level keywords. Each entry tags whether the
/// keyword may appear at the file's top level, inside a block
/// (fn / method body / class body / etc.), or both. The completion
/// fallback filters by the receiver's current brace depth — coarse
/// but enough to keep `init` / `return` / `break` out of top-level
/// suggestions and `fn` / `class` / `use` out of block-internal
/// ones.
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
pub(crate) fn global_completions(
    doc: &Doc,
    at_top_level: bool,
    in_extern_c: bool,
) -> Vec<CompletionItem> {
    let mut out: Vec<CompletionItem> = Vec::new();
    // Suffixes of `@extern(C) struct/union` names whose fields drag in
    // a C-only type — hidden from bare-name completion outside the
    // extern-C scope. Empty inside extern-C so the entries surface.
    let c_only_structs = if in_extern_c {
        std::collections::HashSet::new()
    } else {
        super::c_only_struct_suffixes(doc)
    };
    for (name, sym) in doc.symbols.iter() {
        // Hide every `__`-prefixed name. Covers synthesized @objc
        // desugar helpers (`__objc_*`, `__super_*`) plus C-ABI
        // doubles like `__memcpy` that VSCode would otherwise
        // surface for harmless prefixes like `me`.
        if name.as_str().starts_with("__") {
            continue;
        }
        let kind = classify_signature_kind(&sym.signature);
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
    for (name, sig) in doc.external.signatures.iter() {
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
        // `@lib`-attributed extern declarations (the bare-name half
        // of `use M { * }` over a module that contains `@extern(C,
        // "lib") { @lib pub fn ... }`) are only callable from inside
        // another `@extern(C) { ... }` block — their raw C pointer
        // params can't even be constructed outside one. Drop them
        // from the bare-name list when the cursor isn't in such a
        // block so they don't pollute ordinary code's completion.
        if !in_extern_c && sig.starts_with("@lib(") {
            continue;
        }
        // `@extern(C) { struct / union ... }` declarations whose
        // fields use raw pointer / `char` / `void` / `size_t` are
        // similarly only constructible inside another extern-C
        // block. Same hide-outside rule.
        if c_only_structs.contains(s) {
            continue;
        }
        // `@handle pub struct` opaque-pointer types belong inside
        // another `@extern(C) { ... }` block — hide them from the
        // bare top-level surface for the same reason.
        if !in_extern_c && crate::helpers::is_handle_struct_signature(sig) {
            continue;
        }
        // Module entries (`(module) cocoa`) come back from the harvest
        // under their bare key alongside `cocoa.NSObject` etc. The
        // MODULE listing further down already surfaces them; emitting
        // them here too would push a second `cocoa` item classified
        // as FUNCTION (the default fallthrough below since the
        // signature doesn't begin with `class`/`enum`/...).
        let kind = classify_signature_kind(sig);
        if kind == CompletionItemKind::MODULE {
            continue;
        }
        let (insert_text, fmt) = call_snippet(s, kind);
        let command = trigger_sig_help_command(kind);
        out.push(CompletionItem {
            label: s.to_string(),
            kind: Some(kind),
            detail: Some(sig.clone()),
            documentation: doc.external.docs.get(name).cloned().map(|d| {
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
        if doc.external.signatures.contains_key(bare_name) { continue; }
        let Some(sig) = doc.external.signatures.iter().find_map(|(k, v)| {
            (k.as_str().rsplit_once('.').map(|(_, t)| t) == Some(bare)).then(|| v.clone())
        }) else { continue };
        // Same `@lib(` filter as the wildcard-bare path above — a
        // selectively-imported extern declaration is still only
        // callable inside `@extern(C) { ... }`.
        if !in_extern_c && sig.starts_with("@lib(") {
            continue;
        }
        // And the same hide-outside rule for selectively-imported
        // C-only struct / union names.
        if c_only_structs.contains(bare) {
            continue;
        }
        // Selectively-imported `@handle` structs are likewise only
        // meaningful inside another extern-C block.
        if !in_extern_c && crate::helpers::is_handle_struct_signature(&sig) {
            continue;
        }
        let kind = classify_signature_kind(&sig);
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
    for key in doc.external.signatures.keys() {
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
            || doc.external.enums.contains_key(&m_key)
            || doc.classes.contains_key(&m_key)
            || doc.local_interfaces.contains_key(&m_key)
            || doc.external.interfaces.contains_key(&m_key)
            || is_builtin_generic;
        if is_known_type {
            continue;
        }
        // Transitive deps (a dep of a dep) end up in
        // `external.signatures` too — `use gui` pulls cocoa /
        // appkit / foundation in for the loader, but the user's
        // file only sees `gui`. Skip module names the buffer
        // didn't actually `use`.
        if !doc.imported_modules.contains(&m_key) {
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
