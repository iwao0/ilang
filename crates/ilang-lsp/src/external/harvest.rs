//! Entry-point harvest passes — given a buffer's source (parsed or
//! raw token stream), walk every `use M` statement and pull in the
//! imported modules via `walk_module`, collapsing wildcard /
//! selective imports into bare-keyed entries
//! so the buffer-side reference walker can resolve `Var("X")` from
//! `use M { X }` the same way it resolves `Var("M.X")`.

#![allow(unused_imports)]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use ilang_ast::{Item, Program, Symbol as AstSymbol, Type};
use ilang_lexer::tokenize;
use ilang_parser::parse;

use super::walk::walk_module;
use super::{augment_with_sibling_module_roots, ExternalLoc};
use crate::project::{collect_dep_paths, collect_dep_tree, DepTree};
use crate::ExternalSources;

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
    let dep_tree = collect_dep_tree(entry_path).unwrap_or_default();
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
        // directory rather than the importer's own. Walk up the dep
        // tree's `child → parent` map `super_count` times to find
        // the directory `M.il` should be looked up in; anything else
        // (selective list / wildcard) flows through the existing
        // helpers.
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
            // entry as bare `<X>` so completion / hover / F12 treat
            // the wildcard'd names the same way they treat a
            // `use M { X }` selective list. Mirrors the loader's
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
            // (or its `pub use` chain) and key them under their bare
            // name so the buffer-side walker can resolve a bare
            // `Var("X1")` reference.
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

/// Walk the dep tree's `child → parent` map `count` times, starting
/// from the package that contains `start_dir`. Returns the parent
/// package's directory so the harvester can search for `super.M`
/// modules there.
fn super_base_dir(start_dir: &Path, dep_tree: &DepTree, count: u32) -> Option<PathBuf> {
    let canon = start_dir.canonicalize().ok()?;
    // Find the deepest registered package that contains the importer
    // file's directory.
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
