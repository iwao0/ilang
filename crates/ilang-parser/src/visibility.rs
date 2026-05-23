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
    Block, ClassDecl, CtorArgs, Expr, ExprKind, ExternCItem, Item, MatchArm, Program, Span, Stmt,
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
                        // For `pub use A.b.c.*` the effective nested
                        // module is the deepest segment (`c`) — that's
                        // the file the loader actually mapped under
                        // `<importer>/A/b/c.il`. Single-segment imports
                        // keep their original `module` as the nested key.
                        let nested = if let Some(last) = u.subpath.last() {
                            last.as_str().to_string()
                        } else {
                            u.module.as_str().to_string()
                        };
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
    let ck = Checker { self_module, catalog };
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
            Item::Fn(f) => ck.check_block(&f.body)?,
            Item::Class(c) => ck.check_class(c)?,
            Item::Enum(_) => {}
            Item::Const(c) => ck.check_expr(&c.value)?,
            Item::ExternC(b) => {
                for inner in b.items.iter() {
                    match inner {
                        ExternCItem::FnDef(f) => ck.check_block(&f.body)?,
                        ExternCItem::Class(c) => ck.check_class(c)?,
                        _ => {}
                    }
                }
            }
            Item::Interface(_) => {}
        }
    }
    for s in &prog.stmts {
        ck.check_stmt(s)?;
    }
    if let Some(t) = &prog.tail {
        ck.check_expr(t)?;
    }
    Ok(())
}

/// Threads `self_module` + `catalog` through every recursive check
/// so each call site reads as `self.check_x(...)` instead of
/// `check_x(..., self_module, catalog)`.
struct Checker<'a> {
    self_module: Option<&'a str>,
    catalog: &'a PubCatalog,
}

impl<'a> Checker<'a> {
    fn check_class(&self, c: &ClassDecl) -> Result<(), LoadError> {
        if let Some(parent) = &c.parent {
            self.check_dotted(parent, c.span)?;
        }
        for f in c.fields.iter() {
            self.check_type(&f.ty, f.span)?;
        }
        for sf in c.static_fields.iter() {
            self.check_type(&sf.ty, sf.span)?;
            self.check_expr(&sf.value)?;
        }
        for m in c.methods.iter().chain(c.static_methods.iter()) {
            for p in m.params.iter() {
                self.check_type(&p.ty, p.span)?;
                if let Some(d) = &p.default {
                    self.check_expr(d)?;
                }
            }
            if let Some(r) = &m.ret {
                self.check_type(r, m.span)?;
            }
            self.check_block(&m.body)?;
        }
        for prop in c.properties.iter() {
            self.check_type(&prop.ty, prop.span)?;
            if let Some(g) = &prop.getter {
                self.check_block(&g.body)?;
            }
            if let Some(s) = &prop.setter {
                for p in s.params.iter() {
                    self.check_type(&p.ty, p.span)?;
                }
                self.check_block(&s.body)?;
            }
        }
        Ok(())
    }

