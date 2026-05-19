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
    Block, CtorArgs, Expr, ExprKind, Item, MatchArm, Program, Stmt, StmtKind, Symbol, UseAlias,
};

use crate::error::ParseError;

pub mod async_desugar;
mod dealias;
pub(crate) mod state_machine;
mod validate;

use dealias::dealias_program;
use validate::validate_program;

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
                    ctx.modules.insert(u.module.clone(), u.module.clone());
                }
                UseAlias::Named(name) => {
                    ctx.modules.insert(name.clone(), u.module.clone());
                }
                UseAlias::Discard => {}
            },
            _ => {}
        }
    }
    ctx
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
            f.body = rewrite_block(f.body, ctx);
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
                    m.body = rewrite_block(body, ctx);
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
                    m.body = rewrite_block(body, ctx);
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
                        g.body = rewrite_block(body, ctx);
                    }
                    if let Some(s) = p.setter.as_mut() {
                        rewrite_params(&mut s.params, ctx);
                        let body = std::mem::replace(
                            &mut s.body,
                            Block { stmts: Vec::new(), tail: None },
                        );
                        s.body = rewrite_block(body, ctx);
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
                        f.body = rewrite_block(body, ctx);
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
                                m.body = rewrite_block(body, ctx);
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
                                m.body = rewrite_block(body, ctx);
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
                                    g.body = rewrite_block(body, ctx);
                                }
                                if let Some(s) = p.setter.as_mut() {
                                    rewrite_params(&mut s.params, ctx);
                                    let body = std::mem::replace(
                                        &mut s.body,
                                        Block { stmts: Vec::new(), tail: None },
                                    );
                                    s.body = rewrite_block(body, ctx);
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
    Block {
        stmts: Vec::from(b.stmts).into_iter().map(|s| rewrite_stmt(s, ctx)).collect(),
        tail: b.tail.map(|e| Box::new(rewrite_expr(*e, ctx))),
    }
}

fn rewrite_stmt(s: Stmt, ctx: &Ctx) -> Stmt {
    let kind = match s.kind {
        StmtKind::Let { is_pub, is_const, name, ty, value } => StmtKind::Let {
            is_pub,
            is_const,
            name,
            ty,
            value: rewrite_expr(value, ctx),
        },
        StmtKind::LetTuple { elems, value } => StmtKind::LetTuple {
            elems,
            value: rewrite_expr(value, ctx),
        },
        StmtKind::LetStruct { class, fields, value } => StmtKind::LetStruct {
            class,
            fields,
            value: rewrite_expr(value, ctx),
        },
        StmtKind::Expr(e) => StmtKind::Expr(rewrite_expr(e, ctx)),
    };
    Stmt { kind, span: s.span, source_module: s.source_module.clone() }
}

