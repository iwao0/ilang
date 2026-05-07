mod builtins;
mod formatter;
mod project;
mod text;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ilang_ast::{
    Block, ClassDecl, Expr, ExprKind, FnDecl, Item, Pattern, PatternBindings, PatternKind,
    Program, Span, Stmt, StmtKind, Symbol as AstSymbol, Type, UnOp, VariantPayload,
};
use ilang_lexer::tokenize;
use ilang_parser::parse;
use ilang_types::TypeChecker;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use builtins::{
    array_method_names, array_method_sig, ffi_helper_signature, string_method_names,
    string_method_sig,
};
use project::{collect_dep_paths, find_umbrella};
use text::{
    call_context_at, locate_dot_name, locate_let_name, locate_let_name_with_kw,
    locate_property_name, locate_selective_name, parameter_offsets, receiver_before_dot,
    span_full_to_range, span_to_range, word_at,
};

#[derive(Clone, Debug)]
struct Symbol {
    name: String,
    span: Span,
    signature: String,
    /// `///`-prefixed doc comment lines immediately above the decl.
    doc: Option<String>,
}

#[derive(Clone, Debug)]
struct ClassInfo {
    decl_span: Span,
    fields: HashMap<AstSymbol, MemberInfo>,
    methods: HashMap<AstSymbol, MemberInfo>,
    /// Per-property getter signature, used at read sites (`p.name`).
    /// Falls back to `fields` when the property is set-only.
    getters: HashMap<AstSymbol, MemberInfo>,
    /// Per-property setter signature, used at write sites
    /// (`p.name = v`). Falls back to `fields` when the property is
    /// get-only.
    setters: HashMap<AstSymbol, MemberInfo>,
    /// `true` for classes pulled in via `use module`. Their member
    /// `MemberInfo.span` values are line/col into another file we
    /// don't carry, so F12 must stay at the use site.
    external: bool,
    /// Number of `init` overloads declared on the class. Used to
    /// append `(+N overloads)` to the constructor hover.
    init_overloads: usize,
    /// All `init` overload signatures in declaration order, used by
    /// signature help on `new ClassName(...)`.
    inits: Vec<MemberInfo>,
    /// Decl keyword used in source (`class` / `struct` / `union`).
    /// Drives hover phrasing for ARC classes vs `@extern(C)` structs.
    kind: ClassKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClassKind {
    Class,
    Struct,
    Union,
}

impl ClassKind {
    fn keyword(self) -> &'static str {
        match self {
            ClassKind::Class => "class",
            ClassKind::Struct => "struct",
            ClassKind::Union => "union",
        }
    }
}

#[derive(Clone, Debug)]
struct MemberInfo {
    span: Span,
    signature: String,
    /// For methods: the declared return type. For fields: the field
    /// type. Used to infer `let x = obj.method(...)`.
    ret_ty: Option<Type>,
    /// `true` for `static` fields / methods. Drives `Counter.<.>`
    /// completion (which should only list static members).
    is_static: bool,
    /// `///`-prefixed doc comment lines above the member.
    doc: Option<String>,
}

#[derive(Clone, Debug)]
struct RefEntry {
    line: u32,
    start_col: u32,
    end_col: u32,
    target_span: Span,
    target_name_len: u32,
    signature: String,
    /// `true` when we don't have a real source-file location for the
    /// definition (imported member, built-in, etc). F12 returns no
    /// definition rather than navigating to the use site, which VSCode
    /// reports as "no references found".
    no_definition: bool,
    /// Cross-file F12 target. When set, F12 navigates to this URI at
    /// `target_span` instead of the current document. Used for
    /// `use module`-imported decls whose source lives in another file.
    target_uri: Option<Url>,
    doc: Option<String>,
}

#[derive(Clone, Default)]
struct Doc {
    text: String,
    /// Top-level decls keyed by name.
    symbols: HashMap<AstSymbol, Symbol>,
    /// Per-class field/method index (used when resolving `this.x`).
    #[allow(dead_code)]
    classes: HashMap<AstSymbol, ClassInfo>,
    /// Resolved references with precise spans. Sorted by (line, start_col).
    refs: Vec<RefEntry>,
    /// Variable name → class name, for completion on `obj.`. Populated
    /// from let / param bindings whose static type resolves to a known
    /// class. Last-write-wins across scopes — good enough for most
    /// completion contexts.
    var_classes: HashMap<AstSymbol, String>,
    /// Variable name → full ilang type. Drives `obj.` completion for
    /// non-class types (string / array) so their built-in methods show
    /// up.
    var_types: HashMap<AstSymbol, Type>,
    /// Hover-only signatures for names imported via `use module` (e.g.
    /// `math.sqrt`, `math.pi`). The loader prefixes imported items
    /// with the module name, so this map keyed on `module.fn_name`
    /// catches references the buffer-only walker can't resolve.
    /// F12 to these is not supported because we don't carry per-decl
    /// file paths.
    #[allow(dead_code)]
    external_signatures: HashMap<AstSymbol, String>,
    /// Doc comments (`///`) attached to imported `module.X` decls.
    /// Same key shape as `external_signatures`.
    external_docs: HashMap<AstSymbol, String>,
    /// Return types for `module.fn` declarations brought in via
    /// `use module`. Populated alongside `external_signatures` so
    /// `let x = math.sqrt(...)` infers as f64.
    #[allow(dead_code)]
    external_returns: HashMap<AstSymbol, Type>,
}

struct Backend {
    client: Client,
    docs: Arc<Mutex<HashMap<Url, Doc>>>,
    /// Latest document version per URI, used to drop stale
    /// `did_change` events. Each `did_change` bumps the entry, then
    /// schedules a debounced refresh; when the timer fires we skip
    /// the work if a newer version has arrived in the meantime.
    latest_versions: Arc<Mutex<HashMap<Url, i32>>>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            docs: Arc::new(Mutex::new(HashMap::new())),
            latest_versions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn refresh(&self, uri: Url, text: String) {
        refresh_impl(&self.client, &self.docs, uri, text).await
    }
}

