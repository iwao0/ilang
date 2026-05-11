//! Post-dealias validation pass: every dotted `Var(M.X)` reference
//! must target a real `M`'s exported `X`. The walk runs after
//! [`build_ctx`] has materialised the import map; failures yield
//! a `ParseError` with the offending span.

use std::collections::HashMap;

use ilang_ast::{
    Block, ClassDecl, CtorArgs, Expr, ExprKind, Item, Program, Span, Stmt, StmtKind, Symbol, Type,
};

use crate::error::ParseError;

fn check_dotted_ref(
    name: &Symbol,
    item_label: &str,
    span: Span,
    modules: &HashMap<Symbol, Symbol>,
) -> Result<(), ParseError> {
    let s = name.as_str();
    if let Some((prefix, rest)) = s.split_once('.') {
        if !modules.contains_key(&Symbol::intern(prefix)) {
            return Err(ParseError::UnauthorizedModuleRef {
                module: Symbol::intern(prefix),
                item: Symbol::intern(if item_label.is_empty() { rest } else { item_label }),
                span,
            });
        }
    }
    Ok(())
}

fn validate_type(t: &Type, span: Span, modules: &HashMap<Symbol, Symbol>) -> Result<(), ParseError> {
    match t {
        Type::Object(name) | Type::Enum(name) => {
            check_dotted_ref(name, "", span, modules)?
        }
        Type::Generic(g) => {
            check_dotted_ref(&g.base, "", span, modules)?;
            for a in g.args.iter() {
                validate_type(a, span, modules)?;
            }
        }
        Type::Array { elem, .. } | Type::Optional(elem) | Type::Weak(elem) => {
            validate_type(elem, span, modules)?
        }
        Type::Tuple(elems) => {
            for e in elems.iter() {
                validate_type(e, span, modules)?;
            }
        }
        Type::Fn(ft) => {
            for p in ft.params.iter() {
                validate_type(p, span, modules)?;
            }
            validate_type(&ft.ret, span, modules)?;
        }
        Type::RawPtr { inner, .. } => validate_type(inner, span, modules)?,
        _ => {}
    }
    Ok(())
}

fn validate_block(b: &Block, modules: &HashMap<Symbol, Symbol>) -> Result<(), ParseError> {
    for s in b.stmts.iter() {
        validate_stmt(s, modules)?;
    }
    if let Some(t) = b.tail.as_ref() {
        validate_expr(t, modules)?;
    }
    Ok(())
}

fn validate_stmt(s: &Stmt, modules: &HashMap<Symbol, Symbol>) -> Result<(), ParseError> {
    match &s.kind {
        StmtKind::Let { ty, value, .. } => {
            if let Some(t) = ty {
                validate_type(t, s.span, modules)?;
            }
            validate_expr(value, modules)?;
        }
        StmtKind::LetTuple { value, .. } | StmtKind::LetStruct { value, .. } => {
            validate_expr(value, modules)?;
        }
        StmtKind::Expr(e) => validate_expr(e, modules)?,
    }
    Ok(())
}

fn validate_expr(e: &Expr, modules: &HashMap<Symbol, Symbol>) -> Result<(), ParseError> {
    match &e.kind {
        ExprKind::New { class, type_args, args, .. } => {
            check_dotted_ref(class, "", e.span, modules)?;
            for ta in type_args.iter() {
                validate_type(ta, e.span, modules)?;
            }
            for a in args.iter() {
                validate_expr(a, modules)?;
            }
        }
        ExprKind::Cast { expr, ty }
        | ExprKind::TypeTest { expr, ty }
        | ExprKind::TypeDowncast { expr, ty } => {
            validate_expr(expr, modules)?;
            validate_type(ty, e.span, modules)?;
        }
        ExprKind::FnExpr { params, ret, body } => {
            for p in params.iter() {
                validate_type(&p.ty, p.span, modules)?;
                if let Some(d) = &p.default {
                    validate_expr(d, modules)?;
                }
            }
            if let Some(r) = ret {
                validate_type(r, e.span, modules)?;
            }
            validate_block(body, modules)?;
        }
        ExprKind::Unary { expr, .. }
        | ExprKind::Some(expr)
        | ExprKind::Return(Some(expr))
        | ExprKind::Break(Some(expr)) => validate_expr(expr, modules)?,
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            validate_expr(lhs, modules)?;
            validate_expr(rhs, modules)?;
        }
        ExprKind::Call { args, .. } | ExprKind::SuperCall { args, .. } => {
            for a in args.iter() {
                validate_expr(a, modules)?;
            }
        }
        ExprKind::MethodCall { obj, args, .. } => {
            validate_expr(obj, modules)?;
            for a in args.iter() {
                validate_expr(a, modules)?;
            }
        }
        ExprKind::Field { obj, .. } => validate_expr(obj, modules)?,
        ExprKind::Assign { value, .. } => validate_expr(value, modules)?,
        ExprKind::AssignField { obj, value, .. } => {
            validate_expr(obj, modules)?;
            validate_expr(value, modules)?;
        }
        ExprKind::AssignIndex { obj, index, value } => {
            validate_expr(obj, modules)?;
            validate_expr(index, modules)?;
            validate_expr(value, modules)?;
        }
        ExprKind::EnumCtor { args, .. } => match args {
            CtorArgs::Unit => {}
            CtorArgs::Tuple(es) => {
                for a in es.iter() {
                    validate_expr(a, modules)?;
                }
            }
            CtorArgs::Struct(fs) => {
                for (_, a) in fs.iter() {
                    validate_expr(a, modules)?;
                }
            }
        },
        ExprKind::If { cond, then_branch, else_branch } => {
            validate_expr(cond, modules)?;
            validate_block(then_branch, modules)?;
            if let Some(e2) = else_branch {
                validate_expr(e2, modules)?;
            }
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            validate_expr(expr, modules)?;
            validate_block(then_branch, modules)?;
            if let Some(e2) = else_branch {
                validate_expr(e2, modules)?;
            }
        }
        ExprKind::While { cond, body } => {
            validate_expr(cond, modules)?;
            validate_block(body, modules)?;
        }
        ExprKind::Loop { body } => validate_block(body, modules)?,
        ExprKind::ForIn { iter, body, .. } => {
            validate_expr(iter, modules)?;
            validate_block(body, modules)?;
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                validate_expr(s, modules)?;
            }
            if let Some(e2) = end {
                validate_expr(e2, modules)?;
            }
        }
        ExprKind::Block(b) => validate_block(b, modules)?,
        ExprKind::Array(es) | ExprKind::Tuple(es) => {
            for x in es.iter() {
                validate_expr(x, modules)?;
            }
        }
        ExprKind::StructLit { class, fields } => {
            check_dotted_ref(class, "", e.span, modules)?;
            for (_, x) in fields.iter() {
                validate_expr(x, modules)?;
            }
        }
        ExprKind::MapLit(entries) => {
            for (k, v) in entries.iter() {
                validate_expr(k, modules)?;
                validate_expr(v, modules)?;
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            validate_expr(scrutinee, modules)?;
            for arm in arms.iter() {
                validate_expr(&arm.body, modules)?;
            }
        }
        ExprKind::Index { obj, index } => {
            validate_expr(obj, modules)?;
            validate_expr(index, modules)?;
        }
        // Leaf nodes / nodes with nothing module-qualifiable inside.
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::Var(_)
        | ExprKind::This
        | ExprKind::None
        | ExprKind::Continue
        | ExprKind::Closure { .. }
        | ExprKind::Return(None)
        | ExprKind::Break(None) => {}
    }
    Ok(())
}

