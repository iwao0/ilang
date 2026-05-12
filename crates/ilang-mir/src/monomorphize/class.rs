//! Extracted from `monomorphize/mod.rs`.

use std::collections::{HashMap, HashSet};

use ilang_ast::{
    Block, ClassDecl, Expr, ExprKind, FieldDecl, FnDecl, Item, Param, Program,
    Stmt, StmtKind, Symbol, Type,
};

use super::*;

/// Run the pass. Returns a new `Program` where every reference to a
/// generic class has been replaced by a concrete monomorphized
/// instantiation. Non-generic items pass through unchanged.
pub fn monomorphize(prog: &Program) -> Program {
    // Index original (generic) class decls by name so we can clone +
    // substitute on demand.
    let generic_classes: HashMap<Symbol, &ClassDecl> = prog
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Class(c) if !c.type_params.is_empty() => Some((c.name.clone(), c)),
            _ => None,
        })
        .collect();

    // Generic-enum names are tracked too — we DON'T monomorphize
    // them (per-instantiation enum layouts would need significant
    // JIT infrastructure). Instead the rewrite step leaves their
    // `Type::Generic` references alone so the JIT errors out with a
    // clear "generic enum + JIT" message via UnsupportedType.
    let generic_enum_names: HashSet<Symbol> = prog
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Enum(e) if !e.type_params.is_empty() => Some(e.name.clone()),
            _ => None,
        })
        .collect();
    // Built-in generic enums (Result) are also enum names.
    let mut generic_enum_names = generic_enum_names;
    generic_enum_names.insert("Result".into());
    GENERIC_ENUM_NAMES.with(|set| {
        *set.borrow_mut() = generic_enum_names.clone();
    });

    // Seed the worklist by scanning the entire (untransformed) program
    // for generic instantiations.
    let mut needed: HashSet<Symbol> = HashSet::new();
    let mut worklist: Vec<InstKey> = Vec::new();
    let seed = |t: &Type, needed: &mut HashSet<Symbol>, work: &mut Vec<InstKey>| {
        collect_instantiations(t, needed, work);
    };
    for item in &prog.items {
        match item {
            Item::Class(c) => {
                // Generic-class bodies still get scanned with the
                // class's params left as TypeVars; only concrete refs
                // (no TypeVar) seed the worklist. The substitution
                // pass will expand them properly later.
                for f in &c.fields {
                    seed(&f.ty, &mut needed, &mut worklist);
                }
                for m in &c.methods {
                    scan_fn(m, &mut needed, &mut worklist);
                }
            }
            Item::Fn(f) => scan_fn(f, &mut needed, &mut worklist),
            Item::Enum(_) | Item::Use(_) | Item::Const(_)  | Item::ExternC(_) => {}
        }
    }
    for s in &prog.stmts {
        scan_stmt(s, &mut needed, &mut worklist);
    }
    if let Some(t) = &prog.tail {
        scan_expr(t, &mut needed, &mut worklist);
    }

    // Iteratively monomorphize each pending instantiation. As we
    // substitute T → concrete in method bodies, new generic refs may
    // appear (e.g. `class Wrap<T> { f(): Box<T> { ... } }` instantiated
    // with T=i64 yields a `Box<i64>` ref) — those go back on the
    // worklist.
    let mut synthesized: HashMap<Symbol, ClassDecl> = HashMap::new();
    while let Some(key) = worklist.pop() {
        let mangled = key.mangled();
        if synthesized.contains_key(&mangled) {
            continue;
        }
        let template = match generic_classes.get(&key.class) {
            Some(c) => *c,
            None => continue, // Concrete or undefined — let the type checker have caught it.
        };
        if template.type_params.len() != key.args.len() {
            continue; // arity mismatch — type checker should have rejected
        }
        let new_class = specialize_class(template, &key.args, mangled.as_str());
        // Walk the new class's substituted bodies for further generic refs.
        for f in &new_class.fields {
            scan_type(&f.ty, &mut needed, &mut worklist);
        }
        for m in &new_class.methods {
            scan_fn(m, &mut needed, &mut worklist);
        }
        synthesized.insert(mangled, new_class);
    }

    // Build the output program: drop the generic class definitions,
    // keep everything else, and append the synthesized concrete classes.
    // Then rewrite Type::Generic → Type::Object(mangled) and
    // New.type_args → empty + class = mangled, throughout every
    // remaining node.
    let mut out_items: Vec<Item> = Vec::new();
    for item in &prog.items {
        match item {
            Item::Class(c) if !c.type_params.is_empty() => { /* drop */ }
            other => out_items.push(rewrite_item(other)),
        }
    }
    for (_, c) in synthesized {
        out_items.push(Item::Class(c));
    }
    let stmts: Vec<Stmt> = prog.stmts.iter().map(rewrite_stmt).collect();
    let tail = prog.tail.as_ref().map(rewrite_expr);
    Program {
        items: out_items,
        stmts,
        tail,
    }
}

pub(super) fn scan_fn(f: &FnDecl, needed: &mut HashSet<Symbol>, work: &mut Vec<InstKey>) {
    for Param { ty, .. } in &f.params {
        scan_type(ty, needed, work);
    }
    if let Some(t) = &f.ret {
        scan_type(t, needed, work);
    }
    scan_block(&f.body, needed, work);
}

