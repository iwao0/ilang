//! Type-position completion: the items surfaced after `:` in
//! `let x: T`, `fn f(p: T)`, `class C : T`, etc. Pure builder over
//! `Doc`; the cursor-context probe lives in `completion::context`
//! and the orchestration in `completion::handler`.

use tower_lsp::lsp_types::{
    Command, CompletionItem, CompletionItemKind, InsertTextFormat,
};

use super::{classify_signature_kind, kind_is_type, BUILTIN_GENERIC_TYPES, PRIMITIVE_TYPES};
use crate::Doc;

pub(crate) fn type_completions(doc: &Doc, in_extern_c: bool) -> Vec<CompletionItem> {
    let mut out: Vec<CompletionItem> = Vec::new();
    // Same hide-outside-extern-C gate the value-position paths apply:
    // a struct / union whose fields require raw C types can't be
    // named in a regular `let x: T` slot either.
    let c_only_structs = if in_extern_c {
        std::collections::HashSet::new()
    } else {
        super::c_only_struct_suffixes(doc)
    };
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
    for (name, sym) in doc.symbols.iter() {
        let kind = classify_signature_kind(&sym.signature);
        if !kind_is_type(kind) {
            continue;
        }
        // Hide every `__`-prefixed type — synthesised @objc desugar
        // helpers (`__objc_b*_sel_cache` etc.) plus any other
        // internal-by-convention name.
        if name.as_str().starts_with("__") {
            continue;
        }
        out.push(CompletionItem {
            label: name.as_str().to_string(),
            kind: Some(kind),
            detail: Some(sym.signature.clone()),
            ..CompletionItem::default()
        });
    }
    // Imported types brought in via `use module` show as
    // `module.TypeName`.
    for (name, sig) in doc.external.signatures.iter() {
        let kind = classify_signature_kind(sig);
        if !kind_is_type(kind) {
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
        // Hide `@extern(C) struct/union` types whose fields drag in
        // raw C pointers / `char` / `void` / `size_t` from regular
        // type-position completion — see the rationale in
        // `completion::c_only_struct_suffixes`.
        if c_only_structs.contains(bare) {
            continue;
        }
        // `@handle pub struct` opaque-pointer types live behind
        // `@extern(C) { ... }` boundaries — see the rationale in
        // `helpers::is_handle_struct_signature`. Hide them from
        // `let x: T` / `fn f(p: T)` suggestions outside extern-C.
        if !in_extern_c && crate::helpers::is_handle_struct_signature(sig) {
            continue;
        }
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
    // Per bare suffix, keep only the entry with the shortest dotted
    // prefix. Umbrella modules re-export through chains
    // (`cocoa` → `appkit` → `controls`), so a type defined in
    // `controls.il` ends up registered under three keys
    // (`cocoa.X`, `cocoa.appkit.X`, `cocoa.appkit.controls.X`). All
    // three forms used to surface as separate completion items and
    // the editor's fuzzy matcher would happily pick the deepest one,
    // inserting `cocoa.appkit.controls.X` where the user meant the
    // umbrella `cocoa.X`. Collapse to the shortest path — that's the
    // canonical one the umbrella exists to provide. Kind is
    // intentionally NOT part of the key — `Result` from
    // BUILTIN_GENERIC_TYPES (ENUM) and `Result` from the buffer's
    // class table (CLASS) point at the same type, and listing both
    // is just noise.
    let bare_suffix = |s: &str| -> String {
        s.rsplit_once('.').map(|(_, t)| t).unwrap_or(s).to_string()
    };
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