    fn check_dotted(&self, name: &Symbol, span: Span) -> Result<(), LoadError> {
        let s = name.as_str();
        if let Some((prefix, rest)) = s.split_once('.') {
            // Intra-module qualified ref (own file referencing its own
            // item via the canonical prefix). The per-file AST shouldn't
            // produce these — bare refs stay bare until the loader merge —
            // but be lenient if they appear.
            if Some(prefix) == self.self_module {
                return Ok(());
            }
            let Some(pubs) = self.catalog.get(prefix) else {
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

    fn check_type(&self, t: &Type, span: Span) -> Result<(), LoadError> {
        match t {
            Type::Object(n) | Type::Enum(n) => self.check_dotted(n, span)?,
            Type::Generic(g) => {
                self.check_dotted(&g.base, span)?;
                for a in g.args.iter() {
                    self.check_type(a, span)?;
                }
            }
            Type::Array { elem, .. } | Type::Optional(elem) | Type::Weak(elem) => {
                self.check_type(elem, span)?
            }
            Type::Tuple(elems) => {
                for e in elems.iter() {
                    self.check_type(e, span)?;
                }
            }
            Type::Fn(ft) => {
                for p in ft.params.iter() {
                    self.check_type(p, span)?;
                }
                self.check_type(&ft.ret, span)?;
            }
            Type::RawPtr { inner, .. } => self.check_type(inner, span)?,
            _ => {}
        }
        Ok(())
    }

    fn check_block(&self, b: &Block) -> Result<(), LoadError> {
        for s in b.stmts.iter() {
            self.check_stmt(s)?;
        }
        if let Some(t) = b.tail.as_ref() {
            self.check_expr(t)?;
        }
        Ok(())
    }

    fn check_stmt(&self, s: &Stmt) -> Result<(), LoadError> {
        match &s.kind {
            StmtKind::Let { ty, value, .. } => {
                if let Some(t) = ty {
                    self.check_type(t, s.span)?;
                }
                self.check_expr(value)?;
            }
            StmtKind::LetTuple { value, .. } | StmtKind::LetStruct { value, .. } => {
                self.check_expr(value)?;
            }
            StmtKind::Expr(e) => self.check_expr(e)?,
        }
        Ok(())
    }

    /// Visit an expression: do any variant-specific visibility check
    /// here, then descend into every direct child via
    /// [`walk_expr_children`]. Splitting the two halves keeps the
    /// "what's checked" arms compact — only six variants need
    /// per-node work.
    fn check_expr(&self, e: &Expr) -> Result<(), LoadError> {
        match &e.kind {
            ExprKind::Var(name) => self.check_dotted(name, e.span)?,
            ExprKind::Call { callee, .. } => self.check_dotted(callee, e.span)?,
            ExprKind::New { class, type_args, .. } => {
                self.check_dotted(class, e.span)?;
                for ta in type_args.iter() {
                    self.check_type(ta, e.span)?;
                }
            }
            ExprKind::StructLit { class, .. } => self.check_dotted(class, e.span)?,
            ExprKind::Cast { ty, .. }
            | ExprKind::TypeTest { ty, .. }
            | ExprKind::TypeDowncast { ty, .. } => self.check_type(ty, e.span)?,
            ExprKind::FnExpr { params, ret, .. } => {
                for p in params.iter() {
                    self.check_type(&p.ty, p.span)?;
                }
                if let Some(r) = ret {
                    self.check_type(r, e.span)?;
                }
            }
            _ => {}
        }
        self.walk_expr_children(e)
    }

    /// Recurse into every direct sub-expression (and any contained
    /// `Block`) of `e`, calling `check_expr` / `check_block`. Pure
    /// traversal — no visibility check of its own. Each `ExprKind`
    /// variant is named explicitly so missed-variant regressions
    /// surface as compile errors.
    fn walk_expr_children(&self, e: &Expr) -> Result<(), LoadError> {
        match &e.kind {
            // Single sub-expression.
            ExprKind::Unary { expr, .. }
            | ExprKind::Some(expr)
            | ExprKind::Await(expr)
            | ExprKind::Cast { expr, .. }
            | ExprKind::TypeTest { expr, .. }
            | ExprKind::TypeDowncast { expr, .. }
            | ExprKind::Field { obj: expr, .. }
            | ExprKind::Assign { value: expr, .. } => self.check_expr(expr)?,
            // Optional sub-expression.
            ExprKind::Return(opt) | ExprKind::Break(opt) => {
                if let Some(x) = opt {
                    self.check_expr(x)?;
                }
            }
            // Two sub-expressions.
            ExprKind::Binary { lhs, rhs, .. }
            | ExprKind::Logical { lhs, rhs, .. } => {
                self.check_expr(lhs)?;
                self.check_expr(rhs)?;
            }
            ExprKind::Index { obj, index } => {
                self.check_expr(obj)?;
                self.check_expr(index)?;
            }
            ExprKind::AssignField { obj, value, .. } => {
                self.check_expr(obj)?;
                self.check_expr(value)?;
            }
            ExprKind::AssignIndex { obj, index, value } => {
                self.check_expr(obj)?;
                self.check_expr(index)?;
                self.check_expr(value)?;
            }
            // Receiver + args.
            ExprKind::MethodCall { obj, args, .. } => {
                self.check_expr(obj)?;
                for a in args.iter() {
                    self.check_expr(a)?;
                }
            }
            // Args-only call shapes.
            ExprKind::Call { args, .. }
            | ExprKind::SuperCall { args, .. }
            | ExprKind::New { args, .. } => {
                for a in args.iter() {
                    self.check_expr(a)?;
                }
            }
            ExprKind::StructLit { fields, .. } => {
                for (_, x) in fields.iter() {
                    self.check_expr(x)?;
                }
            }
            ExprKind::EnumCtor { args, .. } => match args {
                CtorArgs::Unit => {}
                CtorArgs::Tuple(es) => {
                    for a in es.iter() {
                        self.check_expr(a)?;
                    }
                }
                CtorArgs::Struct(fs) => {
                    for (_, a) in fs.iter() {
                        self.check_expr(a)?;
                    }
                }
            },
            // Sequences.
            ExprKind::Array(es) | ExprKind::Tuple(es) => {
                for x in es.iter() {
                    self.check_expr(x)?;
                }
            }
            ExprKind::MapLit(entries) => {
                for (k, v) in entries.iter() {
                    self.check_expr(k)?;
                    self.check_expr(v)?;
                }
            }
            // Control flow (Expr + Block combinations).
            ExprKind::If { cond, then_branch, else_branch } => {
                self.check_expr(cond)?;
                self.check_block(then_branch)?;
                if let Some(e2) = else_branch {
                    self.check_expr(e2)?;
                }
            }
            ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
                self.check_expr(expr)?;
                self.check_block(then_branch)?;
                if let Some(e2) = else_branch {
                    self.check_expr(e2)?;
                }
            }
            ExprKind::While { cond, body } => {
                self.check_expr(cond)?;
                self.check_block(body)?;
            }
            ExprKind::Loop { body } => self.check_block(body)?,
            ExprKind::ForIn { iter, body, .. } => {
                self.check_expr(iter)?;
                self.check_block(body)?;
            }
            ExprKind::Match { scrutinee, arms } => {
                self.check_expr(scrutinee)?;
                for arm in arms.iter() {
                    self.check_match_arm(arm)?;
                }
            }
            ExprKind::Range { start, end, .. } => {
                if let Some(s) = start {
                    self.check_expr(s)?;
                }
                if let Some(e2) = end {
                    self.check_expr(e2)?;
                }
            }
            ExprKind::Block(b) => self.check_block(b)?,
            ExprKind::FnExpr { params, body, .. } => {
                // Per-param `check_type` ran in `check_expr`; here
                // only the value-bearing parts (defaults + body).
                for p in params.iter() {
                    if let Some(d) = &p.default {
                        self.check_expr(d)?;
                    }
                }
                self.check_block(body)?;
            }
            // Leaves: no sub-expressions.
            ExprKind::Var(_)
            | ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::Str(_)
            | ExprKind::This
            | ExprKind::None
            | ExprKind::Continue
            | ExprKind::Closure { .. } => {}
        }
        Ok(())
    }

    fn check_match_arm(&self, arm: &MatchArm) -> Result<(), LoadError> {
        self.check_expr(&arm.body)
    }
}
