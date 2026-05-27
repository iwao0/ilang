//! Module-prefix walk. After `apply_use` has resolved which module
//! a top-level item belongs to, this pass rewrites the item's
//! declared names and intra-module references to their
//! `<module>.<name>` form so cross-module references at the call
//! site resolve against the merged Program.
//!
//! The walk is intentionally conservative: builtin types
//! (`Console` / `Map` / `Promise` / `Result` / `ObjCBlock`) and any
//! name that already contains a `.` are left alone — they're
//! either intrinsic or already qualified. Generic class type
//! parameters get accidentally swept up too, so
//! `unprefix_type_params_in_class` undoes the qualification for
//! identifiers listed in the class's `type_params`.

use std::collections::HashSet;

use ilang_ast::{Block, Expr, ExprKind, Item, MatchArm, Stmt, StmtKind, Symbol, Type};

use super::builtin::is_builtin_type;

pub(super) fn prefix_item(item: Item, prefix: &str) -> Item {
    match item {
        Item::Fn(mut f) => {
            f.name = format!("{prefix}.{}", f.name).into();
            f.params = f
                .params
                .iter()
                .map(|p| ilang_ast::Param {
                    name: p.name.clone(),
                    ty: prefix_type(&p.ty, prefix),
                    span: p.span,
                    default: p.default.clone().map(|d| prefix_expr(d, prefix)),
                })
                .collect();
            f.ret = f.ret.as_ref().map(|t| prefix_type(t, prefix));
            f.body = prefix_block_calls(f.body, prefix);
            Item::Fn(f)
        }
        Item::Class(mut c) => {
            prefix_class_decl(&mut c, prefix);
            Item::Class(c)
        }
        Item::Enum(mut e) => {
            e.name = format!("{prefix}.{}", e.name).into();
            for v in &mut e.variants {
                v.payload = match std::mem::replace(&mut v.payload, ilang_ast::VariantPayload::Unit) {
                    ilang_ast::VariantPayload::Unit => ilang_ast::VariantPayload::Unit,
                    ilang_ast::VariantPayload::Tuple(tys) => ilang_ast::VariantPayload::Tuple(
                        Vec::from(tys).into_iter().map(|t| prefix_type(&t, prefix)).collect(),
                    ),
                    ilang_ast::VariantPayload::Struct(fs) => {
                        ilang_ast::VariantPayload::Struct(
                            Vec::from(fs).into_iter()
                                .map(|mut fd| {
                                    fd.ty = prefix_type(&fd.ty, prefix);
                                    fd
                                })
                                .collect(),
                        )
                    }
                };
            }
            Item::Enum(e)
        }
        Item::Use(u) => Item::Use(u),
        Item::Const(mut c) => {
            c.name = format!("{prefix}.{}", c.name).into();
            c.ty = c.ty.as_ref().map(|t| prefix_type(t, prefix));
            // RHS is folded to a literal later by `inline_constants`,
            // but it can still contain `ModuleEnum.Variant` /
            // `ClassName.staticField` / `Call(fn)` references that
            // need the same prefix rewrite as fn bodies before the
            // fold runs.
            let value = std::mem::replace(
                &mut c.value,
                Expr::new(ExprKind::None, c.span),
            );
            c.value = prefix_expr(value, prefix);
            Item::Const(c)
        }
        Item::ExternC(mut b) => {
            // Prefix the ilang-side names of the block's items so
            // callers can write `module.fn` etc. For library-form
            // (@lib) FnDecls, preserve the original C symbol name in
            // `c_symbol` so dlsym still finds it after the ilang name
            // has been rewritten to the prefixed form. Host-form fns
            // (no @lib) keep using the prefixed name as the symbol —
            // host registration code uses the prefixed name to match.
            //
            // Field / param / ret / static types also get prefixed so
            // intra-block references (e.g. `*SDL_Window` returning
            // from a fn that declared the struct) keep resolving.
            for inner in &mut b.items {
                match inner {
                    ilang_ast::ExternCItem::Struct { name, fields, .. }
                    | ilang_ast::ExternCItem::Union { name, fields, .. } => {
                        *name = Symbol::intern(&format!("{prefix}.{name}")).into();
                        for f in fields {
                            f.ty = prefix_type(&f.ty, prefix);
                        }
                    }
                    ilang_ast::ExternCItem::FnDecl {
                        name, libs, c_symbol, params, ret, type_params, ..
                    } => {
                        if !libs.is_empty() && c_symbol.is_none() {
                            *c_symbol = Some(name.clone());
                        }
                        *name = Symbol::intern(&format!("{prefix}.{name}")).into();
                        // Generic params: rewrite `Object("T")` →
                        // `TypeVar("T")` before `prefix_type` runs, so
                        // the prefixer doesn't qualify `T` to
                        // `<prefix>.T`. `prefix_type` already passes
                        // TypeVar through unchanged.
                        for p in params.iter_mut() {
                            let lifted = lift_type_vars(&p.ty, type_params);
                            p.ty = prefix_type(&lifted, prefix);
                        }
                        if let Some(rt) = ret.as_mut() {
                            let lifted = lift_type_vars(rt, type_params);
                            *rt = prefix_type(&lifted, prefix);
                        }
                    }
                    ilang_ast::ExternCItem::FnDef(f) => {
                        f.name = format!("{prefix}.{}", f.name).into();
                        for p in f.params.iter_mut() {
                            p.ty = prefix_type(&p.ty, prefix);
                        }
                        if let Some(rt) = f.ret.as_mut() {
                            *rt = prefix_type(rt, prefix);
                        }
                        let body = std::mem::replace(
                            &mut f.body,
                            Block { stmts: Vec::new(), tail: None },
                        );
                        f.body = prefix_block_calls(body, prefix);
                    }
                    ilang_ast::ExternCItem::Class(c) => {
                        prefix_class_decl(c, prefix);
                    }
                }
            }
            // Prefix @objc interface declarations carried in the
            // sibling `interfaces` list so the post-merge type
            // checker / auto-lift can look them up by their
            // module-qualified name (the same form users get
            // after a `use M { … }` rewrite). A parent reference
            // that already contains a `.` was rewritten by the
            // `rename_in_item` pass to a cross-module form
            // (`ole32.IUnknown` etc.) — leave it alone so the
            // inheritance edge keeps pointing at the canonical
            // declaration rather than this module's namespace.
            for iface in b.interfaces.iter_mut() {
                iface.name = format!("{prefix}.{}", iface.name).into();
                if let Some(parent) = iface.parent.as_mut() {
                    if !parent.as_str().contains('.') {
                        *parent = format!("{prefix}.{}", parent).into();
                    }
                }
                for m in iface.methods.iter_mut() {
                    for p in m.params.iter_mut() {
                        p.ty = prefix_type(&p.ty, prefix);
                    }
                    m.ret = m.ret.as_ref().map(|t| prefix_type(t, prefix));
                }
            }
            // `pub const NULL: *void = …` inside the @extern(C)
            // block needs the same module-prefix treatment as
            // ordinary top-level `pub const`, so cross-module
            // selective imports (`use windows { NULL }`) resolve
            // through the loader's qualified name.
            for c in b.consts.iter_mut() {
                c.name = format!("{prefix}.{}", c.name).into();
                if let Some(t) = c.ty.as_mut() {
                    *t = prefix_type(t, prefix);
                }
            }
            Item::ExternC(b)
        }
        Item::Interface(mut i) => {
            i.name = format!("{prefix}.{}", i.name).into();
            // `@com interface X : IUnknown { … }` carries a parent
            // name that has to live in the same module-prefixed form
            // as the class-side `extends`, so vtable-slot inheritance
            // resolves after the loader merge. An already-qualified
            // parent (one that contains `.`) came from a
            // `use M { Y }` rewrite — leave it pointing at the
            // canonical declaration instead of dragging it into the
            // local namespace.
            if let Some(parent) = i.parent.as_mut() {
                if !parent.as_str().contains('.') {
                    *parent = format!("{prefix}.{}", parent).into();
                }
            }
            for m in i.methods.iter_mut() {
                for p in m.params.iter_mut() {
                    p.ty = prefix_type(&p.ty, prefix);
                }
                m.ret = m.ret.as_ref().map(|t| prefix_type(t, prefix));
            }
            Item::Interface(i)
        }
    }
}