fn rewrite_expr(e: Expr, ctx: &Ctx) -> Expr {
    let span = e.span;
    let kind = match e.kind {
        // Recurse first so any nested `module.Enum` chain in `obj`
        // collapses (`Field(Field(Var("utils"), "Color"), "red")` →
        // `Field(Var("utils.Color"), "red")`) before the enum-ctor
        // check. Module-name bumping is itself a Field rewrite below.
        ExprKind::Field { obj, name } => {
            let obj = rewrite_expr(*obj, ctx);
            // Whole-module reference: `module.X` collapses to a
            // qualified `Var("module.X")` so the loader-merged
            // top-level item with that exact name is found.
            if let ExprKind::Var(receiver) = &obj.kind {
                if let Some(canonical) =
                    ctx.modules.get(&Symbol::intern(receiver.as_str()))
                {
                    return Expr::new(
                        ExprKind::Var(Symbol::intern(&format!("{}.{name}", canonical.as_str()))),
                        span,
                    );
                }
                // Namespaced re-export path: `umbrella.inner.X`. The
                // first level (`umbrella.inner`) already collapsed
                // via the rule above; this branch only fires in
                // `renormalize_merged` (where `ctx.items` is
                // populated). The disambiguator between
                // "sub-module field" and "enum-variant access" is
                // whether the fully-qualified name appears in the
                // merged item set.
                let qualified = format!("{}.{name}", receiver.as_str());
                if !ctx.items.is_empty()
                    && ctx.items.contains(&Symbol::intern(&qualified))
                {
                    return Expr::new(ExprKind::Var(Symbol::intern(&qualified)), span);
                }
                // Existing rule: enum unit ctor.
                if ctx.enums.contains(&Symbol::intern(receiver.as_str())) {
                    return Expr::new(
                        ExprKind::EnumCtor {
                            enum_name: receiver.clone(),
                            variant: name,
                            args: CtorArgs::Unit,
                        },
                        span,
                    );
                }
            }
            ExprKind::Field {
                obj: Box::new(obj),
                name,
            }
        }
        ExprKind::MethodCall { obj, method, args } => {
            let obj = rewrite_expr(*obj, ctx);
            let new_args: Vec<Expr> =
                Vec::from(args).into_iter().map(|a| rewrite_expr(a, ctx)).collect();
            if let ExprKind::Var(receiver) = &obj.kind {
                // Whole-module function call: `module.foo(args)`
                // becomes `Call("module.foo", args)`.
                if let Some(canonical) =
                    ctx.modules.get(&Symbol::intern(receiver.as_str()))
                {
                    return Expr::new(
                        ExprKind::Call {
                            callee: Symbol::intern(&format!(
                                "{}.{method}",
                                canonical.as_str()
                            )),
                            args: new_args.into(),
                        },
                        span,
                    );
                }
                // Namespaced re-export path: `umbrella.inner.fn(args)`.
                // Only fires in `renormalize_merged` — disambiguated
                // by checking the merged item set rather than the
                // per-file module table.
                let qualified = format!("{}.{method}", receiver.as_str());
                if !ctx.items.is_empty()
                    && ctx.items.contains(&Symbol::intern(&qualified))
                {
                    return Expr::new(
                        ExprKind::Call {
                            callee: Symbol::intern(&qualified),
                            args: new_args.into(),
                        },
                        span,
                    );
                }
                if ctx.enums.contains(&Symbol::intern(receiver.as_str())) {
                    return Expr::new(
                        ExprKind::EnumCtor {
                            enum_name: receiver.clone(),
                            variant: method,
                            args: CtorArgs::Tuple(new_args.into()),
                        },
                        span,
                    );
                }
            }
            ExprKind::MethodCall {
                obj: Box::new(obj),
                method,
                args: new_args.into(),
            }
        }
        // Recurse through everything else.
        ExprKind::Unary { op, expr } => ExprKind::Unary {
            op,
            expr: Box::new(rewrite_expr(*expr, ctx)),
        },
        ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
            op,
            lhs: Box::new(rewrite_expr(*lhs, ctx)),
            rhs: Box::new(rewrite_expr(*rhs, ctx)),
        },
        ExprKind::Logical { op, lhs, rhs } => ExprKind::Logical {
            op,
            lhs: Box::new(rewrite_expr(*lhs, ctx)),
            rhs: Box::new(rewrite_expr(*rhs, ctx)),
        },
        ExprKind::Cast { expr, ty } => ExprKind::Cast {
            expr: Box::new(rewrite_expr(*expr, ctx)),
            ty,
        },
        ExprKind::TypeTest { expr, ty } => ExprKind::TypeTest {
            expr: Box::new(rewrite_expr(*expr, ctx)),
            ty,
        },
        ExprKind::TypeDowncast { expr, ty } => ExprKind::TypeDowncast {
            expr: Box::new(rewrite_expr(*expr, ctx)),
            ty,
        },
        ExprKind::FnExpr { params, ret, body } => ExprKind::FnExpr {
            params,
            ret,
            body: rewrite_block(body, ctx),
        },
        ExprKind::Call { callee, args } => ExprKind::Call {
            callee,
            args: Vec::from(args).into_iter().map(|a| rewrite_expr(a, ctx)).collect(),
        },
        ExprKind::New { class, type_args, args, init_method } => ExprKind::New {
            class,
            type_args,
            args: Vec::from(args).into_iter().map(|a| rewrite_expr(a, ctx)).collect(),
            init_method,
        },
        ExprKind::Block(b) => ExprKind::Block(rewrite_block(b, ctx)),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => ExprKind::If {
            cond: Box::new(rewrite_expr(*cond, ctx)),
            then_branch: rewrite_block(then_branch, ctx),
            else_branch: else_branch.map(|e| Box::new(rewrite_expr(*e, ctx))),
        },
        ExprKind::IfLet {
            name,
            expr,
            then_branch,
            else_branch,
        } => ExprKind::IfLet {
            name,
            expr: Box::new(rewrite_expr(*expr, ctx)),
            then_branch: rewrite_block(then_branch, ctx),
            else_branch: else_branch.map(|e| Box::new(rewrite_expr(*e, ctx))),
        },
        ExprKind::While { cond, body } => ExprKind::While {
            cond: Box::new(rewrite_expr(*cond, ctx)),
            body: rewrite_block(body, ctx),
        },
        ExprKind::Loop { body } => ExprKind::Loop {
            body: rewrite_block(body, ctx),
        },
        ExprKind::ForIn { var, iter, body } => ExprKind::ForIn {
            var,
            iter: Box::new(rewrite_expr(*iter, ctx)),
            body: rewrite_block(body, ctx),
        },
        ExprKind::Range { start, end, inclusive } => ExprKind::Range {
            start: start.map(|s| Box::new(rewrite_expr(*s, ctx))),
            end: end.map(|e| Box::new(rewrite_expr(*e, ctx))),
            inclusive,
        },
        ExprKind::Closure { fn_name, captures } => {
            ExprKind::Closure { fn_name, captures }
        }
        ExprKind::SuperCall { method, args } => ExprKind::SuperCall {
            method,
            args: Vec::from(args).into_iter().map(|a| rewrite_expr(a, ctx)).collect(),
        },
        ExprKind::Return(opt) => {
            ExprKind::Return(opt.map(|e| Box::new(rewrite_expr(*e, ctx))))
        }
        ExprKind::Break(opt) => {
            ExprKind::Break(opt.map(|e| Box::new(rewrite_expr(*e, ctx))))
        }
        ExprKind::Assign { target, value } => ExprKind::Assign {
            target,
            value: Box::new(rewrite_expr(*value, ctx)),
        },
        ExprKind::AssignField { obj, field, value, is_init } => ExprKind::AssignField {
            obj: Box::new(rewrite_expr(*obj, ctx)),
            field,
            value: Box::new(rewrite_expr(*value, ctx)),
            is_init,
        },
        ExprKind::AssignIndex { obj, index, value } => ExprKind::AssignIndex {
            obj: Box::new(rewrite_expr(*obj, ctx)),
            index: Box::new(rewrite_expr(*index, ctx)),
            value: Box::new(rewrite_expr(*value, ctx)),
        },
        ExprKind::Array(items) => {
            ExprKind::Array(Vec::from(items).into_iter().map(|e| rewrite_expr(e, ctx)).collect())
        }
        ExprKind::Tuple(items) => {
            ExprKind::Tuple(Vec::from(items).into_iter().map(|e| rewrite_expr(e, ctx)).collect())
        }
        // Struct literal: leave the `StructLit` node intact and just
        // recurse into the field expressions. Validation (CRepr
        // structs require every declared field; `class` literals are
        // already rejected by the type checker) and the actual
        // construction (NewObject + StoreField sequence) happen in
        // the type checker / MIR lower respectively. Keeping the
        // node alive past normalize is what lets those passes see
        // the full literal — including which field names the author
        // wrote — instead of an already-desugared `__sl.x = ...`
        // sequence that loses the "this was a struct literal" intent.
        ExprKind::StructLit { class, fields } => {
            return Expr::new(
                ExprKind::StructLit {
                    class,
                    fields: fields
                        .into_iter()
                        .map(|(n, e)| (n, rewrite_expr(e, ctx)))
                        .collect(),
                },
                span,
            );
        }
        ExprKind::MapLit(entries) => ExprKind::MapLit(
            entries
                .into_iter()
                .map(|(k, v)| (rewrite_expr(k, ctx), rewrite_expr(v, ctx)))
                .collect(),
        ),
        ExprKind::Index { obj, index } => ExprKind::Index {
            obj: Box::new(rewrite_expr(*obj, ctx)),
            index: Box::new(rewrite_expr(*index, ctx)),
        },
        ExprKind::Some(inner) => ExprKind::Some(Box::new(rewrite_expr(*inner, ctx))),
        ExprKind::Await(inner) => ExprKind::Await(Box::new(rewrite_expr(*inner, ctx))),
        ExprKind::EnumCtor {
            enum_name,
            variant,
            args,
        } => ExprKind::EnumCtor {
            enum_name,
            variant,
            args: match args {
                CtorArgs::Unit => CtorArgs::Unit,
                CtorArgs::Tuple(es) => CtorArgs::Tuple(
                    Vec::from(es).into_iter().map(|e| rewrite_expr(e, ctx)).collect(),
                ),
                CtorArgs::Struct(fs) => CtorArgs::Struct(
                    fs.into_iter()
                        .map(|(n, e)| (n, rewrite_expr(e, ctx)))
                        .collect(),
                ),
            },
        },
        ExprKind::Match { scrutinee, arms } => ExprKind::Match {
            scrutinee: Box::new(rewrite_expr(*scrutinee, ctx)),
            arms: arms
                .into_iter()
                .map(|arm: MatchArm| MatchArm {
                    pattern: arm.pattern,
                    body: rewrite_expr(arm.body, ctx),
                    span: arm.span,
                })
                .collect(),
        },
        // Trivial nodes
        other @ (ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::Var(_)
        | ExprKind::This
        | ExprKind::None
        | ExprKind::Continue) => other,
    };
    Expr { kind, span }
}
