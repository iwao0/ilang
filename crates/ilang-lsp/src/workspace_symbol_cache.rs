//! Per-file cache for `workspace/symbol` requests.
//!
//! Each entry stores the file's mtime and the pre-extracted list of
//! symbol records (top-level decls + class members + enum variants).
//! On a request, files whose disk mtime hasn't changed reuse the
//! cached entry list; only the (cheap) name filter runs per query.
//! Open buffers always re-parse — their text is the live in-memory
//! version, which mtime can't represent.

use std::path::Path;
use std::time::SystemTime;

use ilang_ast::{ClassDecl, Item, Program, Span};
use tower_lsp::lsp_types::{Range, SymbolKind};

use crate::text;
use crate::walker::is_parser_synth_field;

/// One cached `.il` file. `entries` is the flat list of symbols
/// the workspace-symbol handler emits — filtered against each
/// query at request time.
#[derive(Clone)]
pub(crate) struct Entry {
    pub mtime: Option<SystemTime>,
    pub items: Vec<Symbol>,
}

#[derive(Clone)]
pub(crate) struct Symbol {
    pub name:      String,
    pub kind:      SymbolKind,
    pub container: Option<String>,
    pub range:     Range,
}

/// Build the symbol list for `text` parsed as a `.il` file. Returns
/// an empty list when the file doesn't tokenize / parse — same
/// failure mode as the existing inline path.
pub(crate) fn build(text: &str) -> Vec<Symbol> {
    let Some(prog) = crate::text::try_parse(text) else { return Vec::new() };
    // Build the line-start table once; every `push_top` below would
    // otherwise rescan the buffer from byte 0 per decl.
    let line_starts = crate::text_utils::compute_line_starts(text);
    let mut out = Vec::new();
    collect_program(&line_starts, text, &prog, &mut out);
    out
}

fn collect_program(line_starts: &[usize], text: &str, prog: &Program, out: &mut Vec<Symbol>) {
    for item in &prog.items {
        collect_item(line_starts, text, item, None, out);
    }
}

fn collect_item(line_starts: &[usize], text: &str, item: &Item, container: Option<&str>, out: &mut Vec<Symbol>) {
    match item {
        Item::Fn(f) => push_top(
            line_starts, text, f.span, "fn", f.name.as_str(), SymbolKind::FUNCTION,
            container, out,
        ),
        Item::Class(c) => collect_class(line_starts, text, c, container, out),
        Item::Interface(i) => {
            push_top(
                line_starts, text, i.span, "interface", i.name.as_str(),
                SymbolKind::INTERFACE, container, out,
            );
            for m in i.methods.iter() {
                push_top(
                    line_starts, text, m.span, "fn", m.name.as_str(),
                    SymbolKind::METHOD, Some(i.name.as_str()), out,
                );
            }
        }
        Item::Enum(e) => {
            push_top(
                line_starts, text, e.span, "enum", e.name.as_str(),
                SymbolKind::ENUM, container, out,
            );
            for v in e.variants.iter() {
                push_at_span(
                    v.span, v.name.as_str(), SymbolKind::ENUM_MEMBER,
                    Some(e.name.as_str()), out,
                );
            }
        }
        Item::Const(c) => push_top(
            line_starts, text, c.span, "const", c.name.as_str(),
            SymbolKind::CONSTANT, container, out,
        ),
        Item::ExternC(b) => {
            for inner in b.items.iter() {
                match inner {
                    ilang_ast::ExternCItem::FnDef(f) => push_top(
                        line_starts, text, f.span, "fn", f.name.as_str(),
                        SymbolKind::FUNCTION, container, out,
                    ),
                    ilang_ast::ExternCItem::FnDecl { name, span, .. } => push_top(
                        line_starts, text, *span, "fn", name.as_str(),
                        SymbolKind::FUNCTION, container, out,
                    ),
                    ilang_ast::ExternCItem::Class(c) => {
                        collect_class(line_starts, text, c, container, out)
                    }
                    ilang_ast::ExternCItem::Struct { name, fields, span, .. } => {
                        push_top(
                            line_starts, text, *span, "struct", name.as_str(),
                            SymbolKind::STRUCT, container, out,
                        );
                        for f in fields.iter() {
                            push_at_span(
                                f.span, f.name.as_str(), SymbolKind::FIELD,
                                Some(name.as_str()), out,
                            );
                        }
                    }
                    ilang_ast::ExternCItem::Union { name, fields, span, .. } => {
                        push_top(
                            line_starts, text, *span, "union", name.as_str(),
                            SymbolKind::STRUCT, container, out,
                        );
                        for f in fields.iter() {
                            push_at_span(
                                f.span, f.name.as_str(), SymbolKind::FIELD,
                                Some(name.as_str()), out,
                            );
                        }
                    }
                }
            }
            for iface in b.interfaces.iter() {
                collect_item(line_starts, text, &Item::Interface(iface.clone()), container, out);
            }
            for c in b.consts.iter() {
                collect_item(line_starts, text, &Item::Const(c.clone()), container, out);
            }
        }
        Item::Use(_) => {}
    }
}