fn prefix_class_decl(c: &mut ilang_ast::ClassDecl, prefix: &str) {
    c.name = format!("{prefix}.{}", c.name).into();
    if let Some(parent) = c.parent.as_mut() {
        *parent = prefix_type_name(parent, prefix);
    }
    for ifn in c.interfaces.iter_mut() {
        *ifn = prefix_type_name(ifn, prefix);
    }
    for m in c.methods.iter_mut().chain(c.static_methods.iter_mut()) {
        let body = std::mem::replace(
            &mut m.body,
            Block { stmts: Vec::new(), tail: None },
        );
        m.body = prefix_block_calls(body, prefix);
        m.params = m
            .params
            .iter()
            .map(|p| ilang_ast::Param {
                name: p.name.clone(),
                ty: prefix_type(&p.ty, prefix),
                span: p.span,
                default: p.default.clone().map(|d| prefix_expr(d, prefix)),
            })
            .collect();
        m.ret = m.ret.as_ref().map(|t| prefix_type(t, prefix));
    }
    for f in &mut c.fields {
        f.ty = prefix_type(&f.ty, prefix);
    }
    for sf in &mut c.static_fields {
        sf.ty = prefix_type(&sf.ty, prefix);
        let value = std::mem::replace(
            &mut sf.value,
            Expr::new(ExprKind::None, sf.span),
        );
        sf.value = prefix_expr(value, prefix);
    }
    for prop in &mut c.properties {
        prop.ty = prefix_type(&prop.ty, prefix);
        if let Some(g) = prop.getter.as_mut() {
            let body = std::mem::replace(
                &mut g.body,
                Block { stmts: Vec::new(), tail: None },
            );
            g.body = prefix_block_calls(body, prefix);
            g.ret = g.ret.as_ref().map(|t| prefix_type(t, prefix));
        }
        if let Some(s) = prop.setter.as_mut() {
            let body = std::mem::replace(
                &mut s.body,
                Block { stmts: Vec::new(), tail: None },
            );
            s.body = prefix_block_calls(body, prefix);
            s.params = s
                .params
                .iter()
                .map(|p| ilang_ast::Param {
                    name: p.name.clone(),
                    ty: prefix_type(&p.ty, prefix),
                    span: p.span,
                    default: p.default.clone().map(|d| prefix_expr(d, prefix)),
                })
                .collect();
        }
    }
    // The class's own type parameters look like bare `Object`
    // names at parse time (the type checker is what later
    // distinguishes them as `TypeVar`s). The prefix walk above
    // accidentally turned them into `prefix.T`; sweep the body
    // and roll those back. Doing it as a post-pass avoids
    // threading an exclusion set through every recursive
    // `prefix_*` helper.
    if !c.type_params.is_empty() {
        let type_params: HashSet<Symbol> = c.type_params.iter().cloned().collect();
        unprefix_type_params_in_class(c, prefix, &type_params);
    }
}

