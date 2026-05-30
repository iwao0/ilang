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
    /// Cached `.il` file lists keyed by workspace root. Populated on the
    /// first `workspace/symbol` request per root and reused afterwards,
    /// so quick-pick keystrokes no longer re-walk the whole tree. A
    /// `didChangeWatchedFiles` notification (registered in `initialized`
    /// when the client supports dynamic registration) clears it on any
    /// `.il` create / delete; when registration isn't available the
    /// handler leaves the cache unpopulated and each request re-walks.
    pub(crate) workspace_file_cache: Arc<Mutex<HashMap<PathBuf, Vec<PathBuf>>>>,
    /// Set once at `initialized` time: whether the file-list cache may be
    /// trusted (i.e. we successfully registered file watching). Without
    /// it a stale cache could hide newly created files, so we skip the
    /// cache entirely and re-walk per request.
    pub(crate) watch_registered: Arc<std::sync::atomic::AtomicBool>,
    /// Captured from the client's `initialize` capabilities: whether it
    /// supports dynamically registering `didChangeWatchedFiles`. Read in
    /// `initialized` to decide whether to attempt registration.
    pub(crate) client_supports_dynamic_watch: Arc<std::sync::atomic::AtomicBool>,
}

impl Backend {
    pub(crate) fn new(client: Client) -> Self {
        Self {
            client,
            docs: Arc::new(Mutex::new(HashMap::new())),
            latest_versions: Arc::new(Mutex::new(HashMap::new())),
            workspace_sym_cache: Arc::new(Mutex::new(HashMap::new())),
            workspace_file_cache: Arc::new(Mutex::new(HashMap::new())),
            watch_registered: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            client_supports_dynamic_watch: Arc::new(std::sync::atomic::AtomicBool::new(
                false,
            )),
        }
    }

    pub(crate) async fn refresh(&self, uri: Url, text: String) {
        refresh_impl(&self.client, &self.docs, uri, text).await
    }