pub(super) fn scan_block(b: &Block, needed: &mut HashSet<Symbol>, work: &mut Vec<InstKey>) {
    for s in &b.stmts {
        scan_stmt(s, needed, work);
    }
    if let Some(t) = &b.tail {
        scan_expr(t, needed, work);
    }
}

pub(super) fn scan_stmt(s: &Stmt, needed: &mut HashSet<Symbol>, work: &mut Vec<InstKey>) {
    match &s.kind {
        StmtKind::Let { value, ty, .. } => {
            if let Some(t) = ty {
                scan_type(t, needed, work);
            }
            scan_expr(value, needed, work);
        }
        StmtKind::LetTuple { value, .. }
        | StmtKind::LetStruct { value, .. } => scan_expr(value, needed, work),
        StmtKind::Expr(e) => scan_expr(e, needed, work),
    }
}

pub(super) fn scan_expr(e: &Expr, needed: &mut HashSet<Symbol>, work: &mut Vec<InstKey>) {
    match &e.kind {
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::Var(_)
        | ExprKind::This
        | ExprKind::None
        | ExprKind::Continue => {}
        ExprKind::Break(opt) => {
            if let Some(e) = opt {
                scan_expr(e, needed, work);
            }
        }
        ExprKind::Some(inner) => scan_expr(inner, needed, work),
        ExprKind::Unary { expr, .. } => scan_expr(expr, needed, work),
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            scan_expr(lhs, needed, work);
            scan_expr(rhs, needed, work);
        }
        ExprKind::Cast { expr, ty }
        | ExprKind::TypeTest { expr, ty }
        | ExprKind::TypeDowncast { expr, ty } => {
            scan_expr(expr, needed, work);
            scan_type(ty, needed, work);
        }
        ExprKind::FnExpr { params, ret, body } => {
            for p in params {
                scan_type(&p.ty, needed, work);
            }
            if let Some(t) = ret {
                scan_type(t, needed, work);
            }
            scan_block(body, needed, work);
        }
        ExprKind::Call { args, .. } => {
            for a in args {
                scan_expr(a, needed, work);
            }
        }
        ExprKind::SuperCall { args, .. } => {
            for a in args {
                scan_expr(a, needed, work);
            }
        }
        ExprKind::Closure { .. } => {}
        ExprKind::Field { obj, .. } => scan_expr(obj, needed, work),
        ExprKind::MethodCall { obj, args, .. } => {
            scan_expr(obj, needed, work);
            for a in args {
                scan_expr(a, needed, work);
            }
        }
        ExprKind::New { type_args, args, class, init_method: _ } => {
            for t in type_args {
                scan_type(t, needed, work);
            }
            for a in args {
                scan_expr(a, needed, work);
            }
            // The `new` itself is also an instantiation seed.
            if !type_args.is_empty() {
                push_inst(class.clone(), type_args.to_vec(), needed, work);
            }
        }
        ExprKind::Block(b) => scan_block(b, needed, work),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            scan_expr(cond, needed, work);
            scan_block(then_branch, needed, work);
            if let Some(e) = else_branch {
                scan_expr(e, needed, work);
            }
        }
        ExprKind::IfLet {
            expr,
            then_branch,
            else_branch,
            ..
        } => {
            scan_expr(expr, needed, work);
            scan_block(then_branch, needed, work);
            if let Some(e) = else_branch {
                scan_expr(e, needed, work);
            }
        }
        ExprKind::While { cond, body } => {
            scan_expr(cond, needed, work);
            scan_block(body, needed, work);
        }
        ExprKind::Loop { body } => scan_block(body, needed, work),
        ExprKind::ForIn { iter, body, .. } => {
            scan_expr(iter, needed, work);
            scan_block(body, needed, work);
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                scan_expr(s, needed, work);
            }
            if let Some(e) = end {
                scan_expr(e, needed, work);
            }
        }
        ExprKind::Return(opt) => {
            if let Some(e) = opt {
                scan_expr(e, needed, work);
            }
        }
        ExprKind::Assign { value, .. } => scan_expr(value, needed, work),
        ExprKind::AssignField { obj, value, .. } => {
            scan_expr(obj, needed, work);
            scan_expr(value, needed, work);
        }
        ExprKind::AssignIndex { obj, index, value } => {
            scan_expr(obj, needed, work);
            scan_expr(index, needed, work);
            scan_expr(value, needed, work);
        }
        ExprKind::Array(items) => {
            for i in items {
                scan_expr(i, needed, work);
            }
        }
        ExprKind::Tuple(items) => {
            for i in items {
                scan_expr(i, needed, work);
            }
        }
        ExprKind::StructLit { fields, .. } => {
            for (_, e) in fields {
                scan_expr(e, needed, work);
            }
        }
        ExprKind::MapLit(entries) => {
            for (k, v) in entries {
                scan_expr(k, needed, work);
                scan_expr(v, needed, work);
            }
        }
        ExprKind::Index { obj, index } => {
            scan_expr(obj, needed, work);
            scan_expr(index, needed, work);
        }
        ExprKind::EnumCtor { args, .. } => {
            if let ilang_ast::CtorArgs::Tuple(es) = args {
                for e in es {
                    scan_expr(e, needed, work);
                }
            } else if let ilang_ast::CtorArgs::Struct(fs) = args {
                for (_, e) in fs {
                    scan_expr(e, needed, work);
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            scan_expr(scrutinee, needed, work);
            for arm in arms {
                scan_expr(&arm.body, needed, work);
            }
        }
    }
}