fn unprefix_type_params_in_class(
    c: &mut ilang_ast::ClassDecl,
    prefix: &str,
    type_params: &HashSet<Symbol>,
) {
    for f in c.fields.iter_mut() {
        unprefix_type_params(&mut f.ty, prefix, type_params);
    }
    for sf in c.static_fields.iter_mut() {
        unprefix_type_params(&mut sf.ty, prefix, type_params);
    }
    for prop in c.properties.iter_mut() {
        unprefix_type_params(&mut prop.ty, prefix, type_params);
    }
    for m in c.methods.iter_mut().chain(c.static_methods.iter_mut()) {
        for p in m.params.iter_mut() {
            unprefix_type_params(&mut p.ty, prefix, type_params);
        }
        if let Some(t) = m.ret.as_mut() {
            unprefix_type_params(t, prefix, type_params);
        }
        unprefix_type_params_in_block(&mut m.body, prefix, type_params);
    }
}

fn unprefix_type_params_in_block(
    b: &mut Block,
    prefix: &str,
    type_params: &HashSet<Symbol>,
) {
    for s in b.stmts.iter_mut() {
        unprefix_type_params_in_stmt(s, prefix, type_params);
    }
    if let Some(t) = b.tail.as_mut() {
        unprefix_type_params_in_expr(t, prefix, type_params);
    }
}

fn unprefix_type_params_in_stmt(
    s: &mut Stmt,
    prefix: &str,
    type_params: &HashSet<Symbol>,
) {
    match &mut s.kind {
        StmtKind::Let { ty, value, .. } => {
            if let Some(t) = ty.as_mut() {
                unprefix_type_params(t, prefix, type_params);
            }
            unprefix_type_params_in_expr(value, prefix, type_params);
        }
        StmtKind::LetTuple { value, .. } | StmtKind::LetStruct { value, .. } => {
            unprefix_type_params_in_expr(value, prefix, type_params);
        }
        StmtKind::Expr(e) => unprefix_type_params_in_expr(e, prefix, type_params),
    }
}

