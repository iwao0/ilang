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
/// Walk `text` back from byte offset `off` over alphanumeric +
/// underscore characters and return the resulting identifier prefix
/// (empty when `off` is not preceded by an ident char). Used by the
/// completion handler to know what the user has typed so far so it
/// can hand VSCode a `filter_text` that's guaranteed to match.
fn typed_prefix_at(text: &str, off: usize) -> String {
    let bytes = text.as_bytes();
    let end = off.min(bytes.len());
    let mut i = end;
    while i > 0 {
        let b = bytes[i - 1];
        if b.is_ascii_alphanumeric() || b == b'_' {
            i -= 1;
        } else {
            break;
        }
    }
    std::str::from_utf8(&bytes[i..end]).unwrap_or("").to_string()
}

/// `true` when every character of `needle` (already lowercased)
/// appears in `haystack` in order, case-insensitively. Cheap
/// subsequence check the LSP uses to decide whether a label is a
/// plausible match for the typed prefix before handing it to the
/// client.
fn subsequence_ci(haystack: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return true;
    }
    let mut needle = needle_lower.chars();
    let mut want = match needle.next() {
        Some(c) => c,
        None => return true,
    };
    for h in haystack.chars().flat_map(|c| c.to_lowercase()) {
        if h == want {
            match needle.next() {
                Some(c) => want = c,
                None => return true,
            }
        }
    }
    false
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
        completion::enclosing_class(&doc.text, text_offset)
    } else if doc.classes.contains_key(&AstSymbol::intern(segments[0])) {
        Some(segments[0].to_string())
    } else if let Some(c) = doc
        .var_classes
        .get(&AstSymbol::intern(segments[0]))
        .cloned()
    {
        Some(c)
    } else {
        completion::enclosing_class(&doc.text, text_offset).and_then(|cls| {
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
                references_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                call_hierarchy_provider: Some(
                    CallHierarchyServerCapability::Simple(true),
                ),
                completion_provider: Some(CompletionOptions {
                    // `:` triggers type-position completion
                    // (`let x: …`, `fn f(p: …)`, `class C : …`).
                    // `,` continues a comma-separated type list
                    // such as `class C : A, …` (additional
                    // interfaces) and `Map<K, …>` generic args.
                    // `<` opens a generic-argument slot (`Map<…`).
                    trigger_characters: Some(vec![
                        ".".to_string(),
                        "@".to_string(),
                        ":".to_string(),
                        ",".to_string(),
                        "<".to_string(),
                    ]),
                    ..CompletionOptions::default()
                }),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec![
                        "(".to_string(),
                        ",".to_string(),
                        "<".to_string(),
                    ]),
                    retrigger_characters: None,
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
                let prefix = typed_prefix_at(&doc.text, off);
                if !prefix.is_empty() {
                    let lowered_prefix = prefix.to_lowercase();
                    items.retain(|it| subsequence_ci(&it.label, &lowered_prefix));
                    for it in items.iter_mut() {
                        it.filter_text = Some(prefix.clone());
                    }
                }
                return Ok(Some(CompletionResponse::List(CompletionList {
                    is_incomplete: true,
                    items,
                })));
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
                        if crate::symbols::is_synthesized_objc_helper(suffix) {
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
                return Ok(Some(CompletionResponse::Array(items)));
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
                            if crate::symbols::is_synthesized_objc_helper(s) {
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
                            if crate::symbols::is_synthesized_objc_helper(s) {
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
                    // Hide @objc desugar's internal scaffolding —
                    // the per-block `__objc_<hash>_class_t` etc.
                    // structs and bookkeeping wrappers are emitted
                    // into the module's namespace but aren't user-
                    // facing.
                    if crate::symbols::is_synthesized_objc_helper(suffix) {
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
            return Ok(Some(SignatureHelp {
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
            }));
        }
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
                // Method call: `obj.method(`. Walk the (possibly
                // dotted) receiver via `resolve_receiver_class` so
                // chains like `this.starTex.update(` resolve through
                // the field's declared type, not just a single
                // `var_classes` hop. Falls back to the built-in
                // string / array signatures below when the receiver
                // is one of those primitives.
                let class = if recv == "console" {
                    Some("Console".to_string())
                } else {
                    let off = text::line_col_to_offset(
                        &doc.text,
                        pos.line + 1,
                        pos.character + 1,
                    )
                    .unwrap_or(doc.text.len());
                    resolve_receiver_class(doc, recv, off)
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

    async fn references(
        &self,
        p: ReferenceParams,
    ) -> LspResult<Option<Vec<Location>>> {
        let uri = p.text_document_position.text_document.uri;
        let pos = p.text_document_position.position;
        let include_decl = p.context.include_declaration;
        let docs = self.docs.lock().unwrap();
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };
        // Resolve the cursor to the same (decl_uri, decl_span,
        // name_len, decl_name_span) tuple `rename` uses; the only
        // difference is we collect `Location`s instead of
        // `TextEdit`s.
        let (target_uri, target, decl_name_span) = if let Some(entry) = lookup_ref(doc, pos)
        {
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

        let mut locations: Vec<Location> = Vec::new();
        let opened_paths: std::collections::HashSet<PathBuf> = docs
            .keys()
            .filter_map(|u| u.to_file_path().ok())
            .filter_map(|p| p.canonicalize().ok())
            .collect();
        let push_ref_locs = |out: &mut Vec<Location>,
                             d_uri: &Url,
                             d: &crate::types::Doc,
                             is_owner: bool| {
            for r in d.refs.iter() {
                if r.signature.starts_with("this:") { continue; }
                if r.target_span != target.0 || r.target_name_len != target.1 { continue; }
                let matches = if is_owner {
                    r.target_uri.is_none()
                } else {
                    r.target_uri.as_ref() == Some(&target_uri)
                };
                if !matches { continue; }
                out.push(Location {
                    uri: d_uri.clone(),
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
                });
            }
        };
        for (doc_uri, d) in docs.iter() {
            let is_owner = doc_uri == &target_uri;
            push_ref_locs(&mut locations, doc_uri, d, is_owner);
            if is_owner && include_decl {
                locations.push(Location {
                    uri: doc_uri.clone(),
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
                });
            }
        }
        if let Ok(anchor_path) = target_uri.to_file_path() {
            for path in collect_workspace_il_files(&anchor_path) {
                if opened_paths.contains(&path) { continue; }
                let Some(d) = analyse_path_to_doc(&path) else { continue };
                let path_uri = match Url::from_file_path(&path) {
                    Ok(u) => u,
                    Err(_) => continue,
                };
                let is_owner = path_uri == target_uri;
                push_ref_locs(&mut locations, &path_uri, &d, is_owner);
                if is_owner && include_decl {
                    locations.push(Location {
                        uri: path_uri,
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
                    });
                }
            }
        }
        // Stable, de-duplicated output.
        locations.sort_by(|a, b| {
            (a.uri.as_str(), a.range.start.line, a.range.start.character)
                .cmp(&(b.uri.as_str(), b.range.start.line, b.range.start.character))
        });
        locations.dedup_by(|a, b| a.uri == b.uri && a.range == b.range);
        if locations.is_empty() {
            return Ok(None);
        }
        Ok(Some(locations))
    }

    async fn prepare_rename(
        &self,
        p: TextDocumentPositionParams,
    ) -> LspResult<Option<PrepareRenameResponse>> {
        let uri = p.text_document.uri;
        let pos = p.position;
        let docs = self.docs.lock().unwrap();
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
            let range = Range {
                start: Position {
                    line: entry.line.saturating_sub(1),
                    character: entry.start_col.saturating_sub(1),
                },
                end: Position {
                    line: entry.line.saturating_sub(1),
                    character: entry.end_col.saturating_sub(1),
                },
            };
            Ok(Some(PrepareRenameResponse::Range(range)))
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
        let new_name = p.new_name;
        // Validate the proposed name before touching any buffers.
        // Reporting an LSP error here lets VSCode show the message
        // to the user instead of silently accepting an invalid name
        // and producing un-parseable source.
        if !is_valid_identifier(&new_name) {
            return Err(tower_lsp::jsonrpc::Error::invalid_params(format!(
                "`{new_name}` is not a valid ilang identifier"
            )));
        }
        if is_keyword(&new_name) {
            return Err(tower_lsp::jsonrpc::Error::invalid_params(format!(
                "`{new_name}` is a reserved keyword"
            )));
        }
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
        let doc_external_interfaces = doc.external_interfaces.clone();
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
            if let Some((insert_byte, new_text, missing_count)) =
                implement_interface_methods_at(
                    &text,
                    &prog,
                    &doc_external_interfaces,
                    p.range.start,
                )
            {
                let pos = byte_to_position(&text, insert_byte);
                let range = Range { start: pos, end: pos };
                let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
                changes.insert(
                    uri.clone(),
                    vec![TextEdit { range, new_text }],
                );
                let title = if missing_count == 1 {
                    "Implement missing interface method".to_string()
                } else {
                    format!("Implement {missing_count} missing interface methods")
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

    async fn document_symbol(
        &self,
        p: DocumentSymbolParams,
    ) -> LspResult<Option<DocumentSymbolResponse>> {
        let uri = p.text_document.uri;
        let docs = self.docs.lock().unwrap();
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };
        let text = doc.text.clone();
        drop(docs);
        let Ok(tokens) = tokenize(&text) else {
            return Ok(None);
        };
        let Ok(prog) = parse(&tokens) else {
            return Ok(None);
        };
        let mut out: Vec<DocumentSymbol> = Vec::new();
        for item in &prog.items {
            collect_item_symbol(&text, item, &mut out);
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
        let query = p.query;
        let q_lower = query.to_lowercase();
        // Anchor the workspace scan on any open document's path so
        // `collect_workspace_il_files` finds the right ilang.toml.
        // When no buffer is open, fall back to the current working
        // directory.
        let (open_paths, anchor) = {
            let docs = self.docs.lock().unwrap();
            let anchor: Option<PathBuf> = docs
                .keys()
                .find_map(|u| u.to_file_path().ok())
                .or_else(|| std::env::current_dir().ok());
            let open: HashSet<PathBuf> = docs
                .keys()
                .filter_map(|u| u.to_file_path().ok())
                .filter_map(|p| p.canonicalize().ok())
                .collect();
            (open, anchor)
        };
        let Some(anchor) = anchor else { return Ok(None) };
        let files = collect_workspace_il_files(&anchor);
        let mut out: Vec<SymbolInformation> = Vec::new();
        // Cap the response to keep VSCode's quick-pick responsive on
        // large workspaces. Picked to be well above any realistic
        // result count for an ilang project.
        const MAX_RESULTS: usize = 2000;
        for path in files {
            if out.len() >= MAX_RESULTS {
                break;
            }
            let _ = &open_paths; // open buffers are read from disk
                                  // anyway; consistency of view across
                                  // unsaved edits isn't required for
                                  // workspace-symbol.
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let Ok(tokens) = tokenize(&text) else { continue };
            let Ok(prog) = parse(&tokens) else { continue };
            let Ok(uri) = Url::from_file_path(&path) else { continue };
            collect_workspace_symbols_from_program(
                &uri, &text, &prog, &q_lower, &mut out, MAX_RESULTS,
            );
        }
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        if out.is_empty() {
            Ok(None)
        } else {
            Ok(Some(out))
        }
    }

    async fn prepare_call_hierarchy(
        &self,
        p: CallHierarchyPrepareParams,
    ) -> LspResult<Option<Vec<CallHierarchyItem>>> {
        let uri = p.text_document_position_params.text_document.uri;
        let pos = p.text_document_position_params.position;
        let docs = self.docs.lock().unwrap();
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
            self.docs.lock().unwrap().clone();
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
        let live = self.docs.lock().unwrap().get(&item.uri).cloned();
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

    async fn semantic_tokens_full(
        &self,
        p: SemanticTokensParams,
    ) -> LspResult<Option<SemanticTokensResult>> {
        let uri = p.text_document.uri;
        let docs = self.docs.lock().unwrap();
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

/// Build a `DocumentSymbol` with the deprecated `deprecated` field
/// explicitly set to `None` (the LSP spec moved to `tags`, but the
/// struct still has the old field).
#[allow(deprecated)]
fn make_doc_sym(
    name: String,
    detail: Option<String>,
    kind: SymbolKind,
    range: Range,
    selection_range: Range,
    children: Option<Vec<DocumentSymbol>>,
) -> DocumentSymbol {
    // LSP requires `selectionRange` ⊆ `range`. Some decl spans don't
    // carry proper end positions, so the keyword-only `range` can land
    // strictly before the name `selection_range`. Expand `range` to
    // contain `selection_range` (and every child) when that happens.
    let mut range = range;
    expand_range(&mut range, &selection_range);
    if let Some(ch) = children.as_ref() {
        for c in ch {
            expand_range(&mut range, &c.range);
        }
    }
    DocumentSymbol {
        name,
        detail,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children,
    }
}

/// Grow `outer` so that `inner` is fully contained: start = min, end = max.
fn expand_range(outer: &mut Range, inner: &Range) {
    if (inner.start.line, inner.start.character)
        < (outer.start.line, outer.start.character)
    {
        outer.start = inner.start;
    }
    if (inner.end.line, inner.end.character)
        > (outer.end.line, outer.end.character)
    {
        outer.end = inner.end;
    }
}

/// Locate the identifier span for a top-level / nested decl. Falls
/// back to a zero-width span at `decl_span` when the name isn't
/// found on the recorded line (e.g. parser-synthesised decls).
fn name_range(text: &str, decl_span: Span, kw: &str, name: &str) -> Range {
    let name_span = text::locate_let_name_with_kw(text, decl_span, kw, name)
        .unwrap_or(decl_span);
    text::span_to_range(name_span, name.len())
}

fn collect_item_symbol(text: &str, item: &Item, out: &mut Vec<DocumentSymbol>) {
    match item {
        Item::Fn(f) => {
            let sel = name_range(text, f.span, "fn", f.name.as_str());
            out.push(make_doc_sym(
                f.name.as_str().to_string(),
                Some(render_fn_detail(f)),
                SymbolKind::FUNCTION,
                text::span_full_to_range(f.span),
                sel,
                None,
            ));
        }
        Item::Class(c) => {
            out.push(class_symbol(text, c));
        }
        Item::Interface(i) => {
            let sel = name_range(text, i.span, "interface", i.name.as_str());
            let mut children: Vec<DocumentSymbol> = Vec::new();
            for m in i.methods.iter() {
                let m_sel = name_range(text, m.span, "fn", m.name.as_str());
                let params = m
                    .params
                    .iter()
                    .map(|p| format!("{}: {}", p.name, p.ty))
                    .collect::<Vec<_>>()
                    .join(", ");
                let detail = match &m.ret {
                    Some(t) => format!("({params}): {t}"),
                    None => format!("({params})"),
                };
                children.push(make_doc_sym(
                    m.name.as_str().to_string(),
                    Some(detail),
                    SymbolKind::METHOD,
                    text::span_full_to_range(m.span),
                    m_sel,
                    None,
                ));
            }
            out.push(make_doc_sym(
                i.name.as_str().to_string(),
                None,
                SymbolKind::INTERFACE,
                text::span_full_to_range(i.span),
                sel,
                if children.is_empty() { None } else { Some(children) },
            ));
        }
        Item::Enum(e) => {
            let sel = name_range(text, e.span, "enum", e.name.as_str());
            let mut children: Vec<DocumentSymbol> = Vec::new();
            for v in e.variants.iter() {
                let v_sel = text::span_to_range(v.span, v.name.as_str().len());
                children.push(make_doc_sym(
                    v.name.as_str().to_string(),
                    None,
                    SymbolKind::ENUM_MEMBER,
                    text::span_full_to_range(v.span),
                    v_sel,
                    None,
                ));
            }
            out.push(make_doc_sym(
                e.name.as_str().to_string(),
                None,
                SymbolKind::ENUM,
                text::span_full_to_range(e.span),
                sel,
                if children.is_empty() { None } else { Some(children) },
            ));
        }
        Item::Const(c) => {
            let sel = name_range(text, c.span, "const", c.name.as_str());
            let detail = c.ty.as_ref().map(|t| format!(": {t}"));
            out.push(make_doc_sym(
                c.name.as_str().to_string(),
                detail,
                SymbolKind::CONSTANT,
                text::span_full_to_range(c.span),
                sel,
                None,
            ));
        }
        Item::ExternC(b) => {
            for inner in b.items.iter() {
                match inner {
                    ilang_ast::ExternCItem::FnDef(f) => {
                        let name = f.name.as_str().to_string();
                        let sel = name_range(text, f.span, "fn", &name);
                        out.push(make_doc_sym(
                            name,
                            Some(render_fn_detail(f)),
                            SymbolKind::FUNCTION,
                            text::span_full_to_range(f.span),
                            sel,
                            None,
                        ));
                    }
                    ilang_ast::ExternCItem::FnDecl { name, params, ret, span, .. } => {
                        let name_s = name.as_str().to_string();
                        let sel = name_range(text, *span, "fn", &name_s);
                        let plist = params
                            .iter()
                            .map(|p| format!("{}: {}", p.name, p.ty))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let detail = match ret {
                            Some(t) => format!("({plist}): {t}"),
                            None => format!("({plist})"),
                        };
                        out.push(make_doc_sym(
                            name_s,
                            Some(detail),
                            SymbolKind::FUNCTION,
                            text::span_full_to_range(*span),
                            sel,
                            None,
                        ));
                    }
                    ilang_ast::ExternCItem::Class(c) => {
                        out.push(class_symbol(text, c));
                    }
                    ilang_ast::ExternCItem::Struct { name, fields, span, .. } => {
                        let sel = name_range(text, *span, "struct", name.as_str());
                        let mut children: Vec<DocumentSymbol> = Vec::new();
                        for f in fields.iter() {
                            let f_sel = text::span_to_range(f.span, f.name.as_str().len());
                            children.push(make_doc_sym(
                                f.name.as_str().to_string(),
                                Some(format!(": {}", f.ty)),
                                SymbolKind::FIELD,
                                text::span_full_to_range(f.span),
                                f_sel,
                                None,
                            ));
                        }
                        out.push(make_doc_sym(
                            name.as_str().to_string(),
                            None,
                            SymbolKind::STRUCT,
                            text::span_full_to_range(*span),
                            sel,
                            if children.is_empty() { None } else { Some(children) },
                        ));
                    }
                    ilang_ast::ExternCItem::Union { name, fields, span, .. } => {
                        let sel = name_range(text, *span, "union", name.as_str());
                        let mut children: Vec<DocumentSymbol> = Vec::new();
                        for f in fields.iter() {
                            let f_sel = text::span_to_range(f.span, f.name.as_str().len());
                            children.push(make_doc_sym(
                                f.name.as_str().to_string(),
                                Some(format!(": {}", f.ty)),
                                SymbolKind::FIELD,
                                text::span_full_to_range(f.span),
                                f_sel,
                                None,
                            ));
                        }
                        out.push(make_doc_sym(
                            name.as_str().to_string(),
                            None,
                            SymbolKind::STRUCT,
                            text::span_full_to_range(*span),
                            sel,
                            if children.is_empty() { None } else { Some(children) },
                        ));
                    }
                }
            }
            for iface in b.interfaces.iter() {
                collect_item_symbol(text, &Item::Interface(iface.clone()), out);
            }
            for c in b.consts.iter() {
                collect_item_symbol(text, &Item::Const(c.clone()), out);
            }
        }
        Item::Use(_) => {}
    }
}

fn class_symbol(text: &str, c: &ClassDecl) -> DocumentSymbol {
    let kw = if c.is_union {
        "union"
    } else if c.is_repr_c {
        "struct"
    } else {
        "class"
    };
    let kind = if c.is_union || c.is_repr_c {
        SymbolKind::STRUCT
    } else {
        SymbolKind::CLASS
    };
    let sel = name_range(text, c.span, kw, c.name.as_str());
    let mut children: Vec<DocumentSymbol> = Vec::new();
    for f in c.fields.iter() {
        let f_sel = text::span_to_range(f.span, f.name.as_str().len());
        children.push(make_doc_sym(
            f.name.as_str().to_string(),
            Some(format!(": {}", f.ty)),
            SymbolKind::FIELD,
            text::span_full_to_range(f.span),
            f_sel,
            None,
        ));
    }
    for f in c.static_fields.iter() {
        let f_sel = text::span_to_range(f.span, f.name.as_str().len());
        let detail = if f.is_const {
            Some(format!("const: {}", f.ty))
        } else {
            Some(format!("static: {}", f.ty))
        };
        children.push(make_doc_sym(
            f.name.as_str().to_string(),
            detail,
            if f.is_const { SymbolKind::CONSTANT } else { SymbolKind::FIELD },
            text::span_full_to_range(f.span),
            f_sel,
            None,
        ));
    }
    for p in c.properties.iter() {
        let p_sel = text::span_to_range(p.span, p.name.as_str().len());
        children.push(make_doc_sym(
            p.name.as_str().to_string(),
            Some(format!(": {}", p.ty)),
            SymbolKind::PROPERTY,
            text::span_full_to_range(p.span),
            p_sel,
            None,
        ));
    }
    for m in c.methods.iter() {
        let m_sel = name_range(text, m.span, "fn", m.name.as_str());
        let sym_kind = if m.name.as_str() == "init" {
            SymbolKind::CONSTRUCTOR
        } else {
            SymbolKind::METHOD
        };
        children.push(make_doc_sym(
            m.name.as_str().to_string(),
            Some(render_fn_detail(m)),
            sym_kind,
            text::span_full_to_range(m.span),
            m_sel,
            None,
        ));
    }
    for m in c.static_methods.iter() {
        let m_sel = name_range(text, m.span, "fn", m.name.as_str());
        children.push(make_doc_sym(
            m.name.as_str().to_string(),
            Some(render_fn_detail(m)),
            SymbolKind::METHOD,
            text::span_full_to_range(m.span),
            m_sel,
            None,
        ));
    }
    make_doc_sym(
        c.name.as_str().to_string(),
        None,
        kind,
        text::span_full_to_range(c.span),
        sel,
        if children.is_empty() { None } else { Some(children) },
    )
}

/// `true` when `s` is a syntactically valid ilang identifier:
/// non-empty, first char is ASCII letter or `_`, rest is ASCII
/// alphanumeric or `_`. Kept ASCII-only to match the lexer.
fn is_valid_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `true` when `s` is one of ilang's reserved keywords. Used by
/// rename validation to refuse `class`, `if`, etc. as a new name.
fn is_keyword(s: &str) -> bool {
    matches!(
        s,
        "fn" | "class"
            | "interface"
            | "enum"
            | "use"
            | "super"
            | "override"
            | "init"
            | "deinit"
            | "static"
            | "get"
            | "set"
            | "let"
            | "const"
            | "if"
            | "elif"
            | "else"
            | "while"
            | "loop"
            | "for"
            | "in"
            | "match"
            | "new"
            | "as"
            | "true"
            | "false"
            | "none"
            | "some"
            | "return"
            | "break"
            | "continue"
            | "this"
            | "pub"
            | "struct"
            | "union"
            | "async"
            | "await"
    )
}

/// Visit a parsed `Program`, emitting `SymbolInformation` for every
/// top-level decl plus class members (fields / methods / properties /
/// static_methods / static_fields) and enum variants. Filtered by
/// `q_lower` using a case-insensitive subsequence match — the same
/// fuzzy rule VSCode uses on its end of `workspace/symbol` queries,
/// so an empty query returns everything.
fn collect_workspace_symbols_from_program(
    uri: &Url,
    text: &str,
    prog: &Program,
    q_lower: &str,
    out: &mut Vec<SymbolInformation>,
    cap: usize,
) {
    for item in &prog.items {
        if out.len() >= cap {
            return;
        }
        collect_ws_item(uri, text, item, None, q_lower, out, cap);
    }
}

fn collect_ws_item(
    uri: &Url,
    text: &str,
    item: &Item,
    container: Option<&str>,
    q_lower: &str,
    out: &mut Vec<SymbolInformation>,
    cap: usize,
) {
    match item {
        Item::Fn(f) => {
            push_ws_sym(
                uri, text, f.span, "fn", f.name.as_str(),
                SymbolKind::FUNCTION, container, q_lower, out,
            );
        }
        Item::Class(c) => {
            push_ws_class(uri, text, c, container, q_lower, out, cap);
        }
        Item::Interface(i) => {
            push_ws_sym(
                uri, text, i.span, "interface", i.name.as_str(),
                SymbolKind::INTERFACE, container, q_lower, out,
            );
            for m in i.methods.iter() {
                if out.len() >= cap {
                    return;
                }
                push_ws_sym(
                    uri, text, m.span, "fn", m.name.as_str(),
                    SymbolKind::METHOD, Some(i.name.as_str()),
                    q_lower, out,
                );
            }
        }
        Item::Enum(e) => {
            push_ws_sym(
                uri, text, e.span, "enum", e.name.as_str(),
                SymbolKind::ENUM, container, q_lower, out,
            );
            for v in e.variants.iter() {
                if out.len() >= cap {
                    return;
                }
                push_ws_sym_at_span(
                    uri, v.span, v.name.as_str(),
                    SymbolKind::ENUM_MEMBER, Some(e.name.as_str()),
                    q_lower, out,
                );
            }
        }
        Item::Const(c) => {
            push_ws_sym(
                uri, text, c.span, "const", c.name.as_str(),
                SymbolKind::CONSTANT, container, q_lower, out,
            );
        }
        Item::ExternC(b) => {
            for inner in b.items.iter() {
                if out.len() >= cap {
                    return;
                }
                match inner {
                    ilang_ast::ExternCItem::FnDef(f) => {
                        push_ws_sym(
                            uri, text, f.span, "fn", f.name.as_str(),
                            SymbolKind::FUNCTION, container, q_lower, out,
                        );
                    }
                    ilang_ast::ExternCItem::FnDecl { name, span, .. } => {
                        push_ws_sym(
                            uri, text, *span, "fn", name.as_str(),
                            SymbolKind::FUNCTION, container, q_lower, out,
                        );
                    }
                    ilang_ast::ExternCItem::Class(c) => {
                        push_ws_class(uri, text, c, container, q_lower, out, cap);
                    }
                    ilang_ast::ExternCItem::Struct { name, fields, span, .. } => {
                        push_ws_sym(
                            uri, text, *span, "struct", name.as_str(),
                            SymbolKind::STRUCT, container, q_lower, out,
                        );
                        for f in fields.iter() {
                            if out.len() >= cap { return; }
                            push_ws_sym_at_span(
                                uri, f.span, f.name.as_str(),
                                SymbolKind::FIELD, Some(name.as_str()),
                                q_lower, out,
                            );
                        }
                    }
                    ilang_ast::ExternCItem::Union { name, fields, span, .. } => {
                        push_ws_sym(
                            uri, text, *span, "union", name.as_str(),
                            SymbolKind::STRUCT, container, q_lower, out,
                        );
                        for f in fields.iter() {
                            if out.len() >= cap { return; }
                            push_ws_sym_at_span(
                                uri, f.span, f.name.as_str(),
                                SymbolKind::FIELD, Some(name.as_str()),
                                q_lower, out,
                            );
                        }
                    }
                }
            }
            for iface in b.interfaces.iter() {
                if out.len() >= cap { return; }
                collect_ws_item(
                    uri, text,
                    &Item::Interface(iface.clone()),
                    container, q_lower, out, cap,
                );
            }
            for c in b.consts.iter() {
                if out.len() >= cap { return; }
                collect_ws_item(
                    uri, text,
                    &Item::Const(c.clone()),
                    container, q_lower, out, cap,
                );
            }
        }
        Item::Use(_) => {}
    }
}

fn push_ws_class(
    uri: &Url,
    text: &str,
    c: &ClassDecl,
    container: Option<&str>,
    q_lower: &str,
    out: &mut Vec<SymbolInformation>,
    cap: usize,
) {
    let kw = if c.is_union {
        "union"
    } else if c.is_repr_c {
        "struct"
    } else {
        "class"
    };
    let kind = if c.is_union || c.is_repr_c {
        SymbolKind::STRUCT
    } else {
        SymbolKind::CLASS
    };
    push_ws_sym(
        uri, text, c.span, kw, c.name.as_str(), kind, container, q_lower, out,
    );
    let class_name = c.name.as_str();
    for f in c.fields.iter() {
        if out.len() >= cap { return; }
        push_ws_sym_at_span(
            uri, f.span, f.name.as_str(),
            SymbolKind::FIELD, Some(class_name), q_lower, out,
        );
    }
    for f in c.static_fields.iter() {
        if out.len() >= cap { return; }
        let k = if f.is_const { SymbolKind::CONSTANT } else { SymbolKind::FIELD };
        push_ws_sym_at_span(
            uri, f.span, f.name.as_str(),
            k, Some(class_name), q_lower, out,
        );
    }
    for p in c.properties.iter() {
        if out.len() >= cap { return; }
        push_ws_sym_at_span(
            uri, p.span, p.name.as_str(),
            SymbolKind::PROPERTY, Some(class_name), q_lower, out,
        );
    }
    for m in c.methods.iter() {
        if out.len() >= cap { return; }
        let k = if m.name.as_str() == "init" {
            SymbolKind::CONSTRUCTOR
        } else {
            SymbolKind::METHOD
        };
        push_ws_sym(
            uri, text, m.span, "fn", m.name.as_str(),
            k, Some(class_name), q_lower, out,
        );
    }
    for m in c.static_methods.iter() {
        if out.len() >= cap { return; }
        push_ws_sym(
            uri, text, m.span, "fn", m.name.as_str(),
            SymbolKind::METHOD, Some(class_name), q_lower, out,
        );
    }
}

/// Push a `SymbolInformation` using `locate_let_name_with_kw` to
/// find the name's exact position; falls back to the decl span when
/// the locate misses.
fn push_ws_sym(
    uri: &Url,
    text: &str,
    decl_span: Span,
    kw: &str,
    name: &str,
    kind: SymbolKind,
    container: Option<&str>,
    q_lower: &str,
    out: &mut Vec<SymbolInformation>,
) {
    if !subsequence_ci(name, q_lower) {
        return;
    }
    let name_span = text::locate_let_name_with_kw(text, decl_span, kw, name)
        .unwrap_or(decl_span);
    let range = text::span_to_range(name_span, name.len());
    push_sym(uri, range, name, kind, container, out);
}

/// Variant of `push_ws_sym` for decls whose `span` already sits at
/// the name (struct / union fields, enum variants, properties).
fn push_ws_sym_at_span(
    uri: &Url,
    name_span: Span,
    name: &str,
    kind: SymbolKind,
    container: Option<&str>,
    q_lower: &str,
    out: &mut Vec<SymbolInformation>,
) {
    if !subsequence_ci(name, q_lower) {
        return;
    }
    let range = text::span_to_range(name_span, name.len());
    push_sym(uri, range, name, kind, container, out);
}

#[allow(deprecated)]
fn push_sym(
    uri: &Url,
    range: Range,
    name: &str,
    kind: SymbolKind,
    container: Option<&str>,
    out: &mut Vec<SymbolInformation>,
) {
    out.push(SymbolInformation {
        name: name.to_string(),
        kind,
        tags: None,
        deprecated: None,
        location: Location { uri: uri.clone(), range },
        container_name: container.map(|s| s.to_string()),
    });
}

fn render_fn_detail(f: &FnDecl) -> String {
    let params = f
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, p.ty))
        .collect::<Vec<_>>()
        .join(", ");
    match &f.ret {
        Some(t) => format!("({params}): {t}"),
        None => format!("({params})"),
    }
}

