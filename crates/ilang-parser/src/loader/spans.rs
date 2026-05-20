//! Walk a freshly-parsed `Program` and stamp every `Span`'s
//! `source_file` with the canonical path string the program came
//! from. The lexer / parser don't know about file paths (they
//! work on a `&str` of source); the loader is the one place that
//! does, so it's responsible for the tag.
//!
//! Without this, errors raised against AST nodes lifted in from a
//! sub-module (e.g. an `appkit.il` parameter's type span) would
//! be reported as if they came from the entry file — same `(line,
//! col)`, different file, no way for the formatter to tell.

use ilang_ast::{
    Block, ClassDecl, ConstDecl, EnumDecl, Expr, ExprKind, ExternCBlock, ExternCItem, FieldDecl,
    FnDecl, InterfaceDecl, Item, Param, Pattern, PropertyDecl, Program, StaticFieldDecl, Stmt,
    StmtKind, Symbol,
};

pub(super) fn tag_program_spans(prog: &mut Program, file: Symbol) {
    for item in prog.items.iter_mut() {
        tag_item(item, file);
    }
    for s in prog.stmts.iter_mut() {
        tag_stmt(s, file);
    }
    if let Some(t) = prog.tail.as_mut() {
        tag_expr(t, file);
    }
}

fn tag_item(item: &mut Item, file: Symbol) {
    match item {
        Item::Fn(f) => tag_fn(f, file),
        Item::Class(c) => tag_class(c, file),
        Item::Interface(i) => tag_interface(i, file),
        Item::Enum(e) => tag_enum(e, file),
        Item::Use(u) => {
            u.span.source_file = file;
        }
        Item::Const(c) => tag_const(c, file),
        Item::ExternC(b) => tag_extern_c(b, file),
    }
}

fn tag_fn(f: &mut FnDecl, file: Symbol) {
    f.span.source_file = file;
    for p in f.params.iter_mut() {
        tag_param(p, file);
    }
    tag_block(&mut f.body, file);
}

fn tag_param(p: &mut Param, file: Symbol) {
    p.span.source_file = file;
    if let Some(d) = p.default.as_mut() {
        tag_expr(d, file);
    }
}

fn tag_class(c: &mut ClassDecl, file: Symbol) {
    c.span.source_file = file;
    for fd in c.fields.iter_mut() {
        tag_field(fd, file);
    }
    for sf in c.static_fields.iter_mut() {
        tag_static_field(sf, file);
    }
    for m in c.methods.iter_mut().chain(c.static_methods.iter_mut()) {
        tag_fn(m, file);
    }
    for prop in c.properties.iter_mut() {
        tag_property(prop, file);
    }
}

fn tag_field(f: &mut FieldDecl, file: Symbol) {
    f.span.source_file = file;
}

fn tag_static_field(sf: &mut StaticFieldDecl, file: Symbol) {
    sf.span.source_file = file;
    tag_expr(&mut sf.value, file);
}

fn tag_property(p: &mut PropertyDecl, file: Symbol) {
    p.span.source_file = file;
    if let Some(g) = p.getter.as_mut() {
        tag_fn(g, file);
    }
    if let Some(s) = p.setter.as_mut() {
        tag_fn(s, file);
    }
}

fn tag_interface(i: &mut InterfaceDecl, file: Symbol) {
    i.span.source_file = file;
    for m in i.methods.iter_mut() {
        m.span.source_file = file;
    }
}

fn tag_enum(e: &mut EnumDecl, file: Symbol) {
    e.span.source_file = file;
    for v in e.variants.iter_mut() {
        v.span.source_file = file;
    }
}

fn tag_const(c: &mut ConstDecl, file: Symbol) {
    c.span.source_file = file;
    tag_expr(&mut c.value, file);
}

fn tag_extern_c(b: &mut ExternCBlock, file: Symbol) {
    b.span.source_file = file;
    for inner in b.items.iter_mut() {
        match inner {
            ExternCItem::FnDef(f) => tag_fn(f, file),
            ExternCItem::FnDecl { params, span, .. } => {
                span.source_file = file;
                for p in params.iter_mut() {
                    tag_param(p, file);
                }
            }
            ExternCItem::Class(c) => tag_class(c, file),
            ExternCItem::Struct { span, fields, .. }
            | ExternCItem::Union { span, fields, .. } => {
                span.source_file = file;
                for f in fields.iter_mut() {
                    tag_field(f, file);
                }
            }
        }
    }
    // `@com interface` and `const` live in dedicated ExternCBlock
    // fields rather than the generic `items` vector, so the loop
    // above misses them. Without tagging here the duplicate-pub
    // diagnostic for COM interfaces reports the entry file path
    // instead of the binding that actually declared the dup.
    for iface in b.interfaces.iter_mut() {
        tag_interface(iface, file);
    }
    for c in b.consts.iter_mut() {
        tag_const(c, file);
    }
}

