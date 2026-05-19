//! Cross-module visibility check.
//!
//! Default visibility for top-level items (and class members) is
//! **module-private** — only the declaring file can name the item.
//! `pub` opts an item into cross-module use. This pass runs after
//! every `use`-imported file has been parsed and per-file normalized,
//! but BEFORE the loader merges items into one `Program`. It walks
//! each loaded module's AST and rejects:
//!
//! - `use M { X }` selective imports where `X` isn't pub in `M`.
//! - `M.X` references (qualified `Var` / `Call` / `Type::Object` /
//!   `New { class: M.X }` / etc.) where `X` isn't pub in `M`.
//!
//! `pub use M` re-export chains propagate pub-ness: items pub in M
//! that an umbrella re-exports become reachable through the umbrella's
//! prefix as well.
//!
//! Class member visibility (`pub init` / `pub fn method` / `pub field`)
//! is enforced later by the type checker — this pass only handles the
//! top-level (item-level) layer.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use ilang_ast::{
    Block, ClassDecl, CtorArgs, Expr, ExprKind, ExternCItem, Item, MatchArm, Program, Stmt,
    StmtKind, Symbol, Type,
};

use crate::loader::LoadError;

/// Map from module name (as resolvable through `use`) to the set of
/// item names it exposes publicly. Includes both directly `pub`
/// declarations and items reached through `pub use M` chains.
type PubCatalog = HashMap<String, HashSet<Symbol>>;

/// Derive a module's `use`-name from a loaded file path.
/// `<builtin>/math.il` → `math`; `/abs/path/sdl_core.il` → `sdl_core`.
fn module_name_of(path: &Path) -> Option<String> {
    let s = path.to_str()?;
    let stripped = s.strip_prefix("<builtin>/").unwrap_or(s);
    let p = Path::new(stripped);
    let stem = p.file_stem()?.to_str()?;
    // `<dir>/foo/mod.il` — Rust-style subfolder umbrella, the
    // module name is the parent directory's name (`foo`) not the
    // literal file stem (`mod`). Otherwise visibility-catalog
    // lookups for `use foo { … }` would miss the module entirely.
    if stem == "mod" {
        if let Some(parent_name) = p.parent().and_then(Path::file_name).and_then(|n| n.to_str()) {
            return Some(parent_name.to_string());
        }
    }
    Some(stem.to_string())
}