pub(super) fn scan_type(t: &Type, needed: &mut HashSet<Symbol>, work: &mut Vec<InstKey>) {
    collect_instantiations(t, needed, work);
}

pub(super) fn collect_instantiations(
    t: &Type,
    needed: &mut HashSet<Symbol>,
    work: &mut Vec<InstKey>,
) {
    match t {
        Type::Generic(g) => {
            // Only enqueue concrete instantiations (no remaining type
            // variables). A `Box<T>` reference inside `class Wrap<T>`'s
            // body is left as-is here; substitute_class produces the
            // concrete `Box<i64>` later, which seeds the worklist on
            // the next round.
            if !contains_type_var(t) {
                push_inst(g.base.clone(), g.args.to_vec(), needed, work);
            }
            for a in &g.args {
                collect_instantiations(a, needed, work);
            }
        }
        Type::Array { elem, .. } => collect_instantiations(elem, needed, work),
        Type::Optional(inner) | Type::Weak(inner) => {
            collect_instantiations(inner, needed, work)
        }
        Type::Fn(ft) => {
            for p in &ft.params {
                collect_instantiations(p, needed, work);
            }
            collect_instantiations(&ft.ret, needed, work);
        }
        _ => {}
    }
}

pub(super) fn push_inst(
    class: Symbol,
    args: Vec<Type>,
    needed: &mut HashSet<Symbol>,
    work: &mut Vec<InstKey>,
) {
    let key = InstKey { class, args };
    if needed.insert(key.mangled()) {
        work.push(key);
    }
}

pub(super) fn contains_type_var(t: &Type) -> bool {
    match t {
        Type::TypeVar(_) => true,
        Type::Array { elem, .. } => contains_type_var(elem),
        Type::Optional(inner) | Type::Weak(inner) => contains_type_var(inner),
        Type::Generic(g) => g.args.iter().any(contains_type_var),
        Type::Fn(ft) => {
            ft.params.iter().any(contains_type_var) || contains_type_var(&ft.ret)
        }
        _ => false,
    }
}

pub(super) fn specialize_class(c: &ClassDecl, args: &[Type], mangled: &str) -> ClassDecl {
    let params = c.type_params.clone();
    // Concrete generic args (e.g. T = Box<i64>) need to be collapsed
    // to their mangled `Object("Box<i64>")` form before substitution,
    // otherwise nested instantiations leak through as `Type::Generic`.
    let args: Vec<Type> = args.iter().map(rewrite_type).collect();
    let args = &args[..];
    let fields = c
        .fields
        .iter()
        .map(|f| FieldDecl {
            is_pub: false,
            name: f.name.clone(),
            ty: subst_type(&f.ty, &params, args),
            span: f.span, bits: f.bits,
        })
        .collect();
    let methods = c
        .methods
        .iter()
        .map(|m| specialize_fn(m, &params, args))
        .collect();
    let static_methods = c
        .static_methods
        .iter()
        .map(|m| specialize_fn(m, &params, args))
        .collect();
    let properties = c
        .properties
        .iter()
        .map(|p| ilang_ast::PropertyDecl {
            is_pub: false,
            name: p.name.clone(),
            ty: subst_type(&p.ty, &params, args),
            getter: p.getter.as_ref().map(|g| specialize_fn(g, &params, args)),
            setter: p.setter.as_ref().map(|s| specialize_fn(s, &params, args)),
            span: p.span,
        })
        .collect();
    ClassDecl {
        is_pub: c.is_pub,
        extern_lib: c.extern_lib.clone(),
        is_repr_c: c.is_repr_c,
        is_packed: c.is_packed,
        is_union: c.is_union,
        name: mangled.into(),
        type_params: Box::new([]),
        parent: c.parent.clone(),
        fields,
        properties,
        methods,
        static_methods,
        // Generic class + static fields shouldn't reach here (the
        // type checker forbids static members on generic classes for
        // now), but pass them through verbatim for completeness.
        static_fields: c.static_fields.clone(),
        span: c.span,
    }
}

pub(super) fn specialize_fn(f: &FnDecl, params: &[Symbol], args: &[Type]) -> FnDecl {
    FnDecl {
        is_pub: f.is_pub,
        name: f.name.clone(),
        type_params: Box::new([]),
        params: f
            .params
            .iter()
            .map(|p| Param {
                name: p.name.clone(),
                ty: subst_type(&p.ty, params, args),
                span: p.span,
                default: p.default.clone(),
            })
            .collect(),
        ret: f.ret.as_ref().map(|t| subst_type(t, params, args)),
        body: subst_block(&f.body, params, args),
        attrs: f.attrs.clone(),
        span: f.span,
        is_override: f.is_override,
    }
}

