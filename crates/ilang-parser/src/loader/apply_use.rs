//! `use M`, `use M { X, Y }`, `pub use M.*` resolver. Walks each
//! `Item::Use` in dependency order, pulls the target module's items
//! into the merged Program under the right prefix, and records
//! per-name rename rules so bare references in selective imports
//! line up with the prefixed declaration after the entry's items
//! are merged.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use ilang_ast::{Item, Program, Stmt, StmtKind, Symbol, UseDecl};

use super::LoadError;
use super::prefix::{prefix_item, prefix_stmt};
use super::qualify::{qualify_var_refs_in_item, qualify_var_refs_in_stmt};
use super::rename::{rename_in_item, rename_in_stmt};
use super::resolve::resolve_module;

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_use(
    u: UseDecl,
    // When `Some(p)`, items from `u`'s module merge under prefix `p`
    // instead of `u.module`. Used by `pub use M` so M's items
    // appear under the re-exporting module's namespace. `None` at
    // the entry-point and on regular nested uses.
    prefix_override: Option<&str>,
    importer_canon: &Path,
    extra_paths: &[PathBuf],
    parents: &HashMap<PathBuf, PathBuf>,
    dep_names_to_dirs: &HashMap<String, PathBuf>,
    loaded: &mut HashMap<PathBuf, Program>,
    merged: &mut Program,
    _whole_imports: &mut HashSet<Symbol>,
    applied: &mut HashSet<(PathBuf, String)>,
    // Per-name rewrite rules accumulated by selective imports that
    // resolve through `pub use` chains. Bare-name `X` refs in
    // the entry's items / stmts / tail get rewritten to the prefixed
    // form `umbrella.X` after all imports are merged, so the bare
    // and prefixed views of the same enum / class / fn line up at
    // the type checker.
    rename_rules: &mut HashMap<Symbol, Symbol>,
    // Per-source-file map of "sibling-class → its source module"
    // built during `load_recursive`'s folder-binding prescan. Used
    // here right before `prefix_item` to qualify bare @objc class
    // refs the auto-lift left behind for sibling category files
    // (e.g. `new SKPhysicsBody(...)` in spritekit/node.il → `new
    // physics.SKPhysicsBody(...)` because node.il can't `use physics`
    // — that would be a circular import).
    sibling_class_maps: &HashMap<PathBuf, HashMap<Symbol, Symbol>>,
) -> Result<(), LoadError> {
    let importer_dir = importer_canon
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let subpath_strs: Vec<String> =
        u.subpath.iter().map(|s| s.as_str().to_string()).collect();
    let canon = resolve_module(
        u.module.as_str(),
        &subpath_strs,
        &importer_dir,
        extra_paths,
        u.super_count,
        parents,
        dep_names_to_dirs,
    )?;
    // Clone instead of remove — the same module may legitimately be
    // applied multiple times (e.g. once via pub use to publish under
    // an umbrella prefix, and once directly so a sibling module that
    // `use`s it sees the items under the original prefix). Each
    // application targets a distinct effective prefix, so the
    // resulting items don't shadow each other.
    let mut module_prog = loaded
        .get(&canon)
        .cloned()
        .expect("loaded before via load_recursive");
    // For path-style imports (`use a.b.c` → module="a",
    // subpath=["b", "c"]), the merged module's items live under the
    // full dotted path (`a.b.c`) so callers reach them exactly as
    // they were written in the `use` declaration. Single-segment
    // `use M` keeps the bare-`M` prefix.
    let nominal_prefix: String = prefix_override
        .map(str::to_string)
        .unwrap_or_else(|| {
            if u.subpath.is_empty() {
                u.module.as_str().to_string()
            } else {
                let mut s = u.module.as_str().to_string();
                for seg in u.subpath.iter() {
                    s.push('.');
                    s.push_str(seg.as_str());
                }
                s
            }
        });
    // If this module's canon is already prefix-merged under some
    // other prefix (e.g. an umbrella's `pub use` ran first and
    // exposed the items under `sdl.X`), reuse that prefix for our
    // rename rules instead of producing a parallel `M.X` copy. The
    // umbrella's view and the explicit `use M { X }` view should
    // refer to the same merged item, otherwise the type checker
    // sees two distinct types with identical content.
    let existing_prefix: Option<String> = applied
        .iter()
        .find_map(|(p, pref)| (p == &canon && !pref.starts_with("@sel:")).then(|| pref.clone()));
    let effective_prefix: String =
        existing_prefix.clone().unwrap_or_else(|| nominal_prefix.clone());
    // If this importer addresses the module under a different
    // prefix than where it was actually merged (typical of
    // `use super.M` reaching a module that the parent umbrella
    // already re-exported under its own prefix), mint per-item
    // rename rules so qualified references like `event.X` in
    // the importer's body route to the registered `gui.X` form.
    if existing_prefix.as_deref() != Some(nominal_prefix.as_str()) {
        if let Some(existing_pref) = existing_prefix.as_deref() {
            if let Some(module_prog_ref) = loaded.get(&canon) {
                for item in &module_prog_ref.items {
                    if let Some(name) = item_name_of(item) {
                        let from = Symbol::intern(&format!("{nominal_prefix}.{name}"));
                        let to   = Symbol::intern(&format!("{existing_pref}.{name}"));
                        rename_rules.insert(from, to);
                    }
                }
            }
        }
    }
    // Selective and whole imports both produce the same prefix-merged
    // view of the module — bare references in selective imports get
    // rewritten to the prefixed form by the rename pass at the end of
    // `load_program`, so the only thing that varies is whether we
    // also expose any names bare. Dedup the prefix-merge step on
    // (canon, prefix) so `use M` followed by `use M { X }` (or vice
    // versa) doesn't double-register every item; the per-selective
    // record below is gated by its own dedup key.
    let merge_key = (canon.clone(), effective_prefix.clone());
    let needs_merge = applied.insert(merge_key);
    // The selective branch (line ~517) writes rename rules into the
    // *caller's* `rename_rules` map, which is per-importer. Each
    // importer that does `use M { X }` needs that mapping recorded
    // into its own map, so this branch must run regardless of whether
    // some other importer already did the same selective import. If
    // there's nothing selective and no merge to do, only then can we
    // skip.
    if !needs_merge && u.selective.is_none() {
        return Ok(());
    }

    // Recursively expand the module's own use items first, into the
    // module_prog's namespace. `pub use N` propagates the
    // current module's effective prefix to N so its items also land
    // under the re-exporting namespace.
    let mut nested_uses = Vec::new();
    let mut local_items = Vec::new();
    for item in module_prog.items {
        match item {
            Item::Use(nu) => nested_uses.push(nu),
            other => local_items.push(other),
        }
    }
    module_prog.items = local_items.into();
    // Keep a copy for the selective branch's `pub use` chain
    // existence check — selective imports may resolve names declared
    // in chained modules rather than this module's own items.
    let nested_uses_for_search: Vec<UseDecl> = nested_uses.clone();

    if needs_merge {
        // Rename rules collected from THIS module's own selective
        // imports — applied to this module's items before
        // `prefix_item` so a `use N { Y }` inside M rewrites the
        // bare `Y` references in M's body to `N.Y`.
        let mut module_rename_rules: HashMap<Symbol, Symbol> = HashMap::new();
        // Process `pub use M.*` re-exports BEFORE other uses so
        // the umbrella's publication of a module's items wins the
        // prefix race against any sub-package's
        // `use super.M { ... }` inside the same umbrella. Without
        // this, a non-re-export selective import that runs first
        // claims the dedup slot under module-stem prefix and the
        // umbrella's `pub use M.*` silently no-ops, leaving the
        // umbrella-prefixed view (`gui.M.X`) unregistered.
        nested_uses.sort_by_key(|u| if u.re_export { 0 } else { 1 });
        // Toposort the `pub use M.*` wildcard re-exports so that
        // when one re-exported module X non-publicly uses another
        // re-exported module Y (e.g. timers.il's
        // `use concurrency { NSQualityOfService }`), the umbrella
        // publishes Y FIRST. Otherwise X's nested non-pub use
        // claims Y's canon under Y's bare prefix, and the
        // subsequent `pub use Y.*` short-circuits — leaving Y's
        // other exports unreachable through the umbrella. Stable
        // tiebreaker preserves the original (already
        // re_export-first) order.
        toposort_pub_use_reexports(
            &mut nested_uses,
            &canon,
            extra_paths,
            parents,
            dep_names_to_dirs,
            loaded,
        );
        for nu in nested_uses {
            // `pub use M as _ { * }` (wildcard): flatten M's items
            // into the umbrella's namespace — override = umbrella prefix.
            // `pub use M` (no wildcard): namespace under the umbrella —
            // override = `<umbrella>.<M>` so items land at
            // `<umbrella>.M.X` and callers reach them via that path.
            let nested_override_owned: Option<String> = if nu.re_export {
                if nu.wildcard {
                    Some(effective_prefix.clone())
                } else {
                    Some(format!("{}.{}", effective_prefix, nu.module.as_str()))
                }
            } else {
                None
            };
            let nested_override: Option<&str> = nested_override_owned.as_deref();
            apply_use(
                nu,
                nested_override,
                &canon,
                extra_paths,
                parents,
                dep_names_to_dirs,
                loaded,
                merged,
                _whole_imports,
                applied,
                &mut module_rename_rules,
                sibling_class_maps,
            )?;
        }
        // Prefix-merge the module's own local items. Even for
        // selective imports we want the module's items present in
        // the merged Program (under their prefixed names) so a
        // selectively-imported class's internal references to other
        // module items resolve.
        let mut named_globals: HashSet<Symbol> = module_prog
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Const(c) => Some(c.name.clone()),
                Item::Class(c) => Some(c.name.clone()),
                // Top-level fns count too — `qualify_var_refs`
                // qualifies bare `Call(name, ...)` callees only
                // when the name is in this set, so the later
                // `prefix_*` walk doesn't accidentally qualify
                // local-closure callees (`let f = ...; f(v)` →
                // not `module.f(v)`).
                Item::Fn(f) => Some(f.name.clone()),
                _ => None,
            })
            .collect();
        for item in &module_prog.items {
            if let Item::ExternC(b) = item {
                for inner in &b.items {
                    match inner {
                        ilang_ast::ExternCItem::Class(c) => {
                            named_globals.insert(c.name.clone());
                        }
                        ilang_ast::ExternCItem::FnDef(f) => {
                            named_globals.insert(f.name.clone());
                        }
                        ilang_ast::ExternCItem::FnDecl { name, .. } => {
                            named_globals.insert(name.clone());
                        }
                        _ => {}
                    }
                }
            }
        }
        // Top-level `let X = ...` in this module — fn bodies (and
        // other top-level stmts) within the module reference X
        // bare; the qualify pass below rewrites those refs to
        // `prefix.X` so they line up with the prefixed `let`
        // binding that the stmt pass below emits.
        for s in &module_prog.stmts {
            if let StmtKind::Let { name, .. } = &s.kind {
                named_globals.insert(name.clone());
            }
        }
        // Fold the module's trailing expression into its stmt list
        // so it executes during import (e.g. a final `counter = 42`
        // tail expression). The entry's tail stays separate; only
        // sub-modules' tails get demoted.
        if let Some(tail) = module_prog.tail.take() {
            let span = tail.span;
            module_prog.stmts.push(Stmt {
                kind: StmtKind::Expr(tail),
                span, source_module: None });
        }
        for item in module_prog.items.iter_mut() {
            qualify_var_refs_in_item(item, &effective_prefix, &named_globals);
        }
        // Apply this module's own selective-import rename rules
        // BEFORE prefixing — `prefix_item` adds the module prefix to
        // every bare `Object`/`Var`/`Call`, which would turn
        // `NeonRenderer` (after `use neon { NeonRenderer }`) into
        // `M.NeonRenderer` instead of the intended `neon.NeonRenderer`.
        if !module_rename_rules.is_empty() {
            for item in module_prog.items.iter_mut() {
                rename_in_item(item, &module_rename_rules);
            }
        }
        // Sibling-class qualification: any bare @objc class symbol
        // (`Class`, not `module.Class`) that survived the rename pass
        // AND has a known sibling source module gets rewritten to
        // `<sibling_module>.Class`. Lets the auto-lift's synthetic
        // refs from a category file (`new SKPhysicsBody(...)` in
        // node.il when SKPhysicsBody lives in sibling physics.il)
        // reach the correct merged-item entry rather than being
        // re-tagged with the local prefix at `prefix_item` time.
        if let Some(sibling_map) = sibling_class_maps.get(&canon) {
            let qualify_rules: HashMap<Symbol, Symbol> = sibling_map
                .iter()
                .map(|(cls, module)| {
                    let qualified = Symbol::intern(&format!(
                        "{}.{}", module.as_str(), cls.as_str()
                    ));
                    (*cls, qualified)
                })
                .collect();
            for item in module_prog.items.iter_mut() {
                rename_in_item(item, &qualify_rules);
            }
        }
        for item in module_prog.items {
            merged.items.push(prefix_item(item, &effective_prefix));
        }
        // Forward this module's top-level stmts (Let bindings + side
        // effects) into the merged program so they execute when the
        // entry runs. `applied` guarantees a given (canon, prefix)
        // only goes through this branch once, so each module's
        // initialization runs exactly once even if multiple `use`
        // sites reach it.
        for stmt in module_prog.stmts {
            let mut s = stmt;
            qualify_var_refs_in_stmt(&mut s, &effective_prefix, &named_globals);
            if !module_rename_rules.is_empty() {
                rename_in_stmt(&mut s, &module_rename_rules);
            }
            let mut s = prefix_stmt(s, &effective_prefix);
            // Top-level `let X = ...` becomes `let prefix.X = ...`
            // so cross-module references (Var("prefix.X")) resolve
            // to the same global slot.
            if let StmtKind::Let { name, .. } = &mut s.kind {
                *name = Symbol::intern(&format!("{effective_prefix}.{name}")).into();
            }
            // Tag the merged stmt with its source module so the
            // type checker judges access from that module's
            // perspective. Without this, the module's own
            // top-level stmts (e.g. `let X = Class.c` referring
            // to a non-pub static of the SAME module) get
            // judged from the entry module and falsely fail the
            // cross-module visibility rule.
            s.source_module = Some(Symbol::intern(&effective_prefix));
            merged.stmts.push(s);
        }
    }

    // Wildcard selective import (`use M { * }`): pull every
    // pub-exported name into the caller's bare namespace. Same
    // mechanism as the explicit `use M { X, Y }` form, but the
    // name list comes from `collect_export_names` (which walks
    // `pub use` chains for umbrella modules like `cocoa.il`).
    // Re-exports (`pub use M { * }`) are handled separately by
    // the nested-uses loop above and don't reach here.
    if u.wildcard && !u.re_export {
        let module_dir = importer_canon
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let mut visited: HashSet<PathBuf> = HashSet::new();
        let mut names: HashSet<Symbol> = HashSet::new();
        let subpath_strs: Vec<String> =
            u.subpath.iter().map(|s| s.as_str().to_string()).collect();
        collect_export_names(
            u.module.as_str(),
            u.super_count,
            &module_dir,
            extra_paths,
            parents,
            dep_names_to_dirs,
            loaded,
            &mut visited,
            &mut names,
            &subpath_strs,
        )?;
        for name in names {
            rename_rules.insert(
                name,
                Symbol::intern(&format!("{effective_prefix}.{name}")).into(),
            );
        }
        return Ok(());
    }
    // Selective imports record one rename rule per requested name so
    // the final pass rewrites bare references in the entry's content
    // to the prefixed form `effective_prefix.name`. We rely on the
    // prefix-merge above (or a sibling whole-import that ran first)
    // to make `effective_prefix.name` actually present in `merged`.
    if let Some(names) = u.selective {
        // Whether the requested names are visible in this module's
        // local items or any of its `pub use` chains. We need an
        // existence check to surface a load error for typos —
        // skipping the check would silently accept any bare name.
        let mut local_names: HashSet<&str> = HashSet::new();
        if let Some(p) = loaded.get(&canon) {
            for item in p.items.iter() {
                if let Some(n) = item_name_of_ref(item) {
                    local_names.insert(n);
                }
                // `@extern(C) { struct S {} fn f() {} ... }` items
                // count as exports too — selective import should be
                // able to pull `S` or `f` out of `a.il`'s extern
                // block.
                if let Item::ExternC(b) = item {
                    for iface in b.interfaces.iter() {
                        local_names.insert(iface.name.as_str());
                    }
                    for c in b.consts.iter() {
                        local_names.insert(c.name.as_str());
                    }
                    for inner in b.items.iter() {
                        match inner {
                            ilang_ast::ExternCItem::Struct { name, .. }
                            | ilang_ast::ExternCItem::Union { name, .. }
                            | ilang_ast::ExternCItem::FnDecl { name, .. } => {
                                local_names.insert(name.as_str());
                            }
                            ilang_ast::ExternCItem::FnDef(f) => {
                                local_names.insert(f.name.as_str());
                            }
                            ilang_ast::ExternCItem::Class(c) => {
                                local_names.insert(c.name.as_str());
                            }
                        }
                    }
                }
            }
            // Top-level `pub let X = ...` lives in `p.stmts`, not in
            // `p.items`. The loader still rewrites these into
            // `let module.X = ...` during prefixing, so they're
            // valid selective-import targets — list them here.
            for s in p.stmts.iter() {
                if let StmtKind::Let { is_pub: true, name, .. } = &s.kind {
                    local_names.insert(name.as_str());
                }
            }
        }
        let module_dir = canon
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        for name in &names {
            let exists = local_names.contains(name.as_str()) || {
                let mut visited: HashSet<PathBuf> = HashSet::new();
                visited.insert(canon.clone());
                let mut hit = false;
                for nu in &nested_uses_for_search {
                    if !nu.re_export {
                        continue;
                    }
                    let subpath_strs: Vec<String> = nu
                        .subpath
                        .iter()
                        .map(|s| s.as_str().to_string())
                        .collect();
                    if find_in_export_chain(
                        nu.module.as_str(),
                        nu.super_count,
                        name.as_str(),
                        &module_dir,
                        extra_paths,
                        parents,
                        dep_names_to_dirs,
                        loaded,
                        &mut visited,
                        &subpath_strs,
                    )? {
                        hit = true;
                        break;
                    }
                }
                hit
            };
            if !exists {
                return Err(LoadError::UnknownImport {
                    module: u.module.clone(),
                    name: name.clone(),
                });
            }
            rename_rules.insert(
                name.clone(),
                Symbol::intern(&format!("{effective_prefix}.{name}")).into(),
            );
        }
    }
    Ok(())
}

