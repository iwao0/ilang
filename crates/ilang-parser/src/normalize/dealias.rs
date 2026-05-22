//! Module-alias de-aliasing pass.
//!
//! `use other as foo` brings `other`'s items into scope under the
//! `foo.` prefix. After [`build_ctx`] records each alias, this pass
//! walks the program and rewrites every reference (`foo.bar` →
//! `other.bar`) so downstream passes only ever see canonical
//! module names.

use std::collections::HashMap;

use ilang_ast::{
    Block, ClassDecl, Expr, ExprKind, Item, Program, Stmt, StmtKind, Symbol, Type,
};

use super::walk::walk_expr_children_mut;

pub(super) fn dealias_sym(s: &Symbol, modules: &HashMap<Symbol, Symbol>) -> Symbol {
    let raw = s.as_str();
    if let Some((prefix, rest)) = raw.split_once('.') {
        if let Some(canonical) = modules.get(&Symbol::intern(prefix)) {
            if canonical.as_str() != prefix {
                return Symbol::intern(&format!("{}.{rest}", canonical.as_str()));
            }
        }
    }
    s.clone()
}

fn dealias_type(t: &mut Type, modules: &HashMap<Symbol, Symbol>) {
    match t {
        Type::Object(n) | Type::Enum(n) => *n = dealias_sym(n, modules),
        Type::Generic(g) => {
            g.base = dealias_sym(&g.base, modules);
            for a in g.args.iter_mut() {
                dealias_type(a, modules);
            }
        }
        Type::Array { elem, .. } | Type::Optional(elem) | Type::Weak(elem) => {
            dealias_type(elem, modules)
        }
        Type::Tuple(elems) => {
            for e in elems.iter_mut() {
                dealias_type(e, modules);
            }
        }
        Type::Fn(ft) => {
            for p in ft.params.iter_mut() {
                dealias_type(p, modules);
            }
            dealias_type(&mut ft.ret, modules);
        }
        Type::RawPtr { inner, .. } => dealias_type(inner, modules),
        _ => {}
    }
}

fn dealias_expr(e: &mut Expr, modules: &HashMap<Symbol, Symbol>) {
    // Special arms: rewrite the dotted symbol / inner Type. The
    // default child walk below mutates every Expr / Block child.
    match &mut e.kind {
        ExprKind::New { class, type_args, .. } => {
            *class = dealias_sym(class, modules);
            for ta in type_args.iter_mut() {
                dealias_type(ta, modules);
            }
        }
        ExprKind::StructLit { class, .. } => {
            *class = dealias_sym(class, modules);
        }
        ExprKind::Cast { ty, .. }
        | ExprKind::TypeTest { ty, .. }
        | ExprKind::TypeDowncast { ty, .. } => {
            dealias_type(ty, modules);
        }
        ExprKind::FnExpr { params, ret, .. } => {
            for p in params.iter_mut() {
                dealias_type(&mut p.ty, modules);
            }
            if let Some(r) = ret {
                dealias_type(r, modules);
            }
        }
        _ => {}
    }
    walk_expr_children_mut(
        e,
        &mut |child| dealias_expr(child, modules),
        &mut |b| dealias_block(b, modules),
    );
}

fn dealias_block(b: &mut Block, modules: &HashMap<Symbol, Symbol>) {
    for s in b.stmts.iter_mut() {
        dealias_stmt(s, modules);
    }
    if let Some(t) = b.tail.as_mut() {
        dealias_expr(t, modules);
    }
}

fn dealias_stmt(s: &mut Stmt, modules: &HashMap<Symbol, Symbol>) {
    match &mut s.kind {
        StmtKind::Let { ty, value, .. } => {
            if let Some(t) = ty {
                dealias_type(t, modules);
            }
            dealias_expr(value, modules);
        }
        StmtKind::LetTuple { value, .. } | StmtKind::LetStruct { value, .. } => {
            dealias_expr(value, modules);
        }
        StmtKind::Expr(e) => dealias_expr(e, modules),
    }
}