pub(super) fn subst_block(b: &Block, params: &[Symbol], args: &[Type]) -> Block {
    Block {
        stmts: b.stmts.iter().map(|s| subst_stmt(s, params, args)).collect(),
        tail: b.tail.as_ref().map(|e| Box::new(subst_expr(e, params, args))),
    }
}

pub(super) fn subst_stmt(s: &Stmt, params: &[Symbol], args: &[Type]) -> Stmt {
    let kind = match &s.kind {
        StmtKind::Let { name, ty, value, .. } => StmtKind::Let {
            is_pub: false,
                is_const: false,
            name: name.clone(),
            ty: ty.as_ref().map(|t| subst_type(t, params, args)),
            value: subst_expr(value, params, args),
        },
        StmtKind::LetTuple { elems, value } => StmtKind::LetTuple {
            elems: elems.clone(),
            value: subst_expr(value, params, args),
        },
        StmtKind::LetStruct { class, fields, value } => StmtKind::LetStruct {
            class: class.clone(),
            fields: fields.clone(),
            value: subst_expr(value, params, args),
        },
        StmtKind::Expr(e) => StmtKind::Expr(subst_expr(e, params, args)),
    };
    Stmt {
        kind,
        span: s.span,
        source_module: s.source_module.clone(),
    }
}