    /// Acquire the `docs` map. Wraps the `lock().unwrap()` boilerplate
    /// — the mutex is only poisoned if a holder panicked, which we
    /// treat as unrecoverable.
    pub(crate) fn docs(&self) -> std::sync::MutexGuard<'_, HashMap<Url, Doc>> {
        self.docs.lock().unwrap()
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
                let extra = dep_tree.dirs.clone();
                let names_to_dirs = dep_tree.names_to_dirs.clone();
                // Overlay the live buffer text onto the *edited* file
                // (`path`) — see `build_overlay` for why this must be the
                // edited file, not the merge entry `p`. Other open
                // buffers seed the overlay too (canonicalised here, off
                // the lock-free path below isn't possible since we need
                // the docs map).
                let edited_canon =
                    path.as_deref().and_then(|edited| edited.canonicalize().ok());
                let others: Vec<(PathBuf, String)> = {
                    let lock = docs.lock().unwrap();
                    lock.iter()
                        .filter_map(|(other_uri, doc)| {
                            let other_path = other_uri.to_file_path().ok()?;
                            let canon = other_path.canonicalize().ok()?;
                            Some((canon, doc.text.clone()))
                        })
                        .collect()
                };
                let overlay = build_overlay(edited_canon, &text, others);
                ilang_parser::loader::load_program_full(
                    p, &extra, &dep_tree.parents, &names_to_dirs, &overlay,
                ).ok()
            })
    };
    let diags = analyse(parsed_buffer.as_ref(), &merged, is_submodule);
    let (mut external_sigs, mut external_rets, mut external_fn_params) = merged
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
        mirror_imported_returns_to_bare(
            buffer_prog,
            &mut external_rets,
            &mut external_fn_params,
        );
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
    let external_enums = merged
        .as_ref()
        .map(collect_external_enums)
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
                &external_fn_params,
                &external_classes,
                &external_sources,
                &external_docs,
                &external_interfaces,
                &external_enums,
            );
            d.external.docs = external_docs;
            d.external.fn_params = external_fn_params;
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
                entry.external.signatures = external_sigs;
            }
            if !external_rets.is_empty() {
                entry.external.returns = external_rets;
            }
            if !external_fn_params.is_empty() {
                entry.external.fn_params = external_fn_params;
            }
            if !external_docs.is_empty() {
                entry.external.docs = external_docs;
            }
            if !external_sources.is_empty() {
                entry.external.sources = external_sources;
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

/// Mirror dotted return-type entries (`a.b.c.X` → `T`) under their bare
/// names for every `use a.b.c { X }` / `use a.b.c { * }` import in
/// `buffer_prog`. The merged-program scan in
/// `collect_external_signatures` only emits dotted keys, but call
/// expressions in the buffer reference the imported names bare. Without
/// this alias step `let x = X(...)` couldn't recover a hover type for
/// `x`. Selective-import lookups use the full `module + subpath` prefix
/// — using just the leaf segment misses every multi-segment import
/// (`use std.ffi { … }` → keys live under `std.ffi.<X>`, not `ffi.<X>`).
pub(crate) fn mirror_imported_returns_to_bare(
    buffer_prog: &Program,
    external_rets: &mut HashMap<AstSymbol, Type>,
    external_fn_params: &mut HashMap<AstSymbol, Vec<Type>>,
) {
    for item in &buffer_prog.items {
        let Item::Use(u) = item else { continue };
        let effective_module = if u.subpath.is_empty() {
            u.module.as_str().to_string()
        } else {
            let mut s = u.module.as_str().to_string();
            for seg in u.subpath.iter() {
                s.push('.');
                s.push_str(seg.as_str());
            }
            s
        };
        if u.wildcard && u.selective.is_none() {
            let module_dot = format!("{effective_module}.");
            let bare_rets: Vec<(AstSymbol, _)> = external_rets
                .iter()
                .filter_map(|(k, v)| {
                    k.as_str()
                        .strip_prefix(&module_dot)
                        .map(|tail| (AstSymbol::intern(tail), v.clone()))
                })
                .collect();
            for (k, v) in bare_rets {
                external_rets.entry(k).or_insert(v);
            }
            let bare_params: Vec<(AstSymbol, _)> = external_fn_params
                .iter()
                .filter_map(|(k, v)| {
                    k.as_str()
                        .strip_prefix(&module_dot)
                        .map(|tail| (AstSymbol::intern(tail), v.clone()))
                })
                .collect();
            for (k, v) in bare_params {
                external_fn_params.entry(k).or_insert(v);
            }
            continue;
        }
        let Some(names) = &u.selective else { continue };
        for name in names.iter() {
            let prefixed = AstSymbol::intern(&format!("{effective_module}.{name}"));
            if let Some(t) = external_rets.get(&prefixed).cloned() {
                external_rets.insert(name.clone(), t);
            }
            if let Some(ps) = external_fn_params.get(&prefixed).cloned() {
                external_fn_params.insert(name.clone(), ps);
            }
        }
    }
}

/// Build the loader overlay for a refresh.
///
/// `edited_canon` is the canonicalised path of the file the user is
/// actually editing, and `text` is its live buffer. Crucially this is
/// the *edited* file — NOT the merge entry the loader is driven from.
/// The two diverge when editing a sub-module: the merge entry is then
/// the umbrella (`sdl.il`), but the unsaved text belongs to the
/// sub-module (`sdl_window.il`). Keying the buffer under the umbrella's
/// path would make the loader parse the umbrella *as* the sub-module
/// body, corrupting the merged program used for hover / F12 /
/// completion. The edited file's entry therefore wins and is never
/// overwritten by the `others` pass.
///
/// `others` is every other open buffer (already canonicalised); each
/// only fills a gap — it never displaces the edited file's text.
fn build_overlay(
    edited_canon: Option<PathBuf>,
    text: &str,
    others: impl IntoIterator<Item = (PathBuf, String)>,
) -> HashMap<PathBuf, String> {
    let mut overlay: HashMap<PathBuf, String> = HashMap::new();
    if let Some(canon) = edited_canon {
        overlay.insert(canon, text.to_string());
    }
    for (canon, txt) in others {
        overlay.entry(canon).or_insert(txt);
    }
    overlay
}

#[cfg(test)]
mod tests {
    use super::build_overlay;
    use std::path::PathBuf;

    /// Regression: editing a sub-module must overlay the buffer onto the
    /// sub-module's own path, leaving the umbrella's on-disk text intact.
    /// Pre-fix the buffer was keyed under the umbrella (the merge entry),
    /// so the loader saw the umbrella replaced by the sub-module body.
    #[test]
    fn buffer_overlays_edited_file_not_umbrella() {
        let submodule = PathBuf::from("/proj/sdl_window.il");
        let umbrella = PathBuf::from("/proj/sdl.il");
        let overlay = build_overlay(
            Some(submodule.clone()),
            "pub fn open() {}",
            vec![(umbrella.clone(), "use sdl_window".to_string())],
        );
        // The edited sub-module carries the live buffer …
        assert_eq!(overlay.get(&submodule).map(String::as_str), Some("pub fn open() {}"));
        // … and the umbrella keeps its own on-disk text, never the buffer.
        assert_eq!(overlay.get(&umbrella).map(String::as_str), Some("use sdl_window"));
    }

    /// The edited file's text must win even if the same path also shows
    /// up in `others` (the file is open in its own buffer too).
    #[test]
    fn edited_text_is_never_overwritten_by_others() {
        let p = PathBuf::from("/proj/a.il");
        let overlay = build_overlay(
            Some(p.clone()),
            "live edit",
            vec![(p.clone(), "stale".to_string())],
        );
        assert_eq!(overlay.get(&p).map(String::as_str), Some("live edit"));
    }

    /// No edited path (unsaved scratch buffer): only the `others` seed
    /// the overlay.
    #[test]
    fn no_edited_path_uses_only_others() {
        let other = PathBuf::from("/proj/b.il");
        let overlay = build_overlay(
            None,
            "ignored",
            vec![(other.clone(), "body".to_string())],
        );
        assert_eq!(overlay.len(), 1);
        assert_eq!(overlay.get(&other).map(String::as_str), Some("body"));
    }
}

