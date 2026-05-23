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

mod collect;
mod enums;
pub(crate) use collect::{
    collect_external_classes, collect_external_interfaces, collect_external_signatures,
};
pub(crate) use enums::{
    discriminant_literal_text, register_builtin_enums, register_enum_variants,
    register_enum_variants_with_sources,
};

/// `true` if `inner` is exposed via `pub` and should appear in
/// another module's `use M.` completion. Used by `walk_module` /
/// `walk_module_aliased` to skip the module's internal helpers
/// (dlsym'd C-runtime hooks like `_autoreleasepool_pop`, the
/// `_make_*_block` thunks etc.) that live in the same `@extern(C)`
/// block as the user-facing wrappers but aren't intended as
/// surface API.
fn is_extern_c_item_pub(inner: &ilang_ast::ExternCItem) -> bool {
    use ilang_ast::ExternCItem;
    // Treat parser-synthesised @objc desugar helpers as not-pub
    // even though the parser marks them with `is_pub: true` (the
    // per-block `<tag>_sel_cache` class etc. need to be reachable
    // by sibling-file dispatch wrappers, but they're not user-
    // facing names).
    let name = match inner {
        ExternCItem::FnDecl { name, .. } => name.as_str(),
        ExternCItem::FnDef(f) => f.name.as_str(),
        ExternCItem::Struct { name, .. } => name.as_str(),
        ExternCItem::Union { name, .. } => name.as_str(),
        ExternCItem::Class(c) => c.name.as_str(),
    };
    if crate::symbols::is_synthesized_objc_helper(name) {
        return false;
    }
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

/// Push the parent of every umbrella-folder ancestor (a directory
/// that holds a `mod.il`) of `entry_dir` onto `extra`. Lets the LSP
/// resolve a `use foundation` from a deep category file like
/// `bindings/cocoa/spritekit/actions.il` against its sibling
/// `bindings/cocoa/foundation/mod.il` even when no `ilang.toml`
/// wires the dep path up — the editor opens binding files directly,
/// without the example project that normally supplies the path.
pub(crate) fn augment_with_sibling_module_roots(entry_dir: &Path, extra: &mut Vec<PathBuf>) {
    let mut dir = entry_dir.to_path_buf();
    while dir.join("mod.il").exists() {
        let Some(parent) = dir.parent() else { break };
        let parent_buf = parent.to_path_buf();
        if !extra.iter().any(|p| p == &parent_buf) {
            extra.push(parent_buf.clone());
        }
        dir = parent_buf;
    }
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
    const_types: &mut HashMap<AstSymbol, Type>,
) {
    let Ok(tokens) = tokenize(entry_src) else { return };
    if let Ok(prog) = parse(&tokens) {
        harvest_from_program(&prog, entry_path, out, sources, docs, const_types);
        return;
    }
    use ilang_lexer::TokenKind;
    let mut extra = collect_dep_paths(entry_path).unwrap_or_default();
    let entry_dir = entry_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    augment_with_sibling_module_roots(&entry_dir, &mut extra);
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut i = 0;
    while i < tokens.len() {
        if matches!(tokens[i].kind, TokenKind::Use) {
            if let Some(t) = tokens.get(i + 1) {
                if let TokenKind::Ident(name) = &t.kind {
                    walk_module(name, &entry_dir, &extra, &mut visited, out, sources, docs, const_types);
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
    const_types: &mut HashMap<AstSymbol, Type>,
) {
    let dep_tree = crate::project::collect_dep_tree(entry_path).unwrap_or_default();
    let mut extra = dep_tree.dirs.clone();
    let entry_dir = entry_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    augment_with_sibling_module_roots(&entry_dir, &mut extra);
    let mut visited: HashSet<PathBuf> = HashSet::new();
    for item in &prog.items {
        let Item::Use(u) = item else { continue };
        // `use super.M`: resolve `M` against an ancestor package
        // directory rather than the importer's own. Walk up the
        // dep tree's `child → parent` map `super_count` times to
        // find the directory `M.il` should be looked up in;
        // anything else (selective list / wildcard) flows through
        // the existing helpers.
        let base_dir: PathBuf = if u.super_count > 0 {
            super_base_dir(&entry_dir, &dep_tree, u.super_count).unwrap_or(entry_dir.clone())
        } else {
            entry_dir.clone()
        };
        // Multi-segment paths (`use a.b.c.*` / `use a.b.c { X }`):
        // collapse the subpath chain into an effective (module, dir)
        // pair so the existing single-segment helpers below stay
        // intact. The deepest segment becomes the effective module
        // name; earlier segments map into nested subdirectories.
        let (effective_module, effective_dir): (String, PathBuf) = if u.subpath.is_empty() {
            (u.module.as_str().to_string(), base_dir.clone())
        } else {
            let mut d = base_dir.join(u.module.as_str());
            let len = u.subpath.len();
            for seg in &u.subpath[..len - 1] {
                d = d.join(seg.as_str());
            }
            (u.subpath[len - 1].as_str().to_string(), d)
        };
        if u.wildcard && u.selective.is_none() {
            // `use M { * }` — walk M, then re-key every `M.<X>`
            // entry as bare `<X>` so completion / hover / F12
            // treat the wildcard'd names the same way they treat
            // a `use M { X }` selective list. Mirrors the loader's
            // rename-rule expansion that turns the bare reference
            // into `M.<X>` at call time.
            harvest_wildcard_names(
                &effective_module,
                &effective_dir,
                &extra,
                &mut visited,
                out,
                sources,
                docs,
                const_types,
            );
            continue;
        }
        if let Some(names) = &u.selective {
            // `use M { X1, X2 }` — pull X1/X2's hover info from M
            // (or its `pub use` chain) and key them under their
            // bare name so the buffer-side walker can resolve a
            // bare `Var("X1")` reference.
            harvest_selective_names(
                &effective_module,
                names,
                &effective_dir,
                &extra,
                out,
                sources,
                docs,
                const_types,
            );
            continue;
        }
        walk_module(
            &effective_module,
            &effective_dir,
            &extra,
            &mut visited,
            out,
            sources,
            docs,
            const_types,
        );
    }
}

/// Walk the dep tree's `child → parent` map `count` times,
/// starting from the package that contains `start_dir`. Returns
/// the parent package's directory so the harvester can search
/// for `super.M` modules there.
fn super_base_dir(
    start_dir: &Path,
    dep_tree: &crate::project::DepTree,
    count: u32,
) -> Option<PathBuf> {
    let canon = start_dir.canonicalize().ok()?;
    // Find the deepest registered package that contains the
    // importer file's directory.
    let mut cur: PathBuf = dep_tree
        .dirs
        .iter()
        .filter(|p| canon.starts_with(p))
        .max_by_key(|p| p.components().count())
        .cloned()?;
    for _ in 0..count {
        cur = dep_tree.parents.get(&cur)?.clone();
    }
    Some(cur)
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
/// Rewrite a fully-qualified declaration signature into its bare-name
/// view. Signatures from `collect_external_signatures` / `walk_module`
/// embed the dotted name (`fn cocoa.makeTextField(...)`,
/// `class cocoa.NSObject`, `const cocoa.NSStringEncoding.utf8: ...`),
/// but selective / wildcard imports surface the item under just its
/// bare name. Without this rewrite, hovering on `makeTextField` shows
/// `fn cocoa.makeTextField(...)` even though the user only wrote
/// `makeTextField` at the call site.
///
/// `replacen(_, _, 1)` is intentional: the qualified name always
/// appears first as the declared item, and the rest of the signature
/// (param types, default values) may reference unrelated dotted
/// names that should stay qualified.
fn strip_module_prefix_in_sig(sig: &str, module: &str, bare: &str) -> String {
    let qualified = format!("{module}.{bare}");
    sig.replacen(&qualified, bare, 1)
}

pub(crate) fn harvest_wildcard_names(
    module: &str,
    entry_dir: &Path,
    extra: &[PathBuf],
    visited: &mut HashSet<PathBuf>,
    out: &mut HashMap<AstSymbol, String>,
    sources: &mut ExternalSources,
    docs: &mut HashMap<AstSymbol, String>,
    const_types: &mut HashMap<AstSymbol, Type>,
) {
    walk_module(module, entry_dir, extra, visited, out, sources, docs, const_types);
    let module_dot = format!("{module}.");
    let bare_entries: Vec<(AstSymbol, String)> = out
        .iter()
        .filter_map(|(k, v)| {
            let tail = k.as_str().strip_prefix(&module_dot)?;
            // Only the top-level item gets its module prefix stripped;
            // nested dotted names (`cocoa.Foo.Bar`) keep the inner
            // `Foo.Bar` part intact in the bare view.
            let bare_head = tail.split('.').next().unwrap_or(tail);
            let rewritten = strip_module_prefix_in_sig(v, module, bare_head);
            Some((AstSymbol::intern(tail), rewritten))
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
    let bare_const_tys: Vec<(AstSymbol, Type)> = const_types
        .iter()
        .filter_map(|(k, v)| {
            k.as_str()
                .strip_prefix(&module_dot)
                .map(|tail| (AstSymbol::intern(tail), v.clone()))
        })
        .collect();
    for (k, v) in bare_const_tys {
        const_types.entry(k).or_insert(v);
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
    const_types: &mut HashMap<AstSymbol, Type>,
) {
    let mut visited: HashSet<PathBuf> = HashSet::new();
    walk_module(module, entry_dir, extra, &mut visited, out, sources, docs, const_types);
    for name in names {
        let prefixed = AstSymbol::intern(&format!("{module}.{name}"));
        if let Some(sig) = out.get(&prefixed).cloned() {
            out.insert(name.clone(), strip_module_prefix_in_sig(&sig, module, name.as_str()));
        }
        if let Some(loc) = sources.get(&prefixed).cloned() {
            sources.insert(name.clone(), loc);
        }
        if let Some(d) = docs.get(&prefixed).cloned() {
            docs.insert(name.clone(), d);
        }
        if let Some(t) = const_types.get(&prefixed).cloned() {
            const_types.insert(name.clone(), t);
        }
        // Selectively-imported enums also expose `<bare>.<variant>`
        // composite keys so `Field { obj: Var(bare), name: variant }`
        // resolves through the same lookup path as `module.Enum.X`.
        let prefix_dot = format!("{module}.{name}.");
        let bare_dot = format!("{name}.");
        let extra_sigs: Vec<(AstSymbol, String)> = out
            .iter()
            .filter_map(|(k, v)| {
                let tail = k.as_str().strip_prefix(&prefix_dot)?;
                let rewritten = strip_module_prefix_in_sig(v, module, name.as_str());
                Some((AstSymbol::intern(&format!("{bare_dot}{tail}")), rewritten))
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
    const_types: &mut HashMap<AstSymbol, Type>,
) {
    let (module_path, module_src) =
        if let Some(s) = ilang_parser::loader::builtin_module_source(prefix) {
            // Prefer the real on-disk `stdlib/<name>.il` so F12 lands
            // in an actual file. Falls back to the synthetic
            // `<builtin>/<name>.il` key in release-only installs where
            // the source tree isn't present (the rest of the LSP — hover,
            // completion — still works off the embedded source string).
            let real = ilang_parser::loader::builtin_module_path(prefix)
                .unwrap_or_else(|| PathBuf::from(format!("<builtin>/{prefix}.il")));
            (real, s.to_string())
        } else {
            // Mirror `loader::resolve_module`: try `<dir>/M.il` first,
            // then fall back to `<dir>/M/mod.il` (Rust-style subfolder
            // umbrella). Without the second arm F12 / hover go blank
            // on every name that lives behind a `pub use mod.*`
            // umbrella, because the harvest never finds the parsed
            // declarations.
            let mut candidates = vec![entry_dir.to_path_buf()];
            candidates.extend(extra.iter().cloned());
            let Some((p, s)) = candidates.into_iter().find_map(|d| {
                let direct = d.join(format!("{prefix}.il"));
                if let Ok(src) = std::fs::read_to_string(&direct) {
                    return Some((direct, src));
                }
                let nested = d.join(prefix).join("mod.il");
                std::fs::read_to_string(&nested).ok().map(|src| (nested, src))
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
    // Top-of-file `///` block — the module-level doc. Surfaces on
    // hover over `use foundation` etc. The signature line is a
    // simple `(module) {prefix}` placeholder so the hover renders
    // something even when the file has no top doc.
    out.entry(AstSymbol::intern(prefix))
        .or_insert_with(|| format!("(module) {prefix}"));
    if let Some(d) = text::extract_module_doc(&module_src) {
        docs.entry(AstSymbol::intern(prefix)).or_insert(d);
    }
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
                let resolved_ty = c
                    .ty
                    .clone()
                    .or_else(|| infer_expr_type_with_scope(&c.value, &[]));
                let ty = match &resolved_ty {
                    Some(t) => format!(": {t}"),
                    None => String::new(),
                };
                let value = render_const_value_with_src(&c.value, Some(&module_src))
                    .map(|v| format!(" = {v}"))
                    .unwrap_or_default();
                let key = format!("{prefix}.{}", c.name);
                out.insert(AstSymbol::intern(&key), format!("const {key}{ty}{value}"));
                if let Some(t) = resolved_ty {
                    const_types.insert(AstSymbol::intern(&key), t);
                }
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
                let bases = render_class_bases(c.parent.as_ref(), &c.interfaces);
                let attrs = render_user_attrs(&c.attrs);
                out.insert(AstSymbol::intern(&key), format!("{attrs}class {key}{bases}"));
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
                        ilang_ast::ExternCItem::Struct {
                            name, span, is_packed, is_handle, ..
                        } => {
                            let attrs = render_struct_attrs(*is_packed, *is_handle);
                            (
                                *name,
                                *span,
                                format!("{attrs}struct {prefix}.{name}"),
                            )
                        }
                        ilang_ast::ExternCItem::Union { name, span, .. } => (
                            *name,
                            *span,
                            format!("union {prefix}.{name}"),
                        ),
                        ilang_ast::ExternCItem::Class(c) => {
                            let bases = render_class_bases(c.parent.as_ref(), &c.interfaces);
                            let attrs = render_user_attrs(&c.attrs);
                            (
                                c.name.into(),
                                c.span,
                                format!("{attrs}class {prefix}.{}{bases}", c.name),
                            )
                        }
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
                            let opt = if m.is_optional { "?" } else { "" };
                            let ps: Vec<String> = m
                                .params
                                .iter()
                                .map(|p| format!("{}: {}", p.name, p.ty))
                                .collect();
                            let r = match &m.ret {
                                Some(t) => format!(": {t}"),
                                None => String::new(),
                            };
                            format!("    {}{}({}){}", m.name, opt, ps.join(", "), r)
                        })
                        .collect();
                    let header = if iface.is_objc { "@objc interface" } else { "interface" };
                    let parent = iface
                        .parent
                        .as_ref()
                        .map(|p| format!(" : {p}"))
                        .unwrap_or_default();
                    let sig = if methods.is_empty() {
                        format!("{header} {prefix}.{}{parent} {{}}", iface.name)
                    } else {
                        format!(
                            "{header} {prefix}.{}{parent} {{\n{}\n}}",
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
                // `pub const` declarations hoisted into the block
                // (e.g. `windows.NULL = 0 as *void` in `winnull.il`).
                // They aren't part of `b.items` — the AST keeps them
                // on `b.consts` so the loader can lift them out as
                // top-level consts with raw-pointer types still
                // legal. Mirror the same harvest as a top-level
                // `Item::Const` so hover finds them.
                for c in b.consts.iter() {
                    if !c.is_pub {
                        continue;
                    }
                    let resolved_ty = c
                        .ty
                        .clone()
                        .or_else(|| infer_expr_type_with_scope(&c.value, &[]));
                    let ty = match &resolved_ty {
                        Some(t) => format!(": {t}"),
                        None => String::new(),
                    };
                    let value = render_const_value_with_src(&c.value, Some(&module_src))
                        .map(|v| format!(" = {v}"))
                        .unwrap_or_default();
                    let key = format!("{prefix}.{}", c.name);
                    out.insert(AstSymbol::intern(&key), format!("const {key}{ty}{value}"));
                    if let Some(t) = resolved_ty {
                        const_types.insert(AstSymbol::intern(&key), t);
                    }
                    track(&key, c.span, c.name.as_str().len() as u32, sources, &module_path);
                    if let Some(d) = text::extract_doc_above(&module_src, c.span.line) {
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
                    const_types,
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
                    const_types,
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
    const_types: &mut HashMap<AstSymbol, Type>,
) {
    let (module_path, module_src) =
        if let Some(s) = ilang_parser::loader::builtin_module_source(actual) {
            (
                PathBuf::from(format!("<builtin>/{actual}.il")),
                s.to_string(),
            )
        } else {
            // Same `<dir>/M.il` → `<dir>/M/mod.il` fallback as
            // `walk_module` — alias chasing must follow the loader's
            // subfolder resolution rule, otherwise F12 on a name
            // re-exported through `pub use core.*` lands nowhere.
            let mut candidates = vec![entry_dir.to_path_buf()];
            candidates.extend(extra.iter().cloned());
            let Some((p, s)) = candidates.into_iter().find_map(|d| {
                let direct = d.join(format!("{actual}.il"));
                if let Ok(src) = std::fs::read_to_string(&direct) {
                    return Some((direct, src));
                }
                let nested = d.join(actual).join("mod.il");
                std::fs::read_to_string(&nested).ok().map(|src| (nested, src))
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
                let resolved_ty = c
                    .ty
                    .clone()
                    .or_else(|| infer_expr_type_with_scope(&c.value, &[]));
                let ty = match &resolved_ty {
                    Some(t) => format!(": {t}"),
                    None => String::new(),
                };
                let value = render_const_value_with_src(&c.value, Some(&module_src))
                    .map(|v| format!(" = {v}"))
                    .unwrap_or_default();
                out.insert(AstSymbol::intern(&key), format!("const {key}{ty}{value}"));
                if let Some(t) = resolved_ty {
                    const_types.insert(AstSymbol::intern(&key), t);
                }
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
                let bases = render_class_bases(c.parent.as_ref(), &c.interfaces);
                let attrs = render_user_attrs(&c.attrs);
                out.insert(AstSymbol::intern(&key), format!("{attrs}class {key}{bases}"));
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
                        ilang_ast::ExternCItem::Struct {
                            name, span, is_packed, is_handle, ..
                        } => {
                            let attrs = render_struct_attrs(*is_packed, *is_handle);
                            Some((
                                (*name).into(),
                                *span,
                                format!("{attrs}struct {alias_prefix}.{name}"),
                            ))
                        }
                        ilang_ast::ExternCItem::Union { name, span, .. } => Some((
                            (*name).into(),
                            *span,
                            format!("union {alias_prefix}.{name}"),
                        )),
                        ilang_ast::ExternCItem::Class(c) => {
                            let bases = render_class_bases(c.parent.as_ref(), &c.interfaces);
                            let attrs = render_user_attrs(&c.attrs);
                            Some((
                                c.name.into(),
                                c.span,
                                format!("{attrs}class {alias_prefix}.{}{bases}", c.name),
                            ))
                        }
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
                            let opt = if m.is_optional { "?" } else { "" };
                            let ps: Vec<String> = m
                                .params
                                .iter()
                                .map(|p| format!("{}: {}", p.name, p.ty))
                                .collect();
                            let r = match &m.ret {
                                Some(t) => format!(": {t}"),
                                None => String::new(),
                            };
                            format!("    {}{}({}){}", m.name, opt, ps.join(", "), r)
                        })
                        .collect();
                    let header = if iface.is_objc { "@objc interface" } else { "interface" };
                    let parent = iface
                        .parent
                        .as_ref()
                        .map(|p| format!(" : {p}"))
                        .unwrap_or_default();
                    let sig = if methods.is_empty() {
                        format!("{header} {alias_prefix}.{}{parent} {{}}", iface.name)
                    } else {
                        format!(
                            "{header} {alias_prefix}.{}{parent} {{\n{}\n}}",
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
                // `pub const` declarations on `b.consts` (e.g.
                // `windows.NULL`). See `walk_module`'s matching arm
                // — keep the alias-prefix in the key so the
                // umbrella's `windows.NULL` hover still works.
                for c in b.consts.iter() {
                    if !c.is_pub {
                        continue;
                    }
                    let resolved_ty = c
                        .ty
                        .clone()
                        .or_else(|| infer_expr_type_with_scope(&c.value, &[]));
                    let ty = match &resolved_ty {
                        Some(t) => format!(": {t}"),
                        None => String::new(),
                    };
                    let value = render_const_value_with_src(&c.value, Some(&module_src))
                        .map(|v| format!(" = {v}"))
                        .unwrap_or_default();
                    let key = format!("{alias_prefix}.{}", c.name);
                    out.insert(AstSymbol::intern(&key), format!("const {key}{ty}{value}"));
                    if let Some(t) = resolved_ty {
                        const_types.insert(AstSymbol::intern(&key), t);
                    }
                    put(&key, c.span, c.name.as_str().len() as u32, sources);
                    if let Some(d) = text::extract_doc_above(&module_src, c.span.line) {
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
                    const_types,
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
#[cfg(test)]
mod tests {
    use super::*;

    /// Opening a deep category file in a folder-binding
    /// (`bindings/cocoa/spritekit/actions.il`) should still resolve
    /// `use foundation { NSObject }` against its sibling
    /// `bindings/cocoa/foundation/` even with no `ilang.toml` in any
    /// ancestor — `augment_with_sibling_module_roots` adds the
    /// umbrella-folder parents as search roots so the harvest finds
    /// the sibling's exports.
    #[test]
    fn harvest_walks_up_to_sibling_module() {
        // Resolve repo-relative path from this crate's manifest dir.
        let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let actions_il = manifest
            .join("../../bindings/cocoa/spritekit/actions.il");
        if !actions_il.exists() {
            // Test only meaningful inside the ilang repo layout.
            return;
        }
        let src = std::fs::read_to_string(&actions_il).unwrap();
        let mut out: HashMap<AstSymbol, String> = HashMap::new();
        let mut sources: ExternalSources = HashMap::new();
        let mut docs: HashMap<AstSymbol, String> = HashMap::new();
        let mut const_types: HashMap<AstSymbol, Type> = HashMap::new();
        harvest_imported_consts(
            &actions_il, &src, &mut out, &mut sources, &mut docs, &mut const_types,
        );
        // `NSObject` is imported via `use foundation { NSObject, ... }`
        // — without the sibling-root augmentation the harvest can't
        // find foundation/mod.il and the symbol stays unresolved.
        assert!(
            sources.contains_key(&AstSymbol::intern("NSObject")),
            "NSObject must resolve through sibling foundation/ module"
        );
    }
}
