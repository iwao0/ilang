//! Post-parse AST normalization: rewrite `EnumName.Variant` shapes
//! that the parser couldn't disambiguate at parse time.
//!
//! With the `.` syntax replacing `::` for enum constructors, the
//! parser produces these shapes for `Color.Green` / `Result.Ok(5)`:
//!
//!   `Color.Green`     → `ExprKind::Field { obj: Var("Color"), name: "Green" }`
//!   `Result.Ok(5)`    → `ExprKind::MethodCall { obj: Var("Result"), method: "Ok", args: [5] }`
//!
//! When the receiver is a bare `Var` whose name matches a top-level
//! `enum` (or the built-in `Result`), this pass rewrites them into
//! `ExprKind::EnumCtor`. Anything that doesn't match (e.g.
//! `obj.field`, `console.log(...)`) passes through unchanged.
//!
//! Struct-payload constructors (`Color.Red { side: 1 }`) are produced
//! directly as `EnumCtor` by the parser via lookahead, so they don't
//! need rewriting here.

use std::collections::{HashMap, HashSet};

use ilang_ast::{
    Block, CtorArgs, Expr, ExprKind, Item, Program, Span, Stmt, StmtKind, Symbol, UseAlias,
};

use crate::error::ParseError;

pub mod async_desugar;
mod dealias;
pub(crate) mod state_machine;
mod validate;

use dealias::dealias_program;
use validate::validate_program;
use ilang_ast::walk::fold_expr_default;

/// Built-in enum names that are always available.
const BUILTIN_ENUMS: &[&str] = &["Result"];

#[derive(Default)]
struct Ctx {
    /// All names that resolve as enums after the program is fully
    /// loaded (built-ins + every `Item::Enum`'s name).
    enums: HashSet<Symbol>,
    /// User-facing namespace name → canonical module name. Default
    /// `use M` records `M → M`; `use M as foo` records `foo → M`;
    /// `use M as _` is omitted entirely. Selective `use M { X }`
    /// also records the namespace, so `M.X` (or `foo.X`) is valid
    /// alongside the bare `X`. The rewrite pass collapses
    /// `Field(Var(prefix), name)` into `Var("M.name")` using the
    /// canonical module value, so the loader's prefix-merged top-
    /// level item with the exact name `M.name` is found.
    modules: HashMap<Symbol, Symbol>,
    /// Set of every top-level item name in the merged Program. Only
    /// populated by `renormalize_merged` (per-file normalize leaves
    /// it empty since the file doesn't know what other modules
    /// exported). Used to collapse multi-level dotted refs like
    /// `umbrella.inner.fn(...)` into `Call("umbrella.inner.fn")`
    /// after `pub use inner` namespaced re-exports — the per-file
    /// pass can only collapse one level (the immediate module).
    items: HashSet<Symbol>,
    /// Stack of in-scope local-binding names (function params + `let`
    /// names) accumulated as the rewrite descends through blocks.
    /// `Field` / `MethodCall` collapse paths consult this set: a
    /// receiver `Var(M)` that shadows a `use`d module name should
    /// dispatch as a value method on the local binding, NOT as a
    /// cross-module reference. Wraps `RefCell` so the rewrite
    /// functions don't have to thread `&mut` through every
    /// arm — they're otherwise read-only against `Ctx`.
    locals: std::cell::RefCell<HashSet<Symbol>>,
}

impl Ctx {
    /// Returns true when `name` is currently shadowed by a local
    /// binding (param or `let`). Field / MethodCall collapse paths
    /// use this to suppress module-name collapse when the bare
    /// receiver is actually a value.
    fn is_local_shadow(&self, name: &Symbol) -> bool {
        self.locals.borrow().contains(name)
    }

    /// Save the current `locals` snapshot, run `f`, then restore.
    /// Block / fn-body scopes call this so a `let` declared inside
    /// doesn't leak past the closing `}`.
    fn with_scope<R>(&self, f: impl FnOnce() -> R) -> R {
        let saved: HashSet<Symbol> = self.locals.borrow().clone();
        let r = f();
        *self.locals.borrow_mut() = saved;
        r
    }

    /// Add `name` to the active local scope. Caller is responsible
    /// for wrapping the surrounding scope in `with_scope` so the
    /// addition pops at block exit.
    fn push_local(&self, name: Symbol) {
        self.locals.borrow_mut().insert(name);
    }

