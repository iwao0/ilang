//! `textDocument/signatureHelp` orchestration — peek at the source
//! around the cursor, figure out whether we're inside generic args
//! (`Map<…>`) or a call's argument list, and produce the matching
//! `SignatureHelp` payload. Extracted from `handlers.rs`.

use ilang_ast::{Span, Symbol as AstSymbol, Type};
use tower_lsp::lsp_types::{
    Documentation, MarkupContent, MarkupKind, ParameterInformation, ParameterLabel, Position,
    SignatureHelp, SignatureInformation,
};

use crate::builtins::{
    array_method_doc, array_method_sig, ffi_helper_signature, primitive_method_doc,
    primitive_method_sig, string_method_doc, string_method_sig,
};
use crate::completion;
use crate::text::{self, call_context_at, generic_args_context_at, parameter_offsets};
use crate::types::{Doc, MemberInfo};

pub(crate) fn handle_signature_help(doc: &Doc, pos: Position) -> Option<SignatureHelp> {
    if let Some(gc) = generic_args_context_at(&doc.text, pos) {
        let label = format!("{}<{}>", gc.type_name, gc.type_params.join(", "));
        let params: Vec<ParameterInformation> = gc
            .type_params
            .iter()
            .map(|p| ParameterInformation {
                label: ParameterLabel::Simple((*p).to_string()),
                documentation: None,
            })
            .collect();
        let remaining = gc.type_params.len().saturating_sub(gc.arg_index);
        let suffix = if gc.arg_index >= gc.type_params.len() {
            format!("All {} generic argument(s) supplied.", gc.type_params.len())
        } else if remaining == 1 {
            "1 more generic argument required.".to_string()
        } else {
            format!("{remaining} more generic arguments required.")
        };
        let doc_value = match gc.short_doc {
            Some(d) => format!("{d}\n\n{suffix}"),
            None => suffix,
        };
        let active = gc
            .arg_index
            .min(gc.type_params.len().saturating_sub(1)) as u32;
        return Some(SignatureHelp {
            signatures: vec![SignatureInformation {
                label,
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: doc_value,
                })),
                parameters: Some(params),
                active_parameter: Some(active),
            }],
            active_signature: Some(0),
            active_parameter: Some(active),
        });
    }
    let call = call_context_at(&doc.text, pos)?;
    // `new ClassName(...)` -> the class's init overloads.
    let sigs: Vec<MemberInfo> = if call.is_new {
        doc.classes
            .get(&AstSymbol::intern(&call.callee))
            .map(|i| i.inits.clone())
            .unwrap_or_default()
    } else {
        // Plain function call. Top-level fn or imported (dotted)
        // fn — we already have signatures stashed by name.
        let mut out: Vec<MemberInfo> = Vec::new();
        if let Some(sym) = doc.symbols.get(&AstSymbol::intern(&call.callee)) {
            out.push(MemberInfo {
                span: sym.span,
                signature: sym.signature.clone(),
                ret_ty: None,
                is_static: false,
                is_pub: true,
                doc: None,
                source_path: None,
            });
        } else if let Some(sig) = ffi_helper_signature(&call.callee) {
            out.push(MemberInfo {
                span: Span::dummy(),
                signature: sig.to_string(),
                ret_ty: None,
                is_static: false,
                is_pub: true,
                doc: None,
                source_path: None,
            });
        } else if let Some(s) = doc.external.signatures.get(&AstSymbol::intern(&call.callee)) {
            out.push(MemberInfo {
                span: Span::dummy(),
                signature: s.clone(),
                ret_ty: None,
                is_static: false,
                is_pub: true,
                doc: None,
                source_path: None,
            });
        } else if let Some(s) = aliased_external_signature(doc, &call.callee) {
            // `use std.math as math` + `math.abs(` — the buffer says
            // `math.abs` but external_signatures keys the item under
            // its canonical dotted path (`std.math.abs`). Translate
            // the receiver via `module_aliases` and retry.
            out.push(MemberInfo {
                span: Span::dummy(),
                signature: s,
                ret_ty: None,
                is_static: false,
                is_pub: true,
                doc: None,
                source_path: None,
            });
        } else if let Some(s) = doc.lookup_selective_bare(&call.callee) {
            // `use cocoa { makeWindow }` registers `makeWindow` only
            // in `selective_use_names`; the signature lives under
            // the dotted key (`cocoa.makeWindow`). Without this
            // fallback signatureHelp drops the parameter overlay
            // for every selectively-imported callable.
            out.push(MemberInfo {
                span: Span::dummy(),
                signature: s,
                ret_ty: None,
                is_static: false,
                is_pub: true,
                doc: None,
                source_path: None,
            });
        } else if let Some((recv, method)) = call.callee.rsplit_once('.') {
            // Method call: `obj.method(`. Walk the (possibly dotted)
            // receiver via `resolve_receiver_class` so chains like
            // `this.starTex.update(` resolve through the field's
            // declared type, not just a single `var_classes` hop.
            // Falls back to the built-in string / array signatures
            // below when the receiver is one of those primitives.
            let off = text::line_col_to_offset(
                &doc.text,
                pos.line + 1,
                pos.character + 1,
            )
            .unwrap_or(doc.text.len());
            let class = if recv == "console" {
                Some("Console".to_string())
            } else {
                completion::resolve_receiver_class(doc, recv, off)
            };
            // Recover the receiver's full type so a `Signal<CloseEvent>`
            // can substitute `T -> CloseEvent` into the member's
            // signature instead of showing the raw `fn(T)`.
            let recv_ty = if recv == "console" {
                None
            } else {
                completion::resolve_receiver_type(doc, recv, off)
            };
            if let Some(c) = class {
                if let Some(info) = doc.classes.get(&AstSymbol::intern(&c)) {
                    if let Some(m) = info.methods.get(&AstSymbol::intern(method)) {
                        let mut m = m.clone();
                        if let Some(generic_args) = recv_ty.as_ref().and_then(generic_args_of) {
                            substitute_type_params(
                                &mut m.signature,
                                &info.type_params,
                                &generic_args,
                            );
                        }
                        out.push(m);
                    }
                }
            }
            if out.is_empty() {
                let inferred_recv_ty: Option<Type> = if recv == text::STR_LITERAL_RECEIVER {
                    Some(Type::Str)
                } else {
                    doc.var_types.get(&AstSymbol::intern(recv)).cloned()
                };
                let builtin = match inferred_recv_ty.as_ref() {
                    Some(Type::Str) => string_method_sig(method)
                        .map(|s| (s, string_method_doc(method))),
                    Some(Type::Array { elem, .. }) => array_method_sig(method, elem)
                        .map(|s| (s, array_method_doc(method))),
                    // Numeric primitives + bool: `toString` is the
                    // only built-in method. Surface its signature so
                    // the popup fires the same way as on strings /
                    // arrays.
                    Some(t) if t.is_numeric() || matches!(t, Type::Bool) => {
                        primitive_method_sig(method, t)
                            .map(|s| (s, primitive_method_doc(method)))
                    }
                    _ => None,
                };
                if let Some((sig, doc_text)) = builtin {
                    out.push(MemberInfo {
                        span: Span::dummy(),
                        signature: sig,
                        ret_ty: None,
                        is_static: false,
                        is_pub: true,
                        doc: doc_text.map(|s| s.to_string()),
                        source_path: None,
                    });
                }
            }
        }
        out
    };
    if sigs.is_empty() {
        return None;
    }
    // Filter: once the user has typed any `,`s, drop overloads whose
    // parameter count can't reach the cursor's position. arg_index
    // == 0 keeps every overload.
    let mut chosen: Vec<&MemberInfo> = sigs
        .iter()
        .filter(|m| {
            let n = parameter_offsets(&m.signature).len();
            call.arg_index == 0 || n > call.arg_index
        })
        .collect();
    if chosen.is_empty() {
        chosen = sigs.iter().collect();
    }
    let signatures: Vec<SignatureInformation> = chosen
        .iter()
        .map(|m| {
            let params = parameter_offsets(&m.signature)
                .into_iter()
                .map(|(s, e)| ParameterInformation {
                    label: ParameterLabel::LabelOffsets([s, e]),
                    documentation: None,
                })
                .collect::<Vec<_>>();
            SignatureInformation {
                label: m.signature.clone(),
                documentation: None,
                parameters: if params.is_empty() { None } else { Some(params) },
                active_parameter: None,
            }
        })
        .collect();
    Some(SignatureHelp {
        signatures,
        active_signature: Some(0),
        active_parameter: Some(call.arg_index as u32),
    })
}


