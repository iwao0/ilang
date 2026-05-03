//! Post-typecheck pass that renames overloaded `Item::Fn` declarations
//! to per-overload mangled names and rewrites every `Call` site to
//! match. Downstream stages (loader-merge, monomorphization,
//! interpreter, JIT) then see plain non-overloaded fn names again.
//!
//! Names with only one declaration are left alone — keeps error
//! messages and stack traces readable.

use std::collections::{HashMap, HashSet};

use ilang_ast::{
    Block, ClassDecl, Expr, ExprKind, FieldDecl, FnDecl, Item, Param, Program, Span, Stmt,
    StmtKind, Type, Variant, VariantPayload,
};

/// Mangle an overloaded fn name to `<name>__<param1>_<param2>_...`.
/// Each param-type Display rendering with `<`/`>`/`,`/spaces stripped
/// so the result is a usable identifier.
fn mangled_name(base: &str, params: &[Type]) -> String {
    if params.is_empty() {
        return format!("{base}__");
    }
    let mut s = String::from(base);
    s.push_str("__");
    for (i, p) in params.iter().enumerate() {
        if i > 0 {
            s.push('_');
        }
        let mut t = format!("{p}");
        t = t.replace([' ', '<', '>', ',', '?', '[', ']', '.', '(', ')'], "_");
        s.push_str(&t);
    }
    s
}

/// Apply the mangling pass. `picks[span] = (callee_name, sig_idx)`
/// gives the typechecker's chosen overload for each non-generic call.
pub fn mangle_overloads(
    prog: Program,
    picks: &HashMap<Span, (String, usize)>,
) -> Program {
    // 1. Group Item::Fn entries by source name to see which ones are
    //    actually overloaded.
    let mut fn_indices: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, item) in prog.items.iter().enumerate() {
        if let Item::Fn(f) = item {
            fn_indices.entry(f.name.clone()).or_default().push(i);
        }
    }
    // Names with exactly one declaration: no mangling needed.
    let overloaded: HashSet<String> = fn_indices
        .iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|(k, _)| k.clone())
        .collect();
    if overloaded.is_empty() {
        return prog;
    }

    // 2. Build a per-(name, sig_idx) → mangled-name table. The
    //    typechecker registers fns in program order, so sig_idx N
    //    corresponds to the N-th Item::Fn with that name.
    let mut new_names: HashMap<(String, usize), String> = HashMap::new();
    // Also build a parallel item-position → new-name map so the
    // rewrite_item pass below can find the right new name without
    // re-deriving sig_idx from item position.
    let mut item_new_name: HashMap<usize, String> = HashMap::new();
    for name in &overloaded {
        for (idx, item_pos) in fn_indices[name].iter().enumerate() {
            if let Item::Fn(f) = &prog.items[*item_pos] {
                let mangled = mangled_name(name, &param_types(f));
                new_names.insert((name.clone(), idx), mangled.clone());
                item_new_name.insert(*item_pos, mangled);
            }
        }
    }

    // 3. Rewrite Items: rename matching FnDecls; recurse into other
    //    items' bodies to rewrite Calls.
    let new_items: Vec<Item> = prog
        .items
        .into_iter()
        .enumerate()
        .map(|(i, item)| rewrite_item(i, item, &item_new_name, &overloaded, &new_names, picks))
        .collect();
    let new_stmts: Vec<Stmt> = prog
        .stmts
        .into_iter()
        .map(|s| rewrite_stmt(s, &overloaded, &new_names, picks))
        .collect();
    let new_tail = prog.tail.map(|e| rewrite_expr(e, &overloaded, &new_names, picks));

    Program {
        items: new_items,
        stmts: new_stmts,
        tail: new_tail,
    }
}

fn param_types(f: &FnDecl) -> Vec<Type> {
    f.params.iter().map(|p| p.ty.clone()).collect()
}

fn rewrite_item(
    item_pos: usize,
    item: Item,
    item_new_name: &HashMap<usize, String>,
    overloaded: &HashSet<String>,
    new_names: &HashMap<(String, usize), String>,
    picks: &HashMap<Span, (String, usize)>,
) -> Item {
    match item {
        Item::Fn(mut f) => {
            if let Some(mangled) = item_new_name.get(&item_pos) {
                f.name = mangled.clone();
            }
            f.body = rewrite_block(f.body, overloaded, new_names, picks);
            Item::Fn(f)
        }
        Item::Class(mut c) => {
            for m in &mut c.methods {
                let body = std::mem::replace(
                    &mut m.body,
                    Block { stmts: Vec::new(), tail: None },
                );
                m.body = rewrite_block(body, overloaded, new_names, picks);
            }
            Item::Class(c)
        }
        Item::Enum(e) => Item::Enum(e),
        Item::Use(u) => Item::Use(u),
        Item::Const(c) => Item::Const(c),
    }
}