pub(super) fn subst_expr(e: &Expr, params: &[Symbol], args: &[Type]) -> Expr {
    let kind = match &e.kind {
        ExprKind::Int(n) => ExprKind::Int(*n),
        ExprKind::Float(f) => ExprKind::Float(*f),
        ExprKind::Bool(b) => ExprKind::Bool(*b),
        ExprKind::Str(s) => ExprKind::Str(s.clone()),
        ExprKind::Var(n) => ExprKind::Var(n.clone()),
        ExprKind::This => ExprKind::This,
        ExprKind::None => ExprKind::None,
        ExprKind::Some(inner) => ExprKind::Some(Box::new(subst_expr(inner, params, args))),
        ExprKind::Break(opt) => ExprKind::Break(opt.as_ref().map(|e| Box::new(subst_expr(e, params, args)))),
        ExprKind::Continue => ExprKind::Continue,
        ExprKind::Unary { op, expr } => ExprKind::Unary {
            op: *op,
            expr: Box::new(subst_expr(expr, params, args)),
        },
        ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
            op: *op,
            lhs: Box::new(subst_expr(lhs, params, args)),
            rhs: Box::new(subst_expr(rhs, params, args)),
        },
        ExprKind::Logical { op, lhs, rhs } => ExprKind::Logical {
            op: *op,
            lhs: Box::new(subst_expr(lhs, params, args)),
            rhs: Box::new(subst_expr(rhs, params, args)),
        },
        ExprKind::Cast { expr, ty } => ExprKind::Cast {
            expr: Box::new(subst_expr(expr, params, args)),
            ty: subst_type(ty, params, args),
        },
        ExprKind::TypeTest { expr, ty } => ExprKind::TypeTest {
            expr: Box::new(subst_expr(expr, params, args)),
            ty: subst_type(ty, params, args),
        },
        ExprKind::TypeDowncast { expr, ty } => ExprKind::TypeDowncast {
            expr: Box::new(subst_expr(expr, params, args)),
            ty: subst_type(ty, params, args),
        },
        ExprKind::FnExpr {
            params: ps,
            ret,
            body,
        } => ExprKind::FnExpr {
            params: ps
                .iter()
                .map(|p| ilang_ast::Param {
                    name: p.name.clone(),
                    ty: subst_type(&p.ty, params, args),
                    span: p.span,
                    default: p.default.clone(),
                })
                .collect(),
            ret: ret.as_ref().map(|t| subst_type(t, params, args)),
            body: subst_block(body, params, args),
        },
        ExprKind::Call { callee, args: a } => ExprKind::Call {
            callee: callee.clone(),
            args: a.iter().map(|x| subst_expr(x, params, args)).collect(),
        },
        ExprKind::SuperCall { method, args: a } => ExprKind::SuperCall {
            method: method.clone(),
            args: a.iter().map(|x| subst_expr(x, params, args)).collect(),
        },
        ExprKind::Closure { fn_name, captures } => ExprKind::Closure {
            fn_name: fn_name.clone(),
            captures: captures.clone(),
        },
        ExprKind::Field { obj, name } => ExprKind::Field {
            obj: Box::new(subst_expr(obj, params, args)),
            name: name.clone(),
        },
        ExprKind::MethodCall { obj, method, args: a } => ExprKind::MethodCall {
            obj: Box::new(subst_expr(obj, params, args)),
            method: method.clone(),
            args: a.iter().map(|x| subst_expr(x, params, args)).collect(),
        },
        ExprKind::New {
            class,
            type_args,
            args: a,
            init_method,
        } => ExprKind::New {
            class: class.clone(),
            type_args: type_args.iter().map(|t| subst_type(t, params, args)).collect(),
            args: a.iter().map(|x| subst_expr(x, params, args)).collect(),
            init_method: init_method.clone(),
        },
        ExprKind::Block(b) => ExprKind::Block(subst_block(b, params, args)),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => ExprKind::If {
            cond: Box::new(subst_expr(cond, params, args)),
            then_branch: subst_block(then_branch, params, args),
            else_branch: else_branch.as_ref().map(|e| Box::new(subst_expr(e, params, args))),
        },
        ExprKind::IfLet {
            name,
            expr,
            then_branch,
            else_branch,
        } => ExprKind::IfLet {
                name: name.clone(),
            expr: Box::new(subst_expr(expr, params, args)),
            then_branch: subst_block(then_branch, params, args),
            else_branch: else_branch.as_ref().map(|e| Box::new(subst_expr(e, params, args))),
        },
        ExprKind::While { cond, body } => ExprKind::While {
            cond: Box::new(subst_expr(cond, params, args)),
            body: subst_block(body, params, args),
        },
        ExprKind::Loop { body } => ExprKind::Loop {
            body: subst_block(body, params, args),
        },
        ExprKind::ForIn { var, iter, body } => ExprKind::ForIn {
            var: var.clone(),
            iter: Box::new(subst_expr(iter, params, args)),
            body: subst_block(body, params, args),
        },
        ExprKind::Range { start, end, inclusive } => ExprKind::Range {
            start: start.as_ref().map(|s| Box::new(subst_expr(s, params, args))),
            end: end.as_ref().map(|e| Box::new(subst_expr(e, params, args))),
            inclusive: *inclusive,
        },
        ExprKind::Return(opt) => ExprKind::Return(
            opt.as_ref().map(|e| Box::new(subst_expr(e, params, args))),
        ),
        ExprKind::Assign { target, value } => ExprKind::Assign {
            target: target.clone(),
            value: Box::new(subst_expr(value, params, args)),
        },
        ExprKind::AssignField { obj, field, value, is_init } => ExprKind::AssignField {
            obj: obj.clone(),
            field: field.clone(),
            value: Box::new(subst_expr(value, params, args)), is_init: *is_init },
        ExprKind::AssignIndex { obj, index, value } => ExprKind::AssignIndex {
            obj: obj.clone(),
            index: index.clone(),
            value: Box::new(subst_expr(value, params, args)),
        },
        ExprKind::Array(items) => ExprKind::Array(
            items.iter().map(|e| subst_expr(e, params, args)).collect(),
        ),
        ExprKind::Tuple(items) => ExprKind::Tuple(
            items.iter().map(|e| subst_expr(e, params, args)).collect(),
        ),
        ExprKind::StructLit { class, fields } => ExprKind::StructLit {
            class: class.clone(),
            fields: fields
                .iter()
                .map(|(n, e)| (n.clone(), subst_expr(e, params, args)))
                .collect(),
        },
        ExprKind::MapLit(entries) => ExprKind::MapLit(
            entries
                .iter()
                .map(|(k, v)| (subst_expr(k, params, args), subst_expr(v, params, args)))
                .collect(),
        ),
        ExprKind::Index { obj, index } => ExprKind::Index {
            obj: Box::new(subst_expr(obj, params, args)),
            index: Box::new(subst_expr(index, params, args)),
        },
        ExprKind::EnumCtor {
            enum_name,
            variant,
            args: a,
        } => ExprKind::EnumCtor {
            enum_name: enum_name.clone(),
            variant: variant.clone(),
            args: match a {
                ilang_ast::CtorArgs::Unit => ilang_ast::CtorArgs::Unit,
                ilang_ast::CtorArgs::Tuple(es) => ilang_ast::CtorArgs::Tuple(
                    es.iter().map(|e| subst_expr(e, params, args)).collect(),
                ),
                ilang_ast::CtorArgs::Struct(fs) => ilang_ast::CtorArgs::Struct(
                    fs.iter()
                        .map(|(n, e)| (n.clone(), subst_expr(e, params, args)))
                        .collect(),
                ),
            },
        },
        ExprKind::Match { scrutinee, arms } => ExprKind::Match {
            scrutinee: Box::new(subst_expr(scrutinee, params, args)),
            arms: arms
                .iter()
                .map(|arm| ilang_ast::MatchArm {
                    pattern: arm.pattern.clone(),
                    body: subst_expr(&arm.body, params, args),
                    span: arm.span,
                })
                .collect(),
        },
    };
    Expr {
        kind,
        span: e.span,
    }
}

