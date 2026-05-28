//! Argument-aware completion helpers: expected-parameter-type lookup
//! for boosting matching arg candidates, and the method-call snippet
//! builder used when completing `obj.method` into a call template.

use ilang_ast::{Symbol as AstSymbol, Type};
use tower_lsp::lsp_types::{CompletionItem, InsertTextFormat};

use crate::completion::call_snippet;
use crate::text;
use crate::Doc;

use super::resolve_receiver_class;

/// Look up the callee identified by `call` and return the bare type
/// name expected at the active argument slot, or `None` when the
/// callee / signature isn't resolvable. Mirrors the lookup order
/// `handle_signature_help` uses so the boost lights up for every
/// callable that gets a signature popup.
pub(super) fn expected_param_type(doc: &Doc, call: &text::CallContext) -> Option<String> {
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
pub(super) fn boost_arg_matches(items: &mut Vec<CompletionItem>, expected: &str, doc: &Doc) {
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
pub(super) fn build_method_call_snippet(
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

