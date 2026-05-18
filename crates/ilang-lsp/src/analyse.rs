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
    doc.refs
        .iter()
        .find(|r| r.line == line && col >= r.start_col && col <= r.end_col)
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
    let is_submodule = find_umbrella(path).is_some();
    let merged = if is_submodule {
        None
    } else {
        let extra = collect_dep_paths(path).unwrap_or_default();
        let mut overlay: HashMap<PathBuf, String> = HashMap::new();
        if let Ok(canon) = path.canonicalize() {
            overlay.insert(canon, text.clone());
        }
        ilang_parser::loader::load_program_with_overlay(path, &extra, &overlay).ok()
    };
    let (mut external_sigs, external_rets) = merged
        .as_ref()
        .map(collect_external_signatures)
        .unwrap_or_default();
    let mut external_sources: ExternalSources = HashMap::new();
    let mut external_docs: HashMap<AstSymbol, String> = HashMap::new();
    harvest_imported_consts(
        &path.to_path_buf(),
        &text,
        &mut external_sigs,
        &mut external_sources,
        &mut external_docs,
    );
    let external_classes = merged
        .as_ref()
        .map(|p| collect_external_classes(p, &external_sources))
        .unwrap_or_default();
    let external_interfaces = merged
        .as_ref()
        .map(collect_external_interfaces)
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
        &external_classes,
        &external_sources,
        &external_docs,
        &external_interfaces,
    );
    doc.external_docs = external_docs;
    Some(doc)
}

/// Walk a workspace looking for every `.il` file. The starting
/// point is the directory containing the renamed file's `ilang.toml`
/// (or the file's own directory if there's no project file). Used
/// by workspace-wide rename to pick up references in files that
/// aren't currently open.
pub(crate) fn collect_workspace_il_files(anchor: &Path) -> Vec<PathBuf> {
    let entry_dir = anchor
        .canonicalize()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let project_file = find_project_file(&entry_dir);
    let workspace_root = project_file
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or(entry_dir);
    let mut out: Vec<PathBuf> = Vec::new();
    walk_il(&workspace_root, &mut out);
    out
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

pub(crate) fn parse_ok(src: &str) -> Result<Program, ()> {
    let tokens = tokenize(src).map_err(|_| ())?;
    parse(&tokens).map_err(|_| ())
}

pub(crate) fn analyse(
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
        for w in tc.warnings() {
            out.push(crate::diag::warn_diag(w.span, w.message));
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
    for w in tc.warnings() {
        out.push(crate::diag::warn_diag(w.span, w.message));
    }
    out
}