    /// Decide how a `Var(receiver).suffix` (Field) or
    /// `Var(receiver).suffix(args)` (MethodCall) should be rewritten.
    /// Both call sites used to repeat the four-step cascade
    /// (shadowing → module bump → namespaced re-export → enum ctor);
    /// this method is the one source of truth so they stay aligned.
    fn resolve_dotted_receiver(
        &self,
        receiver: &Symbol,
        suffix: &Symbol,
    ) -> DottedResolution {
        // A local binding shadows everything — fall back to a plain
        // value access (`device.newLibraryWithSource(...)` stays a
        // method call instead of collapsing to a static reference).
        if self.is_local_shadow(receiver) {
            return DottedResolution::None;
        }
        // `use M [as alias]` — qualify under the module's canonical name.
        if let Some(canonical) = self.modules.get(&Symbol::intern(receiver.as_str())) {
            return DottedResolution::Qualified(format!(
                "{}.{}",
                canonical.as_str(),
                suffix.as_str()
            ));
        }
        // Namespaced re-export path: `umbrella.inner.X` whose joined
        // form actually names a merged top-level item. Only fires in
        // `renormalize_merged` (per-file pass leaves `items` empty).
        let qualified = format!("{}.{}", receiver.as_str(), suffix.as_str());
        if !self.items.is_empty() && self.items.contains(&Symbol::intern(&qualified)) {
            return DottedResolution::Qualified(qualified);
        }
        // Receiver is an enum name — unit / tuple ctor depending on
        // whether the caller is Field or MethodCall.
        if self.enums.contains(&Symbol::intern(receiver.as_str())) {
            return DottedResolution::EnumCtor;
        }
        DottedResolution::None
    }
}

/// Outcome of [`Ctx::resolve_dotted_receiver`].
enum DottedResolution {
    /// Replace the access with a qualified `Var` (Field) or `Call`
    /// (MethodCall) using this already-formatted name.
    Qualified(String),
    /// Lift into an `EnumCtor`. Caller picks `Unit` vs `Tuple(args)`.
    EnumCtor,
    /// No collapse — keep the original Field / MethodCall.
    None,
}

/// Per-file normalize entry point. Validates that every dotted
/// `module.X` reference (in `new`, type position, struct literal,
/// parent class) names a module this file actually `use`s — see
/// `validate_program` — then runs the AST rewrites.
#[allow(dead_code)]
pub fn normalize(prog: Program) -> Result<Program, ParseError> {
    normalize_with_implicit_modules(prog, &[])
}

/// Like `normalize`, but additionally treats each name in
/// `implicit_modules` as if the file had `use <name>` declared at the
/// top. The loader uses this for sibling category files inside a
/// folder-binding (e.g. `bindings/cocoa/spritekit/node.il`'s auto-lift
/// references `physics.SKPhysicsWorld`, even though node.il can't
/// `use physics` without creating a circular import — physics.il
/// itself `use`s node). The implicit-module set is the list of
/// sibling stems present in the same folder, so cross-sibling
/// synthetic refs validate while genuine cross-folder leakage still
/// errors.
pub fn normalize_with_implicit_modules(
    prog: Program,
    implicit_modules: &[Symbol],
) -> Result<Program, ParseError> {
    let mut ctx = build_ctx(&prog);
    for m in implicit_modules {
        ctx.modules.entry(m.clone()).or_insert_with(|| m.clone());
    }
    validate_program(&prog, &ctx.modules)?;
    Ok(rewrite_program(prog, &ctx))
}

/// Loader-side entry point for the post-merge normalize run. The
/// merged Program intentionally contains zero `Item::Use`s (the
/// loader stripped them all), so per-file authorization has already
/// been verified at parse time; running `validate_program` here
/// would falsely reject every legitimate cross-module reference.
pub fn renormalize_merged(prog: Program) -> Program {
    let mut ctx = build_ctx(&prog);
    // Catalog every top-level item name in the merged Program. The
    // rewrite pass uses this to identify multi-level dotted refs
    // (`umbrella.inner.fn`) introduced by namespaced `pub use`.
    for item in &prog.items {
        match item {
            Item::Fn(f) => { ctx.items.insert(f.name.clone()); }
            Item::Class(c) => { ctx.items.insert(c.name.clone()); }
            Item::Enum(e) => { ctx.items.insert(e.name.clone()); }
            Item::Const(c) => { ctx.items.insert(c.name.clone()); }
            _ => {}
        }
    }
    rewrite_program(prog, &ctx)
}