fn item_name_of_ref(item: &Item) -> Option<&str> {
    match item {
        Item::Fn(f) => Some(f.name.as_str()),
        Item::Class(c) => Some(c.name.as_str()),
        Item::Enum(e) => Some(e.name.as_str()),
        Item::Const(c) => Some(c.name.as_str()),
        Item::ExternC(_) | Item::Use(_) => None,
        Item::Interface(i) => Some(i.name.as_str()),
    }
}

fn item_name_of(item: &Item) -> Option<Symbol> {
    match item {
        Item::Fn(f) => Some(f.name.clone()),
        Item::Class(c) => Some(c.name.clone()),
        Item::Enum(e) => Some(e.name.clone()),
        Item::Const(c) => Some(c.name.clone()),
        Item::ExternC(_) => None,
        Item::Use(_) => None,
        Item::Interface(i) => Some(i.name.clone()),
    }
}

/// Reorder a module's `nested_uses` so that every `pub use M.*`
/// wildcard re-export appears before any other `pub use X.*`
/// whose target module `X` non-publicly `use M { … }`s. Without
/// this, X's processing claims M's canon under M's bare prefix
/// (via `apply_use`'s `existing_prefix` shortcut) and the
/// umbrella's later `pub use M.*` short-circuits, leaving M's
/// other exports unreachable through the umbrella.
///
/// Stable: preserves original relative order between independent
/// entries. Bails on cycles (cycle-participants stay in their
/// original positions — the underlying prefix-claim bug then
/// surfaces and is resolvable by ordering them by hand in the
/// umbrella, which is the same workaround in place pre-fix).
fn toposort_pub_use_reexports(
    nested_uses: &mut Vec<UseDecl>,
    importer_canon: &Path,
    extra_paths: &[PathBuf],
    parents: &HashMap<PathBuf, PathBuf>,
    dep_names_to_dirs: &HashMap<String, PathBuf>,
    loaded: &HashMap<PathBuf, Program>,
) {
    let importer_dir = importer_canon
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    // Resolve each `pub use M.*` wildcard re-export to its canon.
    // Map canon back to the (first) index in nested_uses so we
    // can build edges keyed by index.
    let mut canon_of: HashMap<usize, PathBuf> = HashMap::new();
    let mut index_of_canon: HashMap<PathBuf, usize> = HashMap::new();
    for (i, u) in nested_uses.iter().enumerate() {
        if !(u.re_export && u.wildcard) {
            continue;
        }
        let subpath: Vec<String> =
            u.subpath.iter().map(|s| s.as_str().to_string()).collect();
        let Ok(canon) = resolve_module(
            u.module.as_str(),
            &subpath,
            &importer_dir,
            extra_paths,
            u.super_count,
            parents,
            dep_names_to_dirs,
        ) else {
            continue;
        };
        canon_of.insert(i, canon.clone());
        index_of_canon.entry(canon).or_insert(i);
    }
    if canon_of.len() < 2 {
        return;
    }
    // For each pub-use-wildcard entry, find which OTHER entries it
    // depends on by scanning its target module's non-pub `use M { … }`
    // items.
    let mut depends_on: HashMap<usize, HashSet<usize>> = HashMap::new();
    for (&i, canon_i) in &canon_of {
        let Some(prog_i) = loaded.get(canon_i) else { continue };
        let module_dir_i = canon_i
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        for it in &prog_i.items {
            let Item::Use(nu) = it else { continue };
            if nu.re_export {
                continue;
            }
            let nu_subpath: Vec<String> = nu
                .subpath
                .iter()
                .map(|s| s.as_str().to_string())
                .collect();
            let Ok(nu_canon) = resolve_module(
                nu.module.as_str(),
                &nu_subpath,
                &module_dir_i,
                extra_paths,
                nu.super_count,
                parents,
                dep_names_to_dirs,
            ) else {
                continue;
            };
            if let Some(&j) = index_of_canon.get(&nu_canon) {
                if j != i {
                    depends_on.entry(i).or_default().insert(j);
                }
            }
        }
    }
    if depends_on.is_empty() {
        return;
    }
    // Stable toposort with original-index tiebreaker. Repeatedly
    // pull the LOWEST-indexed entry whose dependencies have all
    // been placed.
    let n = nested_uses.len();
    let mut placed: Vec<bool> = vec![false; n];
    let mut order: Vec<usize> = Vec::with_capacity(n);
    loop {
        let mut next: Option<usize> = None;
        for i in 0..n {
            if placed[i] {
                continue;
            }
            let ready = depends_on
                .get(&i)
                .map(|s| s.iter().all(|j| placed[*j]))
                .unwrap_or(true);
            if ready {
                next = Some(i);
                break;
            }
        }
        match next {
            Some(i) => {
                placed[i] = true;
                order.push(i);
            }
            None => break,
        }
    }
    if order.len() != n {
        // Cycle — bail and leave the existing order untouched.
        return;
    }
    // Apply the new order.
    let mut taken: Vec<Option<UseDecl>> = nested_uses.drain(..).map(Some).collect();
    for i in order {
        nested_uses.push(taken[i].take().expect("each index placed exactly once"));
    }
}

