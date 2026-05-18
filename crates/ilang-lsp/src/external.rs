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

/// `true` if `inner` is exposed via `pub` and should appear in
/// another module's `use M.` completion. Used by `walk_module` /
/// `walk_module_aliased` to skip the module's internal helpers
/// (dlsym'd C-runtime hooks like `_autoreleasepool_pop`, the
/// `_make_*_block` thunks etc.) that live in the same `@extern(C)`
/// block as the user-facing wrappers but aren't intended as
/// surface API.
fn is_extern_c_item_pub(inner: &ilang_ast::ExternCItem) -> bool {
    use ilang_ast::ExternCItem;
    match inner {
        ExternCItem::FnDecl { is_pub, .. } => *is_pub,
        ExternCItem::FnDef(f) => f.is_pub,
        ExternCItem::Struct { is_pub, .. } => *is_pub,
        ExternCItem::Union { is_pub, .. } => *is_pub,
        ExternCItem::Class(c) => c.is_pub,
    }
}

/// Pull top-level names with prefix-style identifiers (e.g.
/// `math.sqrt`, `math.pi`) out of a loader-merged program so the LSP
/// can answer hover queries on imported names. Plain (un-dotted) names
/// are skipped — they're already covered by the buffer-only index when
/// declared in the open file.
/// Per-decl source location for `module.<decl>` references — used by
/// cross-file F12 to land on the actual declaration line.
#[derive(Clone, Debug)]
pub(crate) struct ExternalLoc {
    pub(crate) path: PathBuf,
    pub(crate) span: Span,
    pub(crate) name_len: u32,
}