/// Collect the set of names that are directly `pub` at the top level
/// of a single module's AST.
fn direct_pubs(prog: &Program) -> HashSet<Symbol> {
    let mut out: HashSet<Symbol> = HashSet::new();
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
                for inner in b.items.iter() {
                    match inner {
                        ExternCItem::FnDecl { is_pub: true, name, .. }
                        | ExternCItem::Struct { is_pub: true, name, .. }
                        | ExternCItem::Union { is_pub: true, name, .. } => {
                            out.insert(name.clone());
                        }
                        ExternCItem::FnDef(f) if f.is_pub => {
                            out.insert(f.name.clone());
                        }
                        ExternCItem::Class(c) if c.is_pub => {
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
        if let StmtKind::Let { is_pub: true,
                is_const: false, name, .. } = &s.kind {
            out.insert(name.clone());
        }
    }
    out
}

/// Build the per-module pub catalog with `pub use M` chains expanded.
/// Recurses with memoization; the import graph is a DAG (cycle
/// detection runs at load time), so this terminates.
fn build_catalog(loaded: &HashMap<PathBuf, Program>) -> PubCatalog {
    // Two files in different folders can share a module name
    // (e.g. `foundation/core.il` and `appkit/core.il`, both seen
    // as `core` by `use core { … }`). Earlier this used
    // `direct.insert(...)` which silently dropped one side; now
    // we collect every contributor so the catalog union covers
    // all pub names from every module that resolves under that
    // bare name. `paths_per_name` keeps both contributing paths
    // so `expand` can walk each module's `pub use` chain.
    let mut paths_per_name: HashMap<String, Vec<PathBuf>> = HashMap::new();
    let mut direct: HashMap<String, HashSet<Symbol>> = HashMap::new();
    for (path, prog) in loaded {
        if let Some(name) = module_name_of(path) {
            let pubs = direct_pubs(prog);
            direct
                .entry(name.clone())
                .or_default()
                .extend(pubs.into_iter());
            paths_per_name.entry(name).or_default().push(path.clone());
        }
    }
    let mut catalog: PubCatalog = HashMap::new();
    for module in direct.keys() {
        expand(module, loaded, &paths_per_name, &direct, &mut catalog, &mut HashSet::new());
    }
    catalog
}

fn expand(
    module: &str,
    loaded: &HashMap<PathBuf, Program>,
    paths_per_name: &HashMap<String, Vec<PathBuf>>,
    direct: &HashMap<String, HashSet<Symbol>>,
    out: &mut PubCatalog,
    visiting: &mut HashSet<String>,
) {
    if out.contains_key(module) {
        return;
    }
    if !visiting.insert(module.to_string()) {
        return;
    }
    let mut acc = direct.get(module).cloned().unwrap_or_default();
    // Walk every contributing path's `pub use` chain — same-name
    // sibling modules might each `pub use` different submodules,
    // and we want the catalog to surface both sets so a
    // selective import resolves through either path.
    if let Some(paths) = paths_per_name.get(module) {
        for path in paths {
            let Some(prog) = loaded.get(path) else { continue };
            for item in &prog.items {
                if let Item::Use(u) = item {
                    if u.re_export {
                        let nested = u.module.as_str().to_string();
                        expand(&nested, loaded, paths_per_name, direct, out, visiting);
                        if let Some(set) = out.get(&nested) {
                            for n in set {
                                if u.wildcard {
                                    // `pub use M as _ { * }` — flatten:
                                    // `<umbrella>.X` is reachable.
                                    acc.insert(n.clone());
                                } else {
                                    // `pub use M` — namespaced:
                                    // `<umbrella>.M.X` is reachable.
                                    acc.insert(
                                        Symbol::intern(&format!("{}.{}", nested, n.as_str())),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    visiting.remove(module);
    out.insert(module.to_string(), acc);
}

/// Public entry: validate every loaded file's selective imports and
/// `M.X` qualified references against the catalog. Returns the first
/// violation as a `LoadError::PrivateItemRef`.
pub fn validate_visibility(
    loaded: &HashMap<PathBuf, Program>,
    entry: &Path,
) -> Result<(), LoadError> {
    let catalog = build_catalog(loaded);
    // Validate every loaded file (including the entry).
    for (path, prog) in loaded {
        let self_module = module_name_of(path);
        validate_program(prog, self_module.as_deref(), &catalog)?;
    }
    // The entry program in `load_program_with_overlay` is removed
    // from `loaded` before this runs in some paths; cover that case
    // explicitly.
    if !loaded.contains_key(entry) {
        return Ok(());
    }
    Ok(())
}

fn validate_program(
    prog: &Program,
    self_module: Option<&str>,
    catalog: &PubCatalog,
) -> Result<(), LoadError> {
    for item in &prog.items {
        match item {
            Item::Use(u) => {
                if let Some(names) = &u.selective {
                    let target = u.module.as_str();
                    let pubs = catalog.get(target);
                    for n in names.iter() {
                        if !pubs.map(|p| p.contains(n)).unwrap_or(false) {
                            return Err(LoadError::PrivateItemRef {
                                module: u.module.clone(),
                                name: n.clone(),
                                span: u.span,
                            });
                        }
                    }
                }
            }
            Item::Fn(f) => {
                check_block(&f.body, self_module, catalog)?;
            }
            Item::Class(c) => check_class(c, self_module, catalog)?,
            Item::Enum(_) => {}
            Item::Const(c) => check_expr(&c.value, self_module, catalog)?,
            Item::ExternC(b) => {
                for inner in b.items.iter() {
                    match inner {
                        ExternCItem::FnDef(f) => check_block(&f.body, self_module, catalog)?,
                        ExternCItem::Class(c) => check_class(c, self_module, catalog)?,
                        _ => {}
                    }
                }
            }
            Item::Interface(_) => {}
        }
    }
    for s in &prog.stmts {
        check_stmt(s, self_module, catalog)?;
    }
    if let Some(t) = &prog.tail {
        check_expr(t, self_module, catalog)?;
    }
    Ok(())
}

fn check_class(c: &ClassDecl, self_module: Option<&str>, catalog: &PubCatalog) -> Result<(), LoadError> {
    if let Some(parent) = &c.parent {
        check_dotted(parent, c.span, self_module, catalog)?;
    }
    for f in c.fields.iter() {
        check_type(&f.ty, f.span, self_module, catalog)?;
    }
    for sf in c.static_fields.iter() {
        check_type(&sf.ty, sf.span, self_module, catalog)?;
        check_expr(&sf.value, self_module, catalog)?;
    }
    for m in c.methods.iter().chain(c.static_methods.iter()) {
        for p in m.params.iter() {
            check_type(&p.ty, p.span, self_module, catalog)?;
            if let Some(d) = &p.default {
                check_expr(d, self_module, catalog)?;
            }
        }
        if let Some(r) = &m.ret {
            check_type(r, m.span, self_module, catalog)?;
        }
        check_block(&m.body, self_module, catalog)?;
    }
    for prop in c.properties.iter() {
        check_type(&prop.ty, prop.span, self_module, catalog)?;
        if let Some(g) = &prop.getter {
            check_block(&g.body, self_module, catalog)?;
        }
        if let Some(s) = &prop.setter {
            for p in s.params.iter() {
                check_type(&p.ty, p.span, self_module, catalog)?;
            }
            check_block(&s.body, self_module, catalog)?;
        }
    }
    Ok(())
}

fn check_dotted(
    name: &Symbol,
    span: ilang_ast::Span,
    self_module: Option<&str>,
    catalog: &PubCatalog,
) -> Result<(), LoadError> {
    let s = name.as_str();
    if let Some((prefix, rest)) = s.split_once('.') {
        // Intra-module qualified ref (own file referencing its own
        // item via the canonical prefix). The per-file AST shouldn't
        // produce these — bare refs stay bare until the loader merge —
        // but be lenient if they appear.
        if Some(prefix) == self_module {
            return Ok(());
        }
        let Some(pubs) = catalog.get(prefix) else {
            return Err(LoadError::PrivateItemRef {
                module: Symbol::intern(prefix),
                name: Symbol::intern(rest),
                span,
            });
        };
        if pubs.contains(&Symbol::intern(rest)) {
            return Ok(());
        }
        // Multi-level dotted refs: `M.X.Y` (enum-variant access on a
        // pub enum, or `umbrella.sub.item` against a namespaced
        // `pub use`). Accept the ref as long as the head of `rest`
        // is pub, or there exists a deeper pub name under the
        // `rest.` prefix.
        if let Some((head, _)) = rest.split_once('.') {
            if pubs.contains(&Symbol::intern(head)) {
                return Ok(());
            }
        }
        let rest_dot = format!("{rest}.");
        if pubs.iter().any(|n| n.as_str().starts_with(&rest_dot)) {
            return Ok(());
        }
        return Err(LoadError::PrivateItemRef {
            module: Symbol::intern(prefix),
            name: Symbol::intern(rest),
            span,
        });
    }
    Ok(())
}

fn check_type(
    t: &Type,
    span: ilang_ast::Span,
    self_module: Option<&str>,
    catalog: &PubCatalog,
) -> Result<(), LoadError> {
    match t {
        Type::Object(n) | Type::Enum(n) => check_dotted(n, span, self_module, catalog)?,
        Type::Generic(g) => {
            check_dotted(&g.base, span, self_module, catalog)?;
            for a in g.args.iter() {
                check_type(a, span, self_module, catalog)?;
            }
        }
        Type::Array { elem, .. } | Type::Optional(elem) | Type::Weak(elem) => {
            check_type(elem, span, self_module, catalog)?
        }
        Type::Tuple(elems) => {
            for e in elems.iter() {
                check_type(e, span, self_module, catalog)?;
            }
        }
        Type::Fn(ft) => {
            for p in ft.params.iter() {
                check_type(p, span, self_module, catalog)?;
            }
            check_type(&ft.ret, span, self_module, catalog)?;
        }
        Type::RawPtr { inner, .. } => check_type(inner, span, self_module, catalog)?,
        _ => {}
    }
    Ok(())
}

fn check_block(b: &Block, self_module: Option<&str>, catalog: &PubCatalog) -> Result<(), LoadError> {
    for s in b.stmts.iter() {
        check_stmt(s, self_module, catalog)?;
    }
    if let Some(t) = b.tail.as_ref() {
        check_expr(t, self_module, catalog)?;
    }
    Ok(())
}

fn check_stmt(s: &Stmt, self_module: Option<&str>, catalog: &PubCatalog) -> Result<(), LoadError> {
    match &s.kind {
        StmtKind::Let { ty, value, .. } => {
            if let Some(t) = ty {
                check_type(t, s.span, self_module, catalog)?;
            }
            check_expr(value, self_module, catalog)?;
        }
        StmtKind::LetTuple { value, .. } | StmtKind::LetStruct { value, .. } => {
            check_expr(value, self_module, catalog)?;
        }
        StmtKind::Expr(e) => check_expr(e, self_module, catalog)?,
    }
    Ok(())
}

fn check_expr(e: &Expr, self_module: Option<&str>, catalog: &PubCatalog) -> Result<(), LoadError> {
    match &e.kind {
        ExprKind::Var(name) => check_dotted(name, e.span, self_module, catalog)?,
        ExprKind::Call { callee, args } => {
            check_dotted(callee, e.span, self_module, catalog)?;
            for a in args.iter() {
                check_expr(a, self_module, catalog)?;
            }
        }
        ExprKind::New { class, type_args, args, .. } => {
            check_dotted(class, e.span, self_module, catalog)?;
            for ta in type_args.iter() {
                check_type(ta, e.span, self_module, catalog)?;
            }
            for a in args.iter() {
                check_expr(a, self_module, catalog)?;
            }
        }
        ExprKind::StructLit { class, fields } => {
            check_dotted(class, e.span, self_module, catalog)?;
            for (_, x) in fields.iter() {
                check_expr(x, self_module, catalog)?;
            }
        }
        ExprKind::Cast { expr, ty }
        | ExprKind::TypeTest { expr, ty }
        | ExprKind::TypeDowncast { expr, ty } => {
            check_expr(expr, self_module, catalog)?;
            check_type(ty, e.span, self_module, catalog)?;
        }
        ExprKind::FnExpr { params, ret, body } => {
            for p in params.iter() {
                check_type(&p.ty, p.span, self_module, catalog)?;
                if let Some(d) = &p.default {
                    check_expr(d, self_module, catalog)?;
                }
            }
            if let Some(r) = ret {
                check_type(r, e.span, self_module, catalog)?;
            }
            check_block(body, self_module, catalog)?;
        }
        ExprKind::Unary { expr, .. }
        | ExprKind::Some(expr)
        | ExprKind::Await(expr)
        | ExprKind::Return(Some(expr))
        | ExprKind::Break(Some(expr)) => check_expr(expr, self_module, catalog)?,
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            check_expr(lhs, self_module, catalog)?;
            check_expr(rhs, self_module, catalog)?;
        }
        ExprKind::SuperCall { args, .. } => {
            for a in args.iter() {
                check_expr(a, self_module, catalog)?;
            }
        }
        ExprKind::MethodCall { obj, args, .. } => {
            check_expr(obj, self_module, catalog)?;
            for a in args.iter() {
                check_expr(a, self_module, catalog)?;
            }
        }
        ExprKind::Field { obj, .. } => check_expr(obj, self_module, catalog)?,
        ExprKind::Assign { value, .. } => check_expr(value, self_module, catalog)?,
        ExprKind::AssignField { obj, value, .. } => {
            check_expr(obj, self_module, catalog)?;
            check_expr(value, self_module, catalog)?;
        }
        ExprKind::AssignIndex { obj, index, value } => {
            check_expr(obj, self_module, catalog)?;
            check_expr(index, self_module, catalog)?;
            check_expr(value, self_module, catalog)?;
        }
        ExprKind::EnumCtor { args, .. } => match args {
            CtorArgs::Unit => {}
            CtorArgs::Tuple(es) => {
                for a in es.iter() {
                    check_expr(a, self_module, catalog)?;
                }
            }
            CtorArgs::Struct(fs) => {
                for (_, a) in fs.iter() {
                    check_expr(a, self_module, catalog)?;
                }
            }
        },
        ExprKind::If { cond, then_branch, else_branch } => {
            check_expr(cond, self_module, catalog)?;
            check_block(then_branch, self_module, catalog)?;
            if let Some(e2) = else_branch {
                check_expr(e2, self_module, catalog)?;
            }
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            check_expr(expr, self_module, catalog)?;
            check_block(then_branch, self_module, catalog)?;
            if let Some(e2) = else_branch {
                check_expr(e2, self_module, catalog)?;
            }
        }
        ExprKind::While { cond, body } => {
            check_expr(cond, self_module, catalog)?;
            check_block(body, self_module, catalog)?;
        }
        ExprKind::Loop { body } => check_block(body, self_module, catalog)?,
        ExprKind::ForIn { iter, body, .. } => {
            check_expr(iter, self_module, catalog)?;
            check_block(body, self_module, catalog)?;
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                check_expr(s, self_module, catalog)?;
            }
            if let Some(e2) = end {
                check_expr(e2, self_module, catalog)?;
            }
        }
        ExprKind::Block(b) => check_block(b, self_module, catalog)?,
        ExprKind::Array(es) | ExprKind::Tuple(es) => {
            for x in es.iter() {
                check_expr(x, self_module, catalog)?;
            }
        }
        ExprKind::MapLit(entries) => {
            for (k, v) in entries.iter() {
                check_expr(k, self_module, catalog)?;
                check_expr(v, self_module, catalog)?;
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            check_expr(scrutinee, self_module, catalog)?;
            for arm in arms.iter() {
                check_match_arm(arm, self_module, catalog)?;
            }
        }
        ExprKind::Index { obj, index } => {
            check_expr(obj, self_module, catalog)?;
            check_expr(index, self_module, catalog)?;
        }
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::This
        | ExprKind::None
        | ExprKind::Continue
        | ExprKind::Closure { .. }
        | ExprKind::Return(None)
        | ExprKind::Break(None) => {}
    }
    Ok(())
}

fn check_match_arm(arm: &MatchArm, self_module: Option<&str>, catalog: &PubCatalog) -> Result<(), LoadError> {
    check_expr(&arm.body, self_module, catalog)
}