pub(super) fn subst_type(t: &Type, params: &[Symbol], args: &[Type]) -> Type {
    match t {
        // The parser emits `Type::Object(name)` for any user-named type.
        // Inside a generic class body, references to the class's own
        // type parameters arrive here as `Object("T")` — treat those
        // as type variables and substitute. The type checker already
        // performs the same conceptual rewrite via `rewrite_type_params`.
        Type::Object(name) | Type::TypeVar(name) => params
            .iter()
            .position(|p| p == name)
            .and_then(|i| args.get(i).cloned())
            .unwrap_or_else(|| t.clone()),
        Type::Generic(g) => {
            let new_args: Vec<Type> =
                g.args.iter().map(|a| subst_type(a, params, args)).collect();
            // Once concrete (no TypeVar left), collapse to Object(mangled).
            let gen_ty = Type::generic(g.base.clone(), new_args.clone());
            if !contains_type_var(&gen_ty) {
                Type::Object(
                    InstKey {
                        class: g.base.clone(),
                        args: new_args.into(),
                    }
                    .mangled(),
                )
            } else {
                gen_ty
            }
        }
        Type::Array { elem, fixed } => Type::Array {
            elem: Box::new(subst_type(elem, params, args)),
            fixed: *fixed,
        },
        Type::Optional(inner) => Type::Optional(Box::new(subst_type(inner, params, args))),
        Type::Weak(inner) => Type::Weak(Box::new(subst_type(inner, params, args))),
        Type::Fn(ft) => Type::func(
            ft.params.iter().map(|p| subst_type(p, params, args)).collect(),
            subst_type(&ft.ret, params, args),
        ),
        _ => t.clone(),
    }
}

pub(super) fn rewrite_item(item: &Item) -> Item {
    match item {
        Item::Class(c) => Item::Class(ClassDecl {
            is_pub: false,
            extern_lib: c.extern_lib.clone(),
            is_repr_c: c.is_repr_c,
            is_packed: c.is_packed,
            is_union: c.is_union,
            name: c.name.clone(),
            parent: c.parent.clone(),
            type_params: c.type_params.clone(),
            fields: c
                .fields
                .iter()
                .map(|f| FieldDecl {
                    is_pub: false,
                    name: f.name.clone(),
                    ty: rewrite_type(&f.ty),
                    span: f.span, bits: f.bits,
                })
                .collect(),
            methods: c.methods.iter().map(rewrite_fn).collect(),
            static_methods: c.static_methods.iter().map(rewrite_fn).collect(),
            static_fields: c.static_fields.clone(),
            properties: c
                .properties
                .iter()
                .map(|p| ilang_ast::PropertyDecl {
                    is_pub: false,
                    name: p.name.clone(),
                    ty: rewrite_type(&p.ty),
                    getter: p.getter.as_ref().map(rewrite_fn),
                    setter: p.setter.as_ref().map(rewrite_fn),
                    span: p.span,
                })
                .collect(),
            span: c.span,
        }),
        Item::Fn(f) => Item::Fn(rewrite_fn(f)),
        Item::Enum(e) => Item::Enum(e.clone()),
        Item::Use(u) => Item::Use(u.clone()),
        Item::Const(c) => Item::Const(c.clone()),
        Item::ExternC(b) => Item::ExternC(b.clone()),
        
    }
}

pub(super) fn rewrite_fn(f: &FnDecl) -> FnDecl {
    FnDecl {
        is_pub: f.is_pub,
        name: f.name.clone(),
        type_params: f.type_params.clone(),
        params: f
            .params
            .iter()
            .map(|p| Param {
                name: p.name.clone(),
                ty: rewrite_type(&p.ty),
                span: p.span,
                default: p.default.clone(),
            })
            .collect(),
        ret: f.ret.as_ref().map(rewrite_type),
        body: rewrite_block(&f.body),
        attrs: f.attrs.clone(),
        span: f.span,
        is_override: f.is_override,
    }
}

pub(super) fn rewrite_block(b: &Block) -> Block {
    Block {
        stmts: b.stmts.iter().map(rewrite_stmt).collect(),
        tail: b.tail.as_ref().map(|e| Box::new(rewrite_expr(e))),
    }
}

pub(super) fn rewrite_stmt(s: &Stmt) -> Stmt {
    let kind = match &s.kind {
        StmtKind::Let { name, ty, value, .. } => StmtKind::Let {
            is_pub: false,
                is_const: false,
            name: name.clone(),
            ty: ty.as_ref().map(rewrite_type),
            value: rewrite_expr(value),
        },
        StmtKind::LetTuple { elems, value } => StmtKind::LetTuple {
            elems: elems.clone(),
            value: rewrite_expr(value),
        },
        StmtKind::LetStruct { class, fields, value } => StmtKind::LetStruct {
            class: class.clone(),
            fields: fields.clone(),
            value: rewrite_expr(value),
        },
        StmtKind::Expr(e) => StmtKind::Expr(rewrite_expr(e)),
    };
    Stmt {
        kind,
        span: s.span,
        source_module: s.source_module.clone(),
    }
}

