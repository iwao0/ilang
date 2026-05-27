//! Pre-prefix pass that rewrites bare references to top-level
//! module items into their `<module>.X` qualified form, so the
//! later `prefix_item` walk doesn't mangle module-private locals
//! or static fns it can't otherwise distinguish. `consts` lists
//! every top-level name (const / class / fn / `let`) the module
//! exposes — only names in that set get qualified, leaving
//! locally-bound closures (`let f = …; f(v)`) alone.

use std::collections::HashSet;

use ilang_ast::{Block, ClassDecl, Expr, ExprKind, Item, Stmt, StmtKind, Symbol};

use super::builtin::is_builtin_callee;

/// Rewrite bare `Var("X")` → `Var("prefix.X")` inside an item's
/// expression nodes, but only when `X` is in `consts`. Used as a
/// pre-pass before module prefixing so module-level const refs
/// from fn / method / `@extern(C)` bodies survive into
/// `inline_constants` with names that match the prefixed const
/// declaration.
pub(super) fn qualify_var_refs_in_item(
    item: &mut Item,
    prefix: &str,
    consts: &HashSet<Symbol>,
) {
    match item {
        Item::Fn(f) => qualify_var_refs_in_block(&mut f.body, prefix, consts),
        Item::Class(c) => qualify_var_refs_in_class(c, prefix, consts),
        Item::ExternC(b) => {
            for inner in b.items.iter_mut() {
                match inner {
                    ilang_ast::ExternCItem::FnDef(f) => {
                        qualify_var_refs_in_block(&mut f.body, prefix, consts);
                    }
                    ilang_ast::ExternCItem::Class(c) => {
                        qualify_var_refs_in_class(c, prefix, consts);
                    }
                    _ => {}
                }
            }
        }
        // `const NAME = expr` whose RHS contains a bare reference to
        // a same-module class / fn / let needs the same qualification
        // as fn bodies, otherwise `inline_constants`'s fold-fail
        // demotion produces a runtime stmt whose `NSObject.wrap(0)`
        // can't resolve once the class has been renamed to
        // `module.NSObject`.
        Item::Const(c) => qualify_var_refs_in_expr(&mut c.value, prefix, consts),
        _ => {}
    }
}

fn qualify_var_refs_in_class(c: &mut ClassDecl, prefix: &str, consts: &HashSet<Symbol>) {
    for m in c.methods.iter_mut().chain(c.static_methods.iter_mut()) {
        qualify_var_refs_in_block(&mut m.body, prefix, consts);
    }
    for prop in c.properties.iter_mut() {
        if let Some(g) = prop.getter.as_mut() {
            qualify_var_refs_in_block(&mut g.body, prefix, consts);
        }
        if let Some(s) = prop.setter.as_mut() {
            qualify_var_refs_in_block(&mut s.body, prefix, consts);
        }
    }
    for sf in c.static_fields.iter_mut() {
        qualify_var_refs_in_expr(&mut sf.value, prefix, consts);
    }
}

fn qualify_var_refs_in_block(b: &mut Block, prefix: &str, consts: &HashSet<Symbol>) {
    for s in b.stmts.iter_mut() {
        qualify_var_refs_in_stmt(s, prefix, consts);
    }
    if let Some(t) = b.tail.as_mut() {
        qualify_var_refs_in_expr(t, prefix, consts);
    }
}

pub(super) fn qualify_var_refs_in_stmt(s: &mut Stmt, prefix: &str, consts: &HashSet<Symbol>) {
    match &mut s.kind {
        StmtKind::Let { value, .. }
        | StmtKind::LetTuple { value, .. }
        | StmtKind::LetStruct { value, .. } => {
            qualify_var_refs_in_expr(value, prefix, consts)
        }
        StmtKind::Expr(e) => qualify_var_refs_in_expr(e, prefix, consts),
    }
}

