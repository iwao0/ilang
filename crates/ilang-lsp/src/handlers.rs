//! Extracted from `main.rs`.
#![allow(unused_imports)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};



use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::lsp_types::request::{
    GotoImplementationParams, GotoImplementationResponse,
};
use tower_lsp::{Client, LanguageServer};

use ilang_ast::{
    Block, ClassDecl, EnumDecl, Expr, ExprKind, FnDecl, Item, Param, Pattern, PatternBindings,
    PatternKind, Program, Span, Stmt, StmtKind, Symbol as AstSymbol, Type, VariantPayload,
};
use ilang_parser::parse as parse_program;
use ilang_types::{check, TypeError};

use crate::*;
use crate::document_symbol::collect_item_symbol;
use crate::references::{collect_reference_locations, locate_decl_name_range};
use crate::text::{is_keyword, is_valid_identifier, read_word_at, subsequence_ci};

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> LspResult<InitializeResult> {
        // Remember whether the client can dynamically register file
        // watchers — `initialized` consults this before requesting one.
        let dynamic_watch = params
            .capabilities
            .workspace
            .as_ref()
            .and_then(|w| w.did_change_watched_files.as_ref())
            .and_then(|d| d.dynamic_registration)
            .unwrap_or(false);
        self.client_supports_dynamic_watch
            .store(dynamic_watch, std::sync::atomic::Ordering::Relaxed);
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                implementation_provider: Some(
                    ImplementationProviderCapability::Simple(true),
                ),
                references_provider: Some(OneOf::Left(true)),
                document_highlight_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                call_hierarchy_provider: Some(
                    CallHierarchyServerCapability::Simple(true),
                ),
                inlay_hint_provider: Some(OneOf::Left(true)),
                // CodeLens は見た目負荷と "N references" の
                // ワークスペーススキャン負荷の双方で off にして
                // ある。再開する場合は capability を戻し、resolve
                // 側でキャッシュを入れてから有効化する。
                // code_lens_provider: Some(CodeLensOptions { resolve_provider: Some(true) }),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(
                    true,
                )),
                selection_range_provider: Some(
                    SelectionRangeProviderCapability::Simple(true),
                ),
                completion_provider: Some(CompletionOptions {
                    // `:` triggers type-position completion
                    // (`let x: …`, `fn f(p: …)`, `class C : …`).
                    // `,` continues a comma-separated type list
                    // such as `class C : A, …` (additional
                    // interfaces) and `Map<K, …>` generic args.
                    // `<` opens a generic-argument slot (`Map<…`).
                    // ` ` keeps the popup alive after `, ` inside
                    // a function call's argument list — without
                    // it VSCode closes on the first space and the
                    // user has to ⌃Space to reopen.
                    trigger_characters: Some(vec![
                        ".".to_string(),
                        "@".to_string(),
                        ":".to_string(),
                        ",".to_string(),
                        "<".to_string(),
                        " ".to_string(),
                    ]),
                    ..CompletionOptions::default()
                }),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec![
                        "(".to_string(),
                        ",".to_string(),
                        "<".to_string(),
                    ]),
                    // Re-fire signature help on whitespace inside
                    // an argument list so `, ` doesn't drop the
                    // parameter overlay along with the completion
                    // popup.
                    retrigger_characters: Some(vec![" ".to_string()]),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
                document_formatting_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                })),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            work_done_progress_options:
                                WorkDoneProgressOptions::default(),
                            legend: SemanticTokensLegend {
                                token_types: semantic_tokens::TOKEN_TYPES.to_vec(),
                                token_modifiers: semantic_tokens::TOKEN_MODIFIERS
                                    .to_vec(),
                            },
                            range: Some(false),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                        },
                    ),
                ),
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
        // If the client supports it, register a watcher for `.il` files so
        // the workspace-symbol file-list cache can be invalidated on
        // create / delete instead of re-walking the tree every request.
        if self
            .client_supports_dynamic_watch
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            let registration = Registration {
                id: "ilang-watch-il-files".to_string(),
                method: "workspace/didChangeWatchedFiles".to_string(),
                register_options: serde_json::to_value(DidChangeWatchedFilesRegistrationOptions {
                    watchers: vec![FileSystemWatcher {
                        glob_pattern: GlobPattern::String("**/*.il".to_string()),
                        kind: None, // default: Create | Change | Delete
                    }],
                })
                .ok(),
            };
            match self.client.register_capability(vec![registration]).await {
                Ok(()) => {
                    self.watch_registered
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                    crate::project::UMBRELLA_WATCH_TRUSTED
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                }
                Err(e) => {
                    self.client
                        .log_message(
                            MessageType::WARNING,
                            format!("ilang-lsp: file watch registration failed: {e}"),
                        )
                        .await;
                }
            }
        }
        self.client
            .log_message(MessageType::INFO, "ilang-lsp ready")
            .await;
    }

    async fn did_change_watched_files(&self, _: DidChangeWatchedFilesParams) {
        // A `.il` file was created / deleted / changed on disk. Drop the
        // cached file lists so the next `workspace/symbol` re-walks once
        // and repopulates. (Per-file symbol entries stay valid — they're
        // mtime-checked independently.) Same idea for the umbrella
        // resolution cache: under a trusted watcher it stops re-statting
        // siblings per refresh, so this is the only thing that lets a
        // `pub use sub` edit in a sibling re-trigger umbrella detection.
        self.workspace_file_cache.lock().unwrap().clear();
        crate::project::clear_umbrella_cache();
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
            let mut docs = self.docs();
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
            //
            // The fan-out is expensive (one full type-check per
            // open file), so skip it if a newer change to the
            // primary buffer has already arrived — only the
            // latest keystroke's fan-out actually runs. This
            // keeps sustained typing responsive while still
            // catching the cross-module case on the next idle.
            {
                let v = versions.lock().unwrap();
                if v.get(&uri).copied() != Some(version) {
                    return;
                }
            }
            let other_docs: Vec<(Url, String)> = {
                let lock = docs.lock().unwrap();
                lock.iter()
                    .filter(|(u, _)| **u != uri)
                    .map(|(u, d)| (u.clone(), d.text.clone()))
                    .collect()
            };
            for (other_uri, other_text) in other_docs {
                // Re-check before each dependent so the user can
                // still abort the fan-out mid-way by typing.
                {
                    let v = versions.lock().unwrap();
                    if v.get(&uri).copied() != Some(version) {
                        return;
                    }
                }
                refresh_impl(&client, &docs, other_uri, other_text).await;
            }
        });
    }

    async fn did_close(&self, p: DidCloseTextDocumentParams) {
        let mut docs = self.docs();
        docs.remove(&p.text_document.uri);
    }

    async fn hover(&self, p: HoverParams) -> LspResult<Option<Hover>> {
        let uri = p.text_document_position_params.text_document.uri;
        let pos = p.text_document_position_params.position;
        let docs = self.docs();
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
            if let Some(sig) = doc.external.signatures.get(&key) {
                return Ok(Some(make_hover_with_doc(
                    sig,
                    doc.external.docs.get(&key).map(|s| s.as_str()),
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
        let docs = self.docs();
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };
        if let Some(entry) = lookup_ref(doc, pos) {
            if entry.no_definition && entry.target_uri.is_none() {
                return Ok(None);
            }
            let name = read_word_at(
                &doc.text,
                entry.line,
                entry.start_col,
                entry.end_col,
            )
            .unwrap_or_default();
            let target_uri = entry.target_uri.clone().unwrap_or_else(|| uri.clone());
            // `target_span` is the decl keyword's position
            // (`class` / `fn` / ...). Locate the identifier inside
            // the decl so F12 lands on `NSObject` instead of the
            // `class` keyword in `@objc pub class NSObject { … }`.
            let range = locate_decl_name_range(
                &target_uri,
                &uri,
                &doc.text,
                entry.target_span,
                &name,
                entry.target_name_len as usize,
            );
            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                uri: target_uri,
                range,
            })));
        }
        if let Some((word, _)) = word_at(&doc.text, pos) {
            let key = AstSymbol::intern(&word);
            if let Some(sym) = doc.symbols.get(&key) {
                // Same-file decl: snap to the name within the decl
                // text rather than the keyword span stored on
                // `Symbol`.
                let range = locate_decl_name_range(
                    &uri, &uri, &doc.text, sym.span, &sym.name, sym.name.as_str().len(),
                );
                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri,
                    range,
                })));
            }
            // Selectively-imported bare type / fn (`use M { X }`) —
            // `external_sources` carries the file path + decl span
            // for a cross-file jump.
            if let Some(loc) = doc.external.sources.get(&key) {
                if let Ok(target_uri) = Url::from_file_path(&loc.path) {
                    let range = locate_decl_name_range(
                        &target_uri,
                        &uri,
                        &doc.text,
                        loc.span,
                        &word,
                        loc.name_len as usize,
                    );
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
        let docs = self.docs();
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };
        Ok(completion::handle_completion(doc, pos))
    }

    async fn signature_help(
        &self,
        p: SignatureHelpParams,
    ) -> LspResult<Option<SignatureHelp>> {
        let uri = p.text_document_position_params.text_document.uri;
        let pos = p.text_document_position_params.position;
        let docs = self.docs();
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };
        Ok(signature_help::handle_signature_help(doc, pos))
    }

    async fn formatting(
        &self,
        p: DocumentFormattingParams,
    ) -> LspResult<Option<Vec<TextEdit>>> {
        let uri = p.text_document.uri;
        let docs = self.docs();
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

    async fn references(
        &self,
        p: ReferenceParams,
    ) -> LspResult<Option<Vec<Location>>> {
        let uri = p.text_document_position.text_document.uri;
        let pos = p.text_document_position.position;
        let include_decl = p.context.include_declaration;
        let docs = self.docs();
        Ok(references::handle_references(&docs, &uri, pos, include_decl))
    }

    async fn prepare_rename(
        &self,
        p: TextDocumentPositionParams,
    ) -> LspResult<Option<PrepareRenameResponse>> {
        let uri = p.text_document.uri;
        let pos = p.position;
        let docs = self.docs();
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };
        // Reject when the cursor isn't on an identifier we can
        // rename. Mirror the same rejection rules the `rename` call
        // applies, so the editor's popup never opens for a no-op.
        if let Some(entry) = lookup_ref(doc, pos) {
            // `this` is a keyword. Builtins / external decls we can't
            // navigate to (`no_definition`) have no decl site to
            // rewrite either, so refuse cleanly here instead of
            // silently rewriting only the use site.
            if entry.signature.starts_with("this:") || entry.no_definition {
                return Ok(None);
            }
            Ok(Some(PrepareRenameResponse::Range(entry.lsp_range())))
        } else if let Some((word, start_col)) = word_at(&doc.text, pos) {
            // Plain top-level decl whose use site we don't have a
            // `RefEntry` for (e.g. cursor parked on the decl line
            // itself). Anything that doesn't resolve to a known
            // symbol — including bare keywords — is refused.
            if is_keyword(&word) {
                return Ok(None);
            }
            if !doc.symbols.contains_key(&AstSymbol::intern(&word)) {
                return Ok(None);
            }
            let range = Range {
                start: Position {
                    line: pos.line,
                    character: start_col.saturating_sub(1),
                },
                end: Position {
                    line: pos.line,
                    character: start_col
                        .saturating_sub(1)
                        .saturating_add(word.len() as u32),
                },
            };
            Ok(Some(PrepareRenameResponse::Range(range)))
        } else {
            Ok(None)
        }
    }

    async fn rename(
        &self,
        p: RenameParams,
    ) -> LspResult<Option<WorkspaceEdit>> {
        let uri = p.text_document_position.text_document.uri;
        let pos = p.text_document_position.position;
        let docs = self.docs();
        rename::handle_rename(&docs, &uri, pos, p.new_name)
    }

    async fn code_action(
        &self,
        p: CodeActionParams,
    ) -> LspResult<Option<CodeActionResponse>> {
        let docs = self.docs();
        let Some(doc) = docs.get(&p.text_document.uri) else {
            return Ok(None);
        };
        let text = doc.text.clone();
        let var_types = doc.var_types.clone();
        let external_interfaces = doc.external.interfaces.clone();
        drop(docs);
        Ok(code_actions::handle_code_action(&p, &text, &var_types, &external_interfaces))
    }

    async fn document_symbol(
        &self,
        p: DocumentSymbolParams,
    ) -> LspResult<Option<DocumentSymbolResponse>> {
        let uri = p.text_document.uri;
        let docs = self.docs();
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };
        let text = doc.text.clone();
        drop(docs);
        let Some(prog) = text::try_parse(&text) else {
            return Ok(None);
        };
        let line_starts = crate::text_utils::compute_line_starts(&text);
        let mut out: Vec<DocumentSymbol> = Vec::new();
        for item in &prog.items {
            collect_item_symbol(&line_starts, &text, item, &mut out);
        }
        if out.is_empty() {
            Ok(None)
        } else {
            Ok(Some(DocumentSymbolResponse::Nested(out)))
        }
    }

    async fn symbol(
        &self,
        p: WorkspaceSymbolParams,
    ) -> LspResult<Option<Vec<SymbolInformation>>> {
        let anchor: Option<PathBuf> = {
            let docs = self.docs();
            docs.keys()
                .find_map(|u| u.to_file_path().ok())
                .or_else(|| std::env::current_dir().ok())
        };
        let Some(anchor) = anchor else { return Ok(None) };
        let open_texts: HashMap<PathBuf, String> = {
            let docs = self.docs();
            docs.iter()
                .filter_map(|(u, d)| {
                    let p = u.to_file_path().ok()?;
                    let canon = p.canonicalize().ok()?;
                    Some((canon, d.text.clone()))
                })
                .collect()
        };
        let use_file_cache = self
            .watch_registered
            .load(std::sync::atomic::Ordering::Relaxed);
        Ok(workspace_symbol_cache::handle_workspace_symbol(
            &p.query,
            &anchor,
            &open_texts,
            &self.workspace_sym_cache,
            &self.workspace_file_cache,
            use_file_cache,
        ))
    }

    async fn goto_implementation(
        &self,
        p: GotoImplementationParams,
    ) -> LspResult<Option<GotoImplementationResponse>> {
        let uri = p.text_document_position_params.text_document.uri;
        let pos = p.text_document_position_params.position;
        let (target, iface_class, anchor, snapshot) = {
            let docs = self.docs();
            let Some(doc) = docs.get(&uri) else { return Ok(None) };
            let Some(target) = implementation::resolve(doc, pos) else {
                return Ok(None);
            };
            // For `(method) X.y`, X may be an interface even though
            // the signature uses class syntax. Pass the doc-side
            // interface-name check to the collector so it can route.
            let iface_class = match &target {
                implementation::Target::ClassMethod { class, .. } => {
                    if implementation::name_is_interface(doc, class) {
                        Some(class.clone())
                    } else {
                        None
                    }
                }
                _ => None,
            };
            let anchor = uri.to_file_path().ok();
            let snapshot = docs.clone();
            (target, iface_class, anchor, snapshot)
        };
        let Some(anchor) = anchor else { return Ok(None) };
        let locs = implementation::collect(
            &target,
            &anchor,
            &snapshot,
            iface_class.as_deref(),
        );
        if locs.is_empty() {
            Ok(None)
        } else {
            Ok(Some(GotoImplementationResponse::Array(locs)))
        }
    }

    async fn document_highlight(
        &self,
        p: DocumentHighlightParams,
    ) -> LspResult<Option<Vec<DocumentHighlight>>> {
        let uri = p.text_document_position_params.text_document.uri;
        let pos = p.text_document_position_params.position;
        let docs = self.docs();
        let Some(doc) = docs.get(&uri) else { return Ok(None) };
        Ok(references::handle_document_highlight(doc, pos))
    }

    async fn prepare_call_hierarchy(
        &self,
        p: CallHierarchyPrepareParams,
    ) -> LspResult<Option<Vec<CallHierarchyItem>>> {
        let uri = p.text_document_position_params.text_document.uri;
        let pos = p.text_document_position_params.position;
        let docs = self.docs();
        let Some(doc) = docs.get(&uri) else { return Ok(None) };
        let text = doc.text.clone();
        let Some(item) = call_hierarchy::prepare(uri, pos, doc, &text) else {
            return Ok(None);
        };
        drop(docs);
        // Resolve full_range against the file's text — `doc.text`
        // when same file, otherwise read from disk.
        let same_file_text = self
            .docs
            .lock()
            .unwrap()
            .get(&item.uri)
            .map(|d| d.text.clone());
        let target_text = same_file_text.unwrap_or_else(|| {
            item.uri
                .to_file_path()
                .ok()
                .and_then(|p| std::fs::read_to_string(&p).ok())
                .unwrap_or_default()
        });
        let full_range = call_hierarchy::full_range_for(&item, &target_text);
        Ok(Some(vec![item.to_item(full_range)]))
    }

    async fn incoming_calls(
        &self,
        p: CallHierarchyIncomingCallsParams,
    ) -> LspResult<Option<Vec<CallHierarchyIncomingCall>>> {
        let Some(data) = p.item.data.as_ref() else { return Ok(None) };
        let Some(item) = call_hierarchy::ItemRef::from_data(data) else {
            return Ok(None);
        };
        let snapshot: HashMap<Url, crate::types::Doc> =
            self.docs().clone();
        let calls = call_hierarchy::incoming_calls(&item, &snapshot);
        if calls.is_empty() { Ok(None) } else { Ok(Some(calls)) }
    }

    async fn outgoing_calls(
        &self,
        p: CallHierarchyOutgoingCallsParams,
    ) -> LspResult<Option<Vec<CallHierarchyOutgoingCall>>> {
        let Some(data) = p.item.data.as_ref() else { return Ok(None) };
        let Some(item) = call_hierarchy::ItemRef::from_data(data) else {
            return Ok(None);
        };
        // Outgoing analysis runs against the item's home file. Prefer
        // the live buffer when open, else load from disk.
        let live = self.docs().get(&item.uri).cloned();
        let doc = match live {
            Some(d) => d,
            None => {
                let Ok(p) = item.uri.to_file_path() else { return Ok(None) };
                let Some(d) = analyse_path_to_doc(&p) else { return Ok(None) };
                d
            }
        };
        let calls = call_hierarchy::outgoing_calls(&item, &doc);
        if calls.is_empty() { Ok(None) } else { Ok(Some(calls)) }
    }

    async fn code_lens(
        &self,
        p: CodeLensParams,
    ) -> LspResult<Option<Vec<CodeLens>>> {
        let uri = p.text_document.uri;
        let docs = self.docs();
        let Some(doc) = docs.get(&uri) else { return Ok(None) };
        let lenses = code_lens::build(&uri, &doc.text);
        if lenses.is_empty() {
            Ok(None)
        } else {
            Ok(Some(lenses))
        }
    }

    async fn code_lens_resolve(&self, mut lens: CodeLens) -> LspResult<CodeLens> {
        let Some(data) = lens.data.as_ref().and_then(code_lens::decode_data)
        else {
            return Ok(lens);
        };
        match data {
            code_lens::LensData::References {
                uri,
                name: _,
                decl_line,
                decl_col,
                decl_name_len,
            } => {
                let snapshot = self.docs().clone();
                // Collect actual reference locations so the `Peek
                // References` window opens populated when the user
                // clicks the lens.
                let locations = collect_reference_locations(
                    &uri,
                    ilang_ast::Span::new(decl_line, decl_col),
                    decl_name_len,
                    &snapshot,
                );
                let pos = text::lsp_position(decl_line, decl_col);
                lens.command = Some(code_lens::references_command(&uri, pos, locations));
            }
            code_lens::LensData::Implementations {
                uri,
                name,
                is_interface,
                decl_line,
                decl_col,
                decl_name_len: _,
            } => {
                let target = if is_interface {
                    implementation::Target::Interface { name: name.clone() }
                } else {
                    implementation::Target::Class { name: name.clone() }
                };
                let snapshot = self.docs().clone();
                let anchor = match uri.to_file_path() {
                    Ok(p) => p,
                    Err(_) => return Ok(lens),
                };
                let iface_class = if is_interface { Some(name.as_str()) } else { None };
                let locs = implementation::collect(&target, &anchor, &snapshot, iface_class);
                let pos = text::lsp_position(decl_line, decl_col);
                lens.command = Some(code_lens::implementations_command(
                    &uri,
                    pos,
                    locs.len(),
                ));
            }
        }
        Ok(lens)
    }

    async fn folding_range(
        &self,
        p: FoldingRangeParams,
    ) -> LspResult<Option<Vec<FoldingRange>>> {
        let uri = p.text_document.uri;
        let docs = self.docs();
        let Some(doc) = docs.get(&uri) else { return Ok(None) };
        let ranges = folding_range::build(&doc.text);
        if ranges.is_empty() {
            Ok(None)
        } else {
            Ok(Some(ranges))
        }
    }

    async fn selection_range(
        &self,
        p: SelectionRangeParams,
    ) -> LspResult<Option<Vec<SelectionRange>>> {
        let uri = p.text_document.uri;
        let docs = self.docs();
        let Some(doc) = docs.get(&uri) else { return Ok(None) };
        let ranges = crate::selection_range::build_for(&doc.text, &p.positions);
        if ranges.is_empty() {
            Ok(None)
        } else {
            Ok(Some(ranges))
        }
    }

    async fn inlay_hint(
        &self,
        p: InlayHintParams,
    ) -> LspResult<Option<Vec<InlayHint>>> {
        let uri = p.text_document.uri;
        let docs = self.docs();
        let Some(doc) = docs.get(&uri) else { return Ok(None) };
        let hints = inlay_hints::build_hints(doc, p.range);
        if hints.is_empty() { Ok(None) } else { Ok(Some(hints)) }
    }

    async fn semantic_tokens_full(
        &self,
        p: SemanticTokensParams,
    ) -> LspResult<Option<SemanticTokensResult>> {
        let uri = p.text_document.uri;
        let docs = self.docs();
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };
        let data = semantic_tokens::build_tokens(doc);
        if data.is_empty() {
            return Ok(None);
        }
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        })))
    }

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }
}