fn unprefix_type_params_in_expr(
    e: &mut Expr,
    prefix: &str,
    type_params: &HashSet<Symbol>,
) {
    match &mut e.kind {
        ExprKind::Cast { expr, ty }
        | ExprKind::TypeTest { expr, ty }
        | ExprKind::TypeDowncast { expr, ty } => {
            unprefix_type_params(ty, prefix, type_params);
            unprefix_type_params_in_expr(expr, prefix, type_params);
        }
        ExprKind::FnExpr { params, ret, body } => {
            for p in params.iter_mut() {
                unprefix_type_params(&mut p.ty, prefix, type_params);
            }
            if let Some(t) = ret.as_mut() {
                unprefix_type_params(t, prefix, type_params);
            }
            unprefix_type_params_in_block(body, prefix, type_params);
        }
        ExprKind::New { type_args, args, .. } => {
            for t in type_args.iter_mut() {
                unprefix_type_params(t, prefix, type_params);
            }
            for a in args.iter_mut() {
                unprefix_type_params_in_expr(a, prefix, type_params);
            }
        }
        ExprKind::Block(b) => unprefix_type_params_in_block(b, prefix, type_params),
        ExprKind::If { cond, then_branch, else_branch } => {
            unprefix_type_params_in_expr(cond, prefix, type_params);
            unprefix_type_params_in_block(then_branch, prefix, type_params);
            if let Some(e2) = else_branch.as_mut() {
                unprefix_type_params_in_expr(e2, prefix, type_params);
            }
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            unprefix_type_params_in_expr(expr, prefix, type_params);
            unprefix_type_params_in_block(then_branch, prefix, type_params);
            if let Some(e2) = else_branch.as_mut() {
                unprefix_type_params_in_expr(e2, prefix, type_params);
            }
        }
        ExprKind::While { cond, body } => {
            unprefix_type_params_in_expr(cond, prefix, type_params);
            unprefix_type_params_in_block(body, prefix, type_params);
        }
        ExprKind::Loop { body } => unprefix_type_params_in_block(body, prefix, type_params),
        ExprKind::ForIn { iter, body, .. } => {
            unprefix_type_params_in_expr(iter, prefix, type_params);
            unprefix_type_params_in_block(body, prefix, type_params);
        }
        ExprKind::Match { scrutinee, arms } => {
            unprefix_type_params_in_expr(scrutinee, prefix, type_params);
            for arm in arms.iter_mut() {
                unprefix_type_params_in_expr(&mut arm.body, prefix, type_params);
            }
        }
        ExprKind::Call { args, .. } => {
            for a in args.iter_mut() {
                unprefix_type_params_in_expr(a, prefix, type_params);
            }
        }
        ExprKind::MethodCall { obj, args, .. } => {
            unprefix_type_params_in_expr(obj, prefix, type_params);
            for a in args.iter_mut() {
                unprefix_type_params_in_expr(a, prefix, type_params);
            }
        }
        ExprKind::SuperCall { args, .. } => {
            for a in args.iter_mut() {
                unprefix_type_params_in_expr(a, prefix, type_params);
            }
        }
        ExprKind::Field { obj, .. } => unprefix_type_params_in_expr(obj, prefix, type_params),
        ExprKind::AssignField { obj, value, .. } => {
            unprefix_type_params_in_expr(obj, prefix, type_params);
            unprefix_type_params_in_expr(value, prefix, type_params);
        }
        ExprKind::Index { obj, index } => {
            unprefix_type_params_in_expr(obj, prefix, type_params);
            unprefix_type_params_in_expr(index, prefix, type_params);
        }
        ExprKind::AssignIndex { obj, index, value } => {
            unprefix_type_params_in_expr(obj, prefix, type_params);
            unprefix_type_params_in_expr(index, prefix, type_params);
            unprefix_type_params_in_expr(value, prefix, type_params);
        }
        ExprKind::Unary { expr, .. } => unprefix_type_params_in_expr(expr, prefix, type_params),
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            unprefix_type_params_in_expr(lhs, prefix, type_params);
            unprefix_type_params_in_expr(rhs, prefix, type_params);
        }
        ExprKind::Assign { value, .. } => {
            unprefix_type_params_in_expr(value, prefix, type_params);
        }
        ExprKind::Return(v) | ExprKind::Break(v) => {
            if let Some(e2) = v.as_mut() {
                unprefix_type_params_in_expr(e2, prefix, type_params);
            }
        }
        ExprKind::Some(inner) => unprefix_type_params_in_expr(inner, prefix, type_params),
        ExprKind::Await(inner) => unprefix_type_params_in_expr(inner, prefix, type_params),
        ExprKind::Array(items) | ExprKind::Tuple(items) => {
            for item in items.iter_mut() {
                unprefix_type_params_in_expr(item, prefix, type_params);
            }
        }
        ExprKind::MapLit(entries) => {
            for (k, v) in entries.iter_mut() {
                unprefix_type_params_in_expr(k, prefix, type_params);
                unprefix_type_params_in_expr(v, prefix, type_params);
            }
        }
        ExprKind::EnumCtor { args, .. } => match args {
            ilang_ast::CtorArgs::Tuple(es) => {
                for e in es.iter_mut() {
                    unprefix_type_params_in_expr(e, prefix, type_params);
                }
            }
            ilang_ast::CtorArgs::Struct(fs) => {
                for (_, e) in fs.iter_mut() {
                    unprefix_type_params_in_expr(e, prefix, type_params);
                }
            }
            ilang_ast::CtorArgs::Unit => {}
        },
        _ => {}
    }
}

