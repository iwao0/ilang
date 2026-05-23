//! `textDocument/completion` orchestration. Resolves the cursor's
//! context (receiver-before-dot, type-position, attribute-position,
//! etc.) and dispatches to the small builders in `completion::mod`
//! to produce the response payload.

#![allow(unused_imports)]

use ilang_ast::{Symbol as AstSymbol, Type};
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionList, CompletionResponse, Documentation,
    InsertTextFormat, MarkupContent, MarkupKind, Position,
};

use super::{
    at_attribute_position, at_type_position, attribute_completions, brace_depth_at,
    call_snippet, enclosing_class, enclosing_use_module, global_completions,
    in_extern_c_block, preceding_kw_introduces_binder, push_extern_c_keywords,
    push_ffi_helper_completions, trigger_sig_help_command, type_completions,
};
use crate::builtins::{
    array_method_doc, array_method_names, array_method_sig, map_method_doc,
    map_method_names, map_method_sig, string_method_doc, string_method_names,
    string_method_sig,
};
use crate::code_actions::interface_method_stub_completions_textual;
use crate::helpers::{self, sig_body_skip_attrs};
use crate::symbols::is_synthesized_objc_helper;
use crate::text::{self, receiver_before_dot};
use crate::Doc;

/// Walk a dotted receiver chain (`this.starTex`, `obj.foo.bar`, ...)
/// and return the class name of the last segment, or `None` if any
/// hop fails to resolve. Used by both completion and signature_help
/// so the dispatch logic stays in one place.
///
/// The first segment resolves to a class via, in priority order:
///   1. `this` -> the enclosing class found by a text-level scan
///   2. a registered class name (`Counter.method`)
///   3. a `var_classes` entry (let-bound / param)
///   4. the enclosing class's fields / getters / methods (implicit
///      `this` field access, since ilang resolves bare idents
///      against `this` inside method bodies)
///
/// Each subsequent segment looks up a field / getter / method on
/// the current class and continues with the declared return type's
/// class.
pub(crate) fn resolve_receiver_class(
    doc: &Doc,
    receiver: &str,
    text_offset: usize,
) -> Option<String> {
    if receiver.is_empty() {
        return None;
    }
    let segments: Vec<&str> = receiver.split('.').collect();
    let mut current: Option<String> = if segments[0] == "this" {
        enclosing_class(&doc.text, text_offset)
    } else if doc.classes.contains_key(&AstSymbol::intern(segments[0])) {
        Some(segments[0].to_string())
    } else if let Some(c) = doc
        .var_classes
        .get(&AstSymbol::intern(segments[0]))
        .cloned()
    {
        Some(c)
    } else {
        enclosing_class(&doc.text, text_offset).and_then(|cls| {
            let info = doc.classes.get(&AstSymbol::intern(&cls))?;
            let key = AstSymbol::intern(segments[0]);
            let m = info
                .getters
                .get(&key)
                .or_else(|| info.fields.get(&key))
                .or_else(|| info.methods.get(&key))?;
            helpers::type_to_class(m.ret_ty.as_ref()?)
        })
    };
    for seg in &segments[1..] {
        let cls = current.as_deref()?;
        let info = doc.classes.get(&AstSymbol::intern(cls))?;
        let key = AstSymbol::intern(seg);
        let m = info
            .getters
            .get(&key)
            .or_else(|| info.fields.get(&key))
            .or_else(|| info.methods.get(&key))?;
        current = helpers::type_to_class(m.ret_ty.as_ref()?);
    }
    current
}

