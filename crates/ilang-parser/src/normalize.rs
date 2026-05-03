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

use std::collections::HashSet;

use ilang_ast::{
    Block, CtorArgs, Expr, ExprKind, Item, MatchArm, Pattern, Program, Stmt, StmtKind,
};

/// Built-in enum names that are always available.
const BUILTIN_ENUMS: &[&str] = &["Result"];

#[derive(Default)]
struct Ctx {
    /// All names that resolve as enums after the program is fully
    /// loaded (built-ins + every `Item::Enum`'s name).
    enums: HashSet<String>,
    /// Names that come from `use module` (whole-module imports).
    /// References like `module.foo` get rewritten to qualified
    /// `Var("module.foo")` (or `Call("module.foo", ...)`); the loader
    /// will have produced top-level items with those exact names.
    modules: HashSet<String>,
}

pub fn normalize(prog: Program) -> Program {
    let mut ctx = Ctx::default();
    for s in BUILTIN_ENUMS {
        ctx.enums.insert((*s).into());
    }
    for item in &prog.items {
        match item {
            Item::Enum(e) => {
                ctx.enums.insert(e.name.clone());
            }
            Item::Use(u) if u.selective.is_none() => {
                ctx.modules.insert(u.module.clone());
            }
            _ => {}
        }
    }
    let items = prog.items.into_iter().map(|i| rewrite_item(i, &ctx)).collect();
    let stmts = prog.stmts.into_iter().map(|s| rewrite_stmt(s, &ctx)).collect();
    let tail = prog.tail.map(|e| rewrite_expr(e, &ctx));
    Program {
        items,
        stmts,
        tail,
    }
}

fn rewrite_item(item: Item, ctx: &Ctx) -> Item {
    match item {
        Item::Fn(mut f) => {
            f.body = rewrite_block(f.body, ctx);
            Item::Fn(f)
        }
        Item::Class(mut c) => {
            let methods = std::mem::take(&mut c.methods);
            c.methods = methods
                .into_iter()
                .map(|mut m| {
                    let body = std::mem::replace(
                        &mut m.body,
                        Block { stmts: Vec::new(), tail: None },
                    );
                    m.body = rewrite_block(body, ctx);
                    m
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
        Item::Const(c) => Item::Const(c),
    }
}

fn rewrite_block(b: Block, ctx: &Ctx) -> Block {
    Block {
        stmts: b.stmts.into_iter().map(|s| rewrite_stmt(s, ctx)).collect(),
        tail: b.tail.map(|e| Box::new(rewrite_expr(*e, ctx))),
    }
}

fn rewrite_stmt(s: Stmt, ctx: &Ctx) -> Stmt {
    let kind = match s.kind {
        StmtKind::Let { name, ty, value } => StmtKind::Let {
            name,
            ty,
            value: rewrite_expr(value, ctx),
        },
        StmtKind::Expr(e) => StmtKind::Expr(rewrite_expr(e, ctx)),
    };
    Stmt { kind, span: s.span }
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
                if ctx.modules.contains(receiver.as_str()) {
                    return Expr::new(
                        ExprKind::Var(format!("{receiver}.{name}")),
                        span,
                    );
                }
                // Existing rule: enum unit ctor.
                if ctx.enums.contains(receiver.as_str()) {
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
                args.into_iter().map(|a| rewrite_expr(a, ctx)).collect();
            if let ExprKind::Var(receiver) = &obj.kind {
                // Whole-module function call: `module.foo(args)`
                // becomes `Call("module.foo", args)`.
                if ctx.modules.contains(receiver.as_str()) {
                    return Expr::new(
                        ExprKind::Call {
                            callee: format!("{receiver}.{method}"),
                            args: new_args,
                        },
                        span,
                    );
                }
                if ctx.enums.contains(receiver.as_str()) {
                    return Expr::new(
                        ExprKind::EnumCtor {
                            enum_name: receiver.clone(),
                            variant: method,
                            args: CtorArgs::Tuple(new_args),
                        },
                        span,
                    );
                }
            }
            ExprKind::MethodCall {
                obj: Box::new(obj),
                method,
                args: new_args,
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
        ExprKind::FnExpr { params, ret, body } => ExprKind::FnExpr {
            params,
            ret,
            body: rewrite_block(body, ctx),
        },
        ExprKind::Call { callee, args } => ExprKind::Call {
            callee,
            args: args.into_iter().map(|a| rewrite_expr(a, ctx)).collect(),
        },
        ExprKind::New { class, type_args, args, init_method } => ExprKind::New {
            class,
            type_args,
            args: args.into_iter().map(|a| rewrite_expr(a, ctx)).collect(),
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
            start: Box::new(rewrite_expr(*start, ctx)),
            end: Box::new(rewrite_expr(*end, ctx)),
            inclusive,
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
        ExprKind::AssignField { obj, field, value } => ExprKind::AssignField {
            obj: Box::new(rewrite_expr(*obj, ctx)),
            field,
            value: Box::new(rewrite_expr(*value, ctx)),
        },
        ExprKind::AssignIndex { obj, index, value } => ExprKind::AssignIndex {
            obj: Box::new(rewrite_expr(*obj, ctx)),
            index: Box::new(rewrite_expr(*index, ctx)),
            value: Box::new(rewrite_expr(*value, ctx)),
        },
        ExprKind::Array(items) => {
            ExprKind::Array(items.into_iter().map(|e| rewrite_expr(e, ctx)).collect())
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
                    es.into_iter().map(|e| rewrite_expr(e, ctx)).collect(),
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
    let _ = std::any::type_name::<Pattern>(); // silence unused import on Pattern
    Expr { kind, span }
}