fn unprefix_type_params(
    t: &mut Type,
    prefix: &str,
    type_params: &HashSet<Symbol>,
) {
    let candidate = format!("{prefix}.");
    let unprefix_name = |name: &Symbol| -> Option<Symbol> {
        let s = name.as_str();
        let rest = s.strip_prefix(&candidate)?;
        let rest_sym: Symbol = Symbol::intern(rest);
        if type_params.contains(&rest_sym) {
            Some(rest_sym)
        } else {
            None
        }
    };
    match t {
        Type::Object(name) => {
            if let Some(orig) = unprefix_name(name) {
                *name = orig;
            }
        }
        Type::Array { elem, .. } => unprefix_type_params(elem, prefix, type_params),
        Type::Optional(inner) | Type::Weak(inner) => {
            unprefix_type_params(inner, prefix, type_params);
        }
        Type::Generic(g) => {
            if let Some(orig) = unprefix_name(&g.base) {
                g.base = orig;
            }
            for a in g.args.iter_mut() {
                unprefix_type_params(a, prefix, type_params);
            }
        }
        Type::Fn(ft) => {
            for p in ft.params.iter_mut() {
                unprefix_type_params(p, prefix, type_params);
            }
            unprefix_type_params(&mut ft.ret, prefix, type_params);
        }
        Type::RawPtr { inner, .. } => unprefix_type_params(inner, prefix, type_params),
        _ => {}
    }
}

fn prefix_type_name(name: &Symbol, prefix: &str) -> Symbol {
    if name.as_str().contains('.') {
        name.clone()
    } else {
        Symbol::intern(&format!("{prefix}.{name}"))
    }
}

/// Within a prefixed item, references to other top-level items from
/// the same module should also resolve to their prefixed names. We
/// don't have full symbol info here, so we use a heuristic: rewrite
/// bare `Call { callee: name }` and bare `Type::Object(name)` /
/// `Type::Generic { base, .. }` only when the name is *not* already
/// in the prefixed form. This is intentionally conservative — for
/// MVP we only rewrite Calls. Other forms (class refs from inside)
/// stay bare and can be cross-resolved by the type checker.
fn prefix_block_calls(b: Block, prefix: &str) -> Block {
    Block {
        stmts: Vec::from(b.stmts).into_iter().map(|s| prefix_stmt(s, prefix)).collect(),
        tail: b.tail.map(|e| Box::new(prefix_expr(*e, prefix))),
    }
}

pub(super) fn prefix_stmt(s: Stmt, prefix: &str) -> Stmt {
    let kind = match s.kind {
        StmtKind::Let { is_pub, is_const, name, ty, value } => StmtKind::Let {
            is_pub,
            is_const,
            name,
            ty: ty.map(|t| prefix_type(&t, prefix)),
            value: prefix_expr(value, prefix),
        },
        StmtKind::LetTuple { elems, value } => StmtKind::LetTuple {
            elems,
            value: prefix_expr(value, prefix),
        },
        StmtKind::LetStruct { class, fields, value } => StmtKind::LetStruct {
            class,
            fields,
            value: prefix_expr(value, prefix),
        },
        StmtKind::Expr(e) => StmtKind::Expr(prefix_expr(e, prefix)),
    };
    Stmt { kind, span: s.span, source_module: s.source_module.clone() }
}