fn build_ctx(prog: &Program) -> Ctx {
    let mut ctx = Ctx::default();
    for s in BUILTIN_ENUMS {
        ctx.enums.insert((*s).into());
    }
    for item in &prog.items {
        match item {
            Item::Enum(e) => {
                ctx.enums.insert(e.name.clone());
            }
            Item::Use(u) => match &u.alias {
                UseAlias::Default => {
                    // Bare path-style `use a.b.c` — the user-facing
                    // namespace is the full dotted path (`a.b.c`),
                    // which also matches the loader's merge prefix.
                    // Map identity so dealias is a no-op.
                    let canonical = full_use_path(u);
                    ctx.modules.insert(canonical.clone(), canonical);
                }
                UseAlias::Named(name) => {
                    // `use a.b.c as m` — `m.X` in user code should
                    // rewrite to the canonical merge prefix
                    // (`a.b.c.X`). For plain `use a as b` (subpath
                    // empty), the canonical name is just `a`.
                    let canonical = full_use_path(u);
                    ctx.modules.insert(name.clone(), canonical);
                }
                UseAlias::Discard => {}
            },
            _ => {}
        }
    }
    ctx
}

/// Concatenate a UseDecl's module + subpath into the canonical dotted
/// namespace string that the loader merges items under (and that the
/// user writes in their code for bare path-style imports). Single-
/// segment imports return just `module`.
fn full_use_path(u: &ilang_ast::UseDecl) -> ilang_ast::Symbol {
    if u.subpath.is_empty() {
        u.module
    } else {
        let mut s = u.module.as_str().to_string();
        for seg in u.subpath.iter() {
            s.push('.');
            s.push_str(seg.as_str());
        }
        ilang_ast::Symbol::intern(&s)
    }
}

fn rewrite_program(prog: Program, ctx: &Ctx) -> Program {
    let items: Vec<Item> = prog.items.into_iter().map(|i| rewrite_item(i, ctx)).collect();
    let stmts: Vec<Stmt> = prog.stmts.into_iter().map(|s| rewrite_stmt(s, ctx)).collect();
    let tail = prog.tail.map(|e| rewrite_expr(e, ctx));
    let mut prog = Program {
        items,
        stmts,
        tail,
    };
    // If any `use M as foo` introduced a non-trivial alias (key !=
    // value), substitute every `foo.X` symbol still hiding in Type
    // positions / `New`/`StructLit` ctor names / class-parent names
    // so the merged Program references the canonical `M.X` form.
    // Field/MethodCall paths already produce canonical names via
    // `rewrite_expr` above, so they're skipped here.
    if ctx.modules.iter().any(|(k, v)| k != v) {
        dealias_program(&mut prog, &ctx.modules);
    }
    prog
}


fn rewrite_params(params: &mut [ilang_ast::Param], ctx: &Ctx) {
    for p in params.iter_mut() {
        if let Some(d) = p.default.take() {
            p.default = Some(rewrite_expr(d, ctx));
        }
    }
}