fn tag_block(b: &mut Block, file: Symbol) {
    for s in b.stmts.iter_mut() {
        tag_stmt(s, file);
    }
    if let Some(t) = b.tail.as_mut() {
        tag_expr(t, file);
    }
}

fn tag_stmt(s: &mut Stmt, file: Symbol) {
    s.span.source_file = file;
    match &mut s.kind {
        StmtKind::Let { value, .. }
        | StmtKind::LetTuple { value, .. }
        | StmtKind::LetStruct { value, .. } => {
            tag_expr(value, file);
        }
        StmtKind::Expr(e) => tag_expr(e, file),
    }
}

fn tag_pattern(p: &mut Pattern, file: Symbol) {
    p.span.source_file = file;
}

fn tag_expr(e: &mut Expr, file: Symbol) {
    e.span.source_file = file;
    match &mut e.kind {
        ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_)
        | ExprKind::Str(_) | ExprKind::Var(_) | ExprKind::This
        | ExprKind::None | ExprKind::Continue
        | ExprKind::Closure { .. } => {}
        ExprKind::Some(inner) => tag_expr(inner, file),
        ExprKind::Array(items) | ExprKind::Tuple(items) => {
            for it in items.iter_mut() {
                tag_expr(it, file);
            }
        }
        ExprKind::MapLit(entries) => {
            for (k, v) in entries.iter_mut() {
                tag_expr(k, file);
                tag_expr(v, file);
            }
        }
        ExprKind::Field { obj, .. } => tag_expr(obj, file),
        ExprKind::Call { args, .. } => {
            for a in args.iter_mut() {
                tag_expr(a, file);
            }
        }
        ExprKind::MethodCall { obj, args, .. } => {
            tag_expr(obj, file);
            for a in args.iter_mut() {
                tag_expr(a, file);
            }
        }
        ExprKind::Index { obj, index } => {
            tag_expr(obj, file);
            tag_expr(index, file);
        }
        ExprKind::Unary { expr, .. } => tag_expr(expr, file),
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            tag_expr(lhs, file);
            tag_expr(rhs, file);
        }
        ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. } => tag_expr(expr, file),
        ExprKind::Block(b) => tag_block(b, file),
        ExprKind::If { cond, then_branch, else_branch } => {
            tag_expr(cond, file);
            tag_block(then_branch, file);
            if let Some(e) = else_branch.as_mut() {
                tag_expr(e, file);
            }
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            tag_expr(expr, file);
            tag_block(then_branch, file);
            if let Some(e) = else_branch.as_mut() {
                tag_expr(e, file);
            }
        }
        ExprKind::While { cond, body } => {
            tag_expr(cond, file);
            tag_block(body, file);
        }
        ExprKind::Loop { body } => tag_block(body, file),
        ExprKind::ForIn { iter, body, .. } => {
            tag_expr(iter, file);
            tag_block(body, file);
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start.as_mut() {
                tag_expr(s, file);
            }
            if let Some(e) = end.as_mut() {
                tag_expr(e, file);
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            tag_expr(scrutinee, file);
            for arm in arms.iter_mut() {
                arm.span.source_file = file;
                tag_pattern(&mut arm.pattern, file);
                tag_expr(&mut arm.body, file);
            }
        }
        ExprKind::New { args, .. } => {
            for a in args.iter_mut() {
                tag_expr(a, file);
            }
        }
        ExprKind::EnumCtor { args, .. } => {
            match args {
                ilang_ast::CtorArgs::Unit => {}
                ilang_ast::CtorArgs::Tuple(items) => {
                    for it in items.iter_mut() {
                        tag_expr(it, file);
                    }
                }
                ilang_ast::CtorArgs::Struct(named) => {
                    for (_, v) in named.iter_mut() {
                        tag_expr(v, file);
                    }
                }
            }
        }
        ExprKind::FnExpr { params, body, .. } => {
            for p in params.iter_mut() {
                tag_param(p, file);
            }
            tag_block(body, file);
        }
        ExprKind::AssignField { obj, value, .. } => {
            tag_expr(obj, file);
            tag_expr(value, file);
        }
        ExprKind::AssignIndex { obj, index, value } => {
            tag_expr(obj, file);
            tag_expr(index, file);
            tag_expr(value, file);
        }
        ExprKind::Assign { value, .. } => tag_expr(value, file),
        ExprKind::SuperCall { args, .. } => {
            for a in args.iter_mut() {
                tag_expr(a, file);
            }
        }
        ExprKind::Await(inner) => tag_expr(inner, file),
        ExprKind::Return(opt) | ExprKind::Break(opt) => {
            if let Some(e) = opt.as_mut() {
                tag_expr(e, file);
            }
        }
        ExprKind::StructLit { fields, .. } => {
            for (_, v) in fields.iter_mut() {
                tag_expr(v, file);
            }
        }
    }
}