fn dealias_class(c: &mut ClassDecl, modules: &HashMap<Symbol, Symbol>) {
    if let Some(parent) = c.parent.as_mut() {
        *parent = dealias_sym(parent, modules);
    }
    for ifn in c.interfaces.iter_mut() {
        *ifn = dealias_sym(ifn, modules);
    }
    for f in c.fields.iter_mut() {
        dealias_type(&mut f.ty, modules);
    }
    for sf in c.static_fields.iter_mut() {
        dealias_type(&mut sf.ty, modules);
        dealias_expr(&mut sf.value, modules);
    }
    for m in c.methods.iter_mut().chain(c.static_methods.iter_mut()) {
        for p in m.params.iter_mut() {
            dealias_type(&mut p.ty, modules);
            if let Some(d) = &mut p.default {
                dealias_expr(d, modules);
            }
        }
        if let Some(r) = m.ret.as_mut() {
            dealias_type(r, modules);
        }
        dealias_block(&mut m.body, modules);
    }
    for prop in c.properties.iter_mut() {
        dealias_type(&mut prop.ty, modules);
        if let Some(g) = prop.getter.as_mut() {
            dealias_block(&mut g.body, modules);
        }
        if let Some(s) = prop.setter.as_mut() {
            for p in s.params.iter_mut() {
                dealias_type(&mut p.ty, modules);
            }
            dealias_block(&mut s.body, modules);
        }
    }
}

pub(super) fn dealias_program(prog: &mut Program, modules: &HashMap<Symbol, Symbol>) {
    for item in prog.items.iter_mut() {
        match item {
            Item::Fn(f) => {
                for p in f.params.iter_mut() {
                    dealias_type(&mut p.ty, modules);
                    if let Some(d) = &mut p.default {
                        dealias_expr(d, modules);
                    }
                }
                if let Some(r) = f.ret.as_mut() {
                    dealias_type(r, modules);
                }
                dealias_block(&mut f.body, modules);
            }
            Item::Class(c) => dealias_class(c, modules),
            Item::Enum(_) | Item::Use(_) => {}
            Item::Const(c) => {
                if let Some(t) = c.ty.as_mut() {
                    dealias_type(t, modules);
                }
                dealias_expr(&mut c.value, modules);
            }
            Item::ExternC(b) => {
                for inner in b.items.iter_mut() {
                    use ilang_ast::ExternCItem;
                    match inner {
                        ExternCItem::FnDef(f) => {
                            for p in f.params.iter_mut() {
                                dealias_type(&mut p.ty, modules);
                            }
                            if let Some(r) = f.ret.as_mut() {
                                dealias_type(r, modules);
                            }
                            dealias_block(&mut f.body, modules);
                        }
                        ExternCItem::FnDecl { params, ret, .. } => {
                            for p in params.iter_mut() {
                                dealias_type(&mut p.ty, modules);
                            }
                            if let Some(r) = ret {
                                dealias_type(r, modules);
                            }
                        }
                        ExternCItem::Struct { fields, .. }
                        | ExternCItem::Union { fields, .. } => {
                            for f in fields.iter_mut() {
                                dealias_type(&mut f.ty, modules);
                            }
                        }
                        ExternCItem::Class(c) => dealias_class(c, modules),
                    }
                }
            }
            Item::Interface(_) => {}
        }
    }
    for s in prog.stmts.iter_mut() {
        dealias_stmt(s, modules);
    }
    if let Some(t) = prog.tail.as_mut() {
        dealias_expr(t, modules);
    }
}

// ─── Module-prefix authorization check ────────────────────────────────
//
// Only `New` (constructor) and Type-position references are checked
// here. Field / MethodCall paths already require the receiver name
// to be in `ctx.modules` before normalize collapses them to a
// qualified `Var` / `Call`, so they're safely gated.