fn rewrite_item(item: Item, ctx: &Ctx) -> Item {
    match item {
        Item::Fn(mut f) => {
            rewrite_params(&mut f.params, ctx);
            f.body = rewrite_body_with_params(f.body, &f.params, ctx);
            Item::Fn(f)
        }
        Item::Class(mut c) => {
            let methods = std::mem::take(&mut c.methods);
            c.methods = methods
                .into_iter()
                .map(|mut m| {
                    rewrite_params(&mut m.params, ctx);
                    let body = std::mem::replace(
                        &mut m.body,
                        Block { stmts: Vec::new(), tail: None },
                    );
                    m.body = rewrite_body_with_params(body, &m.params, ctx);
                    m
                })
                .collect();
            let static_methods = std::mem::take(&mut c.static_methods);
            c.static_methods = static_methods
                .into_iter()
                .map(|mut m| {
                    rewrite_params(&mut m.params, ctx);
                    let body = std::mem::replace(
                        &mut m.body,
                        Block { stmts: Vec::new(), tail: None },
                    );
                    m.body = rewrite_body_with_params(body, &m.params, ctx);
                    m
                })
                .collect();
            let static_fields = std::mem::take(&mut c.static_fields);
            c.static_fields = static_fields
                .into_iter()
                .map(|mut sf| {
                    sf.value = rewrite_expr(sf.value, ctx);
                    sf
                })
                .collect();
            let properties = std::mem::take(&mut c.properties);
            c.properties = properties
                .into_iter()
                .map(|mut p| {
                    if let Some(g) = p.getter.as_mut() {
                        let body = std::mem::replace(
                            &mut g.body,
                            Block { stmts: Vec::new(), tail: None },
                        );
                        g.body = rewrite_body_with_params(body, &g.params, ctx);
                    }
                    if let Some(s) = p.setter.as_mut() {
                        rewrite_params(&mut s.params, ctx);
                        let body = std::mem::replace(
                            &mut s.body,
                            Block { stmts: Vec::new(), tail: None },
                        );
                        s.body = rewrite_body_with_params(body, &s.params, ctx);
                    }
                    p
                })
                .collect();
            Item::Class(c)
        }
        Item::Enum(e) => Item::Enum(e),
        Item::Use(u) => Item::Use(u),
        Item::Const(mut c) => {
            c.value = rewrite_expr(c.value, ctx);
            Item::Const(c)
        }
        Item::ExternC(mut b) => {
            // Walk fn definitions inside the block so module-qualified
            // calls (`test.foo(x)`) get rewritten to `Call("test.foo", x)`
            // the same way they would in regular fn bodies.
            for inner in &mut b.items {
                match inner {
                    ilang_ast::ExternCItem::FnDef(f) => {
                        rewrite_params(&mut f.params, ctx);
                        let body = std::mem::replace(
                            &mut f.body,
                            Block { stmts: Vec::new(), tail: None },
                        );
                        f.body = rewrite_body_with_params(body, &f.params, ctx);
                    }
                    ilang_ast::ExternCItem::Class(c) => {
                        let methods = std::mem::take(&mut c.methods);
                        c.methods = methods
                            .into_iter()
                            .map(|mut m| {
                                rewrite_params(&mut m.params, ctx);
                                let body = std::mem::replace(
                                    &mut m.body,
                                    Block { stmts: Vec::new(), tail: None },
                                );
                                m.body = rewrite_body_with_params(body, &m.params, ctx);
                                m
                            })
                            .collect();
                        let static_methods = std::mem::take(&mut c.static_methods);
                        c.static_methods = static_methods
                            .into_iter()
                            .map(|mut m| {
                                rewrite_params(&mut m.params, ctx);
                                let body = std::mem::replace(
                                    &mut m.body,
                                    Block { stmts: Vec::new(), tail: None },
                                );
                                m.body = rewrite_body_with_params(body, &m.params, ctx);
                                m
                            })
                            .collect();
                        let static_fields = std::mem::take(&mut c.static_fields);
                        c.static_fields = static_fields
                            .into_iter()
                            .map(|mut sf| {
                                sf.value = rewrite_expr(sf.value, ctx);
                                sf
                            })
                            .collect();
                        let properties = std::mem::take(&mut c.properties);
                        c.properties = properties
                            .into_iter()
                            .map(|mut p| {
                                if let Some(g) = p.getter.as_mut() {
                                    let body = std::mem::replace(
                                        &mut g.body,
                                        Block { stmts: Vec::new(), tail: None },
                                    );
                                    g.body = rewrite_body_with_params(body, &g.params, ctx);
                                }
                                if let Some(s) = p.setter.as_mut() {
                                    rewrite_params(&mut s.params, ctx);
                                    let body = std::mem::replace(
                                        &mut s.body,
                                        Block { stmts: Vec::new(), tail: None },
                                    );
                                    s.body = rewrite_body_with_params(body, &s.params, ctx);
                                }
                                p
                            })
                            .collect();
                    }
                    _ => {}
                }
            }
            Item::ExternC(b)
        }
        Item::Interface(i) => Item::Interface(i),
    }
}

fn rewrite_block(b: Block, ctx: &Ctx) -> Block {
    // Each `{ … }` opens a new lexical scope. Restore the locals
    // snapshot on exit so `let` bindings declared inside don't leak
    // past the closing brace and shadow module names in following
    // sibling statements.
    ctx.with_scope(|| Block {
        stmts: Vec::from(b.stmts).into_iter().map(|s| rewrite_stmt(s, ctx)).collect(),
        tail: b.tail.map(|e| Box::new(rewrite_expr(*e, ctx))),
    })
}