fn prefix_expr(e: Expr, prefix: &str) -> Expr {
    let span = e.span;
    let kind = match e.kind {
        // Function calls: callee qualification has already been
        // done by the earlier `qualify_var_refs` pass (it has the
        // module's top-level fn-name set, so locally-bound
        // closure callees like `let f = ...; f(v)` stay bare and
        // don't get accidentally rewritten to `module.f(v)`).
        // Just recurse into the arguments here.
        ExprKind::Call { callee, args } => ExprKind::Call {
            callee,
            args: Vec::from(args).into_iter().map(|a| prefix_expr(a, prefix)).collect(),
        },
        ExprKind::New { class, type_args, args, init_method } => ExprKind::New {
            // `new module.Class(...)` already qualified — leave as
            // is; only re-prefix bare names so a second pass
            // doesn't produce `module.module.Class`. Builtin
            // types (`Map`, `Result`, …) are also left bare so
            // `new Map<...>()` inside a stdlib module doesn't
            // get rewritten to `new module.Map<...>()`.
            class: if class.as_str().contains('.') || is_builtin_type(class.as_str()) {
                class
            } else {
                format!("{prefix}.{}", class).into()
            },
            type_args: Vec::from(type_args).into_iter().map(|t| prefix_type(&t, prefix)).collect(),
            args: Vec::from(args).into_iter().map(|a| prefix_expr(a, prefix)).collect(),
            init_method,
        },
        ExprKind::EnumCtor {
            enum_name,
            variant,
            args,
        } => ExprKind::EnumCtor {
            enum_name: if enum_name.as_str().contains('.')
                || is_builtin_type(enum_name.as_str())
            {
                enum_name
            } else {
                format!("{prefix}.{}", enum_name).into()
            },
            variant,
            args: match args {
                ilang_ast::CtorArgs::Unit => ilang_ast::CtorArgs::Unit,
                ilang_ast::CtorArgs::Tuple(es) => ilang_ast::CtorArgs::Tuple(
                    Vec::from(es).into_iter().map(|e| prefix_expr(e, prefix)).collect(),
                ),
                ilang_ast::CtorArgs::Struct(fs) => ilang_ast::CtorArgs::Struct(
                    fs.into_iter()
                        .map(|(n, e)| (n, prefix_expr(e, prefix)))
                        .collect(),
                ),
            },
        },
        ExprKind::Cast { expr, ty } => ExprKind::Cast {
            expr: Box::new(prefix_expr(*expr, prefix)),
            ty: prefix_type(&ty, prefix),
        },
        ExprKind::TypeTest { expr, ty } => ExprKind::TypeTest {
            expr: Box::new(prefix_expr(*expr, prefix)),
            ty: prefix_type(&ty, prefix),
        },
        ExprKind::TypeDowncast { expr, ty } => ExprKind::TypeDowncast {
            expr: Box::new(prefix_expr(*expr, prefix)),
            ty: prefix_type(&ty, prefix),
        },
        ExprKind::FnExpr { params, ret, body } => ExprKind::FnExpr {
            params: params
                .into_iter()
                .map(|p| ilang_ast::Param {
                    name: p.name,
                    ty: prefix_type(&p.ty, prefix),
                    span: p.span,
                    default: p.default.map(|d| prefix_expr(d, prefix)),
                })
                .collect(),
            ret: ret.map(|t| prefix_type(&t, prefix)),
            body: prefix_block_calls(body, prefix),
        },
        // Recurse mechanically through everything else.
        ExprKind::Unary { op, expr } => ExprKind::Unary {
            op,
            expr: Box::new(prefix_expr(*expr, prefix)),
        },
        ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
            op,
            lhs: Box::new(prefix_expr(*lhs, prefix)),
            rhs: Box::new(prefix_expr(*rhs, prefix)),
        },
        ExprKind::Logical { op, lhs, rhs } => ExprKind::Logical {
            op,
            lhs: Box::new(prefix_expr(*lhs, prefix)),
            rhs: Box::new(prefix_expr(*rhs, prefix)),
        },
        ExprKind::Field { obj, name } => ExprKind::Field {
            obj: Box::new(prefix_expr(*obj, prefix)),
            name,
        },
        ExprKind::MethodCall { obj, method, args } => ExprKind::MethodCall {
            obj: Box::new(prefix_expr(*obj, prefix)),
            method,
            args: Vec::from(args).into_iter().map(|a| prefix_expr(a, prefix)).collect(),
        },
        ExprKind::Block(b) => ExprKind::Block(prefix_block_calls(b, prefix)),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => ExprKind::If {
            cond: Box::new(prefix_expr(*cond, prefix)),
            then_branch: prefix_block_calls(then_branch, prefix),
            else_branch: else_branch.map(|e| Box::new(prefix_expr(*e, prefix))),
        },
        ExprKind::IfLet {
            name,
            expr,
            then_branch,
            else_branch,
        } => ExprKind::IfLet {
            name,
            expr: Box::new(prefix_expr(*expr, prefix)),
            then_branch: prefix_block_calls(then_branch, prefix),
            else_branch: else_branch.map(|e| Box::new(prefix_expr(*e, prefix))),
        },
        ExprKind::While { cond, body } => ExprKind::While {
            cond: Box::new(prefix_expr(*cond, prefix)),
            body: prefix_block_calls(body, prefix),
        },
        ExprKind::Loop { body } => ExprKind::Loop {
            body: prefix_block_calls(body, prefix),
        },
        ExprKind::ForIn { var, iter, body } => ExprKind::ForIn {
            var,
            iter: Box::new(prefix_expr(*iter, prefix)),
            body: prefix_block_calls(body, prefix),
        },
        ExprKind::Range { start, end, inclusive } => ExprKind::Range {
            start: start.map(|s| Box::new(prefix_expr(*s, prefix))),
            end: end.map(|e| Box::new(prefix_expr(*e, prefix))),
            inclusive,
        },
        ExprKind::Closure { fn_name, captures } => {
            ExprKind::Closure { fn_name, captures }
        }
        ExprKind::SuperCall { method, args } => ExprKind::SuperCall {
            method,
            args: Vec::from(args).into_iter().map(|a| prefix_expr(a, prefix)).collect(),
        },
        ExprKind::Return(opt) => ExprKind::Return(opt.map(|e| Box::new(prefix_expr(*e, prefix)))),
        ExprKind::Break(opt) => ExprKind::Break(opt.map(|e| Box::new(prefix_expr(*e, prefix)))),
        ExprKind::Assign { target, value } => ExprKind::Assign {
            target,
            value: Box::new(prefix_expr(*value, prefix)),
        },
        ExprKind::AssignField { obj, field, value, is_init } => ExprKind::AssignField {
            obj: Box::new(prefix_expr(*obj, prefix)),
            field,
            value: Box::new(prefix_expr(*value, prefix)), is_init },
        ExprKind::AssignIndex { obj, index, value } => ExprKind::AssignIndex {
            obj: Box::new(prefix_expr(*obj, prefix)),
            index: Box::new(prefix_expr(*index, prefix)),
            value: Box::new(prefix_expr(*value, prefix)),
        },
        ExprKind::Array(items) => {
            ExprKind::Array(Vec::from(items).into_iter().map(|e| prefix_expr(e, prefix)).collect())
        }
        ExprKind::Tuple(items) => {
            ExprKind::Tuple(Vec::from(items).into_iter().map(|e| prefix_expr(e, prefix)).collect())
        }
        ExprKind::MapLit(entries) => ExprKind::MapLit(
            Vec::from(entries)
                .into_iter()
                .map(|(k, v)| (prefix_expr(k, prefix), prefix_expr(v, prefix)))
                .collect(),
        ),
        ExprKind::Index { obj, index } => ExprKind::Index {
            obj: Box::new(prefix_expr(*obj, prefix)),
            index: Box::new(prefix_expr(*index, prefix)),
        },
        ExprKind::Some(inner) => ExprKind::Some(Box::new(prefix_expr(*inner, prefix))),
        ExprKind::Await(inner) => ExprKind::Await(Box::new(prefix_expr(*inner, prefix))),
        ExprKind::Match { scrutinee, arms } => ExprKind::Match {
            scrutinee: Box::new(prefix_expr(*scrutinee, prefix)),
            arms: arms
                .into_iter()
                .map(|arm: MatchArm| MatchArm {
                    pattern: arm.pattern,
                    body: prefix_expr(arm.body, prefix),
                    span: arm.span,
                })
                .collect(),
        },
        ExprKind::Template { parts } => ExprKind::Template {
            parts: Vec::from(parts)
                .into_iter()
                .map(|p| match p {
                    ilang_ast::TemplatePart::Str(s) => ilang_ast::TemplatePart::Str(s),
                    ilang_ast::TemplatePart::Expr(e) => {
                        ilang_ast::TemplatePart::Expr(prefix_expr(e, prefix))
                    }
                })
                .collect(),
        },
        // Trivial nodes pass through.
        other @ (ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::Var(_)
        | ExprKind::This
        | ExprKind::None
        | ExprKind::Continue) => other,
        // Struct literals are desugared by `normalize` before the
        // loader walks anything; reaching this arm means a module
        // skipped that pass.
        ExprKind::StructLit { class, fields, field_name_spans } => ExprKind::StructLit {
            class,
            fields: fields
                .into_iter()
                .map(|(n, e)| (n, prefix_expr(e, prefix)))
                .collect(),
            field_name_spans,
        },
    };
    Expr { kind, span }
}