fn validate_class(c: &ClassDecl, modules: &HashMap<Symbol, Symbol>) -> Result<(), ParseError> {
    if let Some(parent) = &c.parent {
        check_dotted_ref(parent, "", c.span, modules)?;
    }
    for f in c.fields.iter() {
        validate_type(&f.ty, f.span, modules)?;
    }
    for sf in c.static_fields.iter() {
        validate_type(&sf.ty, sf.span, modules)?;
        validate_expr(&sf.value, modules)?;
    }
    for m in c.methods.iter().chain(c.static_methods.iter()) {
        for p in m.params.iter() {
            validate_type(&p.ty, p.span, modules)?;
            if let Some(d) = &p.default {
                validate_expr(d, modules)?;
            }
        }
        if let Some(r) = &m.ret {
            validate_type(r, m.span, modules)?;
        }
        validate_block(&m.body, modules)?;
    }
    for prop in c.properties.iter() {
        validate_type(&prop.ty, prop.span, modules)?;
        if let Some(g) = &prop.getter {
            validate_block(&g.body, modules)?;
        }
        if let Some(s) = &prop.setter {
            for p in s.params.iter() {
                validate_type(&p.ty, p.span, modules)?;
            }
            validate_block(&s.body, modules)?;
        }
    }
    Ok(())
}

pub(super) fn validate_program(prog: &Program, modules: &HashMap<Symbol, Symbol>) -> Result<(), ParseError> {
    for item in &prog.items {
        match item {
            Item::Fn(f) => {
                for p in f.params.iter() {
                    validate_type(&p.ty, p.span, modules)?;
                    if let Some(d) = &p.default {
                        validate_expr(d, modules)?;
                    }
                }
                if let Some(r) = &f.ret {
                    validate_type(r, f.span, modules)?;
                }
                validate_block(&f.body, modules)?;
            }
            Item::Class(c) => validate_class(c, modules)?,
            Item::Enum(_) | Item::Use(_) => {}
            Item::Const(c) => {
                if let Some(t) = &c.ty {
                    validate_type(t, c.span, modules)?;
                }
                validate_expr(&c.value, modules)?;
            }
            Item::ExternC(b) => {
                for inner in b.items.iter() {
                    use ilang_ast::ExternCItem;
                    match inner {
                        ExternCItem::FnDef(f) => {
                            for p in f.params.iter() {
                                validate_type(&p.ty, p.span, modules)?;
                            }
                            if let Some(r) = &f.ret {
                                validate_type(r, f.span, modules)?;
                            }
                            validate_block(&f.body, modules)?;
                        }
                        ExternCItem::FnDecl { params, ret, span, .. } => {
                            for p in params.iter() {
                                validate_type(&p.ty, p.span, modules)?;
                            }
                            if let Some(r) = ret {
                                validate_type(r, *span, modules)?;
                            }
                        }
                        ExternCItem::Struct { fields, span, .. }
                        | ExternCItem::Union { fields, span, .. } => {
                            for f in fields.iter() {
                                validate_type(&f.ty, *span, modules)?;
                            }
                        }
                        ExternCItem::Class(c) => validate_class(c, modules)?,
                    }
                }
            }
        }
    }
    for s in &prog.stmts {
        validate_stmt(s, modules)?;
    }
    if let Some(t) = &prog.tail {
        validate_expr(t, modules)?;
    }
    Ok(())
}
