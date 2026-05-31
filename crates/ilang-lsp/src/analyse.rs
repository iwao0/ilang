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

pub(crate) fn make_hover_with_doc(sig: &str, doc: Option<&str>) -> Hover {
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

pub(crate) fn lookup_ref(doc: &Doc, pos: Position) -> Option<&RefEntry> {
    let line = pos.line + 1;
    let col = pos.character + 1;
    // `doc.refs` is sorted by `(line, start_col)` (see `diag.rs`), so
    // binary-search to the first ref on `line` and scan only that
    // line's (short) run instead of walking the whole list.
    let start = doc.refs.partition_point(|r| r.line < line);
    doc.refs[start..]
        .iter()
        .take_while(|r| r.line == line)
        .find(|r| col >= r.start_col && col <= r.end_col)
}

/// Build a `Doc` for a file we don't have open in the editor.
/// Mirrors the disk-loading half of `refresh_impl` so workspace
/// rename can pull `RefEntry` lists out of closed files. Returns
/// `None` for unreadable / unparsable files (silently skipped at
/// the call site).
/// Apply the loader's `@objc` auto-lift to a single-file parse,
/// seeded with the `@objc class` and `@objc interface` names from
/// the merged program. Without this, a top-level
/// `class C : NSObject { ... }` would never expose the synthesized
/// `alloc` / `init` / `register` methods through the buffer-local
/// `collect_classes` (auto-lift runs inside the loader, not on
/// raw parses), so hover on `let x = C.alloc().init()` would
/// report no type. Bare suffix is registered alongside the dotted
/// name so the buffer's bare-name parent (`NSObject`) resolves
/// before dealiasing has had a chance to map it to `cocoa.NSObject`.
pub(crate) fn lift_local_parse_objc(
    prog: ilang_ast::Program,
    merged: Option<&ilang_ast::Program>,
) -> ilang_ast::Program {
    use ilang_ast::ExternCItem;
    let mut objc_class_names: std::collections::HashSet<AstSymbol> =
        std::collections::HashSet::new();
    let mut objc_ifaces: HashMap<AstSymbol, ilang_ast::InterfaceDecl> =
        HashMap::new();
    if let Some(m) = merged {
        for item in &m.items {
            if let ilang_ast::Item::ExternC(blk) = item {
                for iface in blk.interfaces.iter() {
                    if iface.is_objc {
                        objc_ifaces.insert(iface.name, iface.clone());
                        if let Some(bare) =
                            iface.name.as_str().rsplit_once('.').map(|(_, t)| t)
                        {
                            objc_ifaces.insert(AstSymbol::intern(bare), iface.clone());
                        }
                    }
                }
                for inner in blk.items.iter() {
                    if let ExternCItem::Class(cd) = inner {
                        if cd.attrs.iter().any(|a| a.name.as_str() == "objc") {
                            objc_class_names.insert(cd.name);
                            if let Some(bare) =
                                cd.name.as_str().rsplit_once('.').map(|(_, t)| t)
                            {
                                objc_class_names.insert(AstSymbol::intern(bare));
                            }
                        }
                    }
                }
            }
        }
    }
    ilang_parser::loader::auto_lift_objc_subclasses_with(
        prog,
        &objc_ifaces,
        &objc_class_names,
    )
}

pub(crate) fn analyse_path_to_doc(path: &Path) -> Option<Doc> {
    let text = std::fs::read_to_string(path).ok()?;
    let parsed_buffer = parse_ok(&text).ok()?;
    // Sub-modules: load the merged program through the umbrella so
    // cross-module hover / F12 data is available even when the
    // sub-module file is opened alone. Mirror the same change in
    // `backend::refresh_impl`.
    let umbrella = find_umbrella(path);
    let entry: &Path = umbrella.as_deref().unwrap_or(path);
    let merged = {
        let dep_tree = crate::project::collect_dep_tree(entry).unwrap_or_default();
        let extra = dep_tree.dirs.clone();
        let names_to_dirs = dep_tree.names_to_dirs.clone();
        let mut overlay: HashMap<PathBuf, String> = HashMap::new();
        if let Ok(canon) = path.canonicalize() {
            overlay.insert(canon, text.clone());
        }
        ilang_parser::loader::load_program_full(
            entry, &extra, &dep_tree.parents, &names_to_dirs, &overlay,
        ).ok()
    };
    let (mut external_sigs, mut external_rets, mut external_fn_params) = merged
        .as_ref()
        .map(collect_external_signatures)
        .unwrap_or_default();
    crate::backend::mirror_imported_returns_to_bare(
        &parsed_buffer,
        &mut external_rets,
        &mut external_fn_params,
    );
    let mut external_sources: ExternalSources = HashMap::new();
    let mut external_docs: HashMap<AstSymbol, String> = HashMap::new();
    let mut external_const_types: HashMap<AstSymbol, Type> = HashMap::new();
    harvest_imported_consts(
        &path.to_path_buf(),
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
    // Mirror backend's auto-lift on the local parse so a
    // `class C : NSObject { ... }` exposes its synthesized
    // `alloc` / `init` / `register` methods through
    // `collect_classes(parsed_buffer)`.
    let parsed_buffer = lift_local_parse_objc(parsed_buffer, merged.as_ref());
    let mut doc = build_doc(
        text,
        &parsed_buffer,
        &external_sigs,
        &external_rets,
        &external_fn_params,
        &external_classes,
        &external_sources,
        &external_docs,
        &external_interfaces,
        &external_enums,
    );
    doc.external.docs = external_docs;
    doc.external.fn_params = external_fn_params;
    Some(doc)
}

/// Walk every `.il` file under the project rooted at `anchor`,
/// invoke `visit` with each closed file's `(Url, &Doc)`. Files whose
/// canonical path appears in `seen` are skipped — pass open
/// buffers' canonical paths there so cross-file passes don't
/// reprocess them off the on-disk version.
///
/// `cache` memoises `analyse_path_to_doc` results across requests;
/// each entry is keyed by canonical path with the file's mtime
/// stored alongside the `Arc<Doc>`. The cache must be `Some(_)`
/// ONLY when a file watcher is registered — `analyse_path_to_doc`
/// folds in sibling / import / umbrella state, so an entry can go
/// stale when the target file's own mtime hasn't moved. The watcher
/// + `clear_closed_doc_cache` on `didChangeWatchedFiles` is what
/// catches those external edits. Clients that don't expose file
/// events pass `None` and we just recompute every request.
pub(crate) fn for_each_closed_workspace_doc(
    anchor: &Path,
    seen: &HashSet<PathBuf>,
    file_cache: Option<&Mutex<HashMap<PathBuf, Vec<PathBuf>>>>,
    cache: Option<&crate::types::ClosedDocCache>,
    mut visit: impl FnMut(Url, &Doc),
) {
    for path in workspace_il_files_cached(anchor, file_cache) {
        let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
        if seen.contains(&canon) {
            continue;
        }
        let Ok(uri) = Url::from_file_path(&path) else { continue };
        let doc_arc: Arc<Doc> = if let Some(cache) = cache {
            let fp = crate::types::FileFingerprint::for_path(&canon);
            let cached: Option<Arc<Doc>> = {
                let guard = cache.lock().unwrap();
                guard.get(&canon).and_then(|(f, d)| {
                    if Some(*f) == fp {
                        Some(d.clone())
                    } else {
                        None
                    }
                })
            };
            if let Some(d) = cached {
                d
            } else {
                let Some(d) = analyse_path_to_doc(&path) else { continue };
                let arc = Arc::new(d);
                if let Some(f) = fp {
                    cache.lock().unwrap().insert(canon, (f, arc.clone()));
                }
                arc
            }
        } else {
            let Some(d) = analyse_path_to_doc(&path) else { continue };
            Arc::new(d)
        };
        visit(uri, &doc_arc);
    }
}

/// Drop every cached closed-document analyse. Called from
/// `did_change_watched_files` so a `.il` file written outside the
/// editor still surfaces fresh data on the next reference / rename /
/// call-hierarchy request.
pub(crate) fn clear_closed_doc_cache(cache: &crate::types::ClosedDocCache) {
    cache.lock().unwrap().clear();
}

/// Single-file variant of the closed-doc cache lookup. Used by
/// handlers that resolve one closed file at a time (e.g. outgoing
/// call hierarchy on a closed home file) instead of walking the
/// workspace. Same trust contract as `for_each_closed_workspace_doc`:
/// the cache is only consulted when the file watcher is registered
/// — pass `None` from `closed_doc_cache_if_trusted()` otherwise.
pub(crate) fn analyse_path_to_doc_cached(
    path: &Path,
    cache: Option<&crate::types::ClosedDocCache>,
) -> Option<Arc<Doc>> {
    let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if let Some(cache) = cache {
        let fp = crate::types::FileFingerprint::for_path(&canon);
        let cached: Option<Arc<Doc>> = {
            let guard = cache.lock().unwrap();
            guard.get(&canon).and_then(|(f, d)| {
                if Some(*f) == fp {
                    Some(d.clone())
                } else {
                    None
                }
            })
        };
        if let Some(d) = cached {
            return Some(d);
        }
        let arc = Arc::new(analyse_path_to_doc(path)?);
        if let Some(f) = fp {
            cache.lock().unwrap().insert(canon, (f, arc.clone()));
        }
        Some(arc)
    } else {
        Some(Arc::new(analyse_path_to_doc(path)?))
    }
}

/// Lightweight workspace walk for consumers that only need the
/// buffer text + parse, not the full cross-module-resolved `Doc`.
/// Skips `collect_dep_tree` + `load_program_full` + typecheck, so
/// requests like `textDocument/implementation` (which only reads
/// `class` / `interface` AST shapes) stay sub-millisecond per file
/// instead of paying for the full analyse pipeline.
///
/// Parses are mtime-cached per canonical path. Unlike the cross-
/// module `ClosedDocCache`, the value only depends on the file's
/// own bytes, so the cache is safe even without a file watcher;
/// the cache argument is required rather than optional.
pub(crate) fn for_each_closed_workspace_program(
    anchor: &Path,
    seen: &HashSet<PathBuf>,
    file_cache: Option<&Mutex<HashMap<PathBuf, Vec<PathBuf>>>>,
    cache: &crate::types::ClosedParseCache,
    mut visit: impl FnMut(Url, &str, &Program),
) {
    for path in workspace_il_files_cached(anchor, file_cache) {
        let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
        if seen.contains(&canon) {
            continue;
        }
        let Ok(uri) = Url::from_file_path(&path) else { continue };
        let fp = crate::types::FileFingerprint::for_path(&canon);
        let cached: Option<Arc<(String, Program)>> = {
            let guard = cache.lock().unwrap();
            guard.get(&canon).and_then(|(f, e)| {
                if Some(*f) == fp {
                    Some(e.clone())
                } else {
                    None
                }
            })
        };
        let entry: Arc<(String, Program)> = if let Some(e) = cached {
            e
        } else {
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let Ok(prog) = parse_ok(&text) else { continue };
            let arc = Arc::new((text, prog));
            if let Some(f) = fp {
                cache.lock().unwrap().insert(canon, (f, arc.clone()));
            }
            arc
        };
        visit(uri, &entry.0, &entry.1);
    }
}

/// Drop every cached closed-file parse. Called from
/// `did_change_watched_files` to bound memory; correctness doesn't
/// require it since the mtime check inside
/// `for_each_closed_workspace_program` already catches edits.
pub(crate) fn clear_closed_parse_cache(cache: &crate::types::ClosedParseCache) {
    cache.lock().unwrap().clear();
}

/// Walk a workspace looking for every `.il` file. The starting
/// point is the directory containing the renamed file's `ilang.toml`
/// (or the file's own directory if there's no project file). Used
/// by workspace-wide rename to pick up references in files that
/// aren't currently open.
pub(crate) fn collect_workspace_il_files(anchor: &Path) -> Vec<PathBuf> {
    let workspace_root = workspace_root_for(anchor);
    let mut out: Vec<PathBuf> = Vec::new();
    walk_il(&workspace_root, &mut out);
    out
}

/// `collect_workspace_il_files`, optionally backed by `Backend`'s
/// `workspace_file_cache`. When `Some(file_cache)` is passed, the
/// walk happens at most once per workspace root for the lifetime
/// of the cache (cleared on `didChangeWatchedFiles`). When `None`,
/// every call re-walks. Callers gate on `watch_registered` — see
/// `Backend::workspace_file_cache_if_trusted`.
pub(crate) fn workspace_il_files_cached(
    anchor: &Path,
    file_cache: Option<&Mutex<HashMap<PathBuf, Vec<PathBuf>>>>,
) -> Vec<PathBuf> {
    let Some(fc) = file_cache else {
        return collect_workspace_il_files(anchor);
    };
    let root = workspace_root_for(anchor);
    let cached = fc.lock().unwrap().get(&root).cloned();
    match cached {
        Some(list) => list,
        None => {
            let list = collect_workspace_il_files(anchor);
            fc.lock().unwrap().insert(root, list.clone());
            list
        }
    }
}

/// Resolve the workspace root for `anchor`: the directory containing the
/// nearest `ilang.toml`, or the anchor's own directory if there's no
/// project file. Split out from `collect_workspace_il_files` so callers
/// that cache the file list can key it by this stable root.
pub(crate) fn workspace_root_for(anchor: &Path) -> PathBuf {
    let entry_dir = anchor
        .canonicalize()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let project_file = find_project_file(&entry_dir);
    project_file
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or(entry_dir)
}

pub(crate) fn walk_il(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            // Skip the cargo / build / `.git` blackholes.
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if matches!(name, "target" | ".git" | "node_modules" | ".claude") {
                continue;
            }
            walk_il(&p, out);
        } else if p.extension().and_then(|e| e.to_str()) == Some("il") {
            if let Ok(canon) = p.canonicalize() {
                out.push(canon);
            }
        }
    }
}