async fn refresh_impl(
    client: &Client,
    docs: &Mutex<HashMap<Url, Doc>>,
    uri: Url,
    text: String,
) {
    let path = uri.to_file_path().ok();
    // Sub-modules (re-exported by an umbrella sibling like
    // `sdl_window.il` ← `sdl.il`) can't be type-checked alone —
    // their bare `sdl.X` references only resolve inside an entry
    // that does `use sdl`. Skip load-based diagnostics in that
    // case; only syntax errors from the buffer survive.
    let is_submodule = path.as_deref().and_then(find_umbrella).is_some();
    // Parse the buffer once up front. The loader injects the buffer
    // as an overlay, so a buffer that doesn't parse makes the whole
    // merged-program load fail anyway — skipping it here saves the
    // file IO + tokenize + parse for every imported module on each
    // mid-edit refresh (the common case while typing).
    let parsed_buffer = parse_ok(&text);
    let merged = if is_submodule || parsed_buffer.is_err() {
        None
    } else {
        path.as_deref()
            .filter(|p| p.exists())
            .and_then(|p| {
                let extra = collect_dep_paths(p).unwrap_or_default();
                // Use the buffer's text for the entry file so
                // diagnostics reflect unsaved edits immediately.
                let mut overlay: HashMap<PathBuf, String> = HashMap::new();
                if let Ok(canon) = p.canonicalize() {
                    overlay.insert(canon, text.clone());
                }
                ilang_parser::loader::load_program_with_overlay(p, &extra, &overlay).ok()
            })
    };
    let diags = analyse(&text, path.as_deref(), &merged, is_submodule);
    let (mut external_sigs, external_rets) = merged
        .as_ref()
        .map(collect_external_signatures)
        .unwrap_or_default();
    // Augment with `module.const_name` entries — the loader inlines
    // constants away, so they're not in the merged program. Parse
    // each `use module` source separately to recover them.
    let mut external_sources: ExternalSources = HashMap::new();
    let mut external_docs: HashMap<AstSymbol, String> = HashMap::new();
    // Harvest imports from the buffer's `use module` items even
    // without a saved file — built-in modules (math/test/os) still
    // resolve, and on-disk modules resolve relative to the entry
    // directory when we have one.
    let harvest_anchor = path
        .as_deref()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/__lsp_buffer__.il"));
    harvest_imported_consts(
        &harvest_anchor,
        &text,
        &mut external_sigs,
        &mut external_sources,
        &mut external_docs,
    );
    let external_classes = merged
        .as_ref()
        .map(|p| collect_external_classes(p, &external_sources))
        .unwrap_or_default();
    // When the buffer parses cleanly, rebuild the doc from scratch.
    // Otherwise (mid-edit, e.g. just typed `.`), keep the previous
    // doc's classes/symbols so completion / hover still work, and
    // patch only the fields that changed — cloning the entire prev
    // Doc just to swap a few fields was the single largest cost on
    // the keystroke path when the buffer had a transient parse
    // error (which is most of the time during typing).
    match parsed_buffer {
        Ok(prog) => {
            let mut d = build_doc(
                text,
                &prog,
                &external_sigs,
                &external_rets,
                &external_classes,
                &external_sources,
                &external_docs,
            );
            d.external_docs = external_docs;
            let mut docs_lock = docs.lock().unwrap();
            // If the user typed more characters between the start
            // of this refresh and now, `did_change` will have
            // written the newer text into `docs[uri].text`
            // synchronously. Keep that — overwriting it with our
            // (now-stale) `text` would cause cursor-context queries
            // to read characters that no longer match the editor.
            if let Some(existing) = docs_lock.get(&uri) {
                if existing.text != d.text {
                    d.text = existing.text.clone();
                }
            }
            docs_lock.insert(uri.clone(), d);
        }
        Err(_) => {
            let mut docs_lock = docs.lock().unwrap();
            let entry = docs_lock.entry(uri.clone()).or_default();
            // `did_change` always seeds `entry.text` synchronously
            // before spawning a refresh, so by the time we get here
            // the live text is already there and we can leave it
            // alone. The empty-text guard covers `did_open` of a
            // file whose initial buffer fails to parse.
            if entry.text.is_empty() {
                entry.text = text;
            }
            if !external_sigs.is_empty() {
                entry.external_signatures = external_sigs;
            }
            if !external_rets.is_empty() {
                entry.external_returns = external_rets;
            }
            if !external_docs.is_empty() {
                entry.external_docs = external_docs;
            }
        }
    }
    client.publish_diagnostics(uri, diags, None).await;
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
            refresh_impl(&client, &docs, uri, text).await;
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
            if let Some(sym) = doc.symbols.get(&AstSymbol::intern(&word)) {
                let range = span_to_range(sym.span, sym.name.as_str().len());
                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri,
                    range,
                })));
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
            if let Some(ty) = doc.var_types.get(&AstSymbol::intern(&receiver)) {
                let entries: Vec<(String, String)> = match ty {
                    Type::Str => string_method_names()
                        .into_iter()
                        .filter_map(|n| string_method_sig(n).map(|s| (n.to_string(), s)))
                        .collect(),
                    Type::Array { elem, fixed } => array_method_names()
                        .into_iter()
                        .filter(|n| {
                            // Fixed-length arrays can't grow / shrink.
                            !(fixed.is_some() && matches!(**n, "push" | "pop"))
                        })
                        .filter_map(|n| {
                            array_method_sig(n, elem).map(|s| (n.to_string(), s))
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                if !entries.is_empty() {
                    let mut items: Vec<CompletionItem> = entries
                        .into_iter()
                        .map(|(name, sig)| {
                            let (insert_text, fmt) =
                                call_snippet(name.as_str(), CompletionItemKind::METHOD);
                            let command =
                                trigger_sig_help_command(CompletionItemKind::METHOD);
                            CompletionItem {
                                label: name.as_str().to_string(),
                                kind: Some(CompletionItemKind::METHOD),
                                detail: Some(sig.as_str().to_string()),
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
            if name == "init" {
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
                    let builtin = match doc.var_types.get(&AstSymbol::intern(recv)) {
                        Some(Type::Str) => string_method_sig(method),
                        Some(Type::Array { elem, .. }) => {
                            array_method_sig(method, elem)
                        }
                        _ => None,
                    };
                    if let Some(sig) = builtin {
                        out.push(MemberInfo {
                            span: Span::dummy(),
                            signature: sig,
                            ret_ty: None,
                            is_static: false,
                doc: None,
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
        // Resolve the cursor to a (target_span, name_len) pair —
        // the unique identity of the symbol the user clicked. This
        // pins the rename to a single declaration even when a name
        // (`a`) is reused across unrelated decls (an `@extern(C)
        // fn a` and a `Counter.a` property both exist).
        // (target_span, name_len) identifies the decl that all
        // refs share. `decl_name_span` is the span of the *name*
        // itself (sliding past keywords like `fn` / `class`) — used
        // only when emitting the decl-site edit, since the AST
        // anchors fn / class spans at the keyword, not the name.
        let (target, decl_name_span) = if let Some(entry) = lookup_ref(doc, pos)
        {
            // `this` is a keyword — its RefEntry shares (target_span,
            // target_name_len) with the enclosing class, so letting
            // the rename through would also rewrite every reference
            // to the class. Refuse instead of silently corrupting
            // the file.
            if entry.signature.starts_with("this:") {
                return Ok(None);
            }
            (
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
                ((sym.span, sym.name.as_str().len() as u32), name_span)
            } else {
                return Ok(None);
            }
        } else {
            return Ok(None);
        };
        // Collect every RefEntry that resolves to this exact decl.
        // Skip refs that point cross-file (`target_uri` is set) —
        // we don't have a workspace-wide index yet, so the rename
        // stays single-file.
        let mut edits: Vec<TextEdit> = doc
            .refs
            .iter()
            .filter(|r| {
                r.target_uri.is_none()
                    && r.target_span == target.0
                    && r.target_name_len == target.1
                    // `this` refs share their target with the
                    // enclosing class — exclude them so renaming the
                    // class doesn't rewrite `this` to the new name.
                    && !r.signature.starts_with("this:")
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
        // Always include the decl site itself. The walker does not
        // push a `RefEntry` for plain top-level fn / class member
        // names, so without this an unused decl would yield zero
        // edits and VSCode would refuse the rename.
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
        // VSCode dedups identical edits, but be defensive — each
        // (line, start_col) combination should appear once.
        edits.sort_by(|a, b| {
            (a.range.start.line, a.range.start.character)
                .cmp(&(b.range.start.line, b.range.start.character))
        });
        edits.dedup_by(|a, b| a.range == b.range);
        let mut changes = HashMap::new();
        changes.insert(uri, edits);
        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }))
    }

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }
}

fn make_hover_with_doc(sig: &str, doc: Option<&str>) -> Hover {
    let value = match doc {
        Some(d) if !d.is_empty() => format!("```ilang\n{sig}\n```\n\n{d}"),
        _ => format!("```ilang\n{sig}\n```"),
    };
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: None,
    }
}

fn lookup_ref(doc: &Doc, pos: Position) -> Option<&RefEntry> {
    let line = pos.line + 1;
    let col = pos.character + 1;
    doc.refs
        .iter()
        .find(|r| r.line == line && col >= r.start_col && col <= r.end_col)
}

fn parse_ok(src: &str) -> Result<Program, ()> {
    let tokens = tokenize(src).map_err(|_| ())?;
    parse(&tokens).map_err(|_| ())
}

fn analyse(
    src: &str,
    path: Option<&Path>,
    merged: &Option<Program>,
    is_submodule: bool,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    // Always run the lex + parse pass on the in-memory buffer first so
    // unsaved edits surface syntax errors immediately.
    let tokens = match tokenize(src) {
        Ok(t) => t,
        Err(e) => {
            out.push(diag(e.span(), e.to_string()));
            return out;
        }
    };
    if let Err(e) = parse(&tokens) {
        out.push(diag(e.span(), e.to_string()));
        return out;
    }
    // Sub-modules can't resolve cross-module references on their own;
    // typecheck would emit spurious "undefined class sdl.X" errors.
    // Stop after lex + parse for those.
    if is_submodule {
        return out;
    }
    if let Some(prog) = merged {
        let mut tc = TypeChecker::new();
        if let Err(e) = tc.check(prog) {
            out.push(diag(e.span(), e.to_string()));
        }
        return out;
    }
    // Fallback: in-memory parse + typecheck (no module resolution, no
    // const inlining). Used for unsaved buffers without an on-disk file
    // or when loading failed (the load error itself is reported by the
    // caller via `refresh`).
    let _ = path;
    let prog = parse(&tokens).expect("parse already validated");
    let mut tc = TypeChecker::new();
    if let Err(e) = tc.check(&prog) {
        out.push(diag(e.span(), e.to_string()));
    }
    out
}

/// Pull top-level names with prefix-style identifiers (e.g.
/// `math.sqrt`, `math.pi`) out of a loader-merged program so the LSP
/// can answer hover queries on imported names. Plain (un-dotted) names
/// are skipped — they're already covered by the buffer-only index when
/// declared in the open file.
/// Per-decl source location for `module.<decl>` references — used by
/// cross-file F12 to land on the actual declaration line.
#[derive(Clone, Debug)]
struct ExternalLoc {
    path: PathBuf,
    span: Span,
    name_len: u32,
}
type ExternalSources = HashMap<AstSymbol, ExternalLoc>;

/// Walk the buffer's `use module` items and parse each module's source
/// (built-in or on-disk) to extract `Item::Const` declarations. Insert
/// them into `out` keyed by `module.const_name` so the buffer-only
/// walker can still resolve `math.pi` etc. — the main loader pass
/// would have inlined them. Also returns a `module.ClassName` → file
/// path map so cross-file F12 can navigate to the actual definition.
fn harvest_imported_consts(
    entry_path: &Path,
    entry_src: &str,
    out: &mut HashMap<AstSymbol, String>,
    sources: &mut ExternalSources,
    docs: &mut HashMap<AstSymbol, String>,
) {
    let Ok(tokens) = tokenize(entry_src) else { return };
    if let Ok(prog) = parse(&tokens) {
        harvest_from_program(&prog, entry_path, out, sources, docs);
        return;
    }
    use ilang_lexer::TokenKind;
    let extra = collect_dep_paths(entry_path).unwrap_or_default();
    let entry_dir = entry_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut i = 0;
    while i < tokens.len() {
        if matches!(tokens[i].kind, TokenKind::Use) {
            if let Some(t) = tokens.get(i + 1) {
                if let TokenKind::Ident(name) = &t.kind {
                    walk_module(name, &entry_dir, &extra, &mut visited, out, sources, docs);
                    i += 2;
                    continue;
                }
            }
        }
        i += 1;
    }
}

fn harvest_from_program(
    prog: &Program,
    entry_path: &Path,
    out: &mut HashMap<AstSymbol, String>,
    sources: &mut ExternalSources,
    docs: &mut HashMap<AstSymbol, String>,
) {
    let extra = collect_dep_paths(entry_path).unwrap_or_default();
    let entry_dir = entry_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let mut visited: HashSet<PathBuf> = HashSet::new();
    for item in &prog.items {
        let Item::Use(u) = item else { continue };
        if let Some(names) = &u.selective {
            // `use M { X1, X2 }` — pull X1/X2's hover info from M
            // (or its `@export use` chain) and key them under their
            // bare name so the buffer-side walker can resolve a
            // bare `Var("X1")` reference.
            harvest_selective_names(
                u.module.as_str(),
                names,
                &entry_dir,
                &extra,
                out,
                sources,
                docs,
            );
            continue;
        }
        walk_module(u.module.as_str(), &entry_dir, &extra, &mut visited, out, sources, docs);
    }
}

/// Resolve each `name` in `names` against `module` (which may be an
/// umbrella that re-exports its members via `@export use`) and
/// register a bare-keyed entry under `name` in `out` / `sources` /
/// `docs`. This lets the buffer-side walker treat a bare `Var("X")`
/// from `use M { X }` exactly like a dotted `Var("M.X")`.
///
/// Lookups consult the outer maps first — by the time this is called
/// from `harvest_from_program`, the merged-program scan and any
/// preceding whole-module `walk_module` runs have already populated
/// the prefixed entries we need. The local `walk_module` pass then
/// fills in `module.X` keys that the merged-program scan misses
/// (`Item::Const` is inlined out of the merged program; umbrella
/// `walk_module_aliased` only registers consts in `out`).
fn harvest_selective_names(
    module: &str,
    names: &[AstSymbol],
    entry_dir: &Path,
    extra: &[PathBuf],
    out: &mut HashMap<AstSymbol, String>,
    sources: &mut ExternalSources,
    docs: &mut HashMap<AstSymbol, String>,
) {
    let mut visited: HashSet<PathBuf> = HashSet::new();
    walk_module(module, entry_dir, extra, &mut visited, out, sources, docs);
    for name in names {
        let prefixed = AstSymbol::intern(&format!("{module}.{name}"));
        if let Some(sig) = out.get(&prefixed).cloned() {
            out.insert(name.clone(), sig);
        }
        if let Some(loc) = sources.get(&prefixed).cloned() {
            sources.insert(name.clone(), loc);
        }
        if let Some(d) = docs.get(&prefixed).cloned() {
            docs.insert(name.clone(), d);
        }
        // Selectively-imported enums also expose `<bare>.<variant>`
        // composite keys so `Field { obj: Var(bare), name: variant }`
        // resolves through the same lookup path as `module.Enum.X`.
        let prefix_dot = format!("{module}.{name}.");
        let bare_dot = format!("{name}.");
        let extra_sigs: Vec<(AstSymbol, String)> = out
            .iter()
            .filter_map(|(k, v)| {
                k.as_str()
                    .strip_prefix(&prefix_dot)
                    .map(|tail| (AstSymbol::intern(&format!("{bare_dot}{tail}")), v.clone()))
            })
            .collect();
        for (k, v) in extra_sigs {
            out.insert(k, v);
        }
        let extra_sources: Vec<(AstSymbol, ExternalLoc)> = sources
            .iter()
            .filter_map(|(k, v)| {
                k.as_str()
                    .strip_prefix(&prefix_dot)
                    .map(|tail| (AstSymbol::intern(&format!("{bare_dot}{tail}")), v.clone()))
            })
            .collect();
        for (k, v) in extra_sources {
            sources.insert(k, v);
        }
        let extra_docs: Vec<(AstSymbol, String)> = docs
            .iter()
            .filter_map(|(k, v)| {
                k.as_str()
                    .strip_prefix(&prefix_dot)
                    .map(|tail| (AstSymbol::intern(&format!("{bare_dot}{tail}")), v.clone()))
            })
            .collect();
        for (k, v) in extra_docs {
            docs.insert(k, v);
        }
    }
}

fn walk_module(
    prefix: &str,
    entry_dir: &Path,
    extra: &[PathBuf],
    visited: &mut HashSet<PathBuf>,
    out: &mut HashMap<AstSymbol, String>,
    sources: &mut ExternalSources,
    docs: &mut HashMap<AstSymbol, String>,
) {
    let (module_path, module_src) =
        if let Some(s) = ilang_parser::loader::builtin_module_source(prefix) {
            (
                PathBuf::from(format!("<builtin>/{prefix}.il")),
                s.to_string(),
            )
        } else {
            let mut candidates = vec![entry_dir.to_path_buf()];
            candidates.extend(extra.iter().cloned());
            let Some((p, s)) = candidates.into_iter().find_map(|d| {
                let p = d.join(format!("{prefix}.il"));
                std::fs::read_to_string(&p).ok().map(|s| (p, s))
            }) else {
                return;
            };
            (p, s)
        };
    if !visited.insert(module_path.clone()) {
        return;
    }
    // F12 on the module name itself (e.g. `sdl` in `use sdl` or
    // `new sdl.Window()`) navigates to the start of the module file.
    sources.entry(prefix.into()).or_insert(ExternalLoc {
        path: module_path.clone(),
        span: Span::new(1, 1),
        name_len: 0,
    });
    let Ok(tokens) = tokenize(&module_src) else { return };
    let Ok(mod_prog) = parse(&tokens) else { return };
    let mod_dir = module_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let track = |key: &str,
                 span: Span,
                 name_len: u32,
                 sources: &mut ExternalSources,
                 p: &PathBuf| {
        sources.insert(
            key.into(),
            ExternalLoc {
                path: p.clone(),
                span,
                name_len,
            },
        );
    };
    for it in &mod_prog.items {
        match it {
            Item::Const(c) => {
                let ty = match &c.ty {
                    Some(t) => format!(": {t}"),
                    None => String::new(),
                };
                let value = render_const_value(&c.value)
                    .map(|v| format!(" = {v}"))
                    .unwrap_or_default();
                let key = format!("{prefix}.{}", c.name);
                out.insert(AstSymbol::intern(&key), format!("const {key}{ty}{value}"));
                track(&key, c.span, c.name.as_str().len() as u32, sources, &module_path);
                if let Some(d) = text::extract_doc_above(&module_src, c.span.line) {
                    docs.insert(AstSymbol::intern(&key), d);
                }
            }
            Item::Fn(f) => {
                let key = format!("{prefix}.{}", f.name);
                let sig = format!("fn {}", fn_body(f));
                out.insert(AstSymbol::intern(&key), format!("fn {}", sig.trim_start_matches("fn ")));
                track(&key, f.span, f.name.as_str().len() as u32, sources, &module_path);
                if let Some(d) = text::extract_doc_above(&module_src, f.span.line) {
                    docs.insert(AstSymbol::intern(&key), d);
                }
            }
            Item::Class(c) => {
                let key = format!("{prefix}.{}", c.name);
                out.insert(AstSymbol::intern(&key), format!("class {key}"));
                track(&key, c.span, c.name.as_str().len() as u32, sources, &module_path);
                if let Some(d) = text::extract_doc_above(&module_src, c.span.line) {
                    docs.insert(AstSymbol::intern(&key), d);
                }
            }
            Item::Enum(e) => {
                let key = format!("{prefix}.{}", e.name);
                out.insert(AstSymbol::intern(&key), format!("enum {key}"));
                track(&key, e.span, e.name.as_str().len() as u32, sources, &module_path);
                if let Some(d) = text::extract_doc_above(&module_src, e.span.line) {
                    docs.insert(AstSymbol::intern(&key), d);
                }
                register_enum_variants_with_sources(e, &key, out, sources, &module_path);
            }
            Item::ExternC(b) => {
                for inner in &b.items {
                    let (n, span, sig): (AstSymbol, Span, String) = match inner {
                        ilang_ast::ExternCItem::FnDecl {
                            name, span, params, ret, libs, ..
                        } => {
                            let ps = params
                                .iter()
                                .map(|p| format!("{}: {}", p.name, p.ty))
                                .collect::<Vec<_>>()
                                .join(", ");
                            let r = match ret {
                                Some(t) => format!(": {t}"),
                                None => String::new(),
                            };
                            let libs_prefix = if libs.is_empty() {
                                String::new()
                            } else {
                                let names = libs
                                    .iter()
                                    .map(|l| format!("\"{l}\""))
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                format!("@lib({names}) ")
                            };
                            (
                                *name,
                                *span,
                                format!("{libs_prefix}fn {prefix}.{name}({ps}){r}"),
                            )
                        }
                        ilang_ast::ExternCItem::FnDef(f) => (
                            f.name.into(),
                            f.span,
                            format!("fn {prefix}.{} {}", f.name, fn_body(f)).trim_start_matches("fn ").to_string(),
                        ),
                        ilang_ast::ExternCItem::Static { name, span, ty, .. } => (
                            *name,
                            *span,
                            format!("static {prefix}.{name}: {ty}"),
                        ),
                        ilang_ast::ExternCItem::Struct { name, span, .. } => (
                            *name,
                            *span,
                            format!("struct {prefix}.{name}"),
                        ),
                        ilang_ast::ExternCItem::Union { name, span, .. } => (
                            *name,
                            *span,
                            format!("union {prefix}.{name}"),
                        ),
                        ilang_ast::ExternCItem::Class(c) => (
                            c.name.into(),
                            c.span,
                            format!("class {prefix}.{}", c.name),
                        ),
                    };
                    let key = format!("{prefix}.{n}");
                    out.insert(AstSymbol::intern(&key), sig);
                    track(&key, span, n.as_str().len() as u32, sources, &module_path);
                    if let Some(d) = text::extract_doc_above(&module_src, span.line) {
                        docs.insert(AstSymbol::intern(&key), d);
                    }
                }
            }
            // Follow `@export use` chains so umbrella modules
            // (e.g. `sdl.il` re-exporting `sdl_renderer.il`) flow the
            // prefix through to the file that actually declares the
            // class.
            Item::Use(u) if u.re_export && u.selective.is_none() => {
                walk_module(
                    &format!("{prefix}.{}", u.module),
                    &mod_dir,
                    extra,
                    visited,
                    out,
                    sources,
                    docs,
                );
                // Loader collapses one-deep umbrella prefixes so the
                // entry sees `sdl.X` (not `sdl.sdl_renderer.X`). Mirror
                // that: also record the umbrella's own prefix.
                walk_module_aliased(
                    prefix,
                    u.module.as_str(),
                    &mod_dir,
                    extra,
                    visited,
                    out,
                    sources,
                    docs,
                );
            }
            _ => {}
        }
    }
}

fn walk_module_aliased(
    alias_prefix: &str,
    actual: &str,
    entry_dir: &Path,
    extra: &[PathBuf],
    visited: &mut HashSet<PathBuf>,
    out: &mut HashMap<AstSymbol, String>,
    sources: &mut ExternalSources,
    docs: &mut HashMap<AstSymbol, String>,
) {
    let (module_path, module_src) =
        if let Some(s) = ilang_parser::loader::builtin_module_source(actual) {
            (
                PathBuf::from(format!("<builtin>/{actual}.il")),
                s.to_string(),
            )
        } else {
            let mut candidates = vec![entry_dir.to_path_buf()];
            candidates.extend(extra.iter().cloned());
            let Some((p, s)) = candidates.into_iter().find_map(|d| {
                let p = d.join(format!("{actual}.il"));
                std::fs::read_to_string(&p).ok().map(|s| (p, s))
            }) else {
                return;
            };
            (p, s)
        };
    let Ok(tokens) = tokenize(&module_src) else { return };
    let Ok(mod_prog) = parse(&tokens) else { return };
    let put = |key: &str, span: Span, name_len: u32, sources: &mut ExternalSources| {
        sources.insert(
            key.into(),
            ExternalLoc {
                path: module_path.clone(),
                span,
                name_len,
            },
        );
    };
    for it in &mod_prog.items {
        match it {
            Item::Const(c) => {
                let key = format!("{alias_prefix}.{}", c.name);
                let ty = match &c.ty {
                    Some(t) => format!(": {t}"),
                    None => String::new(),
                };
                let value = render_const_value(&c.value)
                    .map(|v| format!(" = {v}"))
                    .unwrap_or_default();
                out.insert(AstSymbol::intern(&key), format!("const {key}{ty}{value}"));
                put(&key, c.span, c.name.as_str().len() as u32, sources);
                if let Some(d) = text::extract_doc_above(&module_src, c.span.line) {
                    docs.insert(AstSymbol::intern(&key), d);
                }
            }
            Item::Fn(f) => {
                let key = format!("{alias_prefix}.{}", f.name);
                put(&key, f.span, f.name.as_str().len() as u32, sources);
                if let Some(d) = text::extract_doc_above(&module_src, f.span.line) {
                    docs.insert(AstSymbol::intern(&key), d);
                }
            }
            Item::Class(c) => {
                let key = format!("{alias_prefix}.{}", c.name);
                put(&key, c.span, c.name.as_str().len() as u32, sources);
                if let Some(d) = text::extract_doc_above(&module_src, c.span.line) {
                    docs.insert(AstSymbol::intern(&key), d);
                }
            }
            Item::Enum(e) => {
                let key = format!("{alias_prefix}.{}", e.name);
                out.insert(AstSymbol::intern(&key), format!("enum {key}"));
                put(&key, e.span, e.name.as_str().len() as u32, sources);
                if let Some(d) = text::extract_doc_above(&module_src, e.span.line) {
                    docs.insert(AstSymbol::intern(&key), d);
                }
                register_enum_variants_with_sources(e, &key, out, sources, &module_path);
            }
            Item::ExternC(b) => {
                for inner in &b.items {
                    let entry = match inner {
                        ilang_ast::ExternCItem::FnDecl { name, span, .. } => {
                            Some((name.clone(), *span))
                        }
                        ilang_ast::ExternCItem::FnDef(f) => Some((f.name.clone(), f.span)),
                        ilang_ast::ExternCItem::Static { name, span, .. } => {
                            Some((name.clone(), *span))
                        }
                        ilang_ast::ExternCItem::Struct { name, span, .. } => {
                            Some((name.clone(), *span))
                        }
                        ilang_ast::ExternCItem::Union { name, span, .. } => {
                            Some((name.clone(), *span))
                        }
                        ilang_ast::ExternCItem::Class(c) => Some((c.name.clone(), c.span)),
                    };
                    if let Some((n, span)) = entry {
                        let len = n.as_str().len() as u32;
                        let key = format!("{alias_prefix}.{n}");
                        put(&key, span, len, sources);
                        if let Some(d) = text::extract_doc_above(&module_src, span.line) {
                            docs.insert(AstSymbol::intern(&key), d);
                        }
                    }
                }
            }
            Item::Use(u) if u.re_export && u.selective.is_none() => {
                let mod_dir = module_path
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."));
                walk_module_aliased(
                    alias_prefix,
                    u.module.as_str(),
                    &mod_dir,
                    extra,
                    visited,
                    out,
                    sources,
                    docs,
                );
            }
            _ => {}
        }
    }
}

/// Walk a loader-merged program for dotted-name classes (e.g.
/// `sdl.Window`) so the hover walker can resolve method / field
/// accesses on imported types. `sources` carries each prefixed
/// name's file path so we can read the source and lift field doc
/// comments — the merged Program itself doesn't carry source
/// strings.
fn collect_external_classes(
    prog: &Program,
    sources: &ExternalSources,
) -> HashMap<AstSymbol, ClassInfo> {
    use ilang_ast::ExternCItem;
    let mut classes: Vec<&ClassDecl> = Vec::new();
    let mut out: HashMap<AstSymbol, ClassInfo> = HashMap::new();
    let mut src_cache: HashMap<PathBuf, String> = HashMap::new();
    let mut field_doc = |class_key: &AstSymbol, field_span: Span| -> Option<String> {
        let path = &sources.get(class_key)?.path;
        if !src_cache.contains_key(path) {
            let txt = std::fs::read_to_string(path).ok()?;
            src_cache.insert(path.clone(), txt);
        }
        text::extract_doc_above(src_cache.get(path)?.as_str(), field_span.line)
    };
    for item in &prog.items {
        match item {
            Item::Class(c) if c.name.as_str().contains('.') => classes.push(c),
            Item::ExternC(b) => {
                for inner in &b.items {
                    match inner {
                        ExternCItem::Class(c) if c.name.as_str().contains('.') => classes.push(c),
                        ExternCItem::Struct { name, fields: fs, span, .. }
                        | ExternCItem::Union { name, fields: fs, span, .. }
                            if name.as_str().contains('.') =>
                        {
                            let kind = matches!(
                                inner,
                                ExternCItem::Struct { .. }
                            )
                                .then_some(ClassKind::Struct)
                                .unwrap_or(ClassKind::Union);
                            let mut fields = HashMap::new();
                            for f in fs {
                                fields.insert(
                                    f.name.into(),
                                    MemberInfo {
                                        span: f.span,
                                        signature: format!(
                                            "(property) {}.{}: {}",
                                            name, f.name, f.ty
                                        ),
                                        ret_ty: Some(f.ty.clone()),
                                        is_static: false,
                                        doc: field_doc(name, f.span),
                                    },
                                );
                            }
                            out.insert(
                                name.clone(),
                                ClassInfo {
                                    decl_span: *span,
                                    fields,
                                    methods: HashMap::new(),
                                    getters: HashMap::new(),
                                    setters: HashMap::new(),
                                    external: true,
                                    init_overloads: 0,
                                    inits: Vec::new(),
                                    kind,
                                },
                            );
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    for c in classes {
        let mut fields = HashMap::new();
        for f in &c.fields {
            fields.insert(
                f.name.into(),
                MemberInfo {
                    span: f.span,
                    signature: format!("(property) {}.{}: {}", c.name, f.name, f.ty),
                    ret_ty: Some(f.ty.clone()),
                    is_static: false,
                    doc: field_doc(&c.name, f.span),
                },
            );
        }
        for f in &c.static_fields {
            fields.insert(
                f.name.into(),
                MemberInfo {
                    span: f.span,
                    signature: format!(
                        "(static property) {}.{}: {}",
                        c.name, f.name, f.ty
                    ),
                    ret_ty: Some(f.ty.clone()),
                    is_static: true,
                    doc: field_doc(&c.name, f.span),
                },
            );
        }
        let mut getters: HashMap<AstSymbol, MemberInfo> = HashMap::new();
        let mut setters: HashMap<AstSymbol, MemberInfo> = HashMap::new();
        for prop in &c.properties {
            let prop_doc = field_doc(&c.name, prop.span);
            fields.insert(
                prop.name.into(),
                MemberInfo {
                    span: prop.span,
                    signature: format!("(property) {}.{}: {}", c.name, prop.name, prop.ty),
                    ret_ty: Some(prop.ty.clone()),
                    is_static: false,
                    doc: prop_doc.clone(),
                },
            );
            if let Some(g) = &prop.getter {
                getters.insert(
                    prop.name.into(),
                    MemberInfo {
                        span: g.span,
                        signature: format!("(getter) {}.{}: {}", c.name, prop.name, prop.ty),
                        ret_ty: Some(prop.ty.clone()),
                        is_static: false,
                        doc: field_doc(&c.name, g.span).or_else(|| prop_doc.clone()),
                    },
                );
            }
            if let Some(s) = &prop.setter {
                setters.insert(
                    prop.name.into(),
                    MemberInfo {
                        span: s.span,
                        signature: format!("(setter) {}.{}: {}", c.name, prop.name, prop.ty),
                        ret_ty: Some(prop.ty.clone()),
                        is_static: false,
                        doc: field_doc(&c.name, s.span).or_else(|| prop_doc.clone()),
                    },
                );
            }
        }
        let mut methods = HashMap::new();
        let mut init_overloads = 0usize;
        let mut inits: Vec<MemberInfo> = Vec::new();
        for m in &c.methods {
            let info = MemberInfo {
                span: m.span,
                signature: format!("(method) {}.{}", c.name, fn_body(m)),
                ret_ty: m.ret.clone(),
                is_static: false,
                doc: field_doc(&c.name, m.span),
            };
            if m.name == "init" {
                init_overloads += 1;
                inits.push(info.clone());
            }
            methods.entry(m.name.clone()).or_insert(info);
        }
        for m in &c.static_methods {
            methods.entry(m.name.clone()).or_insert(MemberInfo {
                span: m.span,
                signature: format!("(static method) {}.{}", c.name, fn_body(m)),
                is_static: true,
                ret_ty: m.ret.clone(),
                doc: field_doc(&c.name, m.span),
            });
        }
        out.insert(
            c.name.into(),
            ClassInfo {
                decl_span: c.span,
                fields,
                methods,
                getters,
                setters,
                external: true,
                init_overloads,
                inits,
                kind: ClassKind::Class,
            },
        );
    }
    out
}

/// Register `Enum.Variant` hover entries for every variant of `e`.
/// `enum_key` is the dotted name the enum lives under (e.g.
/// `sdl.InitFlag`). Each variant is keyed `enum_key.variant_name` so
/// `Field { obj: Var(enum_key), name: variant }` can be resolved by
/// the walker.
fn register_enum_variants(
    e: &ilang_ast::EnumDecl,
    enum_key: &str,
    out: &mut HashMap<AstSymbol, String>,
) {
    let mut auto: i64 = 0;
    for v in e.variants.iter() {
        let val = match v.discriminant {
            Some(d) => {
                auto = d + 1;
                d
            }
            None => {
                let cur = auto;
                auto += 1;
                cur
            }
        };
        let key = format!("{enum_key}.{}", v.name);
        let sig = match &v.payload {
            ilang_ast::VariantPayload::Unit => {
                format!("(variant) {enum_key}.{} = {val}", v.name)
            }
            ilang_ast::VariantPayload::Tuple(_) => {
                format!("(variant) {enum_key}.{}(...)", v.name)
            }
            ilang_ast::VariantPayload::Struct(_) => {
                format!("(variant) {enum_key}.{} {{ ... }}", v.name)
            }
        };
        out.insert(AstSymbol::intern(&key), sig);
    }
}

/// Same as `register_enum_variants`, but also records each variant's
/// source location in `sources` (so F12 jumps to the variant line).
fn register_enum_variants_with_sources(
    e: &ilang_ast::EnumDecl,
    enum_key: &str,
    out: &mut HashMap<AstSymbol, String>,
    sources: &mut ExternalSources,
    module_path: &Path,
) {
    register_enum_variants(e, enum_key, out);
    for v in e.variants.iter() {
        let key = AstSymbol::intern(&format!("{enum_key}.{}", v.name));
        sources.insert(
            key,
            ExternalLoc {
                path: module_path.to_path_buf(),
                span: v.span,
                name_len: v.name.as_str().len() as u32,
            },
        );
    }
}

fn collect_external_signatures(
    prog: &Program,
) -> (HashMap<AstSymbol, String>, HashMap<AstSymbol, Type>) {
    use ilang_ast::ExternCItem;
    let mut out = HashMap::new();
    let mut rets: HashMap<AstSymbol, Type> = HashMap::new();
    let put_dotted = |name: &str, sig: String, m: &mut HashMap<AstSymbol, String>| {
        if name.contains('.') {
            m.insert(name.into(), sig);
        }
    };
    for item in &prog.items {
        match item {
            Item::Fn(f) => {
                put_dotted(f.name.as_str(), fn_signature(f), &mut out);
                if let Some(t) = &f.ret {
                    if f.name.as_str().contains('.') {
                        rets.insert(f.name.clone(), t.clone());
                    }
                }
            }
            Item::Const(c) => {
                let ty = match &c.ty {
                    Some(t) => format!(": {t}"),
                    None => String::new(),
                };
                let value = render_const_value(&c.value)
                    .map(|v| format!(" = {v}"))
                    .unwrap_or_default();
                put_dotted(c.name.as_str(), format!("const {}{ty}{value}", c.name), &mut out);
            }
            Item::Class(c) => {
                put_dotted(c.name.as_str(), format!("class {}", c.name), &mut out);
            }
            Item::Enum(e) => {
                put_dotted(e.name.as_str(), format!("enum {}", e.name), &mut out);
                if e.name.as_str().contains('.') {
                    register_enum_variants(e, e.name.as_str(), &mut out);
                }
            }
            Item::ExternC(b) => {
                for inner in &b.items {
                    match inner {
                        ExternCItem::FnDecl {
                            name, params, ret, libs, ..
                        } => {
                            let ps = params
                                .iter()
                                .map(|p| format!("{}: {}", p.name, p.ty))
                                .collect::<Vec<_>>()
                                .join(", ");
                            let r = match ret {
                                Some(t) => format!(": {t}"),
                                None => String::new(),
                            };
                            let libs_prefix = if libs.is_empty() {
                                String::new()
                            } else {
                                let names = libs
                                    .iter()
                                    .map(|l| format!("\"{l}\""))
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                format!("@lib({names}) ")
                            };
                            put_dotted(
                                name.as_str(),
                                format!("{libs_prefix}fn {}({}){}", name, ps, r),
                                &mut out,
                            );
                            if let Some(t) = ret {
                                if name.as_str().contains('.') {
                                    rets.insert(name.clone(), t.clone());
                                }
                            }
                        }
                        ExternCItem::FnDef(f) => {
                            put_dotted(f.name.as_str(), fn_signature(f), &mut out);
                            if let Some(t) = &f.ret {
                                if f.name.as_str().contains('.') {
                                    rets.insert(f.name.clone(), t.clone());
                                }
                            }
                        }
                        ExternCItem::Static { name, ty, .. } => {
                            put_dotted(name.as_str(), format!("static {}: {}", name, ty), &mut out);
                        }
                        ExternCItem::Struct { name, .. } => {
                            put_dotted(name.as_str(), format!("struct {}", name), &mut out);
                        }
                        ExternCItem::Union { name, .. } => {
                            put_dotted(name.as_str(), format!("union {}", name), &mut out);
                        }
                        ExternCItem::Class(c) => {
                            put_dotted(c.name.as_str(), format!("class {}", c.name), &mut out);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    (out, rets)
}


fn diag(span: Span, msg: String) -> Diagnostic {
    Diagnostic {
        range: span_full_to_range(span),
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("ilang".into()),
        message: msg,
        ..Diagnostic::default()
    }
}

// ─── Index building ────────────────────────────────────────────────────────

fn build_doc(
    text: String,
    prog: &Program,
    external_signatures: &HashMap<AstSymbol, String>,
    external_returns: &HashMap<AstSymbol, Type>,
    external_classes: &HashMap<AstSymbol, ClassInfo>,
    external_sources: &ExternalSources,
    external_docs: &HashMap<AstSymbol, String>,
) -> Doc {
    let symbols = collect_symbols(prog, &text);
    let mut classes = collect_classes(prog, &text);
    install_builtin_classes(&mut classes);
    // Merge in classes the loader pulled in via `use module`. Buffer-
    // local classes win on name collisions.
    for (k, v) in external_classes {
        classes.entry(k.clone()).or_insert_with(|| v.clone());
    }
    let mut fn_returns: HashMap<AstSymbol, Type> = HashMap::new();
    for item in &prog.items {
        match item {
            Item::Fn(f) => {
                if let Some(t) = &f.ret {
                    fn_returns.insert(f.name.clone(), t.clone());
                }
            }
            Item::ExternC(b) => {
                for inner in &b.items {
                    match inner {
                        ilang_ast::ExternCItem::FnDecl { name, ret: Some(t), .. } => {
                            fn_returns.insert(name.clone(), t.clone());
                        }
                        ilang_ast::ExternCItem::FnDef(f) => {
                            if let Some(t) = &f.ret {
                                fn_returns.insert(f.name.clone(), t.clone());
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    let mut refs = Vec::new();
    let mut var_classes: HashMap<AstSymbol, String> = HashMap::new();
    let mut var_types: HashMap<AstSymbol, Type> = HashMap::new();
    {
        let mut walker = Walker {
            text: &text,
            symbols: &symbols,
            classes: &classes,
            fn_returns: &fn_returns,
            external_signatures,
            external_docs,
            external_returns,
            external_sources,
            refs: &mut refs,
            var_classes: &mut var_classes,
            var_types: &mut var_types,
        };
        for item in &prog.items {
            match item {
                Item::Fn(f) => walker.walk_fn(f, None),
                Item::Class(c) => walker.walk_class(c),
                Item::Use(u) => {
                    // `use module` — push a hover entry on the module
                    // identifier itself, with F12 navigating to the
                    // module file's first line.
                    if let Some(name_span) = locate_let_name_with_kw(
                        &text,
                        u.span,
                        "use",
                        u.module.as_str(),
                    ) {
                        let loc = walker.external_sources.get(&u.module);
                        let target_uri = loc
                            .and_then(|l| Url::from_file_path(&l.path).ok());
                        let (target_span, target_name_len, no_def) = match &loc {
                            Some(l) if target_uri.is_some() => (l.span, l.name_len, false),
                            _ => (name_span, u.module.as_str().len() as u32, target_uri.is_none()),
                        };
                        walker.refs.push(RefEntry {
                            line: name_span.line,
                            start_col: name_span.col,
                            end_col: name_span.col + u.module.as_str().len() as u32,
                            target_span,
                            target_name_len,
                            signature: format!("(module) {}", u.module),
                            no_definition: no_def,
                            target_uri,
                            doc: None,
                        });
                    }
                    // `use module { name1, name2 }` — push a hover /
                    // F12 entry on each selectively-imported name so
                    // hovering or jumping from the import line itself
                    // works the same as from a use site.
                    if let Some(names) = &u.selective {
                        for name in names.iter() {
                            let Some((line, col)) =
                                locate_selective_name(&text, u.span, name.as_str())
                            else {
                                continue;
                            };
                            let key = AstSymbol::intern(name.as_str());
                            let sig = walker
                                .external_signatures
                                .get(&key)
                                .cloned()
                                .unwrap_or_else(|| format!("(import) {name}"));
                            let loc = walker.external_sources.get(&key);
                            let target_uri = loc
                                .and_then(|l| Url::from_file_path(&l.path).ok());
                            let (target_span, target_name_len, no_def) = match loc {
                                Some(l) if target_uri.is_some() => (l.span, l.name_len, false),
                                _ => (
                                    Span::new(line, col),
                                    name.as_str().len() as u32,
                                    target_uri.is_none(),
                                ),
                            };
                            walker.refs.push(RefEntry {
                                line,
                                start_col: col,
                                end_col: col + name.as_str().len() as u32,
                                target_span,
                                target_name_len,
                                signature: sig,
                                no_definition: no_def,
                                target_uri,
                                doc: walker.external_docs.get(&key).cloned(),
                            });
                        }
                    }
                }
                Item::ExternC(b) => {
                    for inner in &b.items {
                        match inner {
                            ilang_ast::ExternCItem::FnDef(f) => walker.walk_fn(f, None),
                            ilang_ast::ExternCItem::Class(c) => walker.walk_class(c),
                            ilang_ast::ExternCItem::Struct {
                                name, fields, ..
                            }
                            | ilang_ast::ExternCItem::Union {
                                name, fields, ..
                            } => {
                                for f in fields {
                                    walker.push_decl(
                                        f.name.as_str(),
                                        f.span,
                                        format!("(property) {}.{}: {}", name, f.name, f.ty),
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        // Top-level stmts/tail (script-style code outside any fn).
        let mut top_scope: Vec<Binding> = Vec::new();
        for s in &prog.stmts {
            walker.walk_stmt(s, &mut top_scope, None);
        }
        if let Some(t) = &prog.tail {
            walker.walk_expr(t, &mut top_scope, None);
        }
    }
    refs.sort_by_key(|r| (r.line, r.start_col));
    Doc {
        text,
        symbols,
        classes,
        refs,
        var_classes,
        var_types,
        external_signatures: external_signatures.clone(),
        external_docs: external_docs.clone(),
        external_returns: external_returns.clone(),
    }
}

fn collect_symbols(prog: &Program, src: &str) -> HashMap<AstSymbol, Symbol> {
    use ilang_ast::ExternCItem;
    let mut out = HashMap::new();
    let put_fn = |f: &FnDecl, m: &mut HashMap<AstSymbol, Symbol>| {
        m.insert(
            f.name.into(),
            Symbol {
                name: f.name.as_str().to_string(),
                span: f.span,
                signature: fn_signature(f),
                doc: text::extract_doc_above(src, f.span.line),
            },
        );
    };
    for item in &prog.items {
        match item {
            Item::Fn(f) => put_fn(f, &mut out),
            Item::Class(c) => {
                let signature = format!("class {}", c.name);
                out.insert(
                    c.name.into(),
                    Symbol {
                        name: c.name.as_str().to_string(),
                        span: c.span,
                        signature,
                        doc: text::extract_doc_above(src, c.span.line),
                    },
                );
            }
            Item::Enum(e) => {
                let variants = e
                    .variants
                    .iter()
                    .map(|v| match &v.payload {
                        VariantPayload::Unit => v.name.as_str().to_string(),
                        _ => format!("{}(...)", v.name),
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let signature = format!("enum {} {{ {} }}", e.name, variants);
                out.insert(
                    e.name.into(),
                    Symbol {
                        name: e.name.as_str().to_string(),
                        span: e.span,
                        signature,
                        doc: text::extract_doc_above(src, e.span.line),
                    },
                );
            }
            Item::Const(c) => {
                let ty = match &c.ty {
                    Some(t) => format!(": {t}"),
                    None => String::new(),
                };
                let value = render_const_value(&c.value)
                    .map(|v| format!(" = {v}"))
                    .unwrap_or_default();
                let signature = format!("const {}{}{}", c.name, ty, value);
                out.insert(
                    c.name.into(),
                    Symbol {
                        name: c.name.as_str().to_string(),
                        span: c.span,
                        signature,
                        doc: text::extract_doc_above(src, c.span.line),
                    },
                );
            }
            Item::ExternC(b) => {
                for inner in &b.items {
                    match inner {
                        ExternCItem::FnDecl {
                            name, params, ret, span, libs, ..
                        } => {
                            let ps = params
                                .iter()
                                .map(|p| format!("{}: {}", p.name, p.ty))
                                .collect::<Vec<_>>()
                                .join(", ");
                            let r = match ret {
                                Some(t) => format!(": {t}"),
                                None => String::new(),
                            };
                            let libs_prefix = if libs.is_empty() {
                                String::new()
                            } else {
                                let names = libs
                                    .iter()
                                    .map(|l| format!("\"{l}\""))
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                format!("@lib({names}) ")
                            };
                            out.insert(
                                name.clone(),
                                Symbol {
                                    name: name.as_str().to_string(),
                                    span: *span,
                                    signature: format!("{libs_prefix}fn {}({}){}", name, ps, r),
                                    doc: text::extract_doc_above(src, span.line),
                                },
                            );
                        }
                        ExternCItem::FnDef(f) => put_fn(f, &mut out),
                        ExternCItem::Static { name, ty, span, .. } => {
                            out.insert(
                                name.clone(),
                                Symbol {
                                    name: name.as_str().to_string(),
                                    span: *span,
                                    signature: format!("static {}: {}", name, ty),
                                    doc: text::extract_doc_above(src, span.line),
                                },
                            );
                        }
                        ExternCItem::Struct { name, span, .. } => {
                            out.insert(
                                name.clone(),
                                Symbol {
                                    name: name.as_str().to_string(),
                                    span: *span,
                                    signature: format!("struct {}", name),
                                    doc: text::extract_doc_above(src, span.line),
                                },
                            );
                        }
                        ExternCItem::Union { name, span, .. } => {
                            out.insert(
                                name.clone(),
                                Symbol {
                                    name: name.as_str().to_string(),
                                    span: *span,
                                    signature: format!("union {}", name),
                                    doc: text::extract_doc_above(src, span.line),
                                },
                            );
                        }
                        ExternCItem::Class(c) => {
                            out.insert(
                                c.name.into(),
                                Symbol {
                                    name: c.name.as_str().to_string(),
                                    span: c.span,
                                    signature: format!("class {}", c.name),
                doc: text::extract_doc_above(src, c.span.line),
                                },
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Inject hover info for built-in singletons / classes that the type
/// checker pre-registers (e.g. `console.log`). The buffer doesn't
/// declare these, so users would otherwise see no hover.
fn install_builtin_classes(out: &mut HashMap<AstSymbol, ClassInfo>) {
    let mut methods: HashMap<AstSymbol, MemberInfo> = HashMap::new();
    methods.insert(
        "log".into(),
        MemberInfo {
            span: Span::dummy(),
            signature: "(method) Console.log(...args): ()".to_string(),
            ret_ty: Some(Type::Unit),
            is_static: false,
                doc: None,
        },
    );
    out.entry("Console".into()).or_insert(ClassInfo {
        decl_span: Span::dummy(),
        fields: HashMap::new(),
        methods,
        getters: HashMap::new(),
        setters: HashMap::new(),
        external: true,
        init_overloads: 0,
                                    inits: Vec::new(),
        kind: ClassKind::Class,
    });
}

fn collect_classes(prog: &Program, src: &str) -> HashMap<AstSymbol, ClassInfo> {
    use ilang_ast::ExternCItem;
    let mut classes: Vec<&ClassDecl> = Vec::new();
    let mut out = HashMap::new();
    for item in &prog.items {
        match item {
            Item::Class(c) => classes.push(c),
            Item::ExternC(b) => {
                for inner in &b.items {
                    match inner {
                        ExternCItem::Class(c) => classes.push(c),
                        // Treat extern structs / unions like classes for
                        // field-resolution purposes: build a fields-only
                        // ClassInfo so `point.x` hovers / F12s.
                        ExternCItem::Struct { name, fields: fs, span, .. }
                        | ExternCItem::Union { name, fields: fs, span, .. } => {
                            let kind = matches!(
                                inner,
                                ExternCItem::Struct { .. }
                            )
                                .then_some(ClassKind::Struct)
                                .unwrap_or(ClassKind::Union);
                            let mut fields = HashMap::new();
                            for f in fs {
                                fields.insert(
                                    f.name.into(),
                                    MemberInfo {
                                        span: f.span,
                                        signature: format!(
                                            "(property) {}.{}: {}",
                                            name, f.name, f.ty
                                        ),
                                        ret_ty: Some(f.ty.clone()),
                                        is_static: false,
                                        doc: text::extract_doc_above(src, f.span.line),
                                    },
                                );
                            }
                            out.insert(
                                name.clone(),
                                ClassInfo {
                                    decl_span: *span,
                                    fields,
                                    methods: HashMap::new(),
                                    getters: HashMap::new(),
                                    setters: HashMap::new(),
                                    external: false,
                                    init_overloads: 0,
                                    inits: Vec::new(),
                                    kind,
                                },
                            );
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    for c in classes {
        // Mirror the original body — each block builds a ClassInfo
        // identical to the original `Item::Class` path.
        {
            let mut fields = HashMap::new();
            for f in &c.fields {
                fields.insert(
                    f.name.into(),
                    MemberInfo {
                        span: f.span,
                        signature: format!("(property) {}.{}: {}", c.name, f.name, f.ty),
                        ret_ty: Some(f.ty.clone()),
                        is_static: false,
                        doc: text::extract_doc_above(src, f.span.line),
                    },
                );
            }
            for f in &c.static_fields {
                fields.insert(
                    f.name.into(),
                    MemberInfo {
                        span: f.span,
                        signature: format!(
                            "(static property) {}.{}: {}",
                            c.name, f.name, f.ty
                        ),
                        ret_ty: Some(f.ty.clone()),
                        is_static: true,
                        doc: text::extract_doc_above(src, f.span.line),
                    },
                );
            }
            let mut getters: HashMap<AstSymbol, MemberInfo> = HashMap::new();
            let mut setters: HashMap<AstSymbol, MemberInfo> = HashMap::new();
            for prop in &c.properties {
                fields.insert(
                    prop.name.into(),
                    MemberInfo {
                        span: prop.span,
                        signature: format!(
                            "(property) {}.{}: {}",
                            c.name, prop.name, prop.ty
                        ),
                        ret_ty: Some(prop.ty.clone()),
                        is_static: false,
                        doc: text::extract_doc_above(src, prop.span.line),
                    },
                );
                if let Some(g) = &prop.getter {
                    getters.insert(
                        prop.name.into(),
                        MemberInfo {
                            span: g.span,
                            signature: format!(
                                "(getter) {}.{}: {}",
                                c.name, prop.name, prop.ty
                            ),
                            ret_ty: Some(prop.ty.clone()),
                            is_static: false,
                            doc: text::extract_doc_above(src, g.span.line),
                        },
                    );
                }
                if let Some(s) = &prop.setter {
                    setters.insert(
                        prop.name.into(),
                        MemberInfo {
                            span: s.span,
                            signature: format!(
                                "(setter) {}.{}: {}",
                                c.name, prop.name, prop.ty
                            ),
                            ret_ty: Some(prop.ty.clone()),
                            is_static: false,
                            doc: text::extract_doc_above(src, s.span.line),
                        },
                    );
                }
            }
            let mut methods = HashMap::new();
            let mut init_overloads = 0usize;
            let mut inits: Vec<MemberInfo> = Vec::new();
            for m in &c.methods {
                let info = MemberInfo {
                    span: m.span,
                    signature: format!("(method) {}.{}", c.name, fn_body(m)),
                    ret_ty: m.ret.clone(),
                    is_static: false,
                    doc: text::extract_doc_above(src, m.span.line),
                };
                if m.name == "init" {
                    init_overloads += 1;
                    inits.push(info.clone());
                }
                methods.entry(m.name.clone()).or_insert(info);
            }
            for m in &c.static_methods {
                methods.entry(m.name.clone()).or_insert(MemberInfo {
                    span: m.span,
                    signature: format!("(static method) {}.{}", c.name, fn_body(m)),
                    ret_ty: m.ret.clone(),
                    is_static: true,
                    doc: text::extract_doc_above(src, m.span.line),
                });
            }
            out.insert(
                c.name.into(),
                ClassInfo {
                    decl_span: c.span,
                    fields,
                    methods,
                    getters,
                    setters,
                    external: false,
                    init_overloads,
                    inits,
                    kind: ClassKind::Class,
                },
            );
        }
    }
    out
}

fn fn_signature(f: &FnDecl) -> String {
    format!("fn {}", fn_body(f))
}

/// `name(params): ret` — the part that comes after `fn` / `(method)` /
/// `(static method)`.
fn fn_body(f: &FnDecl) -> String {
    let params = f
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, p.ty))
        .collect::<Vec<_>>()
        .join(", ");
    let ret = match &f.ret {
        Some(t) => format!(": {t}"),
        None => String::new(),
    };
    format!("{}({}){}", f.name, params, ret)
}

// ─── Scope walker ──────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct Binding {
    name: String,
    span: Span,
    /// Statically-known type, if we can pin it down. Used both for hover
    /// signature and to resolve `local.field` accesses to the right class.
    ty: Option<Type>,
    /// What kind of binder introduced this (let / param / for-in / match
    /// pattern). Carried into hover signatures so use sites read like
    /// the declaration.
    kind: BindKind,
    /// When `Some`, replaces the kind/ty-derived hover signature.
    /// Used for `let func = fn(name: T): R { ... }` where we want to
    /// show parameter names that `Type::Fn` itself doesn't carry.
    override_signature: Option<String>,
}

#[derive(Clone, Copy, Debug)]
enum BindKind {
    Let,
    Param,
    ForIn,
    Pattern,
}

impl BindKind {
    fn render(self, name: &str, ty: Option<&Type>) -> String {
        let prefix = match self {
            BindKind::Let => "let ",
            BindKind::Param => "(parameter) ",
            BindKind::ForIn => "(for-binding) ",
            BindKind::Pattern => "(pattern) ",
        };
        match ty {
            Some(t) => format!("{prefix}{name}: {t}"),
            None => format!("{prefix}{name}"),
        }
    }
}

struct Walker<'a> {
    text: &'a str,
    symbols: &'a HashMap<AstSymbol, Symbol>,
    classes: &'a HashMap<AstSymbol, ClassInfo>,
    /// Top-level fn return types, keyed by name. Used to infer
    /// `let x = call()` bindings.
    fn_returns: &'a HashMap<AstSymbol, Type>,
    /// Hover signatures for `module.name` references that the loader
    /// brought in from a `use module` statement.
    external_signatures: &'a HashMap<AstSymbol, String>,
    /// Doc comments for external (imported) decls, keyed the same as
    /// `external_signatures`.
    external_docs: &'a HashMap<AstSymbol, String>,
    /// Return types for the same set of external fns. Used when
    /// inferring `let x = math.sqrt(...)` etc.
    external_returns: &'a HashMap<AstSymbol, Type>,
    /// Source-file path for each `module.<decl>` so cross-file F12
    /// can navigate into the originating module.
    external_sources: &'a ExternalSources,
    refs: &'a mut Vec<RefEntry>,
    /// Variable-name → class-name index, populated whenever a binding's
    /// statically-known type resolves to a class. Drives completion on
    /// `obj.` for ordinary instance variables.
    var_classes: &'a mut HashMap<AstSymbol, String>,
    /// Variable-name → full type, used for completion on built-in
    /// receivers (`string`, `T[]`) where there's no class entry.
    var_types: &'a mut HashMap<AstSymbol, Type>,
}

impl<'a> Walker<'a> {
    fn walk_fn(&mut self, f: &FnDecl, this_class: Option<&str>) {
        let mut scope: Vec<Binding> = Vec::new();
        for p in &f.params {
            let sig = BindKind::Param.render(p.name.as_str(), Some(&p.ty));
            self.push_decl(p.name.as_str(), p.span, sig);
            if let Some(c) = type_to_class(&p.ty) {
                self.var_classes.insert(p.name.clone(), c);
            }
            self.var_types.insert(p.name.clone(), p.ty.clone());
            scope.push(Binding {
                name: p.name.as_str().to_string(),
                span: p.span,
                ty: Some(p.ty.clone()),
                kind: BindKind::Param,
                override_signature: None,
            });
        }
        self.walk_block(&f.body, &mut scope, this_class);
    }

    fn walk_class(&mut self, c: &ClassDecl) {
        // Field declaration name: hover shows the field decl line.
        for f in &c.fields {
            self.push_decl(
                f.name.as_str(),
                f.span,
                format!("(property) {}.{}: {}", c.name, f.name, f.ty),
            );
        }
        for f in &c.static_fields {
            self.push_decl(
                f.name.as_str(),
                f.span,
                format!("(static property) {}.{}: {}", c.name, f.name, f.ty),
            );
        }
        for p in &c.properties {
            // PropertyDecl.span points at the `get` / `set` keyword, so
            // the name identifier sits a few columns to its right. Push
            // a decl entry at that exact location for hover and F12,
            // distinguishing the getter from the setter.
            for (kind, accessor_span) in [
                ("getter", p.getter.as_ref().map(|g| g.span)),
                ("setter", p.setter.as_ref().map(|s| s.span)),
            ] {
                let Some(span) = accessor_span else { continue };
                let sig = format!("({kind}) {}.{}: {}", c.name, p.name, p.ty);
                if let Some(name_span) =
                    locate_property_name(self.text, span, p.name.as_str())
                {
                    self.push_decl(p.name.as_str(), name_span, sig);
                }
            }
        }
        for m in &c.methods {
            self.push_decl(
                m.name.as_str(),
                m.span,
                format!("(method) {}.{}", c.name, fn_body(m)),
            );
            self.walk_fn(m, Some(c.name.as_str()));
        }
        for m in &c.static_methods {
            self.push_decl(
                m.name.as_str(),
                m.span,
                format!("(static method) {}.{}", c.name, fn_body(m)),
            );
            self.walk_fn(m, None);
        }
        for prop in &c.properties {
            // Treat the getter/setter body like a method body so locals
            // and `this.X` resolve normally.
            if let Some(g) = &prop.getter {
                self.walk_fn(g, Some(c.name.as_str()));
            }
            if let Some(s) = &prop.setter {
                self.walk_fn(s, Some(c.name.as_str()));
            }
        }
    }

    fn walk_block(&mut self, b: &Block, scope: &mut Vec<Binding>, this_class: Option<&str>) {
        let depth = scope.len();
        for s in &b.stmts {
            self.walk_stmt(s, scope, this_class);
        }
        if let Some(t) = &b.tail {
            self.walk_expr(t, scope, this_class);
        }
        scope.truncate(depth);
    }

    fn walk_stmt(&mut self, s: &Stmt, scope: &mut Vec<Binding>, this_class: Option<&str>) {
        match &s.kind {
            StmtKind::Let { name, ty, value } => {
                self.walk_expr(value, scope, this_class);
                let inferred = ty
                    .clone()
                    .or_else(|| self.infer_expr(value, scope));
                // For `let f = fn(name: T): R { ... }` keep the param
                // names in the rendered signature (Type::Fn alone drops
                // them).
                let override_sig = match &value.kind {
                    ExprKind::FnExpr { params, ret, .. } => {
                        let ps = params
                            .iter()
                            .map(|p| format!("{}: {}", p.name, p.ty))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let r = match ret {
                            Some(t) => format!(": {t}"),
                            None => String::new(),
                        };
                        Some(format!("let {name}: fn({ps}){r}"))
                    }
                    _ => None,
                };
                let sig = override_sig
                    .clone()
                    .unwrap_or_else(|| BindKind::Let.render(name.as_str(), inferred.as_ref()));
                // s.span points at the `let` keyword. Locate the actual
                // name position by skipping `let` + whitespace.
                let name_span = locate_let_name(self.text, s.span, name.as_str()).unwrap_or(s.span);
                self.push_decl(name.as_str(), name_span, sig);
                if let Some(c) = inferred.as_ref().and_then(type_to_class) {
                    self.var_classes.insert(name.clone(), c);
                }
                if let Some(t) = inferred.as_ref() {
                    self.var_types.insert(name.clone(), t.clone());
                }
                scope.push(Binding {
                    name: name.as_str().to_string(),
                    span: name_span,
                    ty: inferred,
                    kind: BindKind::Let,
                    override_signature: override_sig,
                });
            }
            StmtKind::Expr(e) => self.walk_expr(e, scope, this_class),
        }
    }

    fn walk_expr(&mut self, e: &Expr, scope: &mut Vec<Binding>, this_class: Option<&str>) {
        match &e.kind {
            ExprKind::Var(name) => {
                if let Some(b) = scope.iter().rev().find(|b| b.name == name.as_str()) {
                    let sig = b
                        .override_signature
                        .clone()
                        .unwrap_or_else(|| b.kind.render(name.as_str(), b.ty.as_ref()));
                    self.push_ref(name.as_str(), e.span, b.span, name.as_str().len() as u32, sig);
                } else if name.as_str().contains('.') {
                    self.push_external_dotted_ref(name.as_str(), e.span);
                } else if let Some(m) = this_class.and_then(|c| self.classes.get(&AstSymbol::intern(c))).and_then(
                    |info| {
                        info.getters
                            .get(name)
                            .or_else(|| info.fields.get(name))
                            .or_else(|| info.methods.get(name))
                    },
                ) {
                    // Implicit-`this` member access inside a class method.
                    self.push_ref(name.as_str(), e.span, m.span, name.as_str().len() as u32, m.signature.clone());
                } else if let Some(sym) = self.symbols.get(name) {
                    self.push_ref(
                        name.as_str(),
                        e.span,
                        sym.span,
                        sym.name.as_str().len() as u32,
                        sym.signature.clone(),
                    );
                } else if let Some(sig) = self.external_signatures.get(name) {
                    // Selectively-imported bare name (`use M { X }`).
                    // Source / doc info was harvested under the bare key.
                    let loc = self.external_sources.get(name);
                    let target_uri = loc
                        .and_then(|l| Url::from_file_path(&l.path).ok());
                    let (target_span, target_name_len, no_def) = match loc {
                        Some(l) if target_uri.is_some() => (l.span, l.name_len, false),
                        _ => (e.span, name.as_str().len() as u32, target_uri.is_none()),
                    };
                    self.refs.push(RefEntry {
                        line: e.span.line,
                        start_col: e.span.col,
                        end_col: e.span.col + name.as_str().len() as u32,
                        target_span,
                        target_name_len,
                        signature: sig.clone(),
                        no_definition: no_def,
                        target_uri,
                        doc: self.external_docs.get(name).cloned(),
                    });
                }
            }
            ExprKind::This => {
                if let Some(c) = this_class {
                    if let Some(info) = self.classes.get(&AstSymbol::intern(c)) {
                        // `this` is 4 chars; e.span points at it.
                        self.push_ref("this", e.span, info.decl_span, c.len() as u32, format!("this: {c}"));
                    }
                }
            }
            ExprKind::Field { obj, name } => {
                self.walk_expr(obj, scope, this_class);
                // Built-in `.length` on string / array.
                if name == "length" {
                    let prefix = match self.infer_expr(obj, scope) {
                        Some(Type::Str) => Some("string".to_string()),
                        Some(Type::Array { elem, .. }) => Some(format!("{elem}[]")),
                        _ => None,
                    };
                    if let Some(prefix) = prefix {
                        if let Some((line, col)) = locate_dot_name(self.text, obj.span, name.as_str()) {
                            self.refs.push(RefEntry {
                                line,
                                start_col: col,
                                end_col: col + name.as_str().len() as u32,
                                target_span: obj.span,
                                target_name_len: name.as_str().len() as u32,
                                signature: format!("(property) {prefix}.length: i64"),
                                no_definition: true,
                                target_uri: None,
                            doc: None,
                            });
                            return;
                        }
                    }
                }
                if let Some(class) = self.resolve_obj_class(obj, scope, this_class) {
                    if let Some(info) = self.classes.get(&AstSymbol::intern(&class)) {
                        if let Some(m) = info
                            .getters
                            .get(name)
                            .or_else(|| info.fields.get(name))
                            .or_else(|| info.methods.get(name))
                        {
                            if let Some((line, col)) = locate_dot_name(self.text, obj.span, name.as_str()) {
                                let (target, no_def, uri) = member_target(
                                    m,
                                    info,
                                    &class,
                                    self.external_sources,
                                    line,
                                    col,
                                );
                                self.refs.push(RefEntry {
                                    line,
                                    start_col: col,
                                    end_col: col + name.as_str().len() as u32,
                                    target_span: target,
                                    target_name_len: name.as_str().len() as u32,
                                    signature: m.signature.clone(),
                                    no_definition: no_def,
                                    target_uri: uri,
                                    doc: m.doc.clone(),
                                });
                            }
                        }
                    }
                }
                // Enum variant access: `EnumName.Variant` parses as a
                // Field, with `obj` resolving to a known external enum.
                // Look up the composite `EnumName.Variant` key in the
                // external maps (populated by `register_enum_variants*`)
                // and push a ref so hover / F12 land on the variant
                // declaration.
                if let Some(obj_name) = enum_obj_name(obj) {
                    let key = AstSymbol::intern(&format!("{obj_name}.{}", name));
                    if let Some(sig) = self.external_signatures.get(&key).cloned() {
                        if sig.starts_with("(variant)") {
                            if let Some((line, col)) =
                                locate_dot_name(self.text, obj.span, name.as_str())
                            {
                                let loc = self.external_sources.get(&key);
                                let target_uri = loc
                                    .and_then(|l| Url::from_file_path(&l.path).ok());
                                let (target_span, target_name_len, no_def) = match loc {
                                    Some(l) if target_uri.is_some() => {
                                        (l.span, l.name_len, false)
                                    }
                                    _ => (
                                        Span::new(line, col),
                                        name.as_str().len() as u32,
                                        target_uri.is_none(),
                                    ),
                                };
                                self.refs.push(RefEntry {
                                    line,
                                    start_col: col,
                                    end_col: col + name.as_str().len() as u32,
                                    target_span,
                                    target_name_len,
                                    signature: sig,
                                    no_definition: no_def,
                                    target_uri,
                                    doc: self.external_docs.get(&key).cloned(),
                                });
                            }
                        }
                    }
                }
            }
            ExprKind::MethodCall { obj, method, args } => {
                self.walk_expr(obj, scope, this_class);
                for a in args {
                    self.walk_expr(a, scope, this_class);
                }
                // Built-in string / array methods.
                let builtin_sig = match self.infer_expr(obj, scope) {
                    Some(Type::Str) => string_method_sig(method.as_str()),
                    Some(Type::Array { elem, .. }) => array_method_sig(method.as_str(), &elem),
                    _ => None,
                };
                if let Some(sig) = builtin_sig {
                    if let Some((line, col)) = locate_dot_name(self.text, obj.span, method.as_str()) {
                        self.refs.push(RefEntry {
                            line,
                            start_col: col,
                            end_col: col + method.as_str().len() as u32,
                            target_span: obj.span,
                            target_name_len: method.as_str().len() as u32,
                            signature: sig,
                            no_definition: true,
                            target_uri: None,
                        doc: None,
                        });
                        return;
                    }
                }
                if let Some(class) = self.resolve_obj_class(obj, scope, this_class) {
                    if let Some(info) = self.classes.get(&AstSymbol::intern(&class)) {
                        if let Some(m) = info.methods.get(&AstSymbol::intern(method.as_str())) {
                            if let Some((line, col)) = locate_dot_name(self.text, obj.span, method.as_str())
                            {
                                let (target, no_def, uri) = member_target(
                                    m,
                                    info,
                                    &class,
                                    self.external_sources,
                                    line,
                                    col,
                                );
                                self.refs.push(RefEntry {
                                    line,
                                    start_col: col,
                                    end_col: col + method.as_str().len() as u32,
                                    target_span: target,
                                    target_name_len: method.as_str().len() as u32,
                                    signature: m.signature.clone(),
                                    no_definition: no_def,
                                    target_uri: uri,
                                    doc: m.doc.clone(),
                                });
                            }
                        }
                    }
                }
            }
            ExprKind::Call { callee, args } => {
                if let Some(b) = scope.iter().rev().find(|b| b.name.as_str() == callee.as_str()) {
                    let sig = b
                        .override_signature
                        .clone()
                        .unwrap_or_else(|| b.kind.render(callee.as_str(), b.ty.as_ref()));
                    self.push_ref(callee.as_str(), e.span, b.span, callee.as_str().len() as u32, sig);
                } else if let Some(m) = this_class
                    .and_then(|c| self.classes.get(&AstSymbol::intern(c)))
                    .and_then(|info| info.methods.get(&AstSymbol::intern(callee.as_str())))
                {
                    // Implicit-`this` method call inside a class method.
                    self.push_ref(
                        callee.as_str(),
                        e.span,
                        m.span,
                        callee.as_str().len() as u32,
                        m.signature.clone(),
                    );
                } else if let Some(sym) = self.symbols.get(callee) {
                    self.push_ref(
                        callee.as_str(),
                        e.span,
                        sym.span,
                        sym.name.as_str().len() as u32,
                        sym.signature.clone(),
                    );
                } else if callee.as_str().contains('.') {
                    self.push_external_dotted_ref(callee.as_str(), e.span);
                } else if let Some(sig) = ffi_helper_signature(callee.as_str()) {
                    self.refs.push(RefEntry {
                        line: e.span.line,
                        start_col: e.span.col,
                        end_col: e.span.col + callee.as_str().len() as u32,
                        target_span: e.span,
                        target_name_len: callee.as_str().len() as u32,
                        signature: sig.to_string(),
                        no_definition: true,
                        target_uri: None,
                    doc: None,
                    });
                }
                for a in args {
                    self.walk_expr(a, scope, this_class);
                }
            }
            ExprKind::New { class, args, .. } => {
                let info = self.classes.get(class);
                let class_sig = info
                    .map(|i| class_hover(class.as_str(), i))
                    .unwrap_or_else(|| format!("class {class}"));
                // The `new` keyword span is at e.span; the class name
                // sits after `new ` so locate it explicitly. Without
                // this, our ref entries would land on the keyword
                // (and the dotted-name suffix wouldn't be found).
                let class_str = class.as_str();
                let class_start = locate_let_name_with_kw(
                    self.text,
                    e.span,
                    "new",
                    class_str.split('.').next().unwrap_or(class_str),
                )
                .unwrap_or(e.span);
                // F12 jumps to init when there is one; otherwise to the
                // class declaration itself. `init_member` is `None` for
                // classes without a defined init.
                let init_member = info.and_then(|i| i.methods.get(&"init".into()));
                if let Some(dot) = class_str.find('.') {
                    let prefix = &class_str[..dot];
                    let suffix = &class_str[dot + 1..];
                    let prefix_loc = self.external_sources.get(&AstSymbol::intern(prefix));
                    let prefix_uri = prefix_loc
                        .and_then(|l| Url::from_file_path(&l.path).ok());
                    let (prefix_target_span, prefix_target_name_len, prefix_no_def) =
                        match prefix_loc {
                            Some(l) if prefix_uri.is_some() => (l.span, l.name_len, false),
                            _ => (class_start, prefix.len() as u32, true),
                        };
                    self.refs.push(RefEntry {
                        line: class_start.line,
                        start_col: class_start.col,
                        end_col: class_start.col + prefix.len() as u32,
                        target_span: prefix_target_span,
                        target_name_len: prefix_target_name_len,
                        signature: format!("(module) {prefix}"),
                        no_definition: prefix_no_def,
                        target_uri: prefix_uri,
                        doc: None,
                    });
                    if let Some((line, col)) = locate_dot_name(self.text, class_start, suffix) {
                        let loc = self.external_sources.get(class);
                        let target_uri = loc
                            .and_then(|l| Url::from_file_path(&l.path).ok());
                        let is_external = info.map(|i| i.external).unwrap_or(true);
                        let (target_span, target_name_len, no_def) = match (init_member, is_external) {
                            (Some(im), false) => (im.span, suffix.len() as u32, false),
                            (Some(im), true) if target_uri.is_some() => {
                                (im.span, "init".len() as u32, false)
                            }
                            _ => match info {
                                Some(i) if !i.external => {
                                    (i.decl_span, suffix.len() as u32, false)
                                }
                                _ => match loc {
                                    Some(l) if target_uri.is_some() => {
                                        (l.span, l.name_len, false)
                                    }
                                    _ => {
                                        (class_start, suffix.len() as u32, target_uri.is_none())
                                    }
                                },
                            },
                        };
                        self.refs.push(RefEntry {
                            line,
                            start_col: col,
                            end_col: col + suffix.len() as u32,
                            target_span,
                            target_name_len,
                            signature: class_sig,
                            no_definition: no_def,
                            target_uri,
                        doc: None,
                        });
                    }
                } else if let Some(sym) = self.symbols.get(class) {
                    let target_span = init_member.map(|m| m.span).unwrap_or(sym.span);
                    self.refs.push(RefEntry {
                        line: class_start.line,
                        start_col: class_start.col,
                        end_col: class_start.col + class.as_str().len() as u32,
                        target_span,
                        target_name_len: class.as_str().len() as u32,
                        signature: class_sig,
                        no_definition: false,
                        target_uri: None,
                    doc: None,
                    });
                }
                for a in args {
                    self.walk_expr(a, scope, this_class);
                }
            }
            ExprKind::EnumCtor { enum_name, args, .. } => {
                if let Some(sym) = self.symbols.get(enum_name) {
                    self.push_ref(
                        enum_name.as_str(),
                        e.span,
                        sym.span,
                        sym.name.as_str().len() as u32,
                        sym.signature.clone(),
                    );
                }
                match args {
                    ilang_ast::CtorArgs::Tuple(es) => {
                        for x in es {
                            self.walk_expr(x, scope, this_class);
                        }
                    }
                    ilang_ast::CtorArgs::Struct(pairs) => {
                        for (_, x) in pairs {
                            self.walk_expr(x, scope, this_class);
                        }
                    }
                    ilang_ast::CtorArgs::Unit => {}
                }
            }
            ExprKind::Unary { expr, .. } => self.walk_expr(expr, scope, this_class),
            ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
                self.walk_expr(lhs, scope, this_class);
                self.walk_expr(rhs, scope, this_class);
            }
            ExprKind::If { cond, then_branch, else_branch } => {
                self.walk_expr(cond, scope, this_class);
                self.walk_block(then_branch, scope, this_class);
                if let Some(e) = else_branch {
                    self.walk_expr(e, scope, this_class);
                }
            }
            ExprKind::While { cond, body } => {
                self.walk_expr(cond, scope, this_class);
                self.walk_block(body, scope, this_class);
            }
            ExprKind::ForIn { var, iter, body } => {
                self.walk_expr(iter, scope, this_class);
                let depth = scope.len();
                let elem_ty = match self.infer_expr(iter, scope) {
                    Some(Type::Array { elem, .. }) => Some(*elem),
                    _ => None,
                };
                let sig = BindKind::ForIn.render(var.as_str(), elem_ty.as_ref());
                self.push_decl(var.as_str(), iter.span, sig);
                scope.push(Binding {
                    name: var.as_str().to_string(),
                    span: iter.span,
                    ty: elem_ty,
                    kind: BindKind::ForIn,
                    override_signature: None,
                });
                self.walk_block(body, scope, this_class);
                scope.truncate(depth);
            }
            ExprKind::Loop { body } => self.walk_block(body, scope, this_class),
            ExprKind::Block(b) => self.walk_block(b, scope, this_class),
            ExprKind::Break(opt) | ExprKind::Return(opt) => {
                if let Some(v) = opt {
                    self.walk_expr(v, scope, this_class);
                }
            }
            ExprKind::Assign { target, value } => {
                if let Some(b) = scope.iter().rev().find(|b| b.name == target.as_str()) {
                    let sig = b
                        .override_signature
                        .clone()
                        .unwrap_or_else(|| b.kind.render(target.as_str(), b.ty.as_ref()));
                    self.push_ref(target.as_str(), e.span, b.span, target.as_str().len() as u32, sig);
                } else if let Some(m) = this_class.and_then(|c| self.classes.get(&AstSymbol::intern(c))).and_then(
                    |info| {
                        info.setters
                            .get(target)
                            .or_else(|| info.fields.get(target))
                    },
                ) {
                    self.push_ref(
                        target.as_str(),
                        e.span,
                        m.span,
                        target.as_str().len() as u32,
                        m.signature.clone(),
                    );
                } else if let Some(sym) = self.symbols.get(target) {
                    self.push_ref(
                        target.as_str(),
                        e.span,
                        sym.span,
                        sym.name.as_str().len() as u32,
                        sym.signature.clone(),
                    );
                }
                self.walk_expr(value, scope, this_class);
            }
            ExprKind::AssignField { obj, field, value } => {
                self.walk_expr(obj, scope, this_class);
                if let Some(class) = self.resolve_obj_class(obj, scope, this_class) {
                    if let Some(info) = self.classes.get(&AstSymbol::intern(&class)) {
                        if let Some(m) = info
                            .setters
                            .get(field)
                            .or_else(|| info.fields.get(field))
                        {
                            if let Some((line, col)) = locate_dot_name(self.text, obj.span, field.as_str())
                            {
                                let (target, no_def, uri) = member_target(
                                    m,
                                    info,
                                    &class,
                                    self.external_sources,
                                    line,
                                    col,
                                );
                                self.refs.push(RefEntry {
                                    line,
                                    start_col: col,
                                    end_col: col + field.as_str().len() as u32,
                                    target_span: target,
                                    target_name_len: field.as_str().len() as u32,
                                    signature: m.signature.clone(),
                                    no_definition: no_def,
                                    target_uri: uri,
                                doc: None,
                                });
                            }
                        }
                    }
                }
                self.walk_expr(value, scope, this_class);
            }
            ExprKind::AssignIndex { obj, index, value } => {
                self.walk_expr(obj, scope, this_class);
                self.walk_expr(index, scope, this_class);
                self.walk_expr(value, scope, this_class);
            }
            ExprKind::Cast { expr, .. } => self.walk_expr(expr, scope, this_class),
            ExprKind::FnExpr { params, body, .. } => {
                // Closures capture outer locals by value at runtime, but
                // for hover/F12 it's useful to resolve them inside the
                // body too — start from the enclosing scope and add the
                // closure's own params on top.
                let mut inner: Vec<Binding> = scope.clone();
                for p in params {
                    let sig = BindKind::Param.render(p.name.as_str(), Some(&p.ty));
                    self.push_decl(p.name.as_str(), p.span, sig);
                    inner.push(Binding {
                        name: p.name.as_str().to_string(),
                        span: p.span,
                        ty: Some(p.ty.clone()),
                        kind: BindKind::Param,
                        override_signature: None,
                    });
                }
                self.walk_block(body, &mut inner, this_class);
            }
            ExprKind::Array(es) | ExprKind::Tuple(es) => {
                for x in es {
                    self.walk_expr(x, scope, this_class);
                }
            }
            ExprKind::StructLit { fields, .. } => {
                for (_, x) in fields {
                    self.walk_expr(x, scope, this_class);
                }
            }
            ExprKind::MapLit(pairs) => {
                for (k, v) in pairs {
                    self.walk_expr(k, scope, this_class);
                    self.walk_expr(v, scope, this_class);
                }
            }
            ExprKind::Index { obj, index } => {
                self.walk_expr(obj, scope, this_class);
                self.walk_expr(index, scope, this_class);
            }
            ExprKind::Range { start, end, .. } => {
                self.walk_expr(start, scope, this_class);
                self.walk_expr(end, scope, this_class);
            }
            ExprKind::Match { scrutinee, arms } => {
                self.walk_expr(scrutinee, scope, this_class);
                for arm in arms {
                    let depth = scope.len();
                    bind_pattern(&arm.pattern, scope);
                    self.walk_expr(&arm.body, scope, this_class);
                    scope.truncate(depth);
                }
            }
            ExprKind::SuperCall { args, .. } => {
                for a in args {
                    self.walk_expr(a, scope, this_class);
                }
            }
            _ => {}
        }
    }

    /// Walker-aware variant of `infer_expr_type_with_scope` that can
    /// also resolve `Call(callee)` to the callee's declared return
    /// type and `MethodCall` to the resolved method's return type.
    fn infer_expr(&self, e: &Expr, scope: &[Binding]) -> Option<Type> {
        match &e.kind {
            ExprKind::Call { callee, .. } => self
                .fn_returns
                .get(callee)
                .or_else(|| self.external_returns.get(callee))
                .cloned(),
            ExprKind::MethodCall { obj, method, .. } => {
                let class = self.resolve_obj_class(obj, scope, None)?;
                let info = self.classes.get(&AstSymbol::intern(&class))?;
                info.methods.get(&AstSymbol::intern(method.as_str()))?.ret_ty.clone()
            }
            ExprKind::Field { obj, name } => {
                let class = self.resolve_obj_class(obj, scope, None)?;
                let info = self.classes.get(&AstSymbol::intern(&class))?;
                info.fields.get(name)?.ret_ty.clone()
            }
            ExprKind::Index { obj, .. } => match self.infer_expr(obj, scope)? {
                Type::Array { elem, .. } => Some(*elem),
                Type::Str => Some(Type::U8),
                _ => None,
            },
            ExprKind::If { then_branch, else_branch, .. } => {
                let from_then = then_branch
                    .tail
                    .as_ref()
                    .and_then(|t| self.infer_expr(t, scope));
                from_then.or_else(|| {
                    else_branch.as_ref().and_then(|e| self.infer_expr(e, scope))
                })
            }
            ExprKind::Block(b) => b.tail.as_ref().and_then(|t| self.infer_expr(t, scope)),
            // `loop { ... break v ... }` — the value of the loop is the
            // first `break v` we find. Bare `break` (no value) yields
            // Unit; absence of any break we treat as no info.
            ExprKind::Loop { body } => {
                let mut found: Option<Type> = None;
                find_break_type(body, scope, self, &mut found);
                found
            }
            ExprKind::Match { arms, .. } => arms
                .iter()
                .find_map(|a| self.infer_expr(&a.body, scope)),
            ExprKind::Binary { op, lhs, rhs } => {
                use ilang_ast::BinOp;
                if matches!(
                    op,
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
                ) {
                    return Some(Type::Bool);
                }
                let lt = self.infer_expr(lhs, scope);
                let rt = self.infer_expr(rhs, scope);
                match (lt, rt) {
                    (Some(l), Some(r)) => Some(promote_pair(&l, &r, lhs, rhs)),
                    (Some(t), None) | (None, Some(t)) => Some(t),
                    (None, None) => None,
                }
            }
            ExprKind::Unary { op, expr } => match op {
                ilang_ast::UnOp::Not => Some(Type::Bool),
                _ => self.infer_expr(expr, scope),
            },
            // Fall back to the scope-aware inferer for everything else.
            _ => infer_expr_type_with_scope(e, scope),
        }
    }

    /// For a dotted name like `math.sqrt`, push a hover-only ref entry
    /// at the suffix position (`.sqrt`). Used for names brought in via
    /// `use module` — the loader resolves these to a full signature
    /// but we don't have file-level spans for F12.
    fn push_external_dotted_ref(&mut self, dotted: &str, receiver_span: Span) {
        let Some(sig) = self.external_signatures.get(&AstSymbol::intern(dotted)) else {
            return;
        };
        let Some(dot) = dotted.find('.') else {
            return;
        };
        let prefix = &dotted[..dot];
        let suffix = &dotted[dot + 1..];
        // Hover at the receiver name itself (e.g. `math` in `math.sqrt`).
        // The Call/Var AST span points at the start of the dotted form.
        // F12 on the prefix navigates to the start of the module file.
        let prefix_loc = self.external_sources.get(&AstSymbol::intern(prefix));
        let prefix_uri = prefix_loc
            .and_then(|l| Url::from_file_path(&l.path).ok());
        let (prefix_target_span, prefix_target_name_len, prefix_no_def) = match prefix_loc {
            Some(l) if prefix_uri.is_some() => (l.span, l.name_len, false),
            _ => (receiver_span, prefix.len() as u32, true),
        };
        self.refs.push(RefEntry {
            line: receiver_span.line,
            start_col: receiver_span.col,
            end_col: receiver_span.col + prefix.len() as u32,
            target_span: prefix_target_span,
            target_name_len: prefix_target_name_len,
            signature: format!("(module) {prefix}"),
            no_definition: prefix_no_def,
            target_uri: prefix_uri,
            doc: None,
        });
        if let Some((line, col)) = locate_dot_name(self.text, receiver_span, suffix) {
            // F12 on the suffix (e.g. `.sqrt` in `math.sqrt`) navigates
            // to the actual decl line in the source file when we know
            // it; otherwise hover-only.
            let loc = self.external_sources.get(&AstSymbol::intern(dotted));
            let target_uri = loc
                .and_then(|l| Url::from_file_path(&l.path).ok());
            let (target_span, target_name_len) = match loc {
                Some(l) if target_uri.is_some() => (l.span, l.name_len),
                _ => (receiver_span, suffix.len() as u32),
            };
            self.refs.push(RefEntry {
                line,
                start_col: col,
                end_col: col + suffix.len() as u32,
                target_span,
                target_name_len,
                signature: sig.clone(),
                no_definition: target_uri.is_none(),
                target_uri,
                doc: self.external_docs.get(&AstSymbol::intern(dotted)).cloned(),
            });
        }
    }

    fn push_decl(&mut self, name: &str, span: Span, signature: String) {
        self.refs.push(RefEntry {
            line: span.line,
            start_col: span.col,
            end_col: span.col + name.len() as u32,
            target_span: span,
            target_name_len: name.len() as u32,
            signature,
            no_definition: false,
            target_uri: None,
            doc: None,
        });
    }

    fn push_ref(
        &mut self,
        name: &str,
        use_span: Span,
        target_span: Span,
        target_name_len: u32,
        signature: String,
    ) {
        self.refs.push(RefEntry {
            line: use_span.line,
            start_col: use_span.col,
            end_col: use_span.col + name.len() as u32,
            target_span,
            target_name_len,
            signature,
            no_definition: false,
            target_uri: None,
            doc: None,
        });
    }

    /// Best-effort: figure out which class an `obj` expression refers
    /// to, so `obj.field` / `obj.method()` can resolve. Handles `this`,
    /// known-typed locals, and `new ClassName(...)`.
    fn resolve_obj_class(
        &self,
        obj: &Expr,
        scope: &[Binding],
        this_class: Option<&str>,
    ) -> Option<String> {
        match &obj.kind {
            ExprKind::This => this_class.map(|s| s.to_string()),
            ExprKind::Var(name) => {
                if let Some(b) = scope.iter().rev().find(|b| b.name.as_str() == name.as_str()) {
                    type_to_class(b.ty.as_ref()?)
                } else if self.classes.contains_key(name) {
                    // Bare `ClassName.field/method` — static access on
                    // the class itself.
                    Some(name.as_str().to_string())
                } else if name == "console" {
                    // Built-in singleton: maps to the `Console` class.
                    Some("Console".to_string())
                } else {
                    None
                }
            }
            ExprKind::New { class, .. } => Some(class.as_str().to_string()),
            _ => None,
        }
    }
}

/// Extract the dotted (or bare) name an `Var` carries when it stands
/// in for an enum receiver — e.g. `Var("InitFlag")` →
/// `Some("InitFlag")`, `Var("sdl.InitFlag")` → `Some("sdl.InitFlag")`.
/// Returns `None` for anything that isn't a plain `Var`.
fn enum_obj_name(obj: &Expr) -> Option<String> {
    match &obj.kind {
        ExprKind::Var(name) => Some(name.as_str().to_string()),
        _ => None,
    }
}

/// Render the hover signature shown on `new Foo(...)`. Prefer the
/// first `init(...)` line alone (TypeScript-style constructor hover),
/// with a `(+N overload[s])` tail when the class has multiple init
/// signatures. Falls back to `class Foo` for classes without init.
fn class_hover(class: &str, info: &ClassInfo) -> String {
    if let Some(init) = info.methods.get(&"init".into()) {
        let extras = info.init_overloads.saturating_sub(1);
        let mut out = init.signature.clone();
        if extras == 1 {
            out.push_str(" (+1 overload)");
        } else if extras > 1 {
            out.push_str(&format!(" (+{extras} overloads)"));
        }
        out
    } else {
        format!("{} {class}", info.kind.keyword())
    }
}

/// Resolve the F12 target for a class member reference. Returns
/// `(span, no_definition, target_uri)`.
/// - Buffer-local: span is the member's own span, no URI.
/// - External + source file known: span is the member's span (the
///   file's own coordinates), URI is the source file.
/// - External, no source: no_definition = true; cursor stays put.
fn member_target(
    m: &MemberInfo,
    info: &ClassInfo,
    class_name: &str,
    sources: &ExternalSources,
    use_line: u32,
    use_col: u32,
) -> (Span, bool, Option<Url>) {
    if info.external {
        if let Some(loc) = sources.get(&AstSymbol::intern(class_name)) {
            if let Ok(uri) = Url::from_file_path(&loc.path) {
                return (m.span, false, Some(uri));
            }
        }
        (Span::new(use_line, use_col), true, None)
    } else {
        (m.span, false, None)
    }
}

fn type_to_class(t: &Type) -> Option<String> {
    match t {
        Type::Object(n) => Some(n.as_str().to_string()),
        Type::Generic(g) => Some(g.base.as_str().to_string()),
        _ => None,
    }
}

fn bind_pattern(p: &Pattern, scope: &mut Vec<Binding>) {
    match &p.kind {
        PatternKind::Wildcard
        | PatternKind::IntLit(_)
        | PatternKind::IntRange { .. }
        | PatternKind::BoolLit(_)
        | PatternKind::StrLit(_) => {}
        PatternKind::Variant { bindings, .. } => match bindings {
            PatternBindings::Unit => {}
            // The AST stores binding names as bare strings (no per-name
            // spans), so we register them under the pattern's span. F12
            // on the binding will land on the pattern itself rather
            // than the precise identifier.
            PatternBindings::Tuple(names) => {
                for n in names {
                    if n != "_" {
                        scope.push(Binding {
                            name: n.as_str().to_string(),
                            span: p.span,
                            ty: None,
                            kind: BindKind::Pattern,
                            override_signature: None,
                        });
                    }
                }
            }
            PatternBindings::Struct(pairs) => {
                for (_, alias) in pairs {
                    scope.push(Binding {
                        name: alias.as_str().to_string(),
                        span: p.span,
                        ty: None,
                        kind: BindKind::Pattern,
                        override_signature: None,
                    });
                }
            }
        },
    }
}

/// Quick-and-dirty type inference used only for hover / `obj.field`
/// resolution. Covers the cases the type checker has already validated;
/// anything we can't pin down yields `None`.
/// Best-effort type inference used for hover and `obj.field` class
/// resolution. Falls back to the simpler scope-less variant when no
/// scope is available.
fn infer_expr_type_with_scope(e: &Expr, scope: &[Binding]) -> Option<Type> {
    if let ExprKind::FnExpr { params, ret, .. } = &e.kind {
        let ps = params.iter().map(|p| p.ty.clone()).collect();
        let r = ret.clone().unwrap_or(Type::Unit);
        return Some(Type::func(ps, r));
    }
    use ilang_ast::BinOp;
    match &e.kind {
        ExprKind::Int(_) => Some(Type::I64),
        ExprKind::Float(_) => Some(Type::F64),
        ExprKind::Bool(_) => Some(Type::Bool),
        ExprKind::Str(_) => Some(Type::Str),
        ExprKind::Var(name) => scope
            .iter()
            .rev()
            .find(|b| b.name == name.as_str())
            .and_then(|b| b.ty.clone()),
        ExprKind::New { class, type_args, .. } => {
            if type_args.is_empty() {
                Some(Type::Object(class.clone()))
            } else {
                Some(Type::generic(class.clone(), type_args.to_vec()))
            }
        }
        ExprKind::Cast { ty, .. } => Some(ty.clone()),
        // Comparison / logical produce bool. For arithmetic / bitwise,
        // mirror the type checker's literal-adoption rule: a known
        // typed operand wins over a bare integer / float literal on the
        // other side, so `i32_var % 10` infers as i32 (not i64).
        ExprKind::Binary { op, lhs, rhs } => match op {
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                Some(Type::Bool)
            }
            _ => {
                let lt = infer_expr_type_with_scope(lhs, scope);
                let rt = infer_expr_type_with_scope(rhs, scope);
                match (lt, rt) {
                    (Some(l), Some(r)) => Some(promote_pair(&l, &r, lhs, rhs)),
                    (Some(t), None) | (None, Some(t)) => Some(t),
                    (None, None) => None,
                }
            }
        },
        ExprKind::Logical { .. } => Some(Type::Bool),
        ExprKind::Unary { op, expr } => match op {
            ilang_ast::UnOp::Not => Some(Type::Bool),
            _ => infer_expr_type_with_scope(expr, scope),
        },
        _ => None,
    }
}


/// Pick which operand's type wins for a binary numeric op. Bare integer
/// or float literals defer to the other side when the other side has a
/// concrete narrower / wider numeric type — same shape as the type
/// checker's `numeric_literal_fits` adoption.
fn promote_pair(l: &Type, r: &Type, l_expr: &Expr, r_expr: &Expr) -> Type {
    let l_is_lit = matches!(l_expr.kind, ExprKind::Int(_) | ExprKind::Float(_));
    let r_is_lit = matches!(r_expr.kind, ExprKind::Int(_) | ExprKind::Float(_));
    if l_is_lit && !r_is_lit && r.is_numeric() {
        return r.clone();
    }
    if r_is_lit && !l_is_lit && l.is_numeric() {
        return l.clone();
    }
    l.clone()
}

/// Walk a `loop` body looking for the first `break v` and infer the
/// type of `v`. `break` without a value yields `Unit`. Doesn't descend
/// into nested loops (their `break`s belong to the inner loop).
fn find_break_type(
    block: &Block,
    scope: &[Binding],
    walker: &Walker,
    out: &mut Option<Type>,
) {
    for s in &block.stmts {
        if out.is_some() {
            return;
        }
        if let StmtKind::Expr(e) = &s.kind {
            scan_break(e, scope, walker, out);
        }
    }
    if out.is_none() {
        if let Some(t) = &block.tail {
            scan_break(t, scope, walker, out);
        }
    }
}

fn scan_break(
    e: &Expr,
    scope: &[Binding],
    walker: &Walker,
    out: &mut Option<Type>,
) {
    if out.is_some() {
        return;
    }
    match &e.kind {
        ExprKind::Break(v) => {
            *out = match v {
                Some(inner) => walker.infer_expr(inner, scope).or(Some(Type::Unit)),
                None => Some(Type::Unit),
            };
        }
        ExprKind::Loop { .. } => {
            // Inner loops swallow their own breaks — skip.
        }
        ExprKind::If { then_branch, else_branch, .. } => {
            find_break_type(then_branch, scope, walker, out);
            if let Some(eb) = else_branch {
                if out.is_none() {
                    scan_break(eb, scope, walker, out);
                }
            }
        }
        ExprKind::Block(b) => find_break_type(b, scope, walker, out),
        ExprKind::While { body, .. } | ExprKind::ForIn { body, .. } => {
            find_break_type(body, scope, walker, out);
        }
        ExprKind::Match { arms, .. } => {
            for a in arms {
                if out.is_some() {
                    break;
                }
                scan_break(&a.body, scope, walker, out);
            }
        }
        _ => {}
    }
}

/// Render a `const` initializer back to a short source-like string for
/// hover. Covers primitive literals and a leading unary `-` / `+`; more
/// complex expressions fall back to `None` so we don't print noise.
fn render_const_value(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Int(n) => Some(n.to_string()),
        ExprKind::Float(f) => Some(f.to_string()),
        ExprKind::Bool(b) => Some(b.to_string()),
        ExprKind::Str(s) => Some(format!("{s:?}")),
        ExprKind::Unary { op, expr } => {
            let inner = render_const_value(expr)?;
            let sym = match op {
                UnOp::Neg => "-",
                UnOp::Pos => "+",
                UnOp::Not => "!",
                UnOp::BitNot => "~",
            };
            Some(format!("{sym}{inner}"))
        }
        _ => None,
    }
}

/// Function / method completion items insert just their bare name.
/// (We used to insert `name($0)` to trigger signature help, but that
/// mangled valid uses where the user wants the name alone — passing a
/// fn as a value, referring to a method without calling it, etc.)
fn call_snippet(
    _name: &str,
    _kind: CompletionItemKind,
) -> (Option<String>, Option<InsertTextFormat>) {
    (None, None)
}

fn trigger_sig_help_command(_kind: CompletionItemKind) -> Option<Command> {
    None
}

/// ilang keywords. Each entry tags whether the keyword may appear at
/// the file's top level, inside a block (fn / method body / class
/// body / etc.), or both. The completion fallback filters by the
/// receiver's current brace depth — coarse but enough to keep
/// `init` / `return` / `break` out of top-level suggestions and
/// `fn` / `class` / `use` out of block-internal ones.
const KEYWORDS: &[(&str, KwScope)] = &[
    // Item kw (top level) and class-body-only kw stay in their scope.
    ("fn", KwScope::TopLevel),
    ("class", KwScope::TopLevel),
    ("enum", KwScope::TopLevel),
    ("use", KwScope::TopLevel),
    ("extends", KwScope::TopLevel),
    ("override", KwScope::Block),
    ("init", KwScope::Block),
    ("deinit", KwScope::Block),
    ("static", KwScope::Block),
    ("get", KwScope::Block),
    ("set", KwScope::Block),
    // Stmt / expr keywords are valid in either context — top-level
    // script-style code is a thing in ilang.
    ("let", KwScope::Both),
    ("const", KwScope::Both),
    ("if", KwScope::Both),
    ("elif", KwScope::Both),
    ("else", KwScope::Both),
    ("while", KwScope::Both),
    ("loop", KwScope::Both),
    ("for", KwScope::Both),
    ("in", KwScope::Both),
    ("match", KwScope::Both),
    ("new", KwScope::Both),
    ("as", KwScope::Both),
    ("true", KwScope::Both),
    ("false", KwScope::Both),
    ("none", KwScope::Both),
    ("some", KwScope::Both),
    // Need a surrounding fn / loop / class — but distinguishing those
    // contexts requires more than brace depth, so keep them at Block.
    ("return", KwScope::Block),
    ("break", KwScope::Block),
    ("continue", KwScope::Block),
    ("this", KwScope::Block),
    ("super", KwScope::Block),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum KwScope {
    /// Only relevant at the file's top level (depth = 0).
    TopLevel,
    /// Only inside some `{ ... }` (depth > 0).
    Block,
    /// Allowed in both contexts.
    Both,
}

/// `true` when the cursor sits inside an `@extern(C) { ... }` block.
/// Walks back across balanced braces; the first unmatched `{` is the
/// enclosing block, and we check whether `@extern(C)` precedes it
/// (with optional whitespace).
fn in_extern_c_block(text: &str, offset: usize) -> bool {
    let bytes = text.as_bytes();
    let end = offset.min(bytes.len());
    let mut depth: i32 = 0;
    let mut i = end;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b'}' => depth += 1,
            b'{' => {
                if depth > 0 {
                    depth -= 1;
                } else if extern_c_precedes(bytes, i) {
                    return true;
                }
                // Either way, keep walking past this `{` to inspect
                // outer enclosing braces too.
            }
            _ => {}
        }
    }
    false
}

/// `true` if `@extern(C)` (with optional whitespace) appears
/// immediately before byte index `at`.
fn extern_c_precedes(bytes: &[u8], at: usize) -> bool {
    let mut j = at;
    while j > 0 && matches!(bytes[j - 1], b' ' | b'\t' | b'\r' | b'\n') {
        j -= 1;
    }
    if j == 0 || bytes[j - 1] != b')' {
        return false;
    }
    let mut k = j - 1;
    while k > 0 && matches!(bytes[k - 1], b' ' | b'\t') {
        k -= 1;
    }
    if k == 0 || bytes[k - 1] != b'C' {
        return false;
    }
    k -= 1;
    while k > 0 && matches!(bytes[k - 1], b' ' | b'\t') {
        k -= 1;
    }
    if k == 0 || bytes[k - 1] != b'(' {
        return false;
    }
    k -= 1;
    while k > 0 && matches!(bytes[k - 1], b' ' | b'\t') {
        k -= 1;
    }
    if k < 6 || &bytes[k - 6..k] != b"extern" {
        return false;
    }
    let kk = k - 6;
    kk >= 1 && bytes[kk - 1] == b'@'
}

fn push_ffi_helper_completions(out: &mut Vec<CompletionItem>) {
    for name in [
        "stringFromCstr",
        "cstrFromString",
        "freeCstr",
        "bytesFromBuffer",
        "readI8",
        "readI16",
        "readI32",
        "readI64",
        "readU8",
        "readU16",
        "readU32",
        "readU64",
        "readF32",
        "readF64",
        "writeI8",
        "writeI16",
        "writeI32",
        "writeI64",
        "writeU8",
        "writeU16",
        "writeU32",
        "writeU64",
        "writeF32",
        "writeF64",
        "fnAddr",
        "arrayFromCArray",
        "cstrArrayToStrings",
        "errnoCheck",
        "errnoCheckI64",
    ] {
        if let Some(sig) = ffi_helper_signature(name) {
            out.push(CompletionItem {
                label: name.to_string(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(sig.to_string()),
                ..CompletionItem::default()
            });
        }
    }
}

/// `true` when the cursor sits in attribute syntax — i.e. an `@`
/// (followed by an in-progress identifier) is the previous non-ident
/// character on the line.
fn at_attribute_position(text: &str, offset: usize) -> bool {
    let bytes = text.as_bytes();
    let end = offset.min(bytes.len());
    let mut i = end;
    while i > 0 {
        let b = bytes[i - 1];
        if b.is_ascii_alphanumeric() || b == b'_' {
            i -= 1;
        } else {
            break;
        }
    }
    i > 0 && bytes[i - 1] == b'@'
}

/// ilang attributes for completion. `(args)` snippets are inserted
/// where the attribute typically takes arguments.
fn attribute_completions() -> Vec<CompletionItem> {
    let entries: &[(&str, Option<&str>, &str)] = &[
        ("extern", Some("extern(C)"), "@extern(C) { ... }"),
        ("lib", Some("lib(\"$1\")"), "@lib(\"libname\")"),
        ("optional", None, "@optional"),
        ("symbol", Some("symbol(\"$1\")"), "@symbol(\"name\")"),
        ("packed", None, "@packed"),
        ("bits", Some("bits($1)"), "@bits(N)"),
        ("flags", None, "@flags"),
        ("export", None, "@export"),
        ("override", None, "@override"),
        ("requires", Some("requires($1)"), "@requires(cap)"),
        ("deprecated", Some("deprecated($1)"), "@deprecated(reason)"),
    ];
    entries
        .iter()
        .map(|(label, snippet, detail)| CompletionItem {
            label: (*label).to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some((*detail).to_string()),
            insert_text: snippet.map(|s| (*s).to_string()),
            insert_text_format: snippet.map(|_| InsertTextFormat::SNIPPET),
            ..CompletionItem::default()
        })
        .collect()
}

/// `true` when the cursor follows a `:` (with optional whitespace and
/// a partial ident underway). That's the type slot of `let x: T`,
/// `const x: T`, `fn f(x: T)`, `field: T` etc.
fn at_type_position(text: &str, offset: usize) -> bool {
    let bytes = text.as_bytes();
    let end = offset.min(bytes.len());
    let mut i = end;
    while i > 0 {
        let b = bytes[i - 1];
        if b.is_ascii_alphanumeric() || b == b'_' {
            i -= 1;
        } else {
            break;
        }
    }
    while i > 0 && matches!(bytes[i - 1], b' ' | b'\t') {
        i -= 1;
    }
    i > 0 && bytes[i - 1] == b':'
}

const PRIMITIVE_TYPES: &[&str] = &[
    "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64", "bool", "string",
];

/// Identifiers valid in a type position: primitive types, user classes
/// / enums (incl. structs / unions), and `module.X` names imported via
/// `use`.
fn type_completions(doc: &Doc) -> Vec<CompletionItem> {
    let mut out: Vec<CompletionItem> = Vec::new();
    for t in PRIMITIVE_TYPES {
        out.push(CompletionItem {
            label: (*t).to_string(),
            kind: Some(CompletionItemKind::TYPE_PARAMETER),
            ..CompletionItem::default()
        });
    }
    for (name, sym) in doc.symbols.iter() {
        let is_type = sym.signature.starts_with("class ")
            || sym.signature.starts_with("struct ")
            || sym.signature.starts_with("union ")
            || sym.signature.starts_with("enum ");
        if !is_type {
            continue;
        }
        out.push(CompletionItem {
            label: name.as_str().to_string(),
            kind: Some(CompletionItemKind::CLASS),
            detail: Some(sym.signature.clone()),
            ..CompletionItem::default()
        });
    }
    // Imported types brought in via `use module` show as
    // `module.TypeName`.
    for (name, sig) in doc.external_signatures.iter() {
        let is_type = sig.starts_with("class ")
            || sig.starts_with("struct ")
            || sig.starts_with("union ")
            || sig.starts_with("enum ");
        if !is_type {
            continue;
        }
        out.push(CompletionItem {
            label: name.as_str().to_string(),
            kind: Some(CompletionItemKind::CLASS),
            detail: Some(sig.clone()),
            ..CompletionItem::default()
        });
    }
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}

/// `true` when the cursor is right after a `let` / `const` keyword
/// (with optional whitespace and possibly a partial ident underway).
/// Used to suppress completion at the binder position — anything we
/// suggest there would shadow / overwrite the new name.
fn preceding_kw_introduces_binder(text: &str, offset: usize) -> bool {
    let bytes = text.as_bytes();
    let end = offset.min(bytes.len());
    // Skip the in-progress ident the user is typing.
    let mut i = end;
    while i > 0 {
        let b = bytes[i - 1];
        if b.is_ascii_alphanumeric() || b == b'_' {
            i -= 1;
        } else {
            break;
        }
    }
    while i > 0 && matches!(bytes[i - 1], b' ' | b'\t') {
        i -= 1;
    }
    for kw in ["let", "const"] {
        let n = kw.len();
        if i >= n && &bytes[i - n..i] == kw.as_bytes() {
            let prev = if i > n { Some(bytes[i - n - 1]) } else { None };
            let boundary = prev
                .map(|c| !c.is_ascii_alphanumeric() && c != b'_')
                .unwrap_or(true);
            if boundary {
                return true;
            }
        }
    }
    false
}

/// Brace depth of `text` at byte offset `offset`. Counts `{` and `}`
/// outside string / char / line / block comments. Used by completion
/// to filter keywords by context.
fn brace_depth_at(text: &str, offset: usize) -> i32 {
    let bytes = text.as_bytes();
    let end = offset.min(bytes.len());
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut in_line_comment = false;
    let mut block_depth: i32 = 0;
    let mut i = 0;
    while i < end {
        let b = bytes[i];
        if in_line_comment {
            if b == b'\n' {
                in_line_comment = false;
            }
            i += 1;
            continue;
        }
        if block_depth > 0 {
            if b == b'/' && i + 1 < end && bytes[i + 1] == b'*' {
                block_depth += 1;
                i += 2;
                continue;
            }
            if b == b'*' && i + 1 < end && bytes[i + 1] == b'/' {
                block_depth -= 1;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if in_string {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if b == b'/' && i + 1 < end {
            if bytes[i + 1] == b'/' {
                in_line_comment = true;
                i += 2;
                continue;
            }
            if bytes[i + 1] == b'*' {
                block_depth = 1;
                i += 2;
                continue;
            }
        }
        if b == b'"' {
            in_string = true;
        } else if b == b'{' {
            depth += 1;
        } else if b == b'}' {
            depth -= 1;
        }
        i += 1;
    }
    depth
}

/// Append keyword completions matching the cursor's brace context.
fn keyword_completions(at_top_level: bool, out: &mut Vec<CompletionItem>) {
    for (label, scope) in KEYWORDS {
        let allowed = match scope {
            KwScope::TopLevel => at_top_level,
            KwScope::Block => !at_top_level,
            KwScope::Both => true,
        };
        if allowed {
            out.push(CompletionItem {
                label: (*label).to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                ..CompletionItem::default()
            });
        }
    }
}

/// Top-level identifiers visible in `doc`, used as completion fallback
/// when the user is just typing a name (no receiver). Only the bare
/// names appear — `use module` namespaces show up as the module name
/// itself, not as `module.member` (those land in the `module.`
/// completion list).
fn global_completions(doc: &Doc, at_top_level: bool) -> Vec<CompletionItem> {
    let mut out: Vec<CompletionItem> = Vec::new();
    for (name, sym) in doc.symbols.iter() {
        let kind = if sym.signature.starts_with("class ")
            || sym.signature.starts_with("struct ")
            || sym.signature.starts_with("union ")
        {
            CompletionItemKind::CLASS
        } else if sym.signature.starts_with("enum ") {
            CompletionItemKind::ENUM
        } else if sym.signature.starts_with("const ") {
            CompletionItemKind::CONSTANT
        } else {
            CompletionItemKind::FUNCTION
        };
        let (insert_text, fmt) = call_snippet(name.as_str(), kind);
        let command = trigger_sig_help_command(kind);
        out.push(CompletionItem {
            label: name.as_str().to_string(),
            kind: Some(kind),
            detail: Some(sym.signature.clone()),
            documentation: sym.doc.clone().map(|d| {
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
    // Local variables / params seen anywhere in the file. Last-write-
    // wins so the type info attached is approximate when names recur
    // across scopes.
    for (name, ty) in doc.var_types.iter() {
        if doc.symbols.contains_key(name) {
            continue;
        }
        out.push(CompletionItem {
            label: name.as_str().to_string(),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some(format!("{name}: {ty}")),
            ..CompletionItem::default()
        });
    }
    let mut modules: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for key in doc.external_signatures.keys() {
        if let Some((m, _)) = key.as_str().split_once('.') {
            modules.insert(m.to_string());
        }
    }
    for m in modules {
        out.push(CompletionItem {
            label: m.clone(),
            kind: Some(CompletionItemKind::MODULE),
            detail: Some(format!("(module) {m}")),
            ..CompletionItem::default()
        });
    }
    // Built-in singleton — always available.
    out.push(CompletionItem {
        label: "console".to_string(),
        kind: Some(CompletionItemKind::VARIABLE),
        detail: Some("(builtin) console: Console".to_string()),
        ..CompletionItem::default()
    });
    keyword_completions(at_top_level, &mut out);
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