/// Orchestrate `textDocument/completion`. Returns the response
/// directly (no `LspResult` wrapping) — the trait shell in
/// `handlers.rs` converts to `Ok(...)`. Pure function over `doc` /
/// `pos`; the impl method handles state lookup.
pub(crate) fn handle_completion(doc: &Doc, pos: Position) -> Option<CompletionResponse> {
    // No `.` immediately before the cursor → list visible
    // identifiers from this file + imported decls. Returning
    // something from the LSP keeps VSCode's word-based fallback
    // (which would mix in unrelated identifiers from other open
    // files) from being the only source.
    let Some(receiver) = receiver_before_dot(&doc.text, pos) else {
        let off = text::line_col_to_offset(&doc.text, pos.line + 1, pos.character + 1)
            .unwrap_or(doc.text.len());
        // After `let` / `const` the user is naming a new binding —
        // suppress all suggestions so VSCode doesn't autocomplete
        // an unrelated identifier into the binder slot.
        if preceding_kw_introduces_binder(&doc.text, off) {
            return Some(CompletionResponse::Array(Vec::new()));
        }
        // `@x` -> attribute completion.
        if at_attribute_position(&doc.text, off) {
            return Some(CompletionResponse::Array(attribute_completions()));
        }
        // After `:` we're in a type position — only suggest types.
        if at_type_position(&doc.text, off) {
            let mut items = type_completions(doc);
            // Server-side fuzzy filter against the typed prefix.
            // VSCode's client filter scores `app` against
            // `NSApplicationDelegate` below its visibility
            // threshold and silently drops it; bypass that by
            // filtering here and stamping `filter_text` with the
            // typed prefix verbatim so the client always passes
            // every item we approve. `isIncomplete: true` makes
            // VSCode re-ask on each keystroke instead of running
            // its own filter over a cached list.
            let prefix = text::typed_prefix_at(&doc.text, off);
            if !prefix.is_empty() {
                let lowered_prefix = prefix.to_lowercase();
                items.retain(|it| text::subsequence_ci(&it.label, &lowered_prefix));
                for it in items.iter_mut() {
                    it.filter_text = Some(prefix.clone());
                }
            }
            // Stamp `sortText` so the client ranks
            //   (1) items where the typed prefix appears as a
            //       contiguous substring in the label (earliest
            //       position wins) — typing `nso` puts `NSObject`
            //       above `NSApplication` even though both pass
            //       the server's subsequence filter;
            //   (2) shorter module paths above longer ones —
            //       bare `X` (selectively imported) beats
            //       `cocoa.X` beats `cocoa.appkit.X`;
            //   (3) alphabetical label as the final tiebreak.
            // Without this VSCode sees identical `filterText`
            // across every item and falls back to lexical label
            // sort, which buries `NSObject` under unrelated `NSA`-
            // prefixed names.
            let lowered_prefix = prefix.to_lowercase();
            for it in items.iter_mut() {
                let lowered_label = it.label.to_lowercase();
                let substr_pos = if lowered_prefix.is_empty() {
                    0usize
                } else {
                    lowered_label
                        .find(&lowered_prefix)
                        .unwrap_or(usize::MAX / 2)
                };
                let dots = it.label.matches('.').count();
                it.sort_text =
                    Some(format!("{substr_pos:08}_{dots:02}_{}", it.label));
            }
            return Some(CompletionResponse::List(CompletionList {
                is_incomplete: true,
                items,
            }));
        }
        // Inside `use M { ... }` — list `M`'s exports.
        if let Some(module) = enclosing_use_module(&doc.text, off) {
            let prefix = format!("{module}.");
            let mut items: Vec<CompletionItem> = doc
                .external_signatures
                .iter()
                .filter_map(|(k, sig)| {
                    let suffix = k.as_str().strip_prefix(&prefix)?;
                    if suffix.contains('.') {
                        return None;
                    }
                    if is_synthesized_objc_helper(suffix) {
                        return None;
                    }
                    // Strip leading `@attr` lines (e.g. `@objc\n`,
                    // `@flags\n`) so the kind classifier can still
                    // see the `class` / `enum` keyword on the
                    // first content line.
                    let body = sig_body_skip_attrs(sig);
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
                    Some(CompletionItem {
                        label: suffix.to_string(),
                        kind: Some(kind),
                        detail: Some(sig.clone()),
                        ..CompletionItem::default()
                    })
                })
                .collect();
            items.sort_by(|a, b| a.label.cmp(&b.label));
            return Some(CompletionResponse::Array(items));
        }
        let at_top_level = brace_depth_at(&doc.text, off) <= 0;
        let mut items = global_completions(doc, at_top_level);
        if in_extern_c_block(&doc.text, off) {
            push_ffi_helper_completions(&mut items);
            push_extern_c_keywords(&mut items);
        }
        // Inside a class body: surface every unimplemented
        // interface method the class is supposed to provide
        // as a one-tap snippet candidate. The text-based
        // discovery path (no AST parse needed) keeps working
        // while the user is mid-typing and the buffer
        // doesn't parse cleanly.
        if !at_top_level {
            let stubs = interface_method_stub_completions_textual(
                &doc.text,
                off,
                &doc.local_interfaces,
                &doc.external_interfaces,
            );
            for (label, detail, snippet) in stubs {
                items.push(CompletionItem {
                    label,
                    kind: Some(CompletionItemKind::METHOD),
                    detail,
                    insert_text: Some(snippet),
                    insert_text_format: Some(InsertTextFormat::SNIPPET),
                    ..CompletionItem::default()
                });
            }
        }
        // Inside a method body: surface the enclosing class's
        // instance fields / methods as bare-name candidates.
        // ilang resolves a bare ident inside a method body
        // against the implicit `this` before falling back to
        // module-level names, so the insert text is the bare
        // name itself.
        if !at_top_level {
            if let Some(class) = enclosing_class(&doc.text, off) {
                if let Some(info) = doc.classes.get(&AstSymbol::intern(&class)) {
                    for (name, m) in info.fields.iter() {
                        if m.is_static {
                            continue;
                        }
                        let s = name.as_str();
                        if is_synthesized_objc_helper(s) {
                            continue;
                        }
                        items.push(CompletionItem {
                            label: s.to_string(),
                            kind: Some(CompletionItemKind::FIELD),
                            detail: Some(m.signature.clone()),
                            ..CompletionItem::default()
                        });
                    }
                    for (name, m) in info.methods.iter() {
                        if m.is_static {
                            continue;
                        }
                        let s = name.as_str();
                        if s == "init" || s == "deinit" {
                            continue;
                        }
                        if is_synthesized_objc_helper(s) {
                            continue;
                        }
                        items.push(CompletionItem {
                            label: s.to_string(),
                            kind: Some(CompletionItemKind::METHOD),
                            detail: Some(m.signature.clone()),
                            ..CompletionItem::default()
                        });
                    }
                }
            }
        }
        return Some(CompletionResponse::Array(items));
    };
    // Receiver can be:
    // - a class name (`Counter.`)        -> static members
    // - an enum name (`NSWindowStyleMask.`) -> variants
    // - a variable typed as some class (`c.`) -> instance members
    // Anything else falls through and we return nothing.
    let receiver_key = AstSymbol::intern(&receiver);
    if let Some(en) = doc
        .local_enums
        .get(&receiver_key)
        .or_else(|| doc.external_enums.get(&receiver_key))
    {
        let items: Vec<CompletionItem> = en
            .variants
            .iter()
            .map(|v| CompletionItem {
                label: v.name.as_str().to_string(),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                detail: Some(format!("(variant) {}.{}", en.name, v.name)),
                ..CompletionItem::default()
            })
            .collect();
        return Some(CompletionResponse::Array(items));
    }
    let want_static = doc.classes.contains_key(&receiver_key);
    let class_name = if want_static {
        receiver.clone()
    } else if receiver == "console" {
        // Built-in singleton: instance of `Console`.
        "Console".to_string()
    } else {
        let off = text::line_col_to_offset(&doc.text, pos.line + 1, pos.character + 1)
            .unwrap_or(doc.text.len());
        resolve_receiver_class(doc, &receiver, off).unwrap_or_default()
    };
    if doc.classes.get(&AstSymbol::intern(&class_name)).is_none() {
        // Built-in receiver: string / array. Their member sets are
        // hardcoded — list them from the same helpers used by hover.
        // String literal (`"abc".`) flows in via a sentinel
        // receiver; treat it as `Type::Str` directly.
        let inferred_ty: Option<Type> = if receiver == text::STR_LITERAL_RECEIVER {
            Some(Type::Str)
        } else {
            doc.var_types.get(&AstSymbol::intern(&receiver)).cloned()
        };
        if let Some(ty) = inferred_ty.as_ref() {
            let entries: Vec<(String, String, Option<&'static str>)> = match ty {
                Type::Str => string_method_names()
                    .into_iter()
                    .filter_map(|n| {
                        string_method_sig(n)
                            .map(|s| (n.to_string(), s, string_method_doc(n)))
                    })
                    .collect(),
                Type::Array { elem, fixed } => array_method_names()
                    .into_iter()
                    .filter(|n| {
                        // Fixed-length arrays can't grow / shrink.
                        !(fixed.is_some() && matches!(**n, "push" | "pop"))
                    })
                    .filter_map(|n| {
                        array_method_sig(n, elem)
                            .map(|s| (n.to_string(), s, array_method_doc(n)))
                    })
                    .collect(),
                Type::Generic(g)
                    if g.base.as_str() == "Map" && g.args.len() == 2 =>
                {
                    map_method_names()
                        .into_iter()
                        .filter_map(|n| {
                            map_method_sig(n, &g.args[0], &g.args[1])
                                .map(|s| (n.to_string(), s, map_method_doc(n)))
                        })
                        .collect()
                }
                _ => Vec::new(),
            };
            if !entries.is_empty() {
                let mut items: Vec<CompletionItem> = entries
                    .into_iter()
                    .map(|(name, sig, doc_text)| {
                        let (insert_text, fmt) =
                            call_snippet(name.as_str(), CompletionItemKind::METHOD);
                        let command =
                            trigger_sig_help_command(CompletionItemKind::METHOD);
                        CompletionItem {
                            label: name.as_str().to_string(),
                            kind: Some(CompletionItemKind::METHOD),
                            detail: Some(sig.as_str().to_string()),
                            documentation: doc_text.map(|d| {
                                Documentation::MarkupContent(MarkupContent {
                                    kind: MarkupKind::Markdown,
                                    value: d.to_string(),
                                })
                            }),
                            insert_text,
                            insert_text_format: fmt,
                            command,
                            ..CompletionItem::default()
                        }
                    })
                    .collect();
                // `length` is a property, not a method.
                items.push(CompletionItem {
                    label: "length".to_string(),
                    kind: Some(CompletionItemKind::FIELD),
                    detail: Some(match ty {
                        Type::Str => "(property) string.length: i64".to_string(),
                        Type::Array { elem, .. } => {
                            format!("(property) {elem}[].length: i64")
                        }
                        _ => unreachable!(),
                    }),
                    ..CompletionItem::default()
                });
                items.sort_by(|a, b| a.label.cmp(&b.label));
                return Some(CompletionResponse::Array(items));
            }
        }
        // Receiver may be a `use module` namespace — list its
        // re-exported items (e.g. `math.` -> `sqrt`, `pi`, ...).
        let prefix = format!("{receiver}.");
        let mut items: Vec<CompletionItem> = doc
            .external_signatures
            .iter()
            .filter_map(|(k, sig)| {
                let suffix = k.as_str().strip_prefix(&prefix)?;
                // Skip nested-module names (`sdl.SDL_Rect.field`
                // would re-introduce a dot).
                if suffix.contains('.') {
                    return None;
                }
                // Hide @objc desugar's internal scaffolding —
                // the per-block `__objc_<hash>_class_t` etc.
                // structs and bookkeeping wrappers are emitted
                // into the module's namespace but aren't user-
                // facing.
                if is_synthesized_objc_helper(suffix) {
                    return None;
                }
                let body = sig_body_skip_attrs(sig);
                let kind = if body.starts_with("class ")
                    || body.starts_with("struct ")
                    || body.starts_with("union ")
                {
                    CompletionItemKind::CLASS
                } else if body.starts_with("enum ") {
                    CompletionItemKind::ENUM
                } else if body.starts_with("(variant)") {
                    CompletionItemKind::ENUM_MEMBER
                } else if body.starts_with("const ") {
                    CompletionItemKind::CONSTANT
                } else {
                    CompletionItemKind::FUNCTION
                };
                let (insert_text, fmt) = call_snippet(suffix, kind);
                let command = trigger_sig_help_command(kind);
                let documentation = doc.external_docs.get(k).cloned().map(|d| {
                    Documentation::MarkupContent(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: d,
                    })
                });
                Some(CompletionItem {
                    label: suffix.to_string(),
                    kind: Some(kind),
                    detail: Some(sig.clone()),
                    documentation,
                    insert_text,
                    insert_text_format: fmt,
                    command,
                    ..CompletionItem::default()
                })
            })
            .collect();
        items.sort_by(|a, b| a.label.cmp(&b.label));
        if !items.is_empty() {
            return Some(CompletionResponse::Array(items));
        }
        return None;
    }
    let info = doc.classes.get(&AstSymbol::intern(&class_name)).unwrap();
    let mut items: Vec<CompletionItem> = Vec::new();
    for (name, m) in info.fields.iter() {
        if m.is_static != want_static {
            continue;
        }
        // Hide the @objc desugar's internal bookkeeping
        // fields (`__owns`) — they're not part of the
        // user-facing surface.
        if is_synthesized_objc_helper(name.as_str()) {
            continue;
        }
        // Properties live in both `fields` (the bare entry) and
        // `getters` / `setters`. Prefer the getter signature when
        // we have one so `c.a` shows `(getter)` not `(property)`.
        let display = info.getters.get(name).unwrap_or(m);
        items.push(CompletionItem {
            label: name.as_str().to_string(),
            kind: Some(CompletionItemKind::FIELD),
            detail: Some(display.signature.clone()),
            documentation: display.doc.clone().map(|d| {
                Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: d,
                })
            }),
            ..CompletionItem::default()
        });
    }
    for (name, m) in info.methods.iter() {
        // `init` is callable through `new ClassName(...)`, not via
        // `ClassName.init(...)`, so hide it from static completion.
        // `deinit` is auto-invoked by ARC at refcount-zero; user
        // code shouldn't call it directly either.
        if name == "init" || name == "deinit" {
            continue;
        }
        // Parser-synthesised helpers (the `@objc class` desugar's
        // `__bind_handle` / `__wrap_handle` etc.) shouldn't show in
        // completion. They're invoked only from cocoa.il's wrap()
        // bridge, not by user code directly.
        if is_synthesized_objc_helper(name.as_str()) {
            continue;
        }
        if m.is_static != want_static {
            continue;
        }
        let (insert_text, fmt) = call_snippet(name.as_str(), CompletionItemKind::METHOD);
        let command = trigger_sig_help_command(CompletionItemKind::METHOD);
        items.push(CompletionItem {
            label: name.as_str().to_string(),
            kind: Some(CompletionItemKind::METHOD),
            detail: Some(m.signature.clone()),
            documentation: m.doc.clone().map(|d| {
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
    items.sort_by(|a, b| a.label.cmp(&b.label));
    Some(CompletionResponse::Array(items))
}

