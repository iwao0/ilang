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

pub fn normalize(prog: Program) -> Program {
    let mut enums: HashSet<String> = BUILTIN_ENUMS.iter().map(|s| s.to_string()).collect();
    for item in &prog.items {
        if let Item::Enum(e) = item {
            enums.insert(e.name.clone());
        }
    }
    let items = prog.items.into_iter().map(|i| rewrite_item(i, &enums)).collect();
    let stmts = prog.stmts.into_iter().map(|s| rewrite_stmt(s, &enums)).collect();
    let tail = prog.tail.map(|e| rewrite_expr(e, &enums));
    Program {
        items,
        stmts,
        tail,
    }
}

fn rewrite_item(item: Item, enums: &HashSet<String>) -> Item {
    match item {
        Item::Fn(mut f) => {
            f.body = rewrite_block(f.body, enums);
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
                    m.body = rewrite_block(body, enums);
                    m
                })
                .collect();
            Item::Class(c)
        }
        Item::Enum(e) => Item::Enum(e),
    }
}

fn rewrite_block(b: Block, enums: &HashSet<String>) -> Block {
    Block {
        stmts: b.stmts.into_iter().map(|s| rewrite_stmt(s, enums)).collect(),
        tail: b.tail.map(|e| Box::new(rewrite_expr(*e, enums))),
    }
}

fn rewrite_stmt(s: Stmt, enums: &HashSet<String>) -> Stmt {
    let kind = match s.kind {
        StmtKind::Let { name, ty, value } => StmtKind::Let {
            name,
            ty,
            value: rewrite_expr(value, enums),
        },
        StmtKind::Expr(e) => StmtKind::Expr(rewrite_expr(e, enums)),
    };
    Stmt { kind, span: s.span }
}