/// Walk the buffer's `use module` items and parse each module's source
/// (built-in or on-disk) to extract `Item::Const` declarations. Insert
/// them into `out` keyed by `module.const_name` so the buffer-only
/// walker can still resolve `math.pi` etc. — the main loader pass
/// would have inlined them. Also returns a `module.ClassName` → file
/// path map so cross-file F12 can navigate to the actual definition.
pub(crate) fn harvest_imported_consts(
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

pub(crate) fn harvest_from_program(
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
        if u.wildcard && u.selective.is_none() {
            // `use M { * }` — walk M, then re-key every `M.<X>`
            // entry as bare `<X>` so completion / hover / F12
            // treat the wildcard'd names the same way they treat
            // a `use M { X }` selective list. Mirrors the loader's
            // rename-rule expansion that turns the bare reference
            // into `M.<X>` at call time.
            harvest_wildcard_names(
                u.module.as_str(),
                &entry_dir,
                &extra,
                &mut visited,
                out,
                sources,
                docs,
            );
            continue;
        }
        if let Some(names) = &u.selective {
            // `use M { X1, X2 }` — pull X1/X2's hover info from M
            // (or its `pub use` chain) and key them under their
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
/// umbrella that re-exports its members via `pub use`) and
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
/// `use M { * }` — walk `M`, then promote every `M.<X>` entry (and
/// nested `M.<X>.<variant>` enum-variant key) to a bare `<X>` /
/// `<X>.<variant>` key. The buffer-side walker can then resolve a
/// bare `sharedApplication()` call the same way it resolves an
/// explicit `use cocoa { sharedApplication }`.
pub(crate) fn harvest_wildcard_names(
    module: &str,
    entry_dir: &Path,
    extra: &[PathBuf],
    visited: &mut HashSet<PathBuf>,
    out: &mut HashMap<AstSymbol, String>,
    sources: &mut ExternalSources,
    docs: &mut HashMap<AstSymbol, String>,
) {
    walk_module(module, entry_dir, extra, visited, out, sources, docs);
    let module_dot = format!("{module}.");
    let bare_entries: Vec<(AstSymbol, String)> = out
        .iter()
        .filter_map(|(k, v)| {
            k.as_str()
                .strip_prefix(&module_dot)
                .map(|tail| (AstSymbol::intern(tail), v.clone()))
        })
        .collect();
    for (k, v) in bare_entries {
        out.entry(k).or_insert(v);
    }
    let bare_sources: Vec<(AstSymbol, ExternalLoc)> = sources
        .iter()
        .filter_map(|(k, v)| {
            k.as_str()
                .strip_prefix(&module_dot)
                .map(|tail| (AstSymbol::intern(tail), v.clone()))
        })
        .collect();
    for (k, v) in bare_sources {
        sources.entry(k).or_insert(v);
    }
    let bare_docs: Vec<(AstSymbol, String)> = docs
        .iter()
        .filter_map(|(k, v)| {
            k.as_str()
                .strip_prefix(&module_dot)
                .map(|tail| (AstSymbol::intern(tail), v.clone()))
        })
        .collect();
    for (k, v) in bare_docs {
        docs.entry(k).or_insert(v);
    }
}

pub(crate) fn harvest_selective_names(
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

pub(crate) fn walk_module(
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
                let ty = match c
                    .ty
                    .clone()
                    .or_else(|| infer_expr_type_with_scope(&c.value, &[]))
                {
                    Some(t) => format!(": {t}"),
                    None => String::new(),
                };
                let value = render_const_value_with_src(&c.value, Some(&module_src))
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
                let repr = e
                    .repr_ty
                    .as_ref()
                    .map(|t| format!(": {t}"))
                    .unwrap_or_default();
                let flags_prefix = if e.flags { "@flags\n" } else { "" };
                out.insert(AstSymbol::intern(&key), format!("{flags_prefix}enum {key}{repr}"));
                track(&key, e.span, e.name.as_str().len() as u32, sources, &module_path);
                if let Some(d) = text::extract_doc_above(&module_src, e.span.line) {
                    docs.insert(AstSymbol::intern(&key), d);
                }
                register_enum_variants_with_sources(e, &key, out, sources, &module_path, &module_src);
            }
            Item::ExternC(b) => {
                for inner in &b.items {
                    // Skip module-private items — only `pub` inner
                    // FnDecls / FnDefs / Structs / Unions / Classes
                    // should surface in another file's `M.`
                    // completion. The `_autoreleasepool_pop` /
                    // `_make_obj_block` family live in foundation's
                    // ObjC runtime block without `pub` precisely so
                    // they stay internal.
                    if !is_extern_c_item_pub(inner) {
                        continue;
                    }
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
                                format!("@lib({names})\n")
                            };
                            (
                                *name,
                                *span,
                                format!("{libs_prefix}fn {prefix}.{name}({ps}){r}"),
                            )
                        }
                        ilang_ast::ExternCItem::FnDef(f) => {
                            // Mirror the FnDecl arm above
                            // (`fn {prefix}.name(params): ret`).
                            // Previously the format string used
                            // `fn_body(f)` (which already renders
                            // `name(params): ret`) *and* prepended
                            // `{prefix}.{name}` — producing
                            // `cocoa.sharedApplication sharedApplication(): NSApplication`
                            // in hover.
                            let ps = f
                                .params
                                .iter()
                                .map(|p| format!("{}: {}", p.name, p.ty))
                                .collect::<Vec<_>>()
                                .join(", ");
                            let r = match &f.ret {
                                Some(t) => format!(": {t}"),
                                None => String::new(),
                            };
                            (
                                f.name.into(),
                                f.span,
                                format!("fn {prefix}.{}({ps}){r}", f.name),
                            )
                        }
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
                // @objc interfaces declared alongside the C / @objc
                // items in the same block. Surface them in cross-module
                // completion so `use cocoa { NSApplicationDelegate }`
                // hovers and other-file references find the signature.
                for iface in b.interfaces.iter() {
                    if !iface.is_pub {
                        continue;
                    }
                    let methods: Vec<String> = iface
                        .methods
                        .iter()
                        .map(|m| {
                            let opt = if m.is_optional { "@optional " } else { "" };
                            let ps: Vec<String> = m
                                .params
                                .iter()
                                .map(|p| format!("{}: {}", p.name, p.ty))
                                .collect();
                            let r = match &m.ret {
                                Some(t) => format!(": {t}"),
                                None => String::new(),
                            };
                            format!("    {opt}{}({}){}", m.name, ps.join(", "), r)
                        })
                        .collect();
                    let header = if iface.is_objc { "@objc interface" } else { "interface" };
                    let sig = if methods.is_empty() {
                        format!("{header} {prefix}.{} {{}}", iface.name)
                    } else {
                        format!(
                            "{header} {prefix}.{} {{\n{}\n}}",
                            iface.name,
                            methods.join("\n")
                        )
                    };
                    let key = format!("{prefix}.{}", iface.name);
                    out.insert(AstSymbol::intern(&key), sig);
                    track(
                        &key,
                        iface.span,
                        iface.name.as_str().len() as u32,
                        sources,
                        &module_path,
                    );
                    if let Some(d) = text::extract_doc_above(&module_src, iface.span.line) {
                        docs.insert(AstSymbol::intern(&key), d);
                    }
                }
            }
            // Follow `pub use` chains so umbrella modules
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

pub(crate) fn walk_module_aliased(
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
                let ty = match c
                    .ty
                    .clone()
                    .or_else(|| infer_expr_type_with_scope(&c.value, &[]))
                {
                    Some(t) => format!(": {t}"),
                    None => String::new(),
                };
                let value = render_const_value_with_src(&c.value, Some(&module_src))
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
                let sig = format!("fn {}", fn_body(f));
                out.insert(
                    AstSymbol::intern(&key),
                    format!("fn {}", sig.trim_start_matches("fn ")),
                );
                put(&key, f.span, f.name.as_str().len() as u32, sources);
                if let Some(d) = text::extract_doc_above(&module_src, f.span.line) {
                    docs.insert(AstSymbol::intern(&key), d);
                }
            }
            Item::Class(c) => {
                let key = format!("{alias_prefix}.{}", c.name);
                out.insert(AstSymbol::intern(&key), format!("class {key}"));
                put(&key, c.span, c.name.as_str().len() as u32, sources);
                if let Some(d) = text::extract_doc_above(&module_src, c.span.line) {
                    docs.insert(AstSymbol::intern(&key), d);
                }
            }
            Item::Enum(e) => {
                let key = format!("{alias_prefix}.{}", e.name);
                let repr = e
                    .repr_ty
                    .as_ref()
                    .map(|t| format!(": {t}"))
                    .unwrap_or_default();
                let flags_prefix = if e.flags { "@flags\n" } else { "" };
                out.insert(AstSymbol::intern(&key), format!("{flags_prefix}enum {key}{repr}"));
                put(&key, e.span, e.name.as_str().len() as u32, sources);
                if let Some(d) = text::extract_doc_above(&module_src, e.span.line) {
                    docs.insert(AstSymbol::intern(&key), d);
                }
                register_enum_variants_with_sources(e, &key, out, sources, &module_path, &module_src);
            }
            Item::ExternC(b) => {
                for inner in &b.items {
                    // See `walk_module`'s ExternC arm — same rule
                    // applies to umbrella re-exports.
                    if !is_extern_c_item_pub(inner) {
                        continue;
                    }
                    let entry: Option<(AstSymbol, Span, String)> = match inner {
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
                                format!("@lib({names})\n")
                            };
                            Some((
                                (*name).into(),
                                *span,
                                format!("{libs_prefix}fn {alias_prefix}.{name}({ps}){r}"),
                            ))
                        }
                        ilang_ast::ExternCItem::FnDef(f) => {
                            // See `walk_module`'s same arm — the
                            // double-name bug applies here too.
                            let ps = f
                                .params
                                .iter()
                                .map(|p| format!("{}: {}", p.name, p.ty))
                                .collect::<Vec<_>>()
                                .join(", ");
                            let r = match &f.ret {
                                Some(t) => format!(": {t}"),
                                None => String::new(),
                            };
                            Some((
                                f.name.into(),
                                f.span,
                                format!("fn {alias_prefix}.{}({ps}){r}", f.name),
                            ))
                        }
                        ilang_ast::ExternCItem::Struct { name, span, .. } => Some((
                            (*name).into(),
                            *span,
                            format!("struct {alias_prefix}.{name}"),
                        )),
                        ilang_ast::ExternCItem::Union { name, span, .. } => Some((
                            (*name).into(),
                            *span,
                            format!("union {alias_prefix}.{name}"),
                        )),
                        ilang_ast::ExternCItem::Class(c) => Some((
                            c.name.into(),
                            c.span,
                            format!("class {alias_prefix}.{}", c.name),
                        )),
                    };
                    if let Some((n, span, sig)) = entry {
                        let len = n.as_str().len() as u32;
                        let key = format!("{alias_prefix}.{n}");
                        out.insert(AstSymbol::intern(&key), sig);
                        put(&key, span, len, sources);
                        if let Some(d) = text::extract_doc_above(&module_src, span.line) {
                            docs.insert(AstSymbol::intern(&key), d);
                        }
                    }
                }
                // Aliased re-export side: same enumeration for
                // @objc interfaces declared in the same block.
                for iface in b.interfaces.iter() {
                    if !iface.is_pub {
                        continue;
                    }
                    let methods: Vec<String> = iface
                        .methods
                        .iter()
                        .map(|m| {
                            let opt = if m.is_optional { "@optional " } else { "" };
                            let ps: Vec<String> = m
                                .params
                                .iter()
                                .map(|p| format!("{}: {}", p.name, p.ty))
                                .collect();
                            let r = match &m.ret {
                                Some(t) => format!(": {t}"),
                                None => String::new(),
                            };
                            format!("    {opt}{}({}){}", m.name, ps.join(", "), r)
                        })
                        .collect();
                    let header = if iface.is_objc { "@objc interface" } else { "interface" };
                    let sig = if methods.is_empty() {
                        format!("{header} {alias_prefix}.{} {{}}", iface.name)
                    } else {
                        format!(
                            "{header} {alias_prefix}.{} {{\n{}\n}}",
                            iface.name,
                            methods.join("\n")
                        )
                    };
                    let key = format!("{alias_prefix}.{}", iface.name);
                    let len = iface.name.as_str().len() as u32;
                    out.insert(AstSymbol::intern(&key), sig);
                    put(&key, iface.span, len, sources);
                    if let Some(d) = text::extract_doc_above(&module_src, iface.span.line) {
                        docs.insert(AstSymbol::intern(&key), d);
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
pub(crate) fn collect_external_classes(
    prog: &Program,
    sources: &ExternalSources,
) -> HashMap<AstSymbol, ClassInfo> {
    use ilang_ast::ExternCItem;
    let mut classes: Vec<&ClassDecl> = Vec::new();
    let mut out: HashMap<AstSymbol, ClassInfo> = HashMap::new();
    let mut src_cache: HashMap<PathBuf, String> = HashMap::new();
    pub(crate) fn ensure_src<'a>(
        cache: &'a mut HashMap<PathBuf, String>,
        sources: &ExternalSources,
        class_key: &AstSymbol,
    ) -> Option<&'a str> {
        let path = sources.get(class_key)?.path.clone();
        if !cache.contains_key(&path) {
            let txt = std::fs::read_to_string(&path).ok()?;
            cache.insert(path.clone(), txt);
        }
        cache.get(&path).map(|s| s.as_str())
    }
    pub(crate) fn field_doc_at(
        cache: &mut HashMap<PathBuf, String>,
        sources: &ExternalSources,
        class_key: &AstSymbol,
        line: u32,
    ) -> Option<String> {
        let s = ensure_src(cache, sources, class_key)?;
        text::extract_doc_above(s, line)
    }
    pub(crate) fn static_field_value(
        cache: &mut HashMap<PathBuf, String>,
        sources: &ExternalSources,
        class_key: &AstSymbol,
        value: &Expr,
    ) -> Option<String> {
        let s = ensure_src(cache, sources, class_key)?;
        render_const_value_with_src(value, Some(s))
    }
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
                                        doc: field_doc_at(&mut src_cache, sources, name, f.span.line),
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
    for c in &classes {
        let mut fields = HashMap::new();
        for f in &c.fields {
            fields.insert(
                f.name.into(),
                MemberInfo {
                    span: f.span,
                    signature: format!("(property) {}.{}: {}", c.name, f.name, f.ty),
                    ret_ty: Some(f.ty.clone()),
                    is_static: false,
                    doc: field_doc_at(&mut src_cache, sources, &c.name, f.span.line),
                },
            );
        }
        for f in &c.static_fields {
            let kind = if f.is_const { "static const" } else { "static property" };
            let value = static_field_value(&mut src_cache, sources, &c.name, &f.value)
                .map(|v| format!(" = {v}"))
                .unwrap_or_default();
            fields.insert(
                f.name.into(),
                MemberInfo {
                    span: f.span,
                    signature: format!(
                        "({}) {}.{}: {}{}",
                        kind, c.name, f.name, f.ty, value
                    ),
                    ret_ty: Some(f.ty.clone()),
                    is_static: true,
                    doc: field_doc_at(&mut src_cache, sources, &c.name, f.span.line),
                },
            );
        }
        let mut getters: HashMap<AstSymbol, MemberInfo> = HashMap::new();
        let mut setters: HashMap<AstSymbol, MemberInfo> = HashMap::new();
        for prop in &c.properties {
            let prop_doc = field_doc_at(&mut src_cache, sources, &c.name, prop.span.line);
            let prop_kind = if prop.is_static { "static property" } else { "property" };
            fields.insert(
                prop.name.into(),
                MemberInfo {
                    span: prop.span,
                    signature: format!(
                        "({prop_kind}) {}.{}: {}",
                        c.name, prop.name, prop.ty
                    ),
                    ret_ty: Some(prop.ty.clone()),
                    is_static: prop.is_static,
                    doc: prop_doc.clone(),
                },
            );
            let getter_label = if prop.is_static { "static getter" } else { "getter" };
            let setter_label = if prop.is_static { "static setter" } else { "setter" };
            if let Some(g) = &prop.getter {
                getters.insert(
                    prop.name.into(),
                    MemberInfo {
                        span: g.span,
                        signature: format!(
                            "({getter_label}) {}.{}: {}",
                            c.name, prop.name, prop.ty
                        ),
                        ret_ty: Some(prop.ty.clone()),
                        is_static: prop.is_static,
                        doc: field_doc_at(&mut src_cache, sources, &c.name, g.span.line).or_else(|| prop_doc.clone()),
                    },
                );
            }
            if let Some(s) = &prop.setter {
                setters.insert(
                    prop.name.into(),
                    MemberInfo {
                        span: s.span,
                        signature: format!(
                            "({setter_label}) {}.{}: {}",
                            c.name, prop.name, prop.ty
                        ),
                        ret_ty: Some(prop.ty.clone()),
                        is_static: prop.is_static,
                        doc: field_doc_at(&mut src_cache, sources, &c.name, s.span.line).or_else(|| prop_doc.clone()),
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
                signature: format!(
                    "(method) {}{}.{}",
                    render_user_attrs(&m.attrs),
                    c.name,
                    fn_body(m)
                ),
                ret_ty: m.ret.clone(),
                is_static: false,
                doc: field_doc_at(&mut src_cache, sources, &c.name, m.span.line),
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
                signature: format!(
                    "(static method) {}{}.{}",
                    render_user_attrs(&m.attrs),
                    c.name,
                    fn_body(m)
                ),
                is_static: true,
                ret_ty: m.ret.clone(),
                doc: field_doc_at(&mut src_cache, sources, &c.name, m.span.line),
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
    // Inherit members from the parent chain. Without this, hovering
    // `sprite.setPosition(...)` where `sprite: SKSpriteNode` would
    // fail because `setPosition` is declared on `SKNode` (the
    // parent) and only sits in `SKNode`'s methods map. Walk every
    // class's parent chain and copy fields / methods / getters /
    // setters into the child, leaving existing keys alone so
    // direct overrides win.
    let parents: HashMap<AstSymbol, AstSymbol> = classes
        .iter()
        .filter_map(|c| c.parent.as_ref().map(|p| (c.name.clone(), p.clone())))
        .collect();
    let class_names: Vec<AstSymbol> = out.keys().cloned().collect();
    for child_name in class_names {
        // Walk the parent chain. Each step looks the parent up
        // either by the recorded (possibly already-prefixed) name
        // or by any `*.<bare>` match — same fallback used at the
        // call-site alias pass — so an umbrella-imported parent
        // resolves too.
        let mut visited: std::collections::HashSet<AstSymbol> =
            std::collections::HashSet::new();
        let mut accumulated_fields: HashMap<AstSymbol, MemberInfo> = HashMap::new();
        let mut accumulated_methods: HashMap<AstSymbol, MemberInfo> = HashMap::new();
        let mut accumulated_getters: HashMap<AstSymbol, MemberInfo> = HashMap::new();
        let mut accumulated_setters: HashMap<AstSymbol, MemberInfo> = HashMap::new();
        let mut cursor = parents.get(&child_name).cloned();
        while let Some(parent_name) = cursor {
            if !visited.insert(parent_name.clone()) {
                break;
            }
            let resolved_key = if out.contains_key(&parent_name) {
                Some(parent_name.clone())
            } else {
                let suffix = format!(".{}", parent_name.as_str());
                out.keys()
                    .find(|k| k.as_str().ends_with(&suffix))
                    .cloned()
            };
            let Some(key) = resolved_key else { break };
            if let Some(info) = out.get(&key) {
                for (k, v) in &info.fields {
                    accumulated_fields.entry(k.clone()).or_insert_with(|| v.clone());
                }
                for (k, v) in &info.methods {
                    accumulated_methods.entry(k.clone()).or_insert_with(|| v.clone());
                }
                for (k, v) in &info.getters {
                    accumulated_getters.entry(k.clone()).or_insert_with(|| v.clone());
                }
                for (k, v) in &info.setters {
                    accumulated_setters.entry(k.clone()).or_insert_with(|| v.clone());
                }
            }
            cursor = parents.get(&key).cloned();
        }
        if let Some(info) = out.get_mut(&child_name) {
            for (k, v) in accumulated_fields {
                info.fields.entry(k).or_insert(v);
            }
            for (k, v) in accumulated_methods {
                info.methods.entry(k).or_insert(v);
            }
            for (k, v) in accumulated_getters {
                info.getters.entry(k).or_insert(v);
            }
            for (k, v) in accumulated_setters {
                info.setters.entry(k).or_insert(v);
            }
        }
    }
    out
}

/// Register `Enum.Variant` hover entries for every variant of `e`.
/// `enum_key` is the dotted name the enum lives under (e.g.
/// `sdl.InitFlag`). Each variant is keyed `enum_key.variant_name` so
/// `Field { obj: Var(enum_key), name: variant }` can be resolved by
/// the walker.
pub(crate) fn register_enum_variants(
    e: &ilang_ast::EnumDecl,
    enum_key: &str,
    out: &mut HashMap<AstSymbol, String>,
    src: Option<&str>,
) {
    let mut auto: i64 = 0;
    for v in e.variants.iter() {
        // Hover blurb for one variant. The displayed value is either
        // the integer discriminant (auto-numbered or explicit) or
        // the literal string for `: string`-repr enums.
        let val_int: Option<i64> = match &v.discriminant {
            Some(ilang_ast::DiscriminantLit::Int(d)) => {
                auto = d + 1;
                Some(*d)
            }
            Some(ilang_ast::DiscriminantLit::Str(_)) => None,
            None => {
                let cur = auto;
                auto += 1;
                Some(cur)
            }
        };
        let key = format!("{enum_key}.{}", v.name);
        // Prefer the literal text the user wrote (`0x40000000` rather
        // than `1073741824`, or `"some string"` rather than the auto
        // value) when source is available and the variant has an
        // explicit discriminant. Fall back to the integer form.
        let val_text: String = match (src, &v.discriminant) {
            (Some(s), Some(_)) => discriminant_literal_text(s, v.span)
                .unwrap_or_else(|| val_int.map(|n| n.to_string()).unwrap_or_default()),
            _ => val_int.map(|n| n.to_string()).unwrap_or_default(),
        };
        let sig = match &v.payload {
            ilang_ast::VariantPayload::Unit => {
                format!("(variant) {enum_key}.{} = {val_text}", v.name)
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

/// Read the literal token for an enum variant's `= value` from
/// source — preserves hex / binary / underscore-separated forms
/// the parser collapses into an `i64`. Returns `None` when no
/// `= literal` is found at the variant's span.
pub(crate) fn discriminant_literal_text(src: &str, v_span: Span) -> Option<String> {
    let off = text::line_col_to_offset(src, v_span.line, v_span.col)?;
    let bytes = src.as_bytes();
    let mut i = off;
    // Skip the variant identifier itself.
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
    {
        i += 1;
    }
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'=' {
        return None;
    }
    i += 1;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    // String discriminant: `= "literal"` for `: string`-repr
    // enums. Capture the entire quoted span (including the
    // surrounding quotes) so hover shows `= "SDL_AUDIO"`
    // verbatim.
    if i < bytes.len() && bytes[i] == b'"' {
        let start = i;
        i += 1;
        while i < bytes.len() && bytes[i] != b'"' {
            if bytes[i] == b'\\' && i + 1 < bytes.len() {
                i += 2;
            } else {
                i += 1;
            }
        }
        if i < bytes.len() && bytes[i] == b'"' {
            i += 1;
            return std::str::from_utf8(&bytes[start..i]).ok().map(|s| s.to_string());
        }
        return None;
    }
    let start = i;
    if i < bytes.len() && bytes[i] == b'-' {
        i += 1;
    }
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
    {
        i += 1;
    }
    if i > start {
        std::str::from_utf8(&bytes[start..i]).ok().map(|s| s.to_string())
    } else {
        None
    }
}

/// Same as `register_enum_variants`, but also records each variant's
/// source location in `sources` (so F12 jumps to the variant line).
pub(crate) fn register_enum_variants_with_sources(
    e: &ilang_ast::EnumDecl,
    enum_key: &str,
    out: &mut HashMap<AstSymbol, String>,
    sources: &mut ExternalSources,
    module_path: &Path,
    src: &str,
) {
    register_enum_variants(e, enum_key, out, Some(src));
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

pub(crate) fn collect_external_signatures(
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
                if !f.is_pub {
                    continue;
                }
                put_dotted(f.name.as_str(), fn_signature(f), &mut out);
                if let Some(t) = &f.ret {
                    if f.name.as_str().contains('.') {
                        rets.insert(f.name.clone(), t.clone());
                    }
                }
            }
            Item::Const(c) => {
                if !c.is_pub {
                    continue;
                }
                let ty = match c
                    .ty
                    .clone()
                    .or_else(|| infer_expr_type_with_scope(&c.value, &[]))
                {
                    Some(t) => format!(": {t}"),
                    None => String::new(),
                };
                let value = render_const_value(&c.value)
                    .map(|v| format!(" = {v}"))
                    .unwrap_or_default();
                put_dotted(c.name.as_str(), format!("const {}{ty}{value}", c.name), &mut out);
            }
            Item::Class(c) => {
                if !c.is_pub {
                    continue;
                }
                put_dotted(
                    c.name.as_str(),
                    format!("{}class {}", render_user_attrs(&c.attrs), c.name),
                    &mut out,
                );
            }
            Item::Enum(e) => {
                if !e.is_pub {
                    continue;
                }
                let repr = e
                    .repr_ty
                    .as_ref()
                    .map(|t| format!(": {t}"))
                    .unwrap_or_default();
                let flags_prefix = if e.flags { "@flags\n" } else { "" };
                put_dotted(
                    e.name.as_str(),
                    format!("{}enum {}{}", flags_prefix, e.name, repr),
                    &mut out,
                );
                if e.name.as_str().contains('.') {
                    // No source available in the merged-Program scan;
                    // variant values render as decimal here.
                    register_enum_variants(e, e.name.as_str(), &mut out, None);
                }
            }
            Item::ExternC(b) => {
                for inner in &b.items {
                    if !is_extern_c_item_pub(inner) {
                        continue;
                    }
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
                                format!("@lib({names})\n")
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
                        ExternCItem::Struct { name, .. } => {
                            put_dotted(name.as_str(), format!("struct {}", name), &mut out);
                        }
                        ExternCItem::Union { name, .. } => {
                            put_dotted(name.as_str(), format!("union {}", name), &mut out);
                        }
                        ExternCItem::Class(c) => {
                            put_dotted(
                                c.name.as_str(),
                                format!("{}class {}", render_user_attrs(&c.attrs), c.name),
                                &mut out,
                            );
                        }
                    }
                }
                // @objc interfaces declared in the same block.
                for iface in b.interfaces.iter() {
                    if !iface.is_pub {
                        continue;
                    }
                    let header = if iface.is_objc { "@objc interface" } else { "interface" };
                    put_dotted(
                        iface.name.as_str(),
                        format!("{header} {}", iface.name),
                        &mut out,
                    );
                }
            }
            _ => {}
        }
    }
    (out, rets)
}


/// Collect every `interface` / `@objc interface` declaration in
/// the loaded program, keyed both by the bare name and by the
/// module-prefixed name (when the loader has already applied a
/// prefix). Drives the "implement missing interface methods"
/// code action: a class body that names
/// `NSApplicationDelegate` (bare, via `use cocoa { … }`) or
/// `cocoa.NSApplicationDelegate` (whole-module reference) finds
/// the same `InterfaceDecl` through this map.
pub(crate) fn collect_external_interfaces(
    prog: &Program,
) -> HashMap<AstSymbol, ilang_ast::InterfaceDecl> {
    let mut out: HashMap<AstSymbol, ilang_ast::InterfaceDecl> = HashMap::new();
    for it in &prog.items {
        match it {
            Item::Interface(i) => {
                out.insert(i.name, i.clone());
                if let Some(bare) = i.name.as_str().rsplit_once('.').map(|(_, t)| t) {
                    out.insert(AstSymbol::intern(bare), i.clone());
                }
            }
            Item::ExternC(b) => {
                for iface in b.interfaces.iter() {
                    out.insert(iface.name, iface.clone());
                    if let Some(bare) = iface
                        .name
                        .as_str()
                        .rsplit_once('.')
                        .map(|(_, t)| t)
                    {
                        out.insert(AstSymbol::intern(bare), iface.clone());
                    }
                }
            }
            _ => {}
        }
    }
    out
}