/// Walk a module's `pub` items + `pub use` re-export chains and
/// return every export name reachable from a call site. Used by
/// the `use M { * }` wildcard selective import to mint a rename
/// rule for each export (so `NSWindow` referenced in the entry
/// rewrites to `cocoa.NSWindow` after the umbrella's `pub use`
/// already merged that item into `merged`). Only `pub` items
/// count — module-private names stay invisible.
#[allow(clippy::too_many_arguments)]
fn collect_export_names(
    module: &str,
    super_count: u32,
    importer_dir: &Path,
    extra_paths: &[PathBuf],
    parents: &HashMap<PathBuf, PathBuf>,
    dep_names_to_dirs: &HashMap<String, PathBuf>,
    loaded: &HashMap<PathBuf, Program>,
    visited: &mut HashSet<PathBuf>,
    out: &mut HashSet<Symbol>,
    subpath: &[String],
) -> Result<(), LoadError> {
    let canon = resolve_module(
        module, subpath, importer_dir, extra_paths, super_count, parents, dep_names_to_dirs,
    )?;
    if !visited.insert(canon.clone()) {
        return Ok(());
    }
    let prog = loaded
        .get(&canon)
        .expect("module pre-loaded by load_recursive");
    for item in &prog.items {
        match item {
            Item::Fn(f) if f.is_pub => {
                out.insert(f.name.clone());
            }
            Item::Class(c) if c.is_pub => {
                out.insert(c.name.clone());
            }
            Item::Enum(e) if e.is_pub => {
                out.insert(e.name.clone());
            }
            Item::Const(c) if c.is_pub => {
                out.insert(c.name.clone());
            }
            Item::Interface(i) if i.is_pub => {
                out.insert(i.name.clone());
            }
            Item::ExternC(b) => {
                for iface in b.interfaces.iter() {
                    if iface.is_pub {
                        out.insert(iface.name);
                    }
                }
                for c in b.consts.iter() {
                    if c.is_pub {
                        out.insert(c.name.clone());
                    }
                }
                for inner in &b.items {
                    match inner {
                        ilang_ast::ExternCItem::Struct { is_pub: true, name, .. }
                        | ilang_ast::ExternCItem::Union { is_pub: true, name, .. }
                        | ilang_ast::ExternCItem::FnDecl { is_pub: true, name, .. } => {
                            out.insert(*name);
                        }
                        ilang_ast::ExternCItem::FnDef(f) if f.is_pub => {
                            out.insert(f.name.clone());
                        }
                        ilang_ast::ExternCItem::Class(c) if c.is_pub => {
                            out.insert(c.name.clone());
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    for s in &prog.stmts {
        if let StmtKind::Let { is_pub: true, name, .. } = &s.kind {
            out.insert(name.clone());
        }
    }
    let module_dir = canon
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    for item in &prog.items {
        if let Item::Use(nu) = item {
            if !nu.re_export {
                continue;
            }
            let subpath_strs: Vec<String> = nu
                .subpath
                .iter()
                .map(|s| s.as_str().to_string())
                .collect();
            collect_export_names(
                nu.module.as_str(),
                nu.super_count,
                &module_dir,
                extra_paths,
                parents,
                dep_names_to_dirs,
                loaded,
                visited,
                out,
                &subpath_strs,
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn find_in_export_chain(
    module: &str,
    super_count: u32,
    name: &str,
    importer_dir: &Path,
    extra_paths: &[PathBuf],
    parents: &HashMap<PathBuf, PathBuf>,
    dep_names_to_dirs: &HashMap<String, PathBuf>,
    loaded: &HashMap<PathBuf, Program>,
    visited: &mut HashSet<PathBuf>,
    subpath: &[String],
) -> Result<bool, LoadError> {
    let canon = resolve_module(
        module, subpath, importer_dir, extra_paths, super_count, parents, dep_names_to_dirs,
    )?;
    if !visited.insert(canon.clone()) {
        return Ok(false);
    }
    let prog = loaded
        .get(&canon)
        .expect("module pre-loaded by load_recursive");
    // Local items first — including struct / fn / class / static
    // / fn-decl entries declared inside this module's own
    // `@extern(C) { ... }` block.
    for item in &prog.items {
        if let Some(item_name) = item_name_of(item) {
            if item_name.as_str() == name {
                return Ok(true);
            }
        }
        if let Item::ExternC(b) = item {
            for iface in b.interfaces.iter() {
                if iface.name.as_str() == name {
                    return Ok(true);
                }
            }
            for c in b.consts.iter() {
                if c.name.as_str() == name {
                    return Ok(true);
                }
            }
            for inner in &b.items {
                let n = match inner {
                    ilang_ast::ExternCItem::Struct { name, .. }
                    | ilang_ast::ExternCItem::Union { name, .. }
                    | ilang_ast::ExternCItem::FnDecl { name, .. } => name.as_str(),
                    ilang_ast::ExternCItem::FnDef(f) => f.name.as_str(),
                    ilang_ast::ExternCItem::Class(c) => c.name.as_str(),
                };
                if n == name {
                    return Ok(true);
                }
            }
        }
    }
    // Then follow `pub use` re-exports.
    let module_dir = canon
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    for item in &prog.items {
        if let Item::Use(nu) = item {
            if !nu.re_export {
                continue;
            }
            let subpath_strs: Vec<String> = nu
                .subpath
                .iter()
                .map(|s| s.as_str().to_string())
                .collect();
            if find_in_export_chain(
                nu.module.as_str(),
                nu.super_count,
                name,
                &module_dir,
                extra_paths,
                parents,
                dep_names_to_dirs,
                loaded,
                visited,
                &subpath_strs,
            )? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
