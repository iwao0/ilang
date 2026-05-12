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

pub(crate) struct Backend {
    pub(crate) client: Client,
    pub(crate) docs: Arc<Mutex<HashMap<Url, Doc>>>,
    /// Latest document version per URI, used to drop stale
    /// `did_change` events. Each `did_change` bumps the entry, then
    /// schedules a debounced refresh; when the timer fires we skip
    /// the work if a newer version has arrived in the meantime.
    pub(crate) latest_versions: Arc<Mutex<HashMap<Url, i32>>>,
}

impl Backend {
    pub(crate) fn new(client: Client) -> Self {
        Self {
            client,
            docs: Arc::new(Mutex::new(HashMap::new())),
            latest_versions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) async fn refresh(&self, uri: Url, text: String) {
        refresh_impl(&self.client, &self.docs, uri, text).await
    }
}

pub(crate) async fn refresh_impl(
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
                // Also seed the overlay with every other open
                // buffer — without this, a sub-module's
                // unsaved edits aren't visible while checking
                // the entry, so e.g. adding `pub` to a member
                // in `sample2.il` wouldn't clear the entry's
                // red squiggle until `sample2.il` is saved
                // AND the entry buffer is touched.
                let mut overlay: HashMap<PathBuf, String> = HashMap::new();
                if let Ok(canon) = p.canonicalize() {
                    overlay.insert(canon, text.clone());
                }
                {
                    let lock = docs.lock().unwrap();
                    for (other_uri, doc) in lock.iter() {
                        if let Ok(other_path) = other_uri.to_file_path() {
                            if let Ok(canon) = other_path.canonicalize() {
                                overlay.entry(canon).or_insert_with(|| doc.text.clone());
                            }
                        }
                    }
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
            if !external_sources.is_empty() {
                entry.external_sources = external_sources;
            }
        }
    }
    client.publish_diagnostics(uri, diags, None).await;
}

