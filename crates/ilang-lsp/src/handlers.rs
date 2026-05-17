//! Extracted from `main.rs`.
#![allow(unused_imports)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};



use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use ilang_ast::{
    Block, ClassDecl, EnumDecl, Expr, ExprKind, FnDecl, Item, Param, Pattern, PatternBindings,
    PatternKind, Program, Span, Stmt, StmtKind, Symbol as AstSymbol, Type, VariantPayload,
};
use ilang_parser::parse as parse_program;
use ilang_types::{check, TypeError};

use crate::*;

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> LspResult<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".to_string(), "@".to_string()]),
                    ..CompletionOptions::default()
                }),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: None,
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
                document_formatting_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Options(
                    CodeActionOptions {
                        code_action_kinds: Some(vec![
                            CodeActionKind::SOURCE_ORGANIZE_IMPORTS,
                            CodeActionKind::QUICKFIX,
                        ]),
                        work_done_progress_options: WorkDoneProgressOptions::default(),
                        resolve_provider: Some(false),
                    },
                )),
                ..ServerCapabilities::default()
            },
            server_info: Some(ServerInfo {
                name: "ilang-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "ilang-lsp ready")
            .await;
    }

    async fn did_open(&self, p: DidOpenTextDocumentParams) {
        self.refresh(p.text_document.uri, p.text_document.text).await;
    }

    async fn did_change(&self, mut p: DidChangeTextDocumentParams) {
        let Some(change) = p.content_changes.pop() else { return };
        let uri = p.text_document.uri.clone();
        let version = p.text_document.version;
        // Record this as the latest version for `uri`. Spawned tasks
        // compare against the stored value after their debounce
        // timer fires and bail if a newer change has arrived.
        {
            let mut versions = self.latest_versions.lock().unwrap();
            versions.insert(uri.clone(), version);
        }
        // Update `doc.text` immediately so cursor-context queries
        // (`@` attribute completion, `.` member completion) see the
        // characters the user just typed even when the heavier
        // index rebuild is still debounced. Symbol / class data
        // stays stale until the spawned refresh fires, which is
        // fine — that's just member listings.
        {
            let mut docs = self.docs.lock().unwrap();
            let entry = docs.entry(uri.clone()).or_default();
            entry.text = change.text.clone();
        }
        let client = self.client.clone();
        let docs = self.docs.clone();
        let versions = self.latest_versions.clone();
        let text = change.text;
        tokio::spawn(async move {
            // Coalesce bursts: a 120 ms idle window catches the
            // typical "user is typing" rate while still feeling
            // immediate when they stop.
            tokio::time::sleep(Duration::from_millis(120)).await;
            {
                let v = versions.lock().unwrap();
                if v.get(&uri).copied() != Some(version) {
                    return;
                }
            }
            refresh_impl(&client, &docs, uri.clone(), text).await;
            // Refresh every other open .il document too — a
            // change in module M (e.g. adding `pub` to one of
            // its members) silently invalidates diagnostics in
            // every file that imports M. Without this re-run,
            // the dependents keep stale red squiggles until the
            // user touches each file.
            let other_docs: Vec<(Url, String)> = {
                let lock = docs.lock().unwrap();
                lock.iter()
                    .filter(|(u, _)| **u != uri)
                    .map(|(u, d)| (u.clone(), d.text.clone()))
                    .collect()
            };
            for (other_uri, other_text) in other_docs {
                refresh_impl(&client, &docs, other_uri, other_text).await;
            }
        });
    }

    async fn did_close(&self, p: DidCloseTextDocumentParams) {
        let mut docs = self.docs.lock().unwrap();
        docs.remove(&p.text_document.uri);
    }

    async fn hover(&self, p: HoverParams) -> LspResult<Option<Hover>> {
        let uri = p.text_document_position_params.text_document.uri;
        let pos = p.text_document_position_params.position;
        let docs = self.docs.lock().unwrap();
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };
        if let Some(entry) = lookup_ref(doc, pos) {
            return Ok(Some(make_hover_with_doc(
                &entry.signature,
                entry.doc.as_deref(),
            )));
        }
        if let Some((word, _)) = word_at(&doc.text, pos) {
            let key = AstSymbol::intern(&word);
            if let Some(sym) = doc.symbols.get(&key) {
                return Ok(Some(make_hover_with_doc(
                    &sym.signature,
                    sym.doc.as_deref(),
                )));
            }
            // Selectively-imported bare name (`use M { X }`) — the
            // signature lives in `external_signatures` keyed by the
            // bare name, mirroring the buffer-local index.
            if let Some(sig) = doc.external_signatures.get(&key) {
                return Ok(Some(make_hover_with_doc(
                    sig,
                    doc.external_docs.get(&key).map(|s| s.as_str()),
                )));
            }
        }
        Ok(None)
    }

    async fn goto_definition(
        &self,
        p: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri = p.text_document_position_params.text_document.uri;
        let pos = p.text_document_position_params.position;
        let docs = self.docs.lock().unwrap();
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };
        if let Some(entry) = lookup_ref(doc, pos) {
            if let Some(target_uri) = entry.target_uri.clone() {
                let range = span_to_range(entry.target_span, entry.target_name_len as usize);
                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri: target_uri,
                    range,
                })));
            }
            if entry.no_definition {
                return Ok(None);
            }
            let range = span_to_range(entry.target_span, entry.target_name_len as usize);
            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                uri,
                range,
            })));
        }
        if let Some((word, _)) = word_at(&doc.text, pos) {
            let key = AstSymbol::intern(&word);
            if let Some(sym) = doc.symbols.get(&key) {
                let range = span_to_range(sym.span, sym.name.as_str().len());
                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri,
                    range,
                })));
            }
            // Selectively-imported bare type / fn (`use M { X }`) —
            // `external_sources` carries the file path + decl span
            // for a cross-file jump.
            if let Some(loc) = doc.external_sources.get(&key) {
                if let Ok(target_uri) = Url::from_file_path(&loc.path) {
                    let range = span_to_range(loc.span, loc.name_len as usize);
                    return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                        uri: target_uri,
                        range,
                    })));
                }
            }
        }
        Ok(None)
    }

    async fn completion(&self, p: CompletionParams) -> LspResult<Option<CompletionResponse>> {
        let uri = p.text_document_position.text_document.uri;
        let pos = p.text_document_position.position;
        let docs = self.docs.lock().unwrap();
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };
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
                return Ok(Some(CompletionResponse::Array(Vec::new())));
            }
            // `@x` -> attribute completion.
            if at_attribute_position(&doc.text, off) {
                return Ok(Some(CompletionResponse::Array(attribute_completions())));
            }
            // After `:` we're in a type position — only suggest types.
            if at_type_position(&doc.text, off) {
                return Ok(Some(CompletionResponse::Array(type_completions(doc))));
            }
            let at_top_level = brace_depth_at(&doc.text, off) <= 0;
            let mut items = global_completions(doc, at_top_level);
            if in_extern_c_block(&doc.text, off) {
                push_ffi_helper_completions(&mut items);
                push_extern_c_keywords(&mut items);
            }
            return Ok(Some(CompletionResponse::Array(items)));
        };
        // Receiver can be:
        // - a class name (`Counter.`)        -> static members
        // - a variable typed as some class (`c.`) -> instance members
        // Anything else falls through and we return nothing.
        let want_static = doc.classes.contains_key(&AstSymbol::intern(&receiver));
        let class_name = if want_static {
            receiver.clone()
        } else if receiver == "console" {
            // Built-in singleton: instance of `Console`.
            "Console".to_string()
        } else {
            doc.var_classes.get(&AstSymbol::intern(&receiver)).cloned().unwrap_or_default()
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
                    return Ok(Some(CompletionResponse::Array(items)));
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
                return Ok(Some(CompletionResponse::Array(items)));
            }
            return Ok(None);
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
            if crate::symbols::is_synthesized_objc_helper(name.as_str()) {
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
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn signature_help(
        &self,
        p: SignatureHelpParams,
    ) -> LspResult<Option<SignatureHelp>> {
        let uri = p.text_document_position_params.text_document.uri;
        let pos = p.text_document_position_params.position;
        let docs = self.docs.lock().unwrap();
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };
        let Some(call) = call_context_at(&doc.text, pos) else {
            return Ok(None);
        };
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
                });
            } else if let Some(sig) = ffi_helper_signature(&call.callee) {
                out.push(MemberInfo {
                    span: Span::dummy(),
                    signature: sig.to_string(),
                    ret_ty: None,
                    is_static: false,
                    doc: None,
                });
            } else if let Some(s) = doc.external_signatures.get(&AstSymbol::intern(&call.callee)) {
                out.push(MemberInfo {
                    span: Span::dummy(),
                    signature: s.clone(),
                    ret_ty: None,
                    is_static: false,
                    doc: None,
                });
            } else if let Some((recv, method)) = call.callee.rsplit_once('.') {
                // Method call: `obj.method(`. Resolve the receiver to a
                // class (instance, class name, or `console` singleton),
                // then look up the method on that class. Fall back to
                // built-in string / array signatures when the receiver
                // is one of those primitives.
                let class = if doc.classes.contains_key(&AstSymbol::intern(recv)) {
                    Some(recv.to_string())
                } else if recv == "console" {
                    Some("Console".to_string())
                } else {
                    doc.var_classes.get(&AstSymbol::intern(recv)).cloned()
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
                        });
                    }
                }
            }
            out
        };
        if sigs.is_empty() {
            return Ok(None);
        }
        // Filter: once the user has typed any `,`s, drop overloads
        // whose parameter count can't reach the cursor's position.
        // arg_index == 0 keeps every overload.
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
        Ok(Some(SignatureHelp {
            signatures,
            active_signature: Some(0),
            active_parameter: Some(call.arg_index as u32),
        }))
    }

    async fn formatting(
        &self,
        p: DocumentFormattingParams,
    ) -> LspResult<Option<Vec<TextEdit>>> {
        let uri = p.text_document.uri;
        let docs = self.docs.lock().unwrap();
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };
        let Some(formatted) = formatter::format(&doc.text) else {
            // Buffer is already canonical — no edit to publish.
            return Ok(Some(Vec::new()));
        };
        // Replace the entire buffer in one shot. `split('\n')` (unlike
        // `lines()`) yields a trailing empty segment for files that end
        // in `\n`, so the covering range correctly extends past the
        // final newline — without this, the formatter's own trailing
        // `\n` was being appended *after* the existing one, doubling
        // the line break at EOF.
        let segments: Vec<&str> = doc.text.split('\n').collect();
        let end_line = segments.len().saturating_sub(1) as u32;
        let end_char = segments
            .last()
            .map(|s| s.chars().count() as u32)
            .unwrap_or(0);
        let range = Range {
            start: Position { line: 0, character: 0 },
            end: Position {
                line: end_line,
                character: end_char,
            },
        };
        Ok(Some(vec![TextEdit {
            range,
            new_text: formatted,
        }]))
    }

    async fn rename(
        &self,
        p: RenameParams,
    ) -> LspResult<Option<WorkspaceEdit>> {
        let uri = p.text_document_position.text_document.uri;
        let pos = p.text_document_position.position;
        let new_name = p.new_name;
        let docs = self.docs.lock().unwrap();
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };
        // Resolve the cursor to a target identity:
        //   (decl_uri, decl_span, name_len)
        // — the file + position + length that uniquely identify the
        // decl every reference points at. When the cursor is on a
        // `use module` import the target lives in another file, so
        // we read its URI from the `RefEntry`.
        let (target_uri, target, decl_name_span) = if let Some(entry) = lookup_ref(doc, pos)
        {
            // `this` is a keyword — its RefEntry shares (target_span,
            // target_name_len) with the enclosing class, so letting
            // the rename through would also rewrite every reference
            // to the class. Refuse instead of silently corrupting
            // the file.
            if entry.signature.starts_with("this:") {
                return Ok(None);
            }
            let owner = entry.target_uri.clone().unwrap_or_else(|| uri.clone());
            (
                owner,
                (entry.target_span, entry.target_name_len),
                entry.target_span,
            )
        } else if let Some((word, _)) = word_at(&doc.text, pos) {
            if let Some(sym) = doc.symbols.get(&AstSymbol::intern(&word)) {
                let name_span = ["fn", "class", "enum", "const"]
                    .iter()
                    .find_map(|kw| {
                        text::locate_let_name_with_kw(
                            &doc.text, sym.span, kw, &sym.name,
                        )
                    })
                    .unwrap_or(sym.span);
                (
                    uri.clone(),
                    (sym.span, sym.name.as_str().len() as u32),
                    name_span,
                )
            } else {
                return Ok(None);
            }
        } else {
            return Ok(None);
        };

        // Collect edits per file. For the decl's owning file we
        // also include the decl-site edit; ref-only files only get
        // their cross-file references rewritten.
        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();

        // Track which paths we've already covered via open docs so
        // the workspace walk doesn't double-count them.
        let opened_paths: std::collections::HashSet<PathBuf> = docs
            .keys()
            .filter_map(|u| u.to_file_path().ok())
            .filter_map(|p| p.canonicalize().ok())
            .collect();
        for (doc_uri, d) in docs.iter() {
            let is_owner = doc_uri == &target_uri;
            let mut edits: Vec<TextEdit> = d
                .refs
                .iter()
                .filter(|r| {
                    if r.signature.starts_with("this:") {
                        return false;
                    }
                    if r.target_span != target.0 || r.target_name_len != target.1 {
                        return false;
                    }
                    if is_owner {
                        // Local refs in the decl's own file have
                        // `target_uri == None`. Cross-file refs
                        // here would point at OTHER files, not this
                        // one — skip them.
                        r.target_uri.is_none()
                    } else {
                        // From another file, the ref must explicitly
                        // point back at the decl's owning URI.
                        r.target_uri.as_ref() == Some(&target_uri)
                    }
                })
                .map(|r| TextEdit {
                    range: Range {
                        start: Position {
                            line: r.line.saturating_sub(1),
                            character: r.start_col.saturating_sub(1),
                        },
                        end: Position {
                            line: r.line.saturating_sub(1),
                            character: r.end_col.saturating_sub(1),
                        },
                    },
                    new_text: new_name.clone(),
                })
                .collect();
            if is_owner {
                // Always include the decl site itself. Without
                // this an unused decl would yield zero edits in
                // its own file and VSCode would refuse the rename.
                edits.push(TextEdit {
                    range: Range {
                        start: Position {
                            line: decl_name_span.line.saturating_sub(1),
                            character: decl_name_span.col.saturating_sub(1),
                        },
                        end: Position {
                            line: decl_name_span.line.saturating_sub(1),
                            character: decl_name_span
                                .col
                                .saturating_sub(1)
                                .saturating_add(target.1),
                        },
                    },
                    new_text: new_name.clone(),
                });
            }
            if !edits.is_empty() {
                edits.sort_by(|a, b| {
                    (a.range.start.line, a.range.start.character)
                        .cmp(&(b.range.start.line, b.range.start.character))
                });
                edits.dedup_by(|a, b| a.range == b.range);
                changes.insert(doc_uri.clone(), edits);
            }
        }
        // Workspace walk: also pick up references in `.il` files
        // that aren't currently open in the editor. Anchored on
        // the decl's owning file so the walk starts in the same
        // project (`ilang.toml` directory, or the file's parent).
        if let Ok(anchor_path) = target_uri.to_file_path() {
            for path in collect_workspace_il_files(&anchor_path) {
                if opened_paths.contains(&path) {
                    continue;
                }
                let Some(doc) = analyse_path_to_doc(&path) else {
                    continue;
                };
                let path_uri = match Url::from_file_path(&path) {
                    Ok(u) => u,
                    Err(_) => continue,
                };
                let is_owner = path_uri == target_uri;
                let mut edits: Vec<TextEdit> = doc
                    .refs
                    .iter()
                    .filter(|r| {
                        if r.signature.starts_with("this:") {
                            return false;
                        }
                        if r.target_span != target.0 || r.target_name_len != target.1 {
                            return false;
                        }
                        if is_owner {
                            r.target_uri.is_none()
                        } else {
                            r.target_uri.as_ref() == Some(&target_uri)
                        }
                    })
                    .map(|r| TextEdit {
                        range: Range {
                            start: Position {
                                line: r.line.saturating_sub(1),
                                character: r.start_col.saturating_sub(1),
                            },
                            end: Position {
                                line: r.line.saturating_sub(1),
                                character: r.end_col.saturating_sub(1),
                            },
                        },
                        new_text: new_name.clone(),
                    })
                    .collect();
                if is_owner {
                    edits.push(TextEdit {
                        range: Range {
                            start: Position {
                                line: decl_name_span.line.saturating_sub(1),
                                character: decl_name_span.col.saturating_sub(1),
                            },
                            end: Position {
                                line: decl_name_span.line.saturating_sub(1),
                                character: decl_name_span
                                    .col
                                    .saturating_sub(1)
                                    .saturating_add(target.1),
                            },
                        },
                        new_text: new_name.clone(),
                    });
                }
                if !edits.is_empty() {
                    edits.sort_by(|a, b| {
                        (a.range.start.line, a.range.start.character)
                            .cmp(&(b.range.start.line, b.range.start.character))
                    });
                    edits.dedup_by(|a, b| a.range == b.range);
                    changes.insert(path_uri, edits);
                }
            }
        }
        if changes.is_empty() {
            return Ok(None);
        }
        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }))
    }

    async fn code_action(
        &self,
        p: CodeActionParams,
    ) -> LspResult<Option<CodeActionResponse>> {
        let uri = p.text_document.uri;
        let only = p.context.only.as_ref();
        let want_kind = |k: &CodeActionKind| match only {
            None => true,
            Some(kinds) => kinds.iter().any(|requested| {
                // Match on prefix — e.g. requesting "refactor" should
                // include "refactor.rewrite" too.
                let r = requested.as_str();
                let target = k.as_str();
                target == r || target.starts_with(&format!("{r}."))
            }),
        };
        let want_organize = want_kind(&CodeActionKind::SOURCE_ORGANIZE_IMPORTS)
            || want_kind(&CodeActionKind::SOURCE);
        let want_quickfix = want_kind(&CodeActionKind::QUICKFIX);
        if !want_organize && !want_quickfix {
            return Ok(None);
        }
        let docs = self.docs.lock().unwrap();
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };
        let text = doc.text.clone();
        let var_types = doc.var_types.clone();
        drop(docs);
        let Ok(tokens) = tokenize(&text) else {
            return Ok(None);
        };
        let Ok(prog) = parse(&tokens) else {
            return Ok(None);
        };
        let mut actions: Vec<CodeActionOrCommand> = Vec::new();
        if want_organize {
            if let Some((start_byte, end_byte, new_text)) =
                organize_imports(&text, &prog)
            {
                let range = byte_range_to_lsp_range(&text, start_byte, end_byte);
                let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
                changes.insert(
                    uri.clone(),
                    vec![TextEdit { range, new_text }],
                );
                actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title: "Organize imports".into(),
                    kind: Some(CodeActionKind::SOURCE_ORGANIZE_IMPORTS),
                    edit: Some(WorkspaceEdit {
                        changes: Some(changes),
                        document_changes: None,
                        change_annotations: None,
                    }),
                    diagnostics: None,
                    is_preferred: None,
                    disabled: None,
                    data: None,
                    command: None,
                }));
            }
        }
        if want_quickfix {
            if let Some((insert_byte, new_text)) =
                generate_init_at(&text, &prog, p.range.start)
            {
                let pos = byte_to_position(&text, insert_byte);
                let range = Range { start: pos, end: pos };
                let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
                changes.insert(
                    uri.clone(),
                    vec![TextEdit { range, new_text }],
                );
                actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title: "Generate init from fields".into(),
                    kind: Some(CodeActionKind::QUICKFIX),
                    edit: Some(WorkspaceEdit {
                        changes: Some(changes),
                        document_changes: None,
                        change_annotations: None,
                    }),
                    diagnostics: None,
                    is_preferred: None,
                    disabled: None,
                    data: None,
                    command: None,
                }));
            }
            if let Some((insert_byte, new_text, missing_count)) =
                fill_match_arms_at(&text, &prog, &var_types, p.range.start)
            {
                let pos = byte_to_position(&text, insert_byte);
                let range = Range { start: pos, end: pos };
                let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
                changes.insert(
                    uri.clone(),
                    vec![TextEdit { range, new_text }],
                );
                let title = if missing_count == 1 {
                    "Fill missing match arm".to_string()
                } else {
                    format!("Fill {missing_count} missing match arms")
                };
                actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title,
                    kind: Some(CodeActionKind::QUICKFIX),
                    edit: Some(WorkspaceEdit {
                        changes: Some(changes),
                        document_changes: None,
                        change_annotations: None,
                    }),
                    diagnostics: None,
                    is_preferred: Some(true),
                    disabled: None,
                    data: None,
                    command: None,
                }));
            }
        }
        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(actions))
        }
    }

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }
}