pub(super) fn rewrite_expr(e: &Expr) -> Expr {
    let kind = match &e.kind {
        ExprKind::Cast { expr, ty } => ExprKind::Cast {
            expr: Box::new(rewrite_expr(expr)),
            ty: rewrite_type(ty),
        },
        ExprKind::TypeTest { expr, ty } => ExprKind::TypeTest {
            expr: Box::new(rewrite_expr(expr)),
            ty: rewrite_type(ty),
        },
        ExprKind::TypeDowncast { expr, ty } => ExprKind::TypeDowncast {
            expr: Box::new(rewrite_expr(expr)),
            ty: rewrite_type(ty),
        },
        ExprKind::FnExpr { params, ret, body } => ExprKind::FnExpr {
            params: params
                .iter()
                .map(|p| ilang_ast::Param {
                    name: p.name.clone(),
                    ty: rewrite_type(&p.ty),
                    span: p.span,
                    default: p.default.clone(),
                })
                .collect(),
            ret: ret.as_ref().map(rewrite_type),
            body: rewrite_block(body),
        },
        ExprKind::New { class, type_args, args, init_method } => {
            // Concrete generic instantiation → call into the
            // monomorphized class by its mangled name. Built-in generic
            // classes (Map) skip mangling — the JIT lowers `new Map<..>()`
            // by recognizing the class name + type_args directly.
            let new_args: Vec<Expr> = args.iter().map(rewrite_expr).collect();
            let new_type_args: Vec<Type> = type_args.iter().map(rewrite_type).collect();
            if type_args.is_empty() || is_builtin_generic_class(class.as_str()) {
                ExprKind::New {
                    class: class.clone(),
                    type_args: new_type_args.into(),
                    args: new_args.into(), init_method: init_method.clone(),
                }
            } else {
                let mangled = InstKey {
                    class: class.clone(),
                    args: new_type_args,
                }
                .mangled();
                ExprKind::New {
                    class: mangled,
                    type_args: Box::new([]),
                    args: new_args.into(), init_method: init_method.clone(),
                }
            }
        }
        // Mechanical recursion through the rest. We could derive this
        // if we had a Visitor trait, but the AST is small enough that
        // an explicit walk is the cheapest thing to read.
        ExprKind::Unary { op, expr } => ExprKind::Unary {
            op: *op,
            expr: Box::new(rewrite_expr(expr)),
        },
        ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
            op: *op,
            lhs: Box::new(rewrite_expr(lhs)),
            rhs: Box::new(rewrite_expr(rhs)),
        },
        ExprKind::Logical { op, lhs, rhs } => ExprKind::Logical {
            op: *op,
            lhs: Box::new(rewrite_expr(lhs)),
            rhs: Box::new(rewrite_expr(rhs)),
        },
        ExprKind::Call { callee, args } => ExprKind::Call {
            callee: callee.clone(),
            args: args.iter().map(rewrite_expr).collect(),
        },
        ExprKind::SuperCall { method, args } => ExprKind::SuperCall {
            method: method.clone(),
            args: args.iter().map(rewrite_expr).collect(),
        },
        ExprKind::Closure { fn_name, captures } => ExprKind::Closure {
            fn_name: fn_name.clone(),
            captures: captures.clone(),
        },
        ExprKind::Field { obj, name } => ExprKind::Field {
            obj: Box::new(rewrite_expr(obj)),
            name: name.clone(),
        },
        ExprKind::MethodCall { obj, method, args } => ExprKind::MethodCall {
            obj: Box::new(rewrite_expr(obj)),
            method: method.clone(),
            args: args.iter().map(rewrite_expr).collect(),
        },
        ExprKind::Block(b) => ExprKind::Block(rewrite_block(b)),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => ExprKind::If {
            cond: Box::new(rewrite_expr(cond)),
            then_branch: rewrite_block(then_branch),
            else_branch: else_branch.as_ref().map(|e| Box::new(rewrite_expr(e))),
        },
        ExprKind::IfLet {
            name,
            expr,
            then_branch,
            else_branch,
        } => ExprKind::IfLet {
                name: name.clone(),
            expr: Box::new(rewrite_expr(expr)),
            then_branch: rewrite_block(then_branch),
            else_branch: else_branch.as_ref().map(|e| Box::new(rewrite_expr(e))),
        },
        ExprKind::While { cond, body } => ExprKind::While {
            cond: Box::new(rewrite_expr(cond)),
            body: rewrite_block(body),
        },
        ExprKind::Loop { body } => ExprKind::Loop {
            body: rewrite_block(body),
        },
        ExprKind::ForIn { var, iter, body } => ExprKind::ForIn {
            var: var.clone(),
            iter: Box::new(rewrite_expr(iter)),
            body: rewrite_block(body),
        },
        ExprKind::Range { start, end, inclusive } => ExprKind::Range {
            start: start.as_ref().map(|s| Box::new(rewrite_expr(s))),
            end: end.as_ref().map(|e| Box::new(rewrite_expr(e))),
            inclusive: *inclusive,
        },
        ExprKind::Return(opt) => {
            ExprKind::Return(opt.as_ref().map(|e| Box::new(rewrite_expr(e))))
        }
        ExprKind::Assign { target, value } => ExprKind::Assign {
            target: target.clone(),
            value: Box::new(rewrite_expr(value)),
        },
        ExprKind::AssignField { obj, field, value, is_init } => ExprKind::AssignField {
            obj: obj.clone(),
            field: field.clone(),
            value: Box::new(rewrite_expr(value)), is_init: *is_init },
        ExprKind::AssignIndex { obj, index, value } => ExprKind::AssignIndex {
            obj: obj.clone(),
            index: index.clone(),
            value: Box::new(rewrite_expr(value)),
        },
        ExprKind::Array(items) => {
            ExprKind::Array(items.iter().map(rewrite_expr).collect())
        }
        ExprKind::Tuple(items) => {
            ExprKind::Tuple(items.iter().map(rewrite_expr).collect())
        }
        ExprKind::StructLit { class, fields } => ExprKind::StructLit {
            class: class.clone(),
            fields: fields.iter().map(|(n, e)| (n.clone(), rewrite_expr(e))).collect(),
        },
        ExprKind::MapLit(entries) => ExprKind::MapLit(
            entries
                .iter()
                .map(|(k, v)| (rewrite_expr(k), rewrite_expr(v)))
                .collect(),
        ),
        ExprKind::Index { obj, index } => ExprKind::Index {
            obj: Box::new(rewrite_expr(obj)),
            index: Box::new(rewrite_expr(index)),
        },
        ExprKind::Some(inner) => ExprKind::Some(Box::new(rewrite_expr(inner))),
        ExprKind::EnumCtor {
            enum_name,
            variant,
            args,
        } => ExprKind::EnumCtor {
            enum_name: enum_name.clone(),
            variant: variant.clone(),
            args: match args {
                ilang_ast::CtorArgs::Unit => ilang_ast::CtorArgs::Unit,
                ilang_ast::CtorArgs::Tuple(es) => {
                    ilang_ast::CtorArgs::Tuple(es.iter().map(rewrite_expr).collect())
                }
                ilang_ast::CtorArgs::Struct(fs) => ilang_ast::CtorArgs::Struct(
                    fs.iter().map(|(n, e)| (n.clone(), rewrite_expr(e))).collect(),
                ),
            },
        },
        ExprKind::Match { scrutinee, arms } => ExprKind::Match {
            scrutinee: Box::new(rewrite_expr(scrutinee)),
            arms: arms
                .iter()
                .map(|arm| ilang_ast::MatchArm {
                    pattern: arm.pattern.clone(),
                    body: rewrite_expr(&arm.body),
                    span: arm.span,
                })
                .collect(),
        },
        // Trivial nodes pass through.
        ExprKind::Int(n) => ExprKind::Int(*n),
        ExprKind::Float(f) => ExprKind::Float(*f),
        ExprKind::Bool(b) => ExprKind::Bool(*b),
        ExprKind::Str(s) => ExprKind::Str(s.clone()),
        ExprKind::Var(n) => ExprKind::Var(n.clone()),
        ExprKind::This => ExprKind::This,
        ExprKind::None => ExprKind::None,
        ExprKind::Break(opt) => ExprKind::Break(opt.as_ref().map(|e| Box::new(rewrite_expr(e)))),
        ExprKind::Continue => ExprKind::Continue,
    };
    Expr {
        kind,
        span: e.span,
    }
}

