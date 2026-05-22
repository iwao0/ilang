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
    /// Per-file workspace-symbol cache, keyed by canonicalised path.
    /// Reuses a previously-parsed entry list when the file's mtime
    /// hasn't changed — the parse + walk is by far the most
    /// expensive part of a `workspace/symbol` request on a large
    /// workspace.
    pub(crate) workspace_sym_cache:
        Arc<Mutex<HashMap<PathBuf, crate::workspace_symbol_cache::Entry>>>,
}

impl Backend {
    pub(crate) fn new(client: Client) -> Self {
        Self {
            client,
            docs: Arc::new(Mutex::new(HashMap::new())),
            latest_versions: Arc::new(Mutex::new(HashMap::new())),
            workspace_sym_cache: Arc::new(Mutex::new(HashMap::new())),
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
    let umbrella = path.as_deref().and_then(find_umbrella);
    let is_submodule = umbrella.is_some();
    // Parse the buffer once up front. The loader injects the buffer
    // as an overlay, so a buffer that doesn't parse makes the whole
    // merged-program load fail anyway — skipping it here saves the
    // file IO + tokenize + parse for every imported module on each
    // mid-edit refresh (the common case while typing).
    let parsed_buffer = parse_ok(&text);
    // For a sub-module, drive the loader from its umbrella so the
    // merged program includes everything in scope (parent's `use
    // module` items, sibling sub-modules, etc.). The buffer's own
    // text is injected as an overlay below, so the merged program
    // still reflects unsaved edits. Diagnostics stay suppressed (see
    // `analyse`'s early return on `is_submodule`); the merged
    // program is only used to populate cross-module hover / F12 /
    // completion data.
    let entry_for_merge = umbrella
        .clone()
        .or_else(|| path.as_deref().map(|p| p.to_path_buf()));
    let merged = if parsed_buffer.is_err() {
        None
    } else {
        entry_for_merge
            .as_deref()
            .filter(|p| p.exists())
            .and_then(|p| {
                let dep_tree = crate::project::collect_dep_tree(p).unwrap_or_default();
                let extra = dep_tree.dirs;
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
                ilang_parser::loader::load_program_with_overlay_and_parents(
                    p, &extra, &dep_tree.parents, &overlay,
                ).ok()
            })
    };
    let diags = analyse(&text, path.as_deref(), &merged, is_submodule);
    let (mut external_sigs, mut external_rets) = merged
        .as_ref()
        .map(collect_external_signatures)
        .unwrap_or_default();
    // Built-in generic enums (`Result<T, E>`) aren't declared in any
    // source the loader can see, so seed their variants here. Without
    // this, `Result.` completion comes up empty.
    register_builtin_enums(&mut external_sigs);
    // `external_rets` from `collect_external_signatures` is keyed
    // by the qualified `module.fn` name only. For a `use M { fn }`
    // selective import the buffer sees the bare callee `fn(...)`,
    // so mirror those dotted entries under their bare names. Without
    // this `let x = fn()` hover never picks up the inferred type.
    if let Ok(buffer_prog) = &parsed_buffer {
        for item in &buffer_prog.items {
            let Item::Use(u) = item else { continue };
            if u.wildcard && u.selective.is_none() {
                // `use M { * }` — mirror every `M.<X>` ret type
                // under its bare name. Matches the selective-import
                // path below; same rationale (the buffer references
                // `<X>(...)` bare, but `collect_external_signatures`
                // only emitted the `M.<X>` key).
                let module_dot = format!("{}.", u.module);
                let bare: Vec<(AstSymbol, _)> = external_rets
                    .iter()
                    .filter_map(|(k, v)| {
                        k.as_str()
                            .strip_prefix(&module_dot)
                            .map(|tail| (AstSymbol::intern(tail), v.clone()))
                    })
                    .collect();
                for (k, v) in bare {
                    external_rets.entry(k).or_insert(v);
                }
                continue;
            }
            let Some(names) = &u.selective else { continue };
            for name in names.iter() {
                let prefixed = AstSymbol::intern(&format!("{}.{}", u.module, name));
                if let Some(t) = external_rets.get(&prefixed).cloned() {
                    external_rets.insert(name.clone(), t);
                }
            }
        }
    }
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
    // Cross-module `pub const` types: the loader inlines const
    // literals out of the merged program, so the buffer-side walker
    // can't see them via `prog.items`. Capture each const's resolved
    // `Type` here and merge it into `external_rets` below so a
    // buffer-local `let x = ExternConst` recovers the const's type.
    let mut external_const_types: HashMap<AstSymbol, Type> = HashMap::new();
    harvest_imported_consts(
        &harvest_anchor,
        &text,
        &mut external_sigs,
        &mut external_sources,
        &mut external_docs,
        &mut external_const_types,
    );
    for (k, v) in &external_const_types {
        external_rets.entry(k.clone()).or_insert(v.clone());
    }
    let external_classes = merged
        .as_ref()
        .map(|p| collect_external_classes(p, &external_sources))
        .unwrap_or_default();
    let external_interfaces = merged
        .as_ref()
        .map(collect_external_interfaces)
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
            // Auto-lift the buffer-local parse the same way the
            // loader does for `merged`: a top-level `class X :
            // NSObject { ... }` gets rewritten as an `@extern(ObjC)
            // { @objc class X { ... } }` block with synthesized
            // `alloc` / `init` / `register`. Without this step, the
            // local `collect_classes(prog)` would miss those
            // synthesized methods and hovering on
            // `let x = X.alloc().init()` would report no type.
            let prog = lift_local_parse_objc(prog, merged.as_ref());
            let mut d = build_doc(
                text,
                &prog,
                &external_sigs,
                &external_rets,
                &external_classes,
                &external_sources,
                &external_docs,
                &external_interfaces,
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
    // Group diagnostics by their span's `source_file`. Cross-module
    // errors (e.g. typecheck failure inside a sibling binding) get
    // routed to the file they originated in instead of always
    // attaching to whichever buffer the user is editing — that's
    // what `DiagEntry.source_file` records.
    let mut by_path: HashMap<PathBuf, Vec<Diagnostic>> = HashMap::new();
    let self_path_buf = path.clone();
    for entry in diags {
        let file_str = entry.source_file.as_str();
        let target: PathBuf = if file_str.is_empty() {
            // No file tagged on the span — fall back to the
            // current buffer's path. Diagnostic ends up on the
            // editor view the user is looking at.
            self_path_buf.clone().unwrap_or_else(|| PathBuf::from(""))
        } else {
            PathBuf::from(file_str)
        };
        by_path.entry(target).or_default().push(entry.diagnostic);
    }
    // Always publish to the current URI (even if empty) so its
    // previous-run squiggles clear when the latest analysis
    // returns clean.
    let self_diags = self_path_buf
        .as_deref()
        .and_then(|p| by_path.remove(p))
        .unwrap_or_default();
    client.publish_diagnostics(uri.clone(), self_diags, None).await;
    for (p, file_diags) in by_path {
        if let Ok(target_uri) = Url::from_file_path(&p) {
            client.publish_diagnostics(target_uri, file_diags, None).await;
        }
    }
}

