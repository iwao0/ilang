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
    array_method_doc, array_method_names, array_method_sig,
    float_prim_const_doc, float_prim_const_names, float_prim_const_sig,
    int_prim_const_doc, int_prim_const_names, int_prim_const_sig,
    map_method_doc, map_method_names, map_method_sig,
    primitive_method_doc, primitive_method_names, primitive_method_sig,
    set_method_doc, set_method_names, set_method_sig,
    string_method_doc, string_method_names, string_method_sig,
    string_static_method_doc, string_static_method_names, string_static_method_sig,
};
use crate::code_actions::interface_method_stub_completions_textual;
use crate::helpers::{self, sig_body_skip_attrs};
use crate::symbols::is_synthesized_objc_helper;
use crate::text::{self, receiver_before_dot};
use crate::Doc;

mod snippet;
#[cfg(test)]
mod tests;

use snippet::{boost_arg_matches, build_method_call_snippet, expected_param_type};

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
    // Cursor byte offset, computed once and shared by every context
    // probe below — each `at_*` / `use_path_prefix_at` check needs it,
    // and `line_col_to_offset` would otherwise rescan the buffer per
    // call.
    let off = text::line_col_to_offset(&doc.text, pos.line + 1, pos.character + 1)
        .unwrap_or(doc.text.len());
    // `use M as |` — the alias name is a fresh binder; offering any
    // visible identifier just invites shadowing. Return an empty
    // list so VSCode's word-based fallback doesn't fill in.
    if at_use_alias_position(&doc.text, off) {
        return Some(CompletionResponse::Array(Vec::new()));
    }
    // `use <ident>` or `use a.b.<ident>` — the cursor is on a
    // segment of the module path. This branch fires before the
    // ordinary dot-receiver dispatch because `use std.<cursor>`
    // would otherwise route through "receiver = std" and try to
    // resolve it as a class / variable.
    {
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
        resolve_receiver_class(doc, &receiver, off).unwrap_or_default()
    };
    if doc.classes.get(&AstSymbol::intern(&class_name)).is_none() {
        // `f32.` / `f64.` — primitive-type associated constants live
        // outside the regular `var_types` path because the receiver
        // is a type name, not a value. Emit the constant list before
        // the inferred-type fallthrough so the user gets `NaN` /
        // `Infinity` / ... immediately after the dot.
        if receiver == "f32" || receiver == "f64" {
            let items: Vec<CompletionItem> = float_prim_const_names()
                .iter()
                .filter_map(|n| {
                    let sig = float_prim_const_sig(receiver.as_str(), n)?;
                    let doc_text = float_prim_const_doc(n);
                    let mut item = CompletionItem {
                        label: n.to_string(),
                        kind: Some(CompletionItemKind::CONSTANT),
                        detail: Some(sig),
                        documentation: doc_text.map(|d| {
                            Documentation::MarkupContent(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: d.to_string(),
                            })
                        }),
                        ..Default::default()
                    };
                    item.insert_text = Some(n.to_string());
                    Some(item)
                })
                .collect();
            return Some(CompletionResponse::Array(items));
        }
        // `string.` — list the static factories registered against
        // the primitive `string` type. Mirrors the type-checker's
        // `name == "string"` static-dispatch arm in
        // `crates/ilang-types/src/checker/expr/calls.rs`. Skip the
        // static branch when a local `string` shadows the type
        // name; the inferred-type path below will take over and
        // emit the instance-method list for that local.
        let string_shadowed = doc
            .var_types
            .contains_key(&AstSymbol::intern("string"));
        if receiver == "string" && !string_shadowed {
            let items: Vec<CompletionItem> = string_static_method_names()
                .iter()
                .filter_map(|n| {
                    let sig = string_static_method_sig(n)?;
                    let doc_text = string_static_method_doc(n);
                    let (insert_text, fmt) = call_snippet(n, CompletionItemKind::METHOD);
                    let command = Some(tower_lsp::lsp_types::Command {
                        title: String::new(),
                        command: "editor.action.triggerParameterHints".to_string(),
                        arguments: None,
                    });
                    Some(CompletionItem {
                        label: n.to_string(),
                        kind: Some(CompletionItemKind::METHOD),
                        detail: Some(sig),
                        documentation: doc_text.map(|d| {
                            Documentation::MarkupContent(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: d.to_string(),
                            })
                        }),
                        insert_text,
                        insert_text_format: fmt,
                        command,
                        ..Default::default()
                    })
                })
                .collect();
            return Some(CompletionResponse::Array(items));
        }
        if matches!(
            receiver.as_str(),
            "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64"
        ) {
            let items: Vec<CompletionItem> = int_prim_const_names()
                .iter()
                .filter_map(|n| {
                    let sig = int_prim_const_sig(receiver.as_str(), n)?;
                    let doc_text = int_prim_const_doc(n);
                    let mut item = CompletionItem {
                        label: n.to_string(),
                        kind: Some(CompletionItemKind::CONSTANT),
                        detail: Some(sig),
                        documentation: doc_text.map(|d| {
                            Documentation::MarkupContent(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: d.to_string(),
                            })
                        }),
                        ..Default::default()
                    };
                    item.insert_text = Some(n.to_string());
                    Some(item)
                })
                .collect();
            return Some(CompletionResponse::Array(items));
        }
        // Built-in receiver: string / array. Their member sets are
        // hardcoded — list them from the same helpers used by hover.
        // String literal (`"abc".`) flows in via a sentinel
        // receiver; treat it as `Type::Str` directly.
        let inferred_ty: Option<Type> = if receiver == text::STR_LITERAL_RECEIVER {
            Some(Type::Str)
        } else if receiver == text::FLOAT_LITERAL_RECEIVER {
            // `(1.0).` — parenthesised float literal. Float literals
            // default to f64 in ilang, so list the f64 primitive
            // methods (`toString` / `isFinite` / `isNaN`).
            Some(Type::F64)
        } else if receiver == text::INT_LITERAL_RECEIVER {
            // `(1).` — parenthesised int literal. Surface as i64 (the
            // default int type); the only numeric primitive method is
            // `toString`, so width-specific accuracy doesn't change
            // the listed candidates.
            Some(Type::I64)
        } else if receiver == text::BOOL_LITERAL_RECEIVER {
            // `(true).` / `(false).` — bool literal. `toString` is the
            // sole bool primitive method.
            Some(Type::Bool)
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
                Type::Generic(g)
                    if g.base.as_str() == "Set" && g.args.len() == 1 =>
                {
                    set_method_names()
                        .into_iter()
                        .filter_map(|n| {
                            set_method_sig(n, &g.args[0])
                                .map(|s| (n.to_string(), s, set_method_doc(n)))
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