pub(super) fn is_generic_enum(name: &str) -> bool {
    GENERIC_ENUM_NAMES.with(|set| set.borrow().contains(&Symbol::intern(name)))
}

/// Built-in generic classes whose `Type::Generic { base, args }` should
/// flow through to the JIT verbatim (NOT mangled into a synthetic
/// `Type::Object` like user generic classes). The JIT recognizes these
/// names and produces dedicated `JitTy` variants for them.
pub(super) fn is_builtin_generic_class(name: &str) -> bool {
    name == "Map"
}

/// Collapse `Type::Generic { Box, [i64] }` to `Type::Object("Box<i64>")`
/// so the JIT pipeline (which only knows `Object`) routes to the
/// monomorphized class. Recurses through Array/Optional/Weak.
pub(super) fn rewrite_type(t: &Type) -> Type {
    match t {
        Type::Generic(g) => {
            let new_args: Vec<Type> = g.args.iter().map(rewrite_type).collect();
            // Generic enums aren't monomorphized — leave them as
            // `Type::Generic` so the JIT's `from_ast` errors with a
            // clear UnsupportedType. Built-in generic classes (Map)
            // are also kept as Generic — the JIT handles them
            // specially. User generic classes get the mangled name.
            // Built-in generic classes (Map) are kept as Generic so
            // the JIT handles them specially. Generic enums are also
            // left intact here — the separate `monomorphize_enums` pass
            // converts them to `Type::Object(mangled)` after this pass.
            // User generic classes get the mangled Object name now.
            if is_generic_enum(g.base.as_str()) || is_builtin_generic_class(g.base.as_str()) {
                Type::generic(g.base.clone(), new_args)
            } else {
                Type::Object(
                    InstKey {
                        class: g.base.clone(),
                        args: new_args.into(),
                    }
                    .mangled(),
                )
            }
        }
        Type::Array { elem, fixed } => Type::Array {
            elem: Box::new(rewrite_type(elem)),
            fixed: *fixed,
        },
        Type::Optional(inner) => Type::Optional(Box::new(rewrite_type(inner))),
        Type::Weak(inner) => Type::Weak(Box::new(rewrite_type(inner))),
        Type::Fn(ft) => Type::func(
            ft.params.iter().map(rewrite_type).collect(),
            rewrite_type(&ft.ret),
        ),
        _ => t.clone(),
    }
}

pub(super) fn mangle_fn_name(name: &str, args: &[Type]) -> Symbol {
    let mut s = name.to_string();
    s.push('<');
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&a.to_string());
    }
    s.push('>');
    Symbol::intern(&s)
}