fn rewrite_block(
    b: Block,
    overloaded: &HashSet<String>,
    new_names: &HashMap<(String, usize), String>,
    picks: &HashMap<Span, (String, usize)>,
) -> Block {
    Block {
        stmts: b.stmts.into_iter().map(|s| rewrite_stmt(s, overloaded, new_names, picks)).collect(),
        tail: b.tail.map(|e| Box::new(rewrite_expr(*e, overloaded, new_names, picks))),
    }
}

fn rewrite_stmt(
    s: Stmt,
    overloaded: &HashSet<String>,
    new_names: &HashMap<(String, usize), String>,
    picks: &HashMap<Span, (String, usize)>,
) -> Stmt {
    let kind = match s.kind {
        StmtKind::Let { name, ty, value } => StmtKind::Let {
            name,
            ty,
            value: rewrite_expr(value, overloaded, new_names, picks),
        },
        StmtKind::Expr(e) => StmtKind::Expr(rewrite_expr(e, overloaded, new_names, picks)),
    };
    Stmt { kind, span: s.span }
}

fn rewrite_expr(
    e: Expr,
    overloaded: &HashSet<String>,
    new_names: &HashMap<(String, usize), String>,
    picks: &HashMap<Span, (String, usize)>,
) -> Expr {
    let span = e.span;
    let kind = match e.kind {
        ExprKind::Call { callee, args } => {
            let new_callee = if overloaded.contains(&callee) {
                if let Some((name, idx)) = picks.get(&span) {
                    if name == &callee {
                        new_names
                            .get(&(callee.clone(), *idx))
                            .cloned()
                            .unwrap_or(callee)
                    } else {
                        callee
                    }
                } else {
                    callee
                }
            } else {
                callee
            };
            ExprKind::Call {
                callee: new_callee,
                args: args.into_iter().map(|a| rewrite_expr(a, overloaded, new_names, picks)).collect(),
            }
        }
        // Mechanical recursion through every other expression shape.
        ExprKind::Int(n) => ExprKind::Int(n),
        ExprKind::Float(x) => ExprKind::Float(x),
        ExprKind::Bool(b) => ExprKind::Bool(b),
        ExprKind::Str(s) => ExprKind::Str(s),
        ExprKind::Var(n) => ExprKind::Var(n),
        ExprKind::This => ExprKind::This,
        ExprKind::None => ExprKind::None,
        ExprKind::Break => ExprKind::Break,
        ExprKind::Continue => ExprKind::Continue,
        ExprKind::Some(x) => ExprKind::Some(Box::new(rewrite_expr(*x, overloaded, new_names, picks))),
        ExprKind::Unary { op, expr } => ExprKind::Unary {
            op,
            expr: Box::new(rewrite_expr(*expr, overloaded, new_names, picks)),
        },
        ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
            op,
            lhs: Box::new(rewrite_expr(*lhs, overloaded, new_names, picks)),
            rhs: Box::new(rewrite_expr(*rhs, overloaded, new_names, picks)),
        },
        ExprKind::Logical { op, lhs, rhs } => ExprKind::Logical {
            op,
            lhs: Box::new(rewrite_expr(*lhs, overloaded, new_names, picks)),
            rhs: Box::new(rewrite_expr(*rhs, overloaded, new_names, picks)),
        },
        ExprKind::Cast { expr, ty } => ExprKind::Cast {
            expr: Box::new(rewrite_expr(*expr, overloaded, new_names, picks)),
            ty,
        },
        ExprKind::FnExpr { params, ret, body } => ExprKind::FnExpr {
            params,
            ret,
            body: rewrite_block(body, overloaded, new_names, picks),
        },
        ExprKind::Field { obj, name } => ExprKind::Field {
            obj: Box::new(rewrite_expr(*obj, overloaded, new_names, picks)),
            name,
        },
        ExprKind::MethodCall { obj, method, args } => ExprKind::MethodCall {
            obj: Box::new(rewrite_expr(*obj, overloaded, new_names, picks)),
            method,
            args: args.into_iter().map(|a| rewrite_expr(a, overloaded, new_names, picks)).collect(),
        },
        ExprKind::New { class, type_args, args } => ExprKind::New {
            class,
            type_args,
            args: args.into_iter().map(|a| rewrite_expr(a, overloaded, new_names, picks)).collect(),
        },
        ExprKind::Block(b) => ExprKind::Block(rewrite_block(b, overloaded, new_names, picks)),
        ExprKind::If { cond, then_branch, else_branch } => ExprKind::If {
            cond: Box::new(rewrite_expr(*cond, overloaded, new_names, picks)),
            then_branch: rewrite_block(then_branch, overloaded, new_names, picks),
            else_branch: else_branch.map(|e| Box::new(rewrite_expr(*e, overloaded, new_names, picks))),
        },
        ExprKind::IfLet { name, expr, then_branch, else_branch } => ExprKind::IfLet {
            name,
            expr: Box::new(rewrite_expr(*expr, overloaded, new_names, picks)),
            then_branch: rewrite_block(then_branch, overloaded, new_names, picks),
            else_branch: else_branch.map(|e| Box::new(rewrite_expr(*e, overloaded, new_names, picks))),
        },
        ExprKind::While { cond, body } => ExprKind::While {
            cond: Box::new(rewrite_expr(*cond, overloaded, new_names, picks)),
            body: rewrite_block(body, overloaded, new_names, picks),
        },
        ExprKind::Loop { body } => ExprKind::Loop {
            body: rewrite_block(body, overloaded, new_names, picks),
        },
        ExprKind::ForIn { var, iter, body } => ExprKind::ForIn {
            var,
            iter: Box::new(rewrite_expr(*iter, overloaded, new_names, picks)),
            body: rewrite_block(body, overloaded, new_names, picks),
        },
        ExprKind::Return(opt) => ExprKind::Return(
            opt.map(|e| Box::new(rewrite_expr(*e, overloaded, new_names, picks))),
        ),
        ExprKind::Assign { target, value } => ExprKind::Assign {
            target,
            value: Box::new(rewrite_expr(*value, overloaded, new_names, picks)),
        },
        ExprKind::AssignField { obj, field, value } => ExprKind::AssignField {
            obj,
            field,
            value: Box::new(rewrite_expr(*value, overloaded, new_names, picks)),
        },
        ExprKind::AssignIndex { obj, index, value } => ExprKind::AssignIndex {
            obj,
            index,
            value: Box::new(rewrite_expr(*value, overloaded, new_names, picks)),
        },
        ExprKind::Array(items) => ExprKind::Array(
            items.into_iter().map(|e| rewrite_expr(e, overloaded, new_names, picks)).collect(),
        ),
        ExprKind::MapLit(entries) => ExprKind::MapLit(
            entries
                .into_iter()
                .map(|(k, v)| (
                    rewrite_expr(k, overloaded, new_names, picks),
                    rewrite_expr(v, overloaded, new_names, picks),
                ))
                .collect(),
        ),
        ExprKind::Index { obj, index } => ExprKind::Index {
            obj: Box::new(rewrite_expr(*obj, overloaded, new_names, picks)),
            index: Box::new(rewrite_expr(*index, overloaded, new_names, picks)),
        },
        ExprKind::EnumCtor { enum_name, variant, args } => ExprKind::EnumCtor {
            enum_name,
            variant,
            args: match args {
                ilang_ast::CtorArgs::Unit => ilang_ast::CtorArgs::Unit,
                ilang_ast::CtorArgs::Tuple(es) => ilang_ast::CtorArgs::Tuple(
                    es.into_iter().map(|e| rewrite_expr(e, overloaded, new_names, picks)).collect(),
                ),
                ilang_ast::CtorArgs::Struct(fs) => ilang_ast::CtorArgs::Struct(
                    fs.into_iter().map(|(n, e)| (n, rewrite_expr(e, overloaded, new_names, picks))).collect(),
                ),
            },
        },
        ExprKind::Match { scrutinee, arms } => ExprKind::Match {
            scrutinee: Box::new(rewrite_expr(*scrutinee, overloaded, new_names, picks)),
            arms: arms
                .into_iter()
                .map(|arm| ilang_ast::MatchArm {
                    pattern: arm.pattern,
                    body: rewrite_expr(arm.body, overloaded, new_names, picks),
                    span: arm.span,
                })
                .collect(),
        },
    };
    Expr { kind, span }
}

// Unused imports for now — keep for the inevitable extension to
// methods/struct rewrites.
#[allow(dead_code)]
type _Unused = (ClassDecl, FieldDecl, Param, Variant, VariantPayload);