/// Resolve a dotted callee whose head is a `use ... as <alias>`
/// alias to its canonical external-signatures key. `math.abs` after
/// `use std.math as math` becomes `std.math.abs`; the head is
/// matched against `doc.module_aliases`. Returns `None` when the
/// head isn't an alias.
fn aliased_external_signature(doc: &Doc, callee: &str) -> Option<String> {
    let (head, rest) = callee.split_once('.')?;
    let canonical = doc
        .module_aliases
        .get(&AstSymbol::intern(head))?
        .as_str()
        .to_string();
    let key = AstSymbol::intern(&format!("{canonical}.{rest}"));
    doc.external.signatures.get(&key).cloned()
}

/// Pull the generic-argument list out of a receiver type if it
/// happens to be a `Type::Generic` instantiation. Anything else
/// (plain `Object`, `Array`, primitives, …) returns `None` so the
/// caller can skip substitution.
fn generic_args_of(ty: &Type) -> Option<Vec<Type>> {
    match ty {
        Type::Generic(g) => Some(g.args.to_vec()),
        _ => None,
    }
}

/// Replace every `\bT\b` (and other parameter names) in `sig` with
/// the corresponding concrete type. Walks character-by-character so
/// substrings inside larger identifiers (`Tuple`, `Result`) stay
/// untouched. `params` and `args` are zipped pairwise; surplus
/// entries on either side are silently skipped.
pub(crate) fn substitute_type_params_in(
    sig: &mut String,
    params: &[String],
    args: &[Type],
) {
    substitute_type_params(sig, params, args);
}

