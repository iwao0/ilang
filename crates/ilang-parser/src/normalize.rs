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
    Block, CtorArgs, Expr, ExprKind, Item, MatchArm, Program, Stmt, StmtKind, Symbol,
};

/// Built-in enum names that are always available.
const BUILTIN_ENUMS: &[&str] = &["Result"];

#[derive(Default)]
struct Ctx {
    /// All names that resolve as enums after the program is fully
    /// loaded (built-ins + every `Item::Enum`'s name).
    enums: HashSet<Symbol>,
    /// Names that come from `use module` (whole-module imports).
    /// References like `module.foo` get rewritten to qualified
    /// `Var("module.foo")` (or `Call("module.foo", ...)`); the loader
    /// will have produced top-level items with those exact names.
    modules: HashSet<Symbol>,
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
    let items: Vec<Item> = prog.items.into_iter().map(|i| rewrite_item(i, &ctx)).collect();
    let stmts: Vec<Stmt> = prog.stmts.into_iter().map(|s| rewrite_stmt(s, &ctx)).collect();
    let tail = prog.tail.map(|e| rewrite_expr(e, &ctx));
    Program {
        items,
        stmts,
        tail,
    }
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
        Item::ExternStatic(s) => Item::ExternStatic(s),
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
                if ctx.modules.contains(&Symbol::intern(receiver.as_str())) {
                    return Expr::new(
                        ExprKind::Var(Symbol::intern(&format!("{receiver}.{name}"))),
                        span,
                    );
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
                if ctx.modules.contains(&Symbol::intern(receiver.as_str())) {
                    return Expr::new(
                        ExprKind::Call {
                            callee: Symbol::intern(&format!("{receiver}.{method}")),
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
            start: Box::new(rewrite_expr(*start, ctx)),
            end: Box::new(rewrite_expr(*end, ctx)),
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
            ExprKind::Array(Vec::from(items).into_iter().map(|e| rewrite_expr(e, ctx)).collect())
        }
        ExprKind::Tuple(items) => {
            ExprKind::Tuple(Vec::from(items).into_iter().map(|e| rewrite_expr(e, ctx)).collect())
        }
        // Struct literal: `Foo { a: 1, b: 2 }` desugars to a block
        // `{ let __sl = new Foo(); __sl.a = 1; __sl.b = 2; __sl }`.
        // The temp name embeds the source position so nested struct
        // literals don't collide.
        ExprKind::StructLit { class, fields } => {
            let tmp: Symbol = format!("__struct_lit_{}_{}", span.line, span.col).into();
            let mut stmts: Vec<ilang_ast::Stmt> = Vec::with_capacity(fields.len() + 1);
            stmts.push(ilang_ast::Stmt {
                kind: ilang_ast::StmtKind::Let {
                    name: tmp,
                    ty: None,
                    value: Expr::new(
                        ExprKind::New {
                            class,
                            type_args: Box::new([]),
                            args: Box::new([]),
                            init_method: None,
                        },
                        span,
                    ),
                },
                span,
            });
            for (fname, fval) in fields {
                let assign = Expr::new(
                    ExprKind::AssignField {
                        obj: Box::new(Expr::new(ExprKind::Var(tmp), span)),
                        field: fname,
                        value: Box::new(rewrite_expr(fval, ctx)),
                    },
                    span,
                );
                stmts.push(ilang_ast::Stmt {
                    kind: ilang_ast::StmtKind::Expr(assign),
                    span,
                });
            }
            return Expr::new(
                ExprKind::Block(ilang_ast::Block {
                    stmts: stmts.into(),
                    tail: Some(Box::new(Expr::new(ExprKind::Var(tmp), span))),
                }),
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