/// Convert `Type::Object(name)` to `Type::TypeVar(name)` for every
/// `name` listed in `type_params`. The parser produces `Object` for
/// any bare uppercase identifier; the type-checker would normally do
/// this rewrite later (via `sigs::rewrite_type_params`), but
/// `prefix_item` in the loader would already have qualified `T` to
/// `<prefix>.T` by then. Running the lift here keeps the FnDecl
/// signature intact across module merging.
fn lift_type_vars(t: &Type, type_params: &[Symbol]) -> Type {
    match t {
        Type::Object(name) if type_params.iter().any(|p| p == name) => {
            Type::TypeVar(name.clone())
        }
        Type::Array { elem, fixed } => Type::Array {
            elem: Box::new(lift_type_vars(elem, type_params)),
            fixed: *fixed,
        },
        Type::Optional(inner) => {
            Type::Optional(Box::new(lift_type_vars(inner, type_params)))
        }
        Type::Weak(inner) => {
            Type::Weak(Box::new(lift_type_vars(inner, type_params)))
        }
        Type::Generic(g) => Type::generic(
            g.base.clone(),
            g.args
                .iter()
                .map(|a| lift_type_vars(a, type_params))
                .collect(),
        ),
        Type::Tuple(elems) => Type::Tuple(
            elems
                .iter()
                .map(|e| lift_type_vars(e, type_params))
                .collect(),
        ),
        Type::Fn(ft) => Type::func(
            ft.params
                .iter()
                .map(|p| lift_type_vars(p, type_params))
                .collect(),
            lift_type_vars(&ft.ret, type_params),
        ),
        Type::RawPtr { is_const, inner } => Type::RawPtr {
            is_const: *is_const,
            inner: Box::new(lift_type_vars(inner, type_params)),
        },
        _ => t.clone(),
    }
}