fn qualify_var_refs_in_expr(e: &mut Expr, prefix: &str, consts: &HashSet<Symbol>) {
    match &mut e.kind {
        ExprKind::Var(name) => {
            if consts.contains(name) {
                *name = Symbol::intern(&format!("{prefix}.{name}")).into();
            }
        }
        ExprKind::Unary { expr, .. } => qualify_var_refs_in_expr(expr, prefix, consts),
        ExprKind::Binary { lhs, rhs, .. } => {
            qualify_var_refs_in_expr(lhs, prefix, consts);
            qualify_var_refs_in_expr(rhs, prefix, consts);
        }
        ExprKind::Logical { lhs, rhs, .. } => {
            qualify_var_refs_in_expr(lhs, prefix, consts);
            qualify_var_refs_in_expr(rhs, prefix, consts);
        }
        ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. } => {
            qualify_var_refs_in_expr(expr, prefix, consts)
        }
        ExprKind::Call { callee, args } => {
            // Qualify the callee here (not in the later
            // `prefix_*` walk) so locally-bound closures —
            // `let f = ...; f(v)` — don't get accidentally
            // rewritten to `module.f(v)`. `consts` lists every
            // top-level name (const / class / fn / `let`) the
            // module exposes; bare callee names not in there
            // are presumed local and left alone.
            if !is_builtin_callee(callee.as_str())
                && !callee.as_str().contains('.')
                && consts.contains(callee)
            {
                *callee = Symbol::intern(&format!("{prefix}.{callee}")).into();
            }
            for a in args.iter_mut() {
                qualify_var_refs_in_expr(a, prefix, consts);
            }
        }
        ExprKind::SuperCall { args, .. } => {
            for a in args.iter_mut() {
                qualify_var_refs_in_expr(a, prefix, consts);
            }
        }
        ExprKind::MethodCall { obj, args, .. } => {
            qualify_var_refs_in_expr(obj, prefix, consts);
            for a in args.iter_mut() {
                qualify_var_refs_in_expr(a, prefix, consts);
            }
        }
        ExprKind::Field { obj, .. } => qualify_var_refs_in_expr(obj, prefix, consts),
        ExprKind::AssignField { obj, value, .. } => {
            qualify_var_refs_in_expr(obj, prefix, consts);
            qualify_var_refs_in_expr(value, prefix, consts);
        }
        ExprKind::Index { obj, index } => {
            qualify_var_refs_in_expr(obj, prefix, consts);
            qualify_var_refs_in_expr(index, prefix, consts);
        }
        ExprKind::AssignIndex { obj, index, value } => {
            qualify_var_refs_in_expr(obj, prefix, consts);
            qualify_var_refs_in_expr(index, prefix, consts);
            qualify_var_refs_in_expr(value, prefix, consts);
        }
        ExprKind::Assign { target, value } => {
            // LHS: `state = ...` writing to a top-level let needs
            // the same qualification as a Var read.
            if consts.contains(target) {
                *target = Symbol::intern(&format!("{prefix}.{target}")).into();
            }
            qualify_var_refs_in_expr(value, prefix, consts);
        }
        ExprKind::New { args, .. } => {
            for a in args.iter_mut() {
                qualify_var_refs_in_expr(a, prefix, consts);
            }
        }
        ExprKind::EnumCtor { args, .. } => match args {
            ilang_ast::CtorArgs::Unit => {}
            ilang_ast::CtorArgs::Tuple(es) => {
                for a in es.iter_mut() {
                    qualify_var_refs_in_expr(a, prefix, consts);
                }
            }
            ilang_ast::CtorArgs::Struct(fs) => {
                for (_, e) in fs.iter_mut() {
                    qualify_var_refs_in_expr(e, prefix, consts);
                }
            }
        },
        ExprKind::If { cond, then_branch, else_branch } => {
            qualify_var_refs_in_expr(cond, prefix, consts);
            qualify_var_refs_in_block(then_branch, prefix, consts);
            if let Some(e) = else_branch.as_mut() {
                qualify_var_refs_in_expr(e, prefix, consts);
            }
        }
        ExprKind::While { cond, body } => {
            qualify_var_refs_in_expr(cond, prefix, consts);
            qualify_var_refs_in_block(body, prefix, consts);
        }
        ExprKind::Loop { body } => qualify_var_refs_in_block(body, prefix, consts),
        ExprKind::ForIn { iter, body, .. } => {
            qualify_var_refs_in_expr(iter, prefix, consts);
            qualify_var_refs_in_block(body, prefix, consts);
        }
        ExprKind::Block(b) => qualify_var_refs_in_block(b, prefix, consts),
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                qualify_var_refs_in_expr(s, prefix, consts);
            }
            if let Some(e) = end {
                qualify_var_refs_in_expr(e, prefix, consts);
            }
        }
        ExprKind::Array(es) => {
            for e in es.iter_mut() {
                qualify_var_refs_in_expr(e, prefix, consts);
            }
        }
        ExprKind::Tuple(es) => {
            for e in es.iter_mut() {
                qualify_var_refs_in_expr(e, prefix, consts);
            }
        }
        ExprKind::MapLit(pairs) => {
            for (k, v) in pairs.iter_mut() {
                qualify_var_refs_in_expr(k, prefix, consts);
                qualify_var_refs_in_expr(v, prefix, consts);
            }
        }
        ExprKind::FnExpr { body, .. } => qualify_var_refs_in_block(body, prefix, consts),
        ExprKind::Match { scrutinee, arms } => {
            qualify_var_refs_in_expr(scrutinee, prefix, consts);
            for arm in arms.iter_mut() {
                qualify_var_refs_in_expr(&mut arm.body, prefix, consts);
            }
        }
        ExprKind::Some(e) => qualify_var_refs_in_expr(e, prefix, consts),
        ExprKind::Await(e) => qualify_var_refs_in_expr(e, prefix, consts),
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            qualify_var_refs_in_expr(expr, prefix, consts);
            qualify_var_refs_in_block(then_branch, prefix, consts);
            if let Some(e) = else_branch.as_mut() {
                qualify_var_refs_in_expr(e, prefix, consts);
            }
        }
        ExprKind::Return(Some(e)) => qualify_var_refs_in_expr(e, prefix, consts),
        ExprKind::Break(Some(e)) => qualify_var_refs_in_expr(e, prefix, consts),
        ExprKind::StructLit { fields, .. } => {
            for (_, e) in fields.iter_mut() {
                qualify_var_refs_in_expr(e, prefix, consts);
            }
        }
        ExprKind::Template { parts } => {
            for p in parts.iter_mut() {
                if let ilang_ast::TemplatePart::Expr(e) = p {
                    qualify_var_refs_in_expr(e, prefix, consts);
                }
            }
        }
        // Leaf nodes — nothing to walk into.
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::This
        | ExprKind::None
        | ExprKind::Continue
        | ExprKind::Closure { .. }
        | ExprKind::Break(None)
        | ExprKind::Return(None) => {}
    }
}
