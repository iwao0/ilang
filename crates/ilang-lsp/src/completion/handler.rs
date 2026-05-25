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
    at_attribute_position, at_type_position, at_use_alias_position, attribute_completions,
    brace_depth_at, call_snippet, classify_signature_kind, enclosing_class,
    enclosing_use_module, global_completions, in_extern_c_block, in_extern_objc_block,
    preceding_kw_introduces_binder, push_extern_c_keywords,
    push_ffi_helper_completions, trigger_sig_help_command, type_completions,
    use_path_prefix_at,
};
use crate::builtins::{
    array_method_doc, array_method_names, array_method_sig, map_method_doc,
    map_method_names, map_method_sig, primitive_method_doc, primitive_method_names,
    primitive_method_sig, string_method_doc, string_method_names, string_method_sig,
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
/// Walk the same dotted-chain `resolve_receiver_class` does but
/// return the last segment's declared type instead of the bare class
/// name. Used by signature-help to recover concrete generic
/// arguments (`Signal<CloseEvent>`) so member signatures can render
/// `fn(CloseEvent)` instead of `fn(T)`.
pub(crate) fn resolve_receiver_type(
    doc: &Doc,
    receiver: &str,
    text_offset: usize,
) -> Option<Type> {
    if receiver.is_empty() {
        return None;
    }
    let segments: Vec<&str> = receiver.split('.').collect();
    let mut current_class: Option<String> = if segments[0] == "this" {
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
        return doc.var_types.get(&AstSymbol::intern(segments[0])).cloned();
    };
    let mut current_ty: Option<Type> = current_class
        .as_ref()
        .map(|c| Type::Object(AstSymbol::intern(c)));
    for seg in &segments[1..] {
        let cls = current_class.as_deref()?;
        let info = doc.classes.get(&AstSymbol::intern(cls))?;
        let key = AstSymbol::intern(seg);
        let m = info
            .getters
            .get(&key)
            .or_else(|| info.fields.get(&key))
            .or_else(|| info.methods.get(&key))?;
        let ret = m.ret_ty.clone()?;
        current_class = helpers::type_to_class(&ret);
        current_ty = Some(ret);
    }
    current_ty
}

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
    // `use M as |` — the alias name is a fresh binder; offering any
    // visible identifier just invites shadowing. Return an empty
    // list so VSCode's word-based fallback doesn't fill in.
    {
        let off = text::line_col_to_offset(&doc.text, pos.line + 1, pos.character + 1)
            .unwrap_or(doc.text.len());
        if at_use_alias_position(&doc.text, off) {
            return Some(CompletionResponse::Array(Vec::new()));
        }
    }
    // `use <ident>` or `use a.b.<ident>` — the cursor is on a
    // segment of the module path. This branch fires before the
    // ordinary dot-receiver dispatch because `use std.<cursor>`
    // would otherwise route through "receiver = std" and try to
    // resolve it as a class / variable.
    {
        let off = text::line_col_to_offset(&doc.text, pos.line + 1, pos.character + 1)
            .unwrap_or(doc.text.len());
        if let Some(prefix) = use_path_prefix_at(&doc.text, off) {
            let mut heads: std::collections::BTreeSet<String> =
                std::collections::BTreeSet::new();
            if prefix.is_empty() {
                // Top-level path heads. Always offer the bundled
                // `std` package head, plus every module the LSP has
                // indexed (the part of the dotted key before the
                // first `.`).
                heads.insert("std".to_string());
                for k in doc.external.signatures.keys() {
                    if let Some((head, _rest)) = k.as_str().split_once('.') {
                        heads.insert(head.to_string());
                    }
                }
            } else if prefix == "std" {
                // Bundled standard library — list every `libs/std/*.il`
                // module by name. The LSP doesn't index these by their
                // dotted `std.X` path (the loader merges items under
                // the leaf prefix like `math.X` etc.), so enumerate
                // the canonical list directly.
                for name in [
                    "events", "ffi", "fs", "math", "os", "path",
                    "regex", "test", "time",
                ] {
                    heads.insert(name.to_string());
                }
            } else {
                // Multi-segment dotted prefix — list the children
                // of `prefix.` from the indexed external signatures
                // (e.g. `cocoa.foundation.|`).
                let dot_prefix = format!("{prefix}.");
                for k in doc.external.signatures.keys() {
                    if let Some(rest) = k.as_str().strip_prefix(&dot_prefix) {
                        let head = rest.split_once('.').map(|(h, _)| h).unwrap_or(rest);
                        heads.insert(head.to_string());
                    }
                }
            }
            let items: Vec<CompletionItem> = heads
                .into_iter()
                .map(|h| CompletionItem {
                    label: h,
                    kind: Some(CompletionItemKind::MODULE),
                    ..CompletionItem::default()
                })
                .collect();
            return Some(CompletionResponse::Array(items));
        }
    }
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
            let mut items = type_completions(doc, in_extern_c_block(&doc.text, off));
            // `ObjCBlock<F>` only makes sense inside an
            // `@extern(ObjC) { ... }` block — drop it everywhere
            // else so trigging completion on `O` doesn't surface
            // it in ordinary ilang code.
            if !in_extern_objc_block(&doc.text, off) {
                items.retain(|it| it.label != "ObjCBlock");
            }
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
                .external
                .signatures
                .iter()
                .filter_map(|(k, sig)| {
                    let suffix = k.as_str().strip_prefix(&prefix)?;
                    if suffix.contains('.') {
                        return None;
                    }
                    if is_synthesized_objc_helper(suffix) {
                        return None;
                    }
                    // Hide nested sub-module names — `pub use button.*`
                    // re-exports `button.il`'s contents into the
                    // parent namespace but leaves `button` itself
                    // unreachable from `use M { ... }`.
                    let kind = classify_signature_kind(sig);
                    if kind == CompletionItemKind::MODULE {
                        return None;
                    }
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
        let in_extern_c = in_extern_c_block(&doc.text, off);
        let mut items = global_completions(doc, at_top_level, in_extern_c);
        // `ObjCBlock<F>` is meaningless outside an
        // `@extern(ObjC) { ... }` block — `register_builtin_enums`
        // dumps it into `external_signatures` so hover works, which
        // also makes it bleed into the bare-name completion list.
        // Drop it here when the cursor isn't inside such a block.
        if !in_extern_objc_block(&doc.text, off) {
            items.retain(|it| it.label != "ObjCBlock");
        }
        if in_extern_c {
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
                &doc.external.interfaces,
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
        // When the cursor sits inside a function-call's argument list
        // (`f(a, b, |)`), figure out the expected type of the active
        // slot from the callee's signature and boost matching items
        // — vars typed as that type, the type itself, its enum
        // variants — to the top of the list via `sortText`. Without
        // this, typing `,` in `makeWindow(..., |)` leaves the user
        // staring at an alphabetic dump of every visible identifier.
        if let Some(call) = text::call_context_at(&doc.text, pos) {
            if let Some(expected) = expected_param_type(doc, &call) {
                boost_arg_matches(&mut items, &expected, doc);
            }
        }
        // `is_incomplete: true` keeps VSCode re-asking on every
        // keystroke instead of closing the popup when the user
        // types a non-word character (most notably the space after
        // `,` inside a function call).
        return Some(CompletionResponse::List(CompletionList {
            is_incomplete: true,
            items,
        }));
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
        .or_else(|| doc.external.enums.get(&receiver_key))
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
                        !(fixed.is_some()
                            && matches!(
                                **n,
                                "push" | "pop" | "shift" | "unshift"
                                    | "remove" | "removeAt",
                            ))
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
                // Numeric primitives and `bool` share a small set of
                // built-in methods (`toString`). The type-checker
                // (`checker::expr::calls`) gates them by
                // `is_numeric() || == Type::Bool`; mirror that here.
                t if t.is_numeric() || matches!(t, Type::Bool) => {
                    primitive_method_names()
                        .into_iter()
                        .filter_map(|n| {
                            primitive_method_sig(n, t)
                                .map(|s| (n.to_string(), s, primitive_method_doc(n)))
                        })
                        .collect()
                }
                _ => Vec::new(),
            };
            if !entries.is_empty() {
                let mut items: Vec<CompletionItem> = entries
                    .into_iter()
                    .map(|(name, sig, doc_text)| {
                        // Built-in array methods like `forEach` /
                        // `map` / `filter` take a closure parameter;
                        // route through `build_method_call_snippet`
                        // so the snippet expands to a ready-to-fill
                        // `fn(${1:_}: T) { ${2} }` instead of bare
                        // `forEach(`.
                        let (insert_text, fmt) =
                            build_method_call_snippet(name.as_str(), sig.as_str())
                                .map(|(t, f)| (Some(t), Some(f)))
                                .unwrap_or_else(|| {
                                    call_snippet(
                                        name.as_str(),
                                        CompletionItemKind::METHOD,
                                    )
                                });
                        // Re-fire signature help once the snippet
                        // expansion lands the cursor inside the call
                        // so the param overlay shows up immediately.
                        let command = Some(tower_lsp::lsp_types::Command {
                            title: String::new(),
                            command: "editor.action.triggerParameterHints"
                                .to_string(),
                            arguments: None,
                        });
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
                // `length` is a property, not a method. Only strings
                // and arrays carry it — numeric / bool receivers
                // surface `toString` only, no property side-car.
                let length_detail = match ty {
                    Type::Str => Some("(property) string.length: i64".to_string()),
                    Type::Array { elem, .. } => {
                        Some(format!("(property) {elem}[].length: i64"))
                    }
                    _ => None,
                };
                if let Some(detail) = length_detail {
                    items.push(CompletionItem {
                        label: "length".to_string(),
                        kind: Some(CompletionItemKind::FIELD),
                        detail: Some(detail),
                        ..CompletionItem::default()
                    });
                }
                items.sort_by(|a, b| a.label.cmp(&b.label));
                return Some(CompletionResponse::Array(items));
            }
        }
        // Receiver may be a `use module` namespace — list its
        // re-exported items (e.g. `math.` -> `sqrt`, `pi`, ...).
        // If the receiver is a user-chosen alias (`use std.math as m`),
        // translate it to the canonical module-head name first so the
        // `external_signatures` lookup (keyed by `math.X`) succeeds.
        let canonical = doc
            .module_aliases
            .get(&AstSymbol::intern(&receiver))
            .map(|s| s.as_str().to_string())
            .unwrap_or_else(|| receiver.clone());
        let prefix = format!("{canonical}.");
        let off = text::line_col_to_offset(&doc.text, pos.line + 1, pos.character + 1)
            .unwrap_or(doc.text.len());
        let in_extern_c = in_extern_c_block(&doc.text, off);
        let c_only_structs = if in_extern_c {
            std::collections::HashSet::new()
        } else {
            super::c_only_struct_suffixes(doc)
        };
        let mut items: Vec<CompletionItem> = doc
            .external
            .signatures
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
                // `@lib`-attributed fns from inside `@extern(C, "…")`
                // blocks take raw C pointer / `*char` / `size_t`
                // parameter types that the type checker only allows
                // inside another `@extern(C) { ... }` block. Listing
                // them in a regular `module.<.>` popup invites the
                // user to call them where the call would never
                // type-check anyway, so hide them outside extern-C.
                if !in_extern_c && sig.starts_with("@lib(") {
                    return None;
                }
                // Same idea for `@extern(C) { struct / union ... }`
                // declarations whose fields mention raw pointers /
                // `char` / `void` / `size_t`: those fields can't be
                // populated outside an `@extern(C)` block, so a
                // top-level `windows.STARTUPINFOA` candidate would
                // only lead the user into a type error.
                if c_only_structs.contains(suffix) {
                    return None;
                }
                // `@handle pub struct` opaque-pointer types are
                // declaration-side bindings used inside `@extern(C)`
                // blocks. They surface in `module.<.>` listings as
                // plain structs, but offering them as top-level
                // candidates points the user at a name they can only
                // meaningfully use from another extern-C block.
                if !in_extern_c && helpers::is_handle_struct_signature(sig) {
                    return None;
                }
                // Skip sub-module names (`gui.button` etc.) — the
                // loader registers `(module) gui.button` for every
                // sibling file even when `gui.il` only re-exports
                // `pub use button.*` (which flattens the contents
                // and leaves the `gui.button` namespace itself
                // unreachable from the consumer's side).
                let kind = classify_signature_kind(sig);
                if kind == CompletionItemKind::MODULE {
                    return None;
                }
                let (insert_text, fmt) = call_snippet(suffix, kind);
                let command = trigger_sig_help_command(kind);
                let documentation = doc.external.docs.get(k).cloned().map(|d| {
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
        // Sub-module candidates: when the receiver names a path
        // head (e.g. `std.` after `use std.math`), every key under
        // `std.` produces a `math.X` suffix containing `.`, so the
        // direct filter above drops them. Collect the unique head
        // segments of those dotted suffixes as MODULE entries so
        // `std.` offers `math` (and any sibling sub-modules).
        let mut sub_modules: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        for k in doc.external.signatures.keys() {
            if let Some(rest) = k.as_str().strip_prefix(&prefix) {
                if let Some((head, _)) = rest.split_once('.') {
                    sub_modules.insert(head.to_string());
                }
            }
        }
        for h in sub_modules {
            // Skip if already produced as a non-module candidate.
            if items.iter().any(|it| it.label == h) {
                continue;
            }
            items.push(CompletionItem {
                label: h,
                kind: Some(CompletionItemKind::MODULE),
                ..CompletionItem::default()
            });
        }
        items.sort_by(|a, b| a.label.cmp(&b.label));
        if !items.is_empty() {
            return Some(CompletionResponse::Array(items));
        }
        return None;
    }
    let info = doc.classes.get(&AstSymbol::intern(&class_name)).unwrap();
    // `obj.<.>` from outside the class hides non-`pub` members
    // (`_height` etc.). `this.<.>` is treated as inside-the-class
    // access and surfaces everything.
    let outside_class = receiver != "this";
    // Recover the receiver's concrete generic args (`Signal<KeyEvent>`)
    // so the snippet/detail show `fn(KeyEvent)` instead of the
    // declared `fn(T)`.
    let off = text::line_col_to_offset(&doc.text, pos.line + 1, pos.character + 1)
        .unwrap_or(doc.text.len());
    let generic_args: Vec<Type> = resolve_receiver_type(doc, &receiver, off)
        .and_then(|ty| match ty {
            Type::Generic(g) => Some(g.args.to_vec()),
            _ => None,
        })
        .unwrap_or_default();
    let mut items: Vec<CompletionItem> = Vec::new();
    for (name, m) in info.fields.iter() {
        if m.is_static != want_static {
            continue;
        }
        if outside_class && !m.is_pub {
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
        if outside_class && !m.is_pub {
            continue;
        }
        let mut effective_sig = m.signature.clone();
        if !generic_args.is_empty() && !info.type_params.is_empty() {
            crate::signature_help::substitute_type_params_in(
                &mut effective_sig,
                &info.type_params,
                &generic_args,
            );
        }
        let (insert_text, fmt) = build_method_call_snippet(name.as_str(), &effective_sig)
            .map(|(t, f)| (Some(t), Some(f)))
            .unwrap_or_else(|| call_snippet(name.as_str(), CompletionItemKind::METHOD));
        // Re-fire signature help once the snippet expansion lands the
        // cursor inside `(` so the user sees the parameter overlay
        // without having to type `(` themselves.
        let command = Some(tower_lsp::lsp_types::Command {
            title: String::new(),
            command: "editor.action.triggerParameterHints".to_string(),
            arguments: None,
        });
        items.push(CompletionItem {
            label: name.as_str().to_string(),
            kind: Some(CompletionItemKind::METHOD),
            detail: Some(effective_sig),
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


/// Look up the callee identified by `call` and return the bare type
/// name expected at the active argument slot, or `None` when the
/// callee / signature isn't resolvable. Mirrors the lookup order
/// `handle_signature_help` uses so the boost lights up for every
/// callable that gets a signature popup.
fn expected_param_type(doc: &Doc, call: &text::CallContext) -> Option<String> {
    let key = AstSymbol::intern(&call.callee);
    if call.is_new {
        let info = doc.classes.get(&key)?;
        let init = info.inits.first()?;
        return text::nth_param_type_name(&init.signature, call.arg_index);
    }
    let sig: String = if let Some(sym) = doc.symbols.get(&key) {
        sym.signature.clone()
    } else if let Some(s) = doc.external.signatures.get(&key) {
        s.clone()
    } else if let Some(s) = doc.lookup_selective_bare(&call.callee) {
        // `use cocoa { makeWindow }` registers `makeWindow` only in
        // `selective_use_names` — the signature lives under the
        // dotted key (`cocoa.makeWindow`). Walk the external map to
        // recover it. Without this, sig-driven boosting silently
        // gives up on every selectively-imported callable.
        s
    } else if let Some((recv, method)) = call.callee.rsplit_once('.') {
        // Method call: walk the receiver chain through
        // `resolve_receiver_class` and look up `method` on the
        // resolved class. Matches the signature_help path so
        // `this.foo.bar(<here>)` gets the same expected-type
        // treatment as a bare call.
        let class = if recv == "console" {
            Some("Console".to_string())
        } else {
            // The cursor offset for chain resolution doesn't matter
            // here — we just need the receiver's static class. Pass
            // the buffer's end to keep within range.
            resolve_receiver_class(doc, recv, doc.text.len())
        }?;
        let info = doc.classes.get(&AstSymbol::intern(&class))?;
        let m = info.methods.get(&AstSymbol::intern(method))?;
        m.signature.clone()
    } else {
        return None;
    };
    text::nth_param_type_name(&sig, call.arg_index)
}

/// Push items whose declared type or label matches `expected` to
/// the top of the list by stamping a `sortText` prefix. Variables
/// typed as `expected`, the type / enum name itself, and the type's
/// `EnumName.variant` entries all rank above the alphabetic
/// fallback that handles everything else.
fn boost_arg_matches(items: &mut Vec<CompletionItem>, expected: &str, doc: &Doc) {
    for it in items.iter_mut() {
        let label = it.label.as_str();
        let var_match = doc
            .var_classes
            .get(&AstSymbol::intern(label))
            .map(|c| c == expected)
            .unwrap_or(false)
            || doc
                .var_types
                .get(&AstSymbol::intern(label))
                .and_then(|t| match t {
                    Type::Object(n) => Some(n.as_str() == expected),
                    _ => None,
                })
                .unwrap_or(false);
        let name_match = label == expected;
        let bucket = if var_match || name_match { "0_" } else { "9_" };
        it.sort_text = Some(format!("{bucket}{label}"));
    }
}


/// Build a snippet for `name(${1:p1}, ${2:p2}, ...)` from a method's
/// signature string. Parses each parameter slot via
/// `text::parameter_offsets`, takes the bit before the first `:` as
/// the parameter name, and wraps each name in a numbered LSP snippet
/// placeholder so accepting the completion drops the cursor into the
/// first argument with the param name pre-selected. Returns `None`
/// when the signature has no parsable parameter list — the caller
/// falls back to the no-snippet default.
fn build_method_call_snippet(
    name: &str,
    signature: &str,
) -> Option<(String, InsertTextFormat)> {
    let offsets = text::parameter_offsets(signature);
    if offsets.is_empty() {
        return Some((format!("{name}()"), InsertTextFormat::SNIPPET));
    }
    // Every placeholder is `_` — neutral, doesn't trigger VSCode's
    // "select similar identifier" highlight, and signals "fill me
    // in" without prescribing a name (the user can overtype with
    // whatever makes sense for their call site).
    let mut slots: Vec<String> = Vec::with_capacity(offsets.len());
    let mut tab_idx = 1usize;
    for (s, e) in offsets.iter() {
        let slot = signature.get(*s as usize..*e as usize)?;
        let param_ty = slot.split_once(':').map(|(_, t)| t.trim());
        // When the param's type is itself a closure (`fn(T)`),
        // expand to `fn(${1:_}: T) { ${2} }` so the user gets a
        // ready-to-fill lambda instead of having to type the whole
        // `fn(...) { ... }` scaffolding.
        if let Some(inner) = param_ty.and_then(fn_param_type_inner) {
            let inner = inner.trim();
            let body_ret = param_ty.and_then(fn_param_return_type);
            // Pick an initial body literal so the expanded lambda is
            // accept-clean even before the user types anything.
            // Without this, `filter` lands `fn(_: i64) { }` which
            // returns unit and trips the `fn(T): bool` check.
            let body = |idx: usize| match body_ret {
                Some("bool") => format!("${{{idx}:true}}"),
                _ => format!("${{{idx}}}"),
            };
            // Explicit return-type annotation on the closure literal.
            // ilang doesn't infer the return type from the
            // surrounding call site's expected closure type, so a
            // bare `fn(_: T) { true }` for `filter` still trips
            // `fn(T): bool`. Annotate for the concrete primitives
            // where we can spell the type; skip `()` (the default)
            // and anything that looks generic (single uppercase
            // letter) since `: U` wouldn't resolve inside the
            // closure literal.
            let ret_ann = match body_ret {
                Some(r) if needs_explicit_ret_ann(r) => format!(": {r}"),
                _ => String::new(),
            };
            if inner.is_empty() {
                let i = tab_idx;
                tab_idx += 1;
                slots.push(format!("fn(){ret_ann} {{ {} }}", body(i)));
            } else if !inner.contains(',') {
                let i1 = tab_idx;
                let i2 = tab_idx + 1;
                tab_idx += 2;
                slots.push(format!(
                    "fn(${{{}:_}}: {}){ret_ann} {{ {} }}",
                    i1,
                    inner,
                    body(i2),
                ));
            } else {
                // Multi-arg closure — splitting on `,` is unsafe
                // (`Map<K, V>` tears apart). Drop back to a plain
                // `_` slot so the user types the whole closure
                // themselves.
                let i = tab_idx;
                tab_idx += 1;
                slots.push(format!("${{{}:_}}", i));
            }
        } else {
            let i = tab_idx;
            tab_idx += 1;
            slots.push(format!("${{{}:_}}", i));
        }
    }
    Some((
        format!("{name}({})", slots.join(", ")),
        InsertTextFormat::SNIPPET,
    ))
}

/// `true` when the closure literal we synthesise should carry an
/// explicit `: <ret>` annotation. Concrete primitives need it
/// because ilang doesn't propagate the surrounding expected-fn
/// type into the closure body's return-type inference; `()` is the
/// default so an empty body already matches; a bare uppercase
/// letter is a generic param from the outer signature and the
/// closure literal can't name it.
fn needs_explicit_ret_ann(ret: &str) -> bool {
    let r = ret.trim();
    if r.is_empty() || r == "()" {
        return false;
    }
    // Generic-looking single identifier (`T`, `U`, `Key`, …) starts
    // with an uppercase ASCII letter and has no further punctuation.
    // Skip those — emitting `: T` would compile-error inside the
    // closure literal because T isn't in scope.
    let is_word = r
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_');
    let starts_upper = r.chars().next().is_some_and(|c| c.is_ascii_uppercase());
    if is_word && starts_upper {
        return false;
    }
    true
}

/// `Some(ret)` when `ty` is a top-level `fn(...): R` type, where
/// `ret` is the textual return type after the outer `): `. Returns
/// `None` for fn types without a written return (`fn(T)`) and for
/// non-fn types.
fn fn_param_return_type(ty: &str) -> Option<&str> {
    let t = ty.trim();
    let rest = t.strip_prefix("fn(")?;
    let bytes = rest.as_bytes();
    let mut depth = 1i32;
    for (i, b) in bytes.iter().enumerate() {
        match *b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    let after = rest[i + 1..].trim_start();
                    return after.strip_prefix(':').map(str::trim_start);
                }
            }
            _ => {}
        }
    }
    None
}

/// `Some(inner)` when `ty` is a top-level `fn(...)` type, where
/// `inner` is whatever sits between the outer parens. Returns
/// `None` for non-fn types (`i64`, `string`, `Map<K, V>`, ...).
fn fn_param_type_inner(ty: &str) -> Option<&str> {
    let t = ty.trim();
    let rest = t.strip_prefix("fn(")?;
    // Ignore trailing `: RetTy` etc. by chopping at the matching
    // `)` via paren balance — `fn(fn(T))` style nested closures
    // are rare but the balance keeps them parseable.
    let bytes = rest.as_bytes();
    let mut depth = 1i32;
    for (i, b) in bytes.iter().enumerate() {
        match *b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&rest[..i]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod lib_filter_tests {
    use super::*;
    use crate::types::Doc;

    fn doc_with_windows_module() -> Doc {
        use crate::types::{ClassInfo, ClassKind, MemberInfo};
        use ilang_ast::Span;
        use std::collections::HashMap;
        let mut doc = Doc::default();
        doc.text = "use windows\n\nwindows.\n".to_string();
        // Module marker — `analyse_path_to_doc` would normally emit
        // this so `windows` resolves as a receiver in the completion
        // dispatch.
        doc.external.signatures.insert(
            AstSymbol::intern("windows"),
            "(module) windows".to_string(),
        );
        // Two children of the `windows` namespace: one with the
        // `@lib(...)` prefix that the harvest emits for `@extern(C,
        // "kernel32") { @lib pub fn ... }` declarations, and one
        // plain re-export that should always remain visible.
        doc.external.signatures.insert(
            AstSymbol::intern("windows.GetModuleHandleA"),
            "@lib(\"kernel32\")\nfn windows.GetModuleHandleA(lpModuleName: *const char): HMODULE"
                .to_string(),
        );
        doc.external.signatures.insert(
            AstSymbol::intern("windows.WindowsHelper"),
            "fn windows.WindowsHelper(x: i64): i64".to_string(),
        );
        // Two struct entries: STARTUPINFOA holds a `*char` field
        // (C-only — hide from non-extern completion), and PLAIN_RECT
        // is all `i32` (must stay visible).
        doc.external.signatures.insert(
            AstSymbol::intern("windows.STARTUPINFOA"),
            "struct windows.STARTUPINFOA".to_string(),
        );
        doc.external.signatures.insert(
            AstSymbol::intern("windows.PLAIN_RECT"),
            "struct windows.PLAIN_RECT".to_string(),
        );
        let mk_field = |name: &str, ty: Type| -> (AstSymbol, MemberInfo) {
            (
                AstSymbol::intern(name),
                MemberInfo {
                    span: Span::new(1, 1),
                    signature: format!("(property) X.{name}: {ty}"),
                    ret_ty: Some(ty),
                    is_static: false,
                    is_pub: true,
                    doc: None,
                    source_path: None,
                },
            )
        };
        let mut startup_fields = HashMap::new();
        startup_fields.extend([mk_field("cb", Type::U32), mk_field(
            "lpTitle",
            Type::RawPtr { is_const: false, inner: Box::new(Type::CChar) },
        )]);
        doc.classes.insert(
            AstSymbol::intern("kernel32.STARTUPINFOA"),
            ClassInfo {
                decl_span: Span::new(1, 1),
                type_params: Vec::new(),
                fields: startup_fields,
                methods: HashMap::new(),
                getters: HashMap::new(),
                setters: HashMap::new(),
                external: true,
                init_overloads: 0,
                inits: Vec::new(),
                kind: ClassKind::Struct,
            },
        );
        let mut rect_fields = HashMap::new();
        rect_fields.extend([
            mk_field("x", Type::I32),
            mk_field("y", Type::I32),
            mk_field("w", Type::I32),
            mk_field("h", Type::I32),
        ]);
        doc.classes.insert(
            AstSymbol::intern("windef.PLAIN_RECT"),
            ClassInfo {
                decl_span: Span::new(1, 1),
                type_params: Vec::new(),
                fields: rect_fields,
                methods: HashMap::new(),
                getters: HashMap::new(),
                setters: HashMap::new(),
                external: true,
                init_overloads: 0,
                inits: Vec::new(),
                kind: ClassKind::Struct,
            },
        );
        doc.imported_modules.insert(AstSymbol::intern("windows"));
        doc
    }

    fn labels_after_dot(text: &str, after_dot_line: u32, after_dot_col: u32) -> Vec<String> {
        let mut doc = doc_with_windows_module();
        doc.text = text.to_string();
        let resp = handle_completion(
            &doc,
            Position { line: after_dot_line, character: after_dot_col },
        )
        .expect("expected a completion response");
        let items = match resp {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };
        items.into_iter().map(|it| it.label).collect()
    }

    #[test]
    fn lib_fn_hidden_after_windows_dot_at_top_level() {
        // Cursor is at the end of `windows.` on line 3.
        let labels = labels_after_dot("use windows\n\nwindows.\n", 2, 8);
        assert!(
            !labels.iter().any(|l| l == "GetModuleHandleA"),
            "expected @lib fn `GetModuleHandleA` to be hidden outside @extern(C), \
             got: {labels:?}"
        );
        assert!(
            labels.iter().any(|l| l == "WindowsHelper"),
            "non-@lib re-exports must still surface, got: {labels:?}"
        );
    }

    #[test]
    fn lib_fn_visible_after_windows_dot_inside_extern_c() {
        // Same dotted access, but cursor sits inside an
        // `@extern(C) { ... }` block — the @lib fn now belongs.
        let src = "use windows\n@extern(C) {\n    windows.\n}\n";
        let labels = labels_after_dot(src, 2, 12);
        assert!(
            labels.iter().any(|l| l == "GetModuleHandleA"),
            "@lib fn must surface inside @extern(C), got: {labels:?}"
        );
    }

    #[test]
    fn c_only_struct_hidden_after_windows_dot_at_top_level() {
        let labels = labels_after_dot("use windows\n\nwindows.\n", 2, 8);
        assert!(
            !labels.iter().any(|l| l == "STARTUPINFOA"),
            "STARTUPINFOA (has `*char` field) must be hidden outside \
             @extern(C), got: {labels:?}"
        );
        assert!(
            labels.iter().any(|l| l == "PLAIN_RECT"),
            "PLAIN_RECT (only i32 fields) must stay visible, got: {labels:?}"
        );
    }

    #[test]
    fn c_only_struct_visible_after_windows_dot_inside_extern_c() {
        let src = "use windows\n@extern(C) {\n    windows.\n}\n";
        let labels = labels_after_dot(src, 2, 12);
        assert!(
            labels.iter().any(|l| l == "STARTUPINFOA"),
            "C-only struct must surface inside @extern(C), got: {labels:?}"
        );
    }

    #[test]
    fn array_filter_completion_seeds_true_body() {
        // `b.filter` needs a `fn(T): bool` closure. Seed the body
        // with `true` so accepting the completion produces a
        // type-checkable lambda, not an empty-body unit lambda
        // that fails the return-type check.
        use std::collections::HashMap;
        let mut doc = Doc::default();
        doc.text = "let b: i64[] = []\nb.\n".to_string();
        doc.var_types = HashMap::new();
        doc.var_types.insert(
            AstSymbol::intern("b"),
            Type::Array { elem: Box::new(Type::I64), fixed: None },
        );
        let pos = Position { line: 1, character: 2 };
        let resp = handle_completion(&doc, pos)
            .expect("expected a completion response for `b.`");
        let items = match resp {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };
        let filter = items
            .iter()
            .find(|it| it.label == "filter")
            .expect("filter must be in the candidates");
        let snippet = filter
            .insert_text
            .as_ref()
            .expect("filter completion must carry a snippet");
        assert!(
            snippet.contains("true"),
            "filter body must seed a bool literal so the lambda \
             returns the expected `bool`, got: {snippet}"
        );
        assert!(
            snippet.contains("): bool"),
            "filter closure must carry an explicit `: bool` return \
             annotation — ilang doesn't infer it from the call site, \
             got: {snippet}"
        );
    }

    #[test]
    fn array_for_each_completion_expands_lambda_snippet() {
        // Typing `b.` for `let b: i64[] = []` should offer `forEach`
        // with a snippet that drops the cursor into a pre-built
        // `fn(${1:_}: i64) { ${2} }` body — same expansion the
        // user-defined-method path already provides.
        use std::collections::HashMap;
        let mut doc = Doc::default();
        doc.text = "let b: i64[] = []\nb.\n".to_string();
        doc.var_types = HashMap::new();
        doc.var_types.insert(
            AstSymbol::intern("b"),
            Type::Array { elem: Box::new(Type::I64), fixed: None },
        );
        let pos = Position { line: 1, character: 2 };
        let resp = handle_completion(&doc, pos)
            .expect("expected a completion response for `b.`");
        let items = match resp {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };
        let for_each = items
            .iter()
            .find(|it| it.label == "forEach")
            .expect("forEach must be in the candidates");
        assert_eq!(
            for_each.insert_text_format,
            Some(InsertTextFormat::SNIPPET),
            "forEach must be inserted as a SNIPPET so placeholders are honoured"
        );
        let snippet = for_each
            .insert_text
            .as_ref()
            .expect("forEach completion must carry a snippet");
        assert!(
            snippet.contains("fn(") && snippet.contains("i64"),
            "snippet should pre-build a `fn(_: i64) {{ }}` lambda, got: {snippet}"
        );
    }

    #[test]
    fn primitive_receiver_surfaces_to_string() {
        // `let a: i64 = 0\na.` — the receiver is a numeric primitive,
        // so completion should at minimum suggest `toString` (which
        // the type checker accepts on every numeric / bool value).
        // Before the primitive_method_* hookup the dispatch fell
        // through to the empty case and the user saw nothing.
        use std::collections::HashMap;
        let mut doc = Doc::default();
        doc.text = "let a: i64 = 0\na.\n".to_string();
        doc.var_types = HashMap::new();
        doc.var_types.insert(AstSymbol::intern("a"), Type::I64);
        let pos = Position { line: 1, character: 2 };
        let resp = handle_completion(&doc, pos)
            .expect("expected a completion response for `a.`");
        let items = match resp {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };
        let labels: Vec<&str> = items.iter().map(|it| it.label.as_str()).collect();
        assert!(
            labels.iter().any(|l| *l == "toString"),
            "expected `toString` in i64 receiver completion, got: {labels:?}"
        );
    }

    #[test]
    fn c_only_struct_hidden_in_type_position_at_top_level() {
        // `let x: <here>` — VSCode invokes completion in a type
        // position, which goes through `type_completions`. Dotted
        // labels like `windows.STARTUPINFOA` flow through this path
        // separately from the value-position bare list.
        let doc = doc_with_windows_module();
        let mut local = doc.clone();
        local.text = "use windows\nlet x: \n".to_string();
        let pos = Position { line: 1, character: 7 };
        let resp = handle_completion(&local, pos).expect("type completion response");
        let items = match resp {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };
        let labels: Vec<&str> = items.iter().map(|it| it.label.as_str()).collect();
        assert!(
            !labels.iter().any(|l| *l == "windows.STARTUPINFOA"),
            "type-position completion must hide C-only struct outside \
             @extern(C), got: {labels:?}"
        );
        assert!(
            labels.iter().any(|l| *l == "windows.PLAIN_RECT"),
            "plain C struct must stay visible in type position, got: {labels:?}"
        );
    }
}
