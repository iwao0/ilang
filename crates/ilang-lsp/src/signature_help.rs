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
    array_method_doc, array_method_sig, ffi_helper_signature, string_method_doc,
    string_method_sig,
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
                doc: None,
                source_path: None,
            });
        } else if let Some(sig) = ffi_helper_signature(&call.callee) {
            out.push(MemberInfo {
                span: Span::dummy(),
                signature: sig.to_string(),
                ret_ty: None,
                is_static: false,
                doc: None,
                source_path: None,
            });
        } else if let Some(s) = doc.external_signatures.get(&AstSymbol::intern(&call.callee)) {
            out.push(MemberInfo {
                span: Span::dummy(),
                signature: s.clone(),
                ret_ty: None,
                is_static: false,
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
            let class = if recv == "console" {
                Some("Console".to_string())
            } else {
                let off = text::line_col_to_offset(
                    &doc.text,
                    pos.line + 1,
                    pos.character + 1,
                )
                .unwrap_or(doc.text.len());
                completion::resolve_receiver_class(doc, recv, off)
            };
            if let Some(c) = class {
                if let Some(info) = doc.classes.get(&AstSymbol::intern(&c)) {
                    if let Some(m) = info.methods.get(&AstSymbol::intern(method)) {
                        out.push(m.clone());
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
                    _ => None,
                };
                if let Some((sig, doc_text)) = builtin {
                    out.push(MemberInfo {
                        span: Span::dummy(),
                        signature: sig,
                        ret_ty: None,
                        is_static: false,
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