fn rewrite_stmt(s: Stmt, ctx: &Ctx) -> Stmt {
    let kind = match s.kind {
        StmtKind::Let { is_pub, is_const, name, ty, value } => {
            // Rewrite the value FIRST — the let-name only enters
            // scope for subsequent statements, not its own RHS.
            // (Recursive bindings aren't a thing here.)
            let value = rewrite_expr(value, ctx);
            ctx.push_local(name);
            StmtKind::Let { is_pub, is_const, name, ty, value }
        }
        StmtKind::LetTuple { elems, value } => {
            let value = rewrite_expr(value, ctx);
            for e in elems.iter() {
                if let Some(n) = e {
                    ctx.push_local(*n);
                }
            }
            StmtKind::LetTuple { elems, value }
        }
        StmtKind::LetStruct { class, fields, value } => {
            let value = rewrite_expr(value, ctx);
            for f in fields.iter() {
                ctx.push_local(*f);
            }
            StmtKind::LetStruct { class, fields, value }
        }
        StmtKind::Expr(e) => StmtKind::Expr(rewrite_expr(e, ctx)),
    };
    Stmt { kind, span: s.span, source_module: s.source_module.clone() }
}

/// Rewrite a fn / method body with its params pushed onto the local
/// scope. The surrounding `with_scope` pops them at exit so the params
/// don't shadow module names in sibling items.
fn rewrite_body_with_params(body: Block, params: &[ilang_ast::Param], ctx: &Ctx) -> Block {
    ctx.with_scope(|| {
        for p in params.iter() {
            ctx.push_local(p.name);
        }
        rewrite_block(body, ctx)
    })
}

fn rewrite_expr(e: Expr, ctx: &Ctx) -> Expr {
    let span = e.span;
    let kind = match e.kind {
        // Receiver resolution: `module.foo` / `Enum.Variant` collapse.
        // Recurse first so a nested chain (`Field(Field(Var("utils"),
        // "Color"), "red")`) flattens before the enum-ctor check.
        ExprKind::Field { obj, name } => return rewrite_field(obj, name, span, ctx),
        ExprKind::MethodCall { obj, method, args } => {
            return rewrite_method_call(obj, method, args, span, ctx);
        }
        // FnExpr: closures introduce their own scope. Push the
        // params so a closure body like `fn(device: …) { device.foo() }`
        // dispatches `device.foo` as a value method rather than a
        // module call. Param defaults are NOT rewritten (preserving
        // the pre-refactor behaviour).
        ExprKind::FnExpr { params, ret, body } => {
            let body = rewrite_body_with_params(body, &params, ctx);
            ExprKind::FnExpr { params, ret, body }
        }
        // IfLet: the `name` binding is in scope only for the
        // then-branch. Push it inside `with_scope` so it doesn't
        // leak into the else-branch or the surrounding expression.
        ExprKind::IfLet { name, expr, then_branch, else_branch } => {
            let expr = Box::new(rewrite_expr(*expr, ctx));
            let then_branch = ctx.with_scope(|| {
                ctx.push_local(name);
                rewrite_block(then_branch, ctx)
            });
            let else_branch = else_branch.map(|e| Box::new(rewrite_expr(*e, ctx)));
            ExprKind::IfLet { name, expr, then_branch, else_branch }
        }
        // ForIn: same scope dance for the loop variable.
        ExprKind::ForIn { var, iter, body } => {
            let iter = Box::new(rewrite_expr(*iter, ctx));
            let body = ctx.with_scope(|| {
                ctx.push_local(var);
                rewrite_block(body, ctx)
            });
            ExprKind::ForIn { var, iter, body }
        }
        // Everything else: mechanical recursion through children.
        other => fold_expr_default(
            other,
            &mut |c| rewrite_expr(c, ctx),
            &mut |b| rewrite_block(b, ctx),
        ),
    };
    Expr { kind, span }
}