fn prefix_type(t: &Type, prefix: &str) -> Type {
    match t {
        Type::Object(name) if !name.as_str().contains('.') && !is_builtin_type(&name.as_str()) => {
            Type::Object(Symbol::intern(&format!("{prefix}.{name}")).into())
        }
        Type::Array { elem, fixed } => Type::Array {
            elem: Box::new(prefix_type(elem, prefix)),
            fixed: *fixed,
        },
        Type::Optional(inner) => Type::Optional(Box::new(prefix_type(inner, prefix))),
        Type::Weak(inner) => Type::Weak(Box::new(prefix_type(inner, prefix))),
        Type::Generic(g) => Type::generic(
            if !g.base.as_str().contains('.') && !is_builtin_type(g.base.as_str()) {
                Symbol::intern(&format!("{prefix}.{}", g.base))
            } else {
                g.base
            },
            g.args.iter().map(|a| prefix_type(a, prefix)).collect(),
        ),
        Type::Fn(ft) => Type::func(
            ft.params.iter().map(|p| prefix_type(p, prefix)).collect(),
            prefix_type(&ft.ret, prefix),
        ),
        Type::RawPtr { is_const, inner } => Type::RawPtr {
            is_const: *is_const,
            inner: Box::new(prefix_type(inner, prefix)),
        },
        Type::Tuple(elems) => Type::Tuple(
            elems.iter().map(|e| prefix_type(e, prefix)).collect(),
        ),
        _ => t.clone(),
    }
}