fn substitute_type_params(sig: &mut String, params: &[String], args: &[Type]) {
    if params.is_empty() || args.is_empty() {
        return;
    }
    let n = params.len().min(args.len());
    for i in 0..n {
        let name = &params[i];
        let replacement = format!("{}", args[i]);
        if name == &replacement {
            continue;
        }
        *sig = replace_whole_word(sig, name, &replacement);
    }
}

/// Replace every whole-word occurrence of `needle` in `src` with
/// `repl`. A "word" boundary is anything that isn't an ASCII letter,
/// digit, or `_`. Avoids touching `Tuple` when `T` is the needle.
///
/// The needle is always an ASCII identifier (a type-parameter name
/// like `T` or `K`), so byte-level scanning safely lines up with
/// `char` boundaries — but the surrounding source can contain
/// multi-byte UTF-8 (e.g. Japanese inside a doc comment), so we copy
/// pass-through chunks via string slicing rather than reinterpreting
/// each byte as a `char`.
fn replace_whole_word(src: &str, needle: &str, repl: &str) -> String {
    let bytes = src.as_bytes();
    let needle_bytes = needle.as_bytes();
    if needle_bytes.is_empty() || needle_bytes.len() > bytes.len() {
        return src.to_string();
    }
    let mut out = String::with_capacity(src.len());
    let mut copied = 0;
    let mut i = 0;
    while i + needle_bytes.len() <= bytes.len() {
        if &bytes[i..i + needle_bytes.len()] == needle_bytes {
            let before_ok = i == 0 || {
                let b = bytes[i - 1];
                !(b.is_ascii_alphanumeric() || b == b'_')
            };
            let after_ok = i + needle_bytes.len() == bytes.len() || {
                let b = bytes[i + needle_bytes.len()];
                !(b.is_ascii_alphanumeric() || b == b'_')
            };
            if before_ok && after_ok {
                out.push_str(&src[copied..i]);
                out.push_str(repl);
                i += needle_bytes.len();
                copied = i;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&src[copied..]);
    out
}

#[cfg(test)]
mod tests {
    use super::replace_whole_word;

    #[test]
    fn replace_whole_word_basic() {
        assert_eq!(replace_whole_word("fn f(x: T): T", "T", "i64"), "fn f(x: i64): i64");
    }

    #[test]
    fn replace_whole_word_respects_identifier_boundary() {
        // `Tuple` must not be touched when `T` is the needle.
        assert_eq!(
            replace_whole_word("fn f(x: Tuple): T", "T", "i64"),
            "fn f(x: Tuple): i64"
        );
    }

    #[test]
    fn replace_whole_word_preserves_non_ascii() {
        // Doc-comment content with multi-byte characters must round-trip
        // intact — the old byte-at-a-time `push(b as char)` corrupted
        // these.
        let src = "fn f(x: T) // 日本語コメント";
        let got = replace_whole_word(src, "T", "i64");
        assert_eq!(got, "fn f(x: i64) // 日本語コメント");
    }

    #[test]
    fn replace_whole_word_no_match_returns_clone() {
        let src = "abc 日本語 def";
        assert_eq!(replace_whole_word(src, "X", "Y"), src);
    }
}