fn rewrite_field(obj: Box<Expr>, name: Symbol, span: Span, ctx: &Ctx) -> Expr {
    let obj = rewrite_expr(*obj, ctx);
    if let ExprKind::Var(receiver) = &obj.kind {
        match ctx.resolve_dotted_receiver(receiver, &name) {
            DottedResolution::Qualified(s) => {
                return Expr::new(ExprKind::Var(Symbol::intern(&s)), span);
            }
            DottedResolution::EnumCtor => {
                return Expr::new(
                    ExprKind::EnumCtor {
                        enum_name: receiver.clone(),
                        variant: name,
                        args: CtorArgs::Unit,
                    },
                    span,
                );
            }
            DottedResolution::None => {}
        }
    }
    // Multi-segment path-style access: `use std.math` (no alias)
    // lets the caller reach items as `std.math.X`. Flatten the
    // `Var("std").Field("math")` chain into the dotted string and
    // check if that names a registered module — only the bare-path
    // import registers itself under the full dotted key (aliased
    // imports register under the alias name instead, so this branch
    // doesn't fire for `use std.math as math` and the alias is the
    // only way in for those). Skip the collapse when the head of the
    // chain shadows a local binding (param / `let`) — `predicate` in
    // a method param shouldn't pick up the sibling-folder `predicate`
    // module.
    if let Some(flat) = flatten_var_dot_chain_expr(&obj) {
        let head = flat.split('.').next().unwrap_or(&flat);
        if !ctx.is_local_shadow(&Symbol::intern(head)) {
            if let Some(canonical) = ctx.modules.get(&Symbol::intern(&flat)) {
                return Expr::new(
                    ExprKind::Var(Symbol::intern(&format!(
                        "{}.{}",
                        canonical.as_str(),
                        name.as_str()
                    ))),
                    span,
                );
            }
        }
    }
    Expr::new(ExprKind::Field { obj: Box::new(obj), name }, span)
}

/// Flatten a `Var` / `Field` chain into a dotted string, matching the
/// `expr::flatten_var_dot_chain` semantics. Used to recognise the
/// multi-segment path-style access (`std.math.X`) introduced by bare
/// `use std.math` imports.
fn flatten_var_dot_chain_expr(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Var(n) => Some(n.as_str().to_string()),
        ExprKind::Field { obj, name } => {
            let base = flatten_var_dot_chain_expr(obj)?;
            Some(format!("{base}.{name}"))
        }
        _ => None,
    }
}

fn rewrite_method_call(
    obj: Box<Expr>,
    method: Symbol,
    args: Box<[Expr]>,
    span: Span,
    ctx: &Ctx,
) -> Expr {
    let obj = rewrite_expr(*obj, ctx);
    let new_args: Vec<Expr> =
        Vec::from(args).into_iter().map(|a| rewrite_expr(a, ctx)).collect();
    if let ExprKind::Var(receiver) = &obj.kind {
        match ctx.resolve_dotted_receiver(receiver, &method) {
            DottedResolution::Qualified(s) => {
                return Expr::new(
                    ExprKind::Call {
                        callee: Symbol::intern(&s),
                        args: new_args.into(),
                    },
                    span,
                );
            }
            DottedResolution::EnumCtor => {
                return Expr::new(
                    ExprKind::EnumCtor {
                        enum_name: receiver.clone(),
                        variant: method,
                        args: CtorArgs::Tuple(new_args.into()),
                    },
                    span,
                );
            }
            DottedResolution::None => {}
        }
    }
    // Multi-segment path-style call: `std.math.sqrt(2.0)` after bare
    // `use std.math`. Mirror the `rewrite_field` lookup — only the
    // bare-path import registers under the full dotted key, so the
    // collapse fires only when no alias was given. Skip when the
    // head of the chain shadows a local binding.
    if let Some(flat) = flatten_var_dot_chain_expr(&obj) {
        let head = flat.split('.').next().unwrap_or(&flat);
        if !ctx.is_local_shadow(&Symbol::intern(head)) {
            if let Some(canonical) = ctx.modules.get(&Symbol::intern(&flat)) {
                return Expr::new(
                    ExprKind::Call {
                        callee: Symbol::intern(&format!(
                            "{}.{}",
                            canonical.as_str(),
                            method.as_str()
                        )),
                        args: new_args.into(),
                    },
                    span,
                );
            }
        }
    }
    Expr::new(
        ExprKind::MethodCall { obj: Box::new(obj), method, args: new_args.into() },
        span,
    )
}