/// Buffer-side parse error carried alongside `parse_ok`'s `Result`.
/// Span + message is enough to surface the failure as a diagnostic;
/// keeping it concrete here means callers can forward the failure
/// to `analyse` without re-tokenizing to recover the span.
pub(crate) type ParseDiag = (Span, String);

pub(crate) fn parse_ok(src: &str) -> Result<Program, ParseDiag> {
    let tokens = tokenize(src).map_err(|e| (e.span(), e.to_string()))?;
    parse(&tokens).map_err(|e| (e.span(), e.to_string()))
}

/// Run the type checker against a (possibly merged) buffer parse and
/// return diagnostics. `parsed_buffer` is the buffer's own parse —
/// `refresh_impl` already produces it via `parse_ok` and threads it
/// in so analyse doesn't redo the lex+parse pass on every keystroke.
pub(crate) fn analyse(
    parsed_buffer: Result<&Program, &ParseDiag>,
    merged: &Option<Program>,
    is_submodule: bool,
) -> Vec<crate::diag::DiagEntry> {
    let mut out = Vec::new();
    let buffer_prog = match parsed_buffer {
        Ok(p) => p,
        Err((span, msg)) => {
            out.push(diag(*span, msg.clone()));
            return out;
        }
    };
    // Sub-modules can't resolve cross-module references on their own;
    // typecheck would emit spurious "undefined class sdl.X" errors.
    // Stop after parse for those.
    if is_submodule {
        return out;
    }
    if let Some(prog) = merged {
        let mut tc = TypeChecker::new();
        let (_, errs) = tc.check(prog);
        for e in errs {
            out.push(diag(e.span(), e.to_string()));
        }
        for w in tc.warnings() {
            out.push(crate::diag::warn_diag(w.span, w.message));
        }
        return out;
    }
    // Fallback: typecheck the buffer-local parse alone (no module
    // resolution, no const inlining). Used for unsaved buffers without
    // an on-disk file or when loading failed (the load error itself is
    // reported by the caller via `refresh`).
    let mut tc = TypeChecker::new();
    let (_, errs) = tc.check(buffer_prog);
    for e in errs {
        out.push(diag(e.span(), e.to_string()));
    }
    for w in tc.warnings() {
        out.push(crate::diag::warn_diag(w.span, w.message));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::DiagnosticSeverity;

    fn err_diagnostics(src: &str) -> Vec<crate::diag::DiagEntry> {
        let parsed = parse_ok(src);
        let parsed_ref = parsed.as_ref();
        analyse(parsed_ref, &None, false)
            .into_iter()
            .filter(|d| d.diagnostic.severity == Some(DiagnosticSeverity::ERROR))
            .collect()
    }

    #[test]
    fn single_undefined_var_reports_one_diagnostic() {
        let diags = err_diagnostics("let x = undef");
        assert_eq!(diags.len(), 1, "got: {diags:?}");
    }

    // The LSP layer maps the checker's `Vec<TypeError>` 1:1 onto
    // diagnostics, so `console.log(aa, bb)` surfaces a diagnostic on
    // both `aa` and `bb`.
    #[test]
    fn console_log_two_bad_args_show_two_diagnostics() {
        let diags = err_diagnostics("console.log(aa, bb)");
        assert_eq!(
            diags.len(),
            2,
            "expected diagnostic per bad arg, got {diags:?}"
        );
    }

    /// Regression: `slot.onClick(...)` where `onClick: fn(...)` is
    /// a fn-typed instance field used to produce no hover entry at
    /// all — `walk_expr_method_call` only looked at
    /// `info.methods.get(...)` and bailed. Falling back to
    /// `info.fields` for fn-typed members gives callback-style
    /// fields the same hover treatment as methods.
    #[test]
    fn fn_typed_field_call_has_hover() {
        let src = "\
class Slot {
    pub onClick: fn(f64, f64, i32)
    pub init() { this.onClick = fn(x: f64, y: f64, b: i32) {} }
}
fn run() {
    let s = new Slot()
    s.onClick(0.0, 0.0, 0)
}
run()
";
        let tmp = std::env::temp_dir()
            .join("ilang_lsp_probe_fn_field_call.il");
        std::fs::write(&tmp, src).unwrap();
        let doc = analyse_path_to_doc(&tmp).expect("doc built");
        let on_click = doc
            .refs
            .iter()
            .find(|r| {
                r.signature.contains("onClick") && r.signature.contains("fn(")
            })
            .unwrap_or_else(|| panic!("no `onClick` ref with fn signature; got {:#?}", doc.refs));
        assert!(
            on_click.signature.contains("(property)"),
            "expected onClick hover to render as a property hover, got {:?}",
            on_click.signature,
        );
    }

    /// Regression: `let classW = "BUTTON".encodeUtf16()` (and any
    /// other built-in string method call as the value) used to hover
    /// untyped because `infer_expr`'s MethodCall arm only consulted
    /// the `classes` table. String methods aren't registered there;
    /// route them through the dedicated `string_method_return_type`
    /// lookup so the binding picks up `u16[]` / `string` / etc.
    #[test]
    fn string_method_call_let_has_typed_hover() {
        let src = "\
@extern(C) {
    pub fn run() {
        let classW = \"BUTTON\".encodeUtf16()
        let trimmed = \"  x  \".trim()
        let chars = \"a,b\".split(\",\")
        let i = \"hello\".indexOf(\"l\")
    }
}
";
        let tmp = std::env::temp_dir()
            .join("ilang_lsp_probe_string_method_let.il");
        std::fs::write(&tmp, src).unwrap();
        let doc = analyse_path_to_doc(&tmp).expect("doc built");
        let expect = |name: &str, ty: &str| {
            let r = doc
                .refs
                .iter()
                .find(|r| r.signature.starts_with(&format!("let {name}")))
                .unwrap_or_else(|| panic!("no `let {name}` ref"));
            assert!(
                r.signature.ends_with(ty),
                "expected `let {name}` to end with {ty:?}, got {:?}",
                r.signature,
            );
        };
        expect("classW", ": u16[]");
        expect("trimmed", ": string");
        expect("chars", ": string[]");
        expect("i", ": i64");
    }

    /// Regression: hovering on `parent` at `libs/gui/win32/button.il`
    /// line 50 used to show "let parent" (no type) on macOS because
    /// the cross-target sub-package (`gui_impl` selects cocoa on the
    /// host, so win32 isn't walked as a dep of `libs/gui`) had no
    /// parent edge recorded in the LSP's dep tree. Loading the
    /// merged program then failed on win32's `use super.events`,
    /// `external_returns` stayed empty, and every imported call
    /// inferred to no type. We now add a filesystem-based parent
    /// edge during dep-tree collection, so the merge succeeds and
    /// `let parent` resolves to its HWND return type.
    #[test]
    fn button_il_parent_has_typed_hover() {
        let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.pop();
        p.pop();
        p.push("libs/gui/win32/button.il");
        if !p.exists() {
            eprintln!("skip: {} missing", p.display());
            return;
        }
        let doc = analyse_path_to_doc(&p).expect("doc built");
        let parent_decl = doc
            .refs
            .iter()
            .find(|r| r.signature.starts_with("let parent"))
            .unwrap_or_else(|| panic!("no `let parent` ref; got: {:#?}", doc.refs));
        assert!(
            parent_decl.signature.contains(": "),
            "expected `parent` hover to include an inferred type, got {:?}",
            parent_decl.signature,
        );
    }

    /// Regression: in a submodule whose umbrella re-exports a sibling
    /// (`pub use core.*`), a single-segment selective import from that
    /// sibling (`use core { windowHwnd }`) must still mirror the
    /// dotted return type into the bare name so a `let parent =
    /// windowHwnd(...)` hover renders with its inferred type.
    #[test]
    fn submodule_sibling_selective_import_mirrors_ret_type() {
        let tmp = std::env::temp_dir()
            .join(format!("ilang_lsp_probe_submod_sibling_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("ilang.toml"), "[package]\nname = \"t\"\n").unwrap();
        // The field bug surfaced when the fn returned `HWND` — a
        // struct declared in a THIRD module that `core` itself
        // selectively imports. Mirror that shape.
        std::fs::write(
            tmp.join("windows.il"),
            "\
@extern(C) {
    pub struct HWND {}
}
",
        )
        .unwrap();
        std::fs::write(
            tmp.join("core.il"),
            "\
use windows { HWND }
pub fn windowHwnd(h: i64): HWND { 0 as HWND }
",
        )
        .unwrap();
        std::fs::write(
            tmp.join("gui_impl.il"),
            "pub use windows.*\npub use core.*\npub use button.*\n",
        )
        .unwrap();
        let button = tmp.join("button.il");
        std::fs::write(
            &button,
            "\
use windows { HWND }
use core { windowHwnd }

@extern(C) {
    pub fn createNativeButton(handle: i64): i64 {
        let parent = windowHwnd(handle)
        0
    }
}
",
        )
        .unwrap();
        let doc = analyse_path_to_doc(&button).expect("doc built");
        let parent_decl = doc
            .refs
            .iter()
            .find(|r| r.signature.starts_with("let parent"))
            .unwrap_or_else(|| panic!("no `let parent` ref; got: {:#?}", doc.refs));
        assert!(
            parent_decl.signature.contains(": "),
            "expected `parent` hover to include an inferred type, got {:?}",
            parent_decl.signature,
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Multi-segment selective imports (`use std.ffi { arrayFromCArray }`)
    /// must mirror the dotted ret-type entry to the bare name so
    /// `let x = arrayFromCArray(...)` hovers with a type. The buggy
    /// path used the leaf segment (`ffi`) instead of the full prefix
    /// (`std.ffi`) and the alias lookup never hit.
    #[test]
    fn selective_import_with_subpath_mirrors_ret_type() {
        let src = "\
use std.ffi { arrayFromCArray }

@extern(C) {
    pub fn run() {
        let p = 0 as *const u8
        let units = arrayFromCArray(p, 4 as u64)
        units.length
    }
}
";
        let tmp = std::env::temp_dir().join("ilang_lsp_probe_subpath_import.il");
        std::fs::write(&tmp, src).unwrap();
        let doc = analyse_path_to_doc(&tmp).expect("doc built");
        let units_decl = doc
            .refs
            .iter()
            .find(|r| r.signature.starts_with("let units"))
            .unwrap_or_else(|| panic!("no `let units` ref; got: {:#?}", doc.refs));
        assert!(
            units_decl.signature.contains(": "),
            "expected `units` hover to include an inferred type, got {:?}",
            units_decl.signature,
        );
    }

    /// Generic intrinsics (`arrayFromCArray<T>(p: *const T, …): T[]`)
    /// must have `T` instantiated from the call's argument types so
    /// hover shows the concrete element type instead of `T[]`. The
    /// call below binds `T = u16` via the `*const u16` argument.
    #[test]
    fn generic_intrinsic_call_substitutes_typevar_in_return() {
        let src = "\
use std.ffi { arrayFromCArray }

@extern(C) {
    pub fn run() {
        let p = 0 as *const u16
        let units = arrayFromCArray(p, 4 as u64)
        units.length
    }
}
";
        let tmp = std::env::temp_dir().join("ilang_lsp_probe_typevar_subst.il");
        std::fs::write(&tmp, src).unwrap();
        let doc = analyse_path_to_doc(&tmp).expect("doc built");
        let units_decl = doc
            .refs
            .iter()
            .find(|r| r.signature.starts_with("let units"))
            .unwrap_or_else(|| panic!("no `let units` ref; got: {:#?}", doc.refs));
        assert_eq!(
            units_decl.signature, "let units: u16[]",
            "expected T to be substituted with u16 from the *const u16 arg",
        );
    }

    /// VS Code's faint `: u16[]` ghost text after `let units = …` is
    /// produced by `build_hints`, not the hover walker. Keep them in
    /// lock-step so users see the same instantiated type either way.
    #[test]
    fn generic_intrinsic_inlay_hint_substitutes_typevar() {
        use tower_lsp::lsp_types::{Position, Range};
        let src = "\
use std.ffi { arrayFromCArray }

@extern(C) {
    pub fn run() {
        let p = 0 as *const u16
        let units = arrayFromCArray(p, 4 as u64)
        units.length
    }
}
";
        let tmp = std::env::temp_dir().join("ilang_lsp_probe_inlay_typevar.il");
        std::fs::write(&tmp, src).unwrap();
        let doc = analyse_path_to_doc(&tmp).expect("doc built");
        let hints = crate::inlay_hints::build_hints(
            &doc,
            Range {
                start: Position { line: 0, character: 0 },
                end: Position { line: 99, character: 0 },
            },
        );
        // `let units = …` sits on line 5 (0-indexed) of the fixture.
        // Filter to that line so we don't accidentally match the
        // `: *const u16` hint on the preceding `let p`.
        let units_hint = hints
            .iter()
            .find(|h| h.position.line == 5)
            .unwrap_or_else(|| panic!("no inlay hint on line 5; got: {hints:#?}"));
        if let tower_lsp::lsp_types::InlayHintLabel::String(s) = &units_hint.label {
            assert_eq!(s, ": u16[]");
        } else {
            panic!("expected String label, got {:?}", units_hint.label);
        }
    }

    #[test]
    fn external_sources_track_subfolder_mod_il_definitions() {
        // After the `bindings/cocoa/foundation/` split, `NSString` /
        // `NSObject` live in `foundation/core.il`, re-exported by
        // `foundation/mod.il`. F12 from a sibling binding
        // (`bindings/cocoa/spritekit.il` does `use foundation
        // { NSString }`) must still land on the real declaration —
        // the harvest used to give up when `<dir>/foundation.il`
        // didn't exist and miss the `<dir>/foundation/mod.il`
        // fallback the loader now accepts.
        use std::path::PathBuf;
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop();
        path.pop();
        path.push("bindings/cocoa/spritekit/node.il");
        let doc = analyse_path_to_doc(&path)
            .expect("bindings/cocoa/spritekit/node.il must load");
        let ns_string_loc = doc
            .external
            .sources
            .get(&AstSymbol::intern("NSString"))
            .expect("F12 should resolve `NSString` through foundation/mod.il");
        // The target file must be the actual declaration site, not
        // the umbrella stub.
        // Normalise the platform path separator: the source-of-truth
        // assertion is the trailing `foundation/core.il` segments,
        // and we only care that the F12 target sits in that file.
        let path_str = ns_string_loc
            .path
            .to_string_lossy()
            .replace('\\', "/");
        assert!(
            path_str.ends_with("foundation/core.il"),
            "expected F12 target inside foundation/core.il, got {path_str}"
        );
    }

    // macOS-only: `libs/gui` resolves its `gui_impl` dep to `cocoa`
    // only when the host target is macOS (`[[deps.gui_impl]] target =
    // "macos"` in `libs/gui/ilang.toml`). On Windows / Linux the dep
    // tree picks `win32` / `linux` instead, so the loader can't
    // resolve combo.il's `use super.events` and `analyse_path_to_doc`
    // returns a degraded doc without external classes. Skip there.
    #[cfg(target_os = "macos")]
    #[test]
    fn inherited_external_method_lookup_keeps_parent_source_path() {
        // `libs/gui/cocoa/combo.il` calls `slot.combo.setFrame(...)`
        // where `slot.combo: NSPopUpButton`. `setFrame` is declared
        // on NSView (its grandparent), not on NSPopUpButton itself,
        // so the parent-chain flatten in `collect_external_classes`
        // is what makes the method visible on NSPopUpButton. F12
        // must route through `MemberInfo.source_path` so the jump
        // lands in NSView's declaring file (appkit/core.il) at the
        // recorded line, not in NSPopUpButton's file at the same
        // line number (which is a completely unrelated identifier).
        use std::path::PathBuf;
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop();
        path.pop();
        path.push("libs/gui/cocoa/combo.il");
        if !path.exists() {
            return;
        }
        let doc = analyse_path_to_doc(&path)
            .expect("libs/gui/cocoa/combo.il must load");
        let popup_info = doc
            .classes
            .get(&AstSymbol::intern("NSPopUpButton"))
            .expect("NSPopUpButton must be visible through `use cocoa`");
        let set_frame = popup_info
            .methods
            .get(&AstSymbol::intern("setFrame"))
            .expect("NSPopUpButton's flattened methods must include the inherited setFrame");
        let src_path = set_frame
            .source_path
            .as_ref()
            .expect("inherited methods must carry the parent's source_path");
        let path_str = src_path.to_string_lossy().replace('\\', "/");
        assert!(
            path_str.ends_with("appkit/core.il"),
            "expected setFrame's recorded path to live in NSView's file (appkit/core.il), got {path_str}"
        );
    }

    #[test]
    fn local_class_inheriting_nsobject_has_synth_alloc_init_types() {
        // `examples/macos/cocoa/main.il` declares
        //   class AppDelegate : NSApplicationDelegate { ... }
        //   class FormHandler : NSObject { ... }
        // The loader's auto-lift gives both classes synthesized
        // `alloc` / `init` / `register` methods. Confirm the LSP's
        // local parse (post-lift) sees them — without the lift on
        // the buffer-local path, `let appDel = AppDelegate.alloc().init()`
        // would infer no type and hover would come up blank.
        use std::path::PathBuf;
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop();
        path.pop();
        path.push("examples/macos/cocoa/main.il");
        let doc = analyse_path_to_doc(&path)
            .expect("examples/macos/cocoa/main.il must load");

        let app_del_key = AstSymbol::intern("AppDelegate");
        let info = doc
            .classes
            .get(&app_del_key)
            .expect("AppDelegate must be in doc.classes");
        assert!(
            info.methods.contains_key(&AstSymbol::intern("init")),
            "AppDelegate is missing the synth `init` method"
        );
        // `alloc` is a static method on @objc classes.
        let alloc_present = info
            .methods
            .get(&AstSymbol::intern("alloc"))
            .map(|m| m.is_static)
            .unwrap_or(false);
        assert!(
            alloc_present,
            "AppDelegate is missing the synth static `alloc` method"
        );

        // Likewise, AppDelegate.alloc().init() should be inferrable
        // as Object("AppDelegate"). The buffer binds the result to
        // `appDel`; var_types stores the walker-inferred type.
        let app_del_ty = doc.var_types.get(&AstSymbol::intern("appDel"));
        assert!(
            matches!(
                app_del_ty,
                Some(ilang_ast::Type::Object(n)) if n.as_str() == "AppDelegate"
            ),
            "expected appDel: AppDelegate, got {:?}",
            app_del_ty
        );
    }
}