fn collect_class(
    line_starts: &[usize],
    text: &str,
    c: &ClassDecl,
    container: Option<&str>,
    out: &mut Vec<Symbol>,
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
    push_top(line_starts, text, c.span, kw, c.name.as_str(), kind, container, out);
    let class_name = c.name.as_str();
    for f in c.fields.iter() {
        if is_parser_synth_field(f, c.span) {
            continue;
        }
        push_at_span(
            f.span, f.name.as_str(), SymbolKind::FIELD,
            Some(class_name), out,
        );
    }
    for f in c.static_fields.iter() {
        let k = if f.is_const { SymbolKind::CONSTANT } else { SymbolKind::FIELD };
        push_at_span(
            f.span, f.name.as_str(), k, Some(class_name), out,
        );
    }
    for p in c.properties.iter() {
        push_at_span(
            p.span, p.name.as_str(), SymbolKind::PROPERTY,
            Some(class_name), out,
        );
    }
    for m in c.methods.iter() {
        let k = if m.name.as_str() == "init" {
            SymbolKind::CONSTRUCTOR
        } else {
            SymbolKind::METHOD
        };
        push_top(line_starts, text, m.span, "fn", m.name.as_str(), k, Some(class_name), out);
    }
    for m in c.static_methods.iter() {
        push_top(
            line_starts, text, m.span, "fn", m.name.as_str(), SymbolKind::METHOD,
            Some(class_name), out,
        );
    }
}

fn push_top(
    line_starts: &[usize],
    text: &str,
    decl_span: Span,
    kw: &str,
    name: &str,
    kind: SymbolKind,
    container: Option<&str>,
    out: &mut Vec<Symbol>,
) {
    let name_span = text::locate_let_name_with_kw_at(line_starts, text, decl_span, kw, name)
        .unwrap_or(decl_span);
    out.push(Symbol {
        name:      name.to_string(),
        kind,
        container: container.map(|s| s.to_string()),
        range:     text::span_to_range(name_span, name.len()),
    });
}

fn push_at_span(
    name_span: Span,
    name: &str,
    kind: SymbolKind,
    container: Option<&str>,
    out: &mut Vec<Symbol>,
) {
    out.push(Symbol {
        name:      name.to_string(),
        kind,
        container: container.map(|s| s.to_string()),
        range:     text::span_to_range(name_span, name.len()),
    });
}

/// Disk mtime for the file, or `None` if metadata isn't readable.
pub(crate) fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}


use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use tower_lsp::lsp_types::{Location, SymbolInformation, Url};

use crate::analyse::{collect_workspace_il_files, workspace_root_for};
use crate::text::subsequence_ci;

/// Cap the response to keep VSCode's quick-pick responsive on large
/// workspaces. Picked to be well above any realistic result count
/// for an ilang project.
const MAX_RESULTS: usize = 2000;

/// Orchestrate `workspace/symbol`. `open_texts` is a snapshot of
/// the live buffers (keyed by canonical path), so the caller can
/// release the docs lock before this walks the workspace. `cache`
/// is the persistent disk-symbol cache the LSP keeps across
/// requests.
pub(crate) fn handle_workspace_symbol(
    query: &str,
    anchor: &Path,
    open_texts: &HashMap<PathBuf, String>,
    cache: &Mutex<HashMap<PathBuf, Entry>>,
    file_cache: &Mutex<HashMap<PathBuf, Vec<PathBuf>>>,
    use_file_cache: bool,
) -> Option<Vec<SymbolInformation>> {
    let q_lower = query.to_lowercase();
    // File-list lookup. When watching is registered the list is cached
    // per workspace root and reused across keystrokes; otherwise re-walk
    // every request so a freshly created file can't be missed.
    let files = if use_file_cache {
        let root = workspace_root_for(anchor);
        let cached = file_cache.lock().unwrap().get(&root).cloned();
        match cached {
            Some(list) => list,
            None => {
                let list = collect_workspace_il_files(anchor);
                file_cache.lock().unwrap().insert(root, list.clone());
                list
            }
        }
    } else {
        collect_workspace_il_files(anchor)
    };
    let mut out: Vec<SymbolInformation> = Vec::new();
    for path in files {
        if out.len() >= MAX_RESULTS {
            break;
        }
        let Ok(uri) = Url::from_file_path(&path) else { continue };
        let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
        // Open buffer: parse the live text (may have unsaved edits),
        // don't touch cache. Closed file: serve from cache when its
        // on-disk mtime matches the cached entry; else parse and
        // refresh the cache.
        let entries: Vec<Symbol> = if let Some(text) = open_texts.get(&canon) {
            build(text)
        } else {
            let disk_mtime = mtime(&canon);
            let guard = cache.lock().unwrap();
            let hit = guard
                .get(&canon)
                .filter(|e| e.mtime == disk_mtime)
                .map(|e| e.items.clone());
            match hit {
                Some(items) => items,
                None => {
                    drop(guard);
                    let Ok(text) = std::fs::read_to_string(&path) else { continue };
                    let items = build(&text);
                    cache.lock().unwrap().insert(
                        canon.clone(),
                        Entry { mtime: disk_mtime, items: items.clone() },
                    );
                    items
                }
            }
        };
        for s in entries {
            if !subsequence_ci(&s.name, &q_lower) {
                continue;
            }
            if out.len() >= MAX_RESULTS {
                break;
            }
            #[allow(deprecated)]
            out.push(SymbolInformation {
                name: s.name,
                kind: s.kind,
                tags: None,
                deprecated: None,
                location: Location {
                    uri: uri.clone(),
                    range: s.range,
                },
                container_name: s.container,
            });
        }
    }
    // Lower-case each name once into the sort key rather than twice per
    // comparison — comparison count grows with result size.
    out.sort_by_cached_key(|s| s.name.to_lowercase());
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}