fn rewrite_expr(e: Expr, enums: &HashSet<String>) -> Expr {
    let span = e.span;
    let kind = match e.kind {
        // The two shapes that may need rewriting to EnumCtor.
        ExprKind::Field { obj, name } => {
            if let ExprKind::Var(enum_name) = &obj.kind {
                if enums.contains(enum_name.as_str()) {
                    return Expr::new(
                        ExprKind::EnumCtor {
                            enum_name: enum_name.clone(),
                            variant: name,
                            args: CtorArgs::Unit,
                        },
                        span,
                    );
                }
            }
            ExprKind::Field {
                obj: Box::new(rewrite_expr(*obj, enums)),
                name,
            }
        }
        ExprKind::MethodCall { obj, method, args } => {
            if let ExprKind::Var(enum_name) = &obj.kind {
                if enums.contains(enum_name.as_str()) {
                    let new_args: Vec<Expr> =
                        args.into_iter().map(|a| rewrite_expr(a, enums)).collect();
                    return Expr::new(
                        ExprKind::EnumCtor {
                            enum_name: enum_name.clone(),
                            variant: method,
                            args: CtorArgs::Tuple(new_args),
                        },
                        span,
                    );
                }
            }
            ExprKind::MethodCall {
                obj: Box::new(rewrite_expr(*obj, enums)),
                method,
                args: args.into_iter().map(|a| rewrite_expr(a, enums)).collect(),
            }
        }
        // Recurse through everything else.
        ExprKind::Unary { op, expr } => ExprKind::Unary {
            op,
            expr: Box::new(rewrite_expr(*expr, enums)),
        },
        ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
            op,
            lhs: Box::new(rewrite_expr(*lhs, enums)),
            rhs: Box::new(rewrite_expr(*rhs, enums)),
        },
        ExprKind::Logical { op, lhs, rhs } => ExprKind::Logical {
            op,
            lhs: Box::new(rewrite_expr(*lhs, enums)),
            rhs: Box::new(rewrite_expr(*rhs, enums)),
        },
        ExprKind::Cast { expr, ty } => ExprKind::Cast {
            expr: Box::new(rewrite_expr(*expr, enums)),
            ty,
        },
        ExprKind::FnExpr { params, ret, body } => ExprKind::FnExpr {
            params,
            ret,
            body: rewrite_block(body, enums),
        },
        ExprKind::Call { callee, args } => ExprKind::Call {
            callee,
            args: args.into_iter().map(|a| rewrite_expr(a, enums)).collect(),
        },
        ExprKind::New {
            class,
            type_args,
            args,
        } => ExprKind::New {
            class,
            type_args,
            args: args.into_iter().map(|a| rewrite_expr(a, enums)).collect(),
        },
        ExprKind::Block(b) => ExprKind::Block(rewrite_block(b, enums)),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => ExprKind::If {
            cond: Box::new(rewrite_expr(*cond, enums)),
            then_branch: rewrite_block(then_branch, enums),
            else_branch: else_branch.map(|e| Box::new(rewrite_expr(*e, enums))),
        },
        ExprKind::IfLet {
            name,
            expr,
            then_branch,
            else_branch,
        } => ExprKind::IfLet {
            name,
            expr: Box::new(rewrite_expr(*expr, enums)),
            then_branch: rewrite_block(then_branch, enums),
            else_branch: else_branch.map(|e| Box::new(rewrite_expr(*e, enums))),
        },
        ExprKind::While { cond, body } => ExprKind::While {
            cond: Box::new(rewrite_expr(*cond, enums)),
            body: rewrite_block(body, enums),
        },
        ExprKind::Loop { body } => ExprKind::Loop {
            body: rewrite_block(body, enums),
        },
        ExprKind::ForIn { var, iter, body } => ExprKind::ForIn {
            var,
            iter: Box::new(rewrite_expr(*iter, enums)),
            body: rewrite_block(body, enums),
        },
        ExprKind::Return(opt) => {
            ExprKind::Return(opt.map(|e| Box::new(rewrite_expr(*e, enums))))
        }
        ExprKind::Assign { target, value } => ExprKind::Assign {
            target,
            value: Box::new(rewrite_expr(*value, enums)),
        },
        ExprKind::AssignField { obj, field, value } => ExprKind::AssignField {
            obj: Box::new(rewrite_expr(*obj, enums)),
            field,
            value: Box::new(rewrite_expr(*value, enums)),
        },
        ExprKind::AssignIndex { obj, index, value } => ExprKind::AssignIndex {
            obj: Box::new(rewrite_expr(*obj, enums)),
            index: Box::new(rewrite_expr(*index, enums)),
            value: Box::new(rewrite_expr(*value, enums)),
        },
        ExprKind::Array(items) => {
            ExprKind::Array(items.into_iter().map(|e| rewrite_expr(e, enums)).collect())
        }
        ExprKind::MapLit(entries) => ExprKind::MapLit(
            entries
                .into_iter()
                .map(|(k, v)| (rewrite_expr(k, enums), rewrite_expr(v, enums)))
                .collect(),
        ),
        ExprKind::Index { obj, index } => ExprKind::Index {
            obj: Box::new(rewrite_expr(*obj, enums)),
            index: Box::new(rewrite_expr(*index, enums)),
        },
        ExprKind::Some(inner) => ExprKind::Some(Box::new(rewrite_expr(*inner, enums))),
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
                    es.into_iter().map(|e| rewrite_expr(e, enums)).collect(),
                ),
                CtorArgs::Struct(fs) => CtorArgs::Struct(
                    fs.into_iter()
                        .map(|(n, e)| (n, rewrite_expr(e, enums)))
                        .collect(),
                ),
            },
        },
        ExprKind::Match { scrutinee, arms } => ExprKind::Match {
            scrutinee: Box::new(rewrite_expr(*scrutinee, enums)),
            arms: arms
                .into_iter()
                .map(|arm: MatchArm| MatchArm {
                    pattern: arm.pattern,
                    body: rewrite_expr(arm.body, enums),
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
        | ExprKind::Break
        | ExprKind::Continue) => other,
    };
    let _ = std::any::type_name::<Pattern>(); // silence unused import on Pattern
    Expr { kind, span }
}
