//! Extracted from `monomorphize/mod.rs`.

use std::collections::{HashMap, HashSet};

use ilang_ast::{
    Block, ClassDecl, EnumDecl, Expr, ExprKind, FieldDecl, FnDecl, Item, Param, Program, Span,
    Stmt, StmtKind, Symbol, Type, Variant, VariantPayload,
};

use super::walk::{map_expr_children, walk_expr_children};
use super::class::*;
use super::*;

pub(super) fn result_template() -> EnumDecl {
    let span = Span::dummy();
    EnumDecl {
        is_pub: true,
        name: "Result".into(),
        type_params: Box::new(["T".into(), "E".into()]),
        repr_ty: None,
        flags: false,
        variants: Box::new([
            Variant {
                name: "ok".into(),
                payload: VariantPayload::Tuple(Box::new([Type::Object("T".into())])),
                discriminant: None,
                span,
            },
            Variant {
                name: "err".into(),
                payload: VariantPayload::Tuple(Box::new([Type::Object("E".into())])),
                discriminant: None,
                span,
            },
        ]),
        span,
    }
}

pub fn monomorphize_enums(
    prog: &Program,
    enum_ctor_type_args: &HashMap<Span, (Symbol, Vec<Type>)>,
) -> Program {
    // Catalog generic enums. Result is always available (built-in).
    let mut generic_enums: HashMap<Symbol, EnumDecl> = prog
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Enum(e) if !e.type_params.is_empty() => Some((e.name.clone(), e.clone())),
            _ => None,
        })
        .collect();
    generic_enums.entry("Result".into()).or_insert_with(result_template);

    if generic_enums.is_empty() {
        return prog.clone();
    }

    let mut requested: HashSet<Symbol> = HashSet::new();
    let mut worklist: Vec<InstKey> = Vec::new();

    // Closure that classifies an instantiation as either an enum
    // (worklist-bound) or anything else (skipped).
    let enqueue_enum =
        |name: &str, args: &[Type], wl: &mut Vec<InstKey>, req: &mut HashSet<Symbol>| {
            if !generic_enums.contains_key(&Symbol::intern(name)) {
                return;
            }
            if args.iter().any(contains_type_var) {
                return;
            }
            let key = InstKey { class: name.into(), args: args.to_vec() };
            if req.insert(key.mangled()) {
                wl.push(key);
            }
        };

    // Seed pass A: walk every Type slot for `Type::Generic { Enum, ... }`.
    for item in &prog.items {
        seed_enums_in_item(item, &mut |n, a| enqueue_enum(n, a, &mut worklist, &mut requested));
    }
    for s in &prog.stmts {
        seed_enums_in_stmt(s, &mut |n, a| enqueue_enum(n, a, &mut worklist, &mut requested));
    }
    if let Some(t) = &prog.tail {
        seed_enums_in_expr(t, &mut |n, a| enqueue_enum(n, a, &mut worklist, &mut requested));
    }

    // Seed pass B: walk every EnumCtor with the side-table.
    let empty_params: Vec<Symbol> = Vec::new();
    let empty_args: Vec<Type> = Vec::new();
    for item in &prog.items {
        seed_enum_ctors_in_item(
            item,
            enum_ctor_type_args,
            &empty_params,
            &empty_args,
            &mut |n, a| enqueue_enum(n, a, &mut worklist, &mut requested),
        );
    }
    for s in &prog.stmts {
        seed_enum_ctors_in_stmt(
            s,
            enum_ctor_type_args,
            &empty_params,
            &empty_args,
            &mut |n, a| enqueue_enum(n, a, &mut worklist, &mut requested),
        );
    }
    if let Some(t) = &prog.tail {
        seed_enum_ctors_in_expr(
            t,
            enum_ctor_type_args,
            &empty_params,
            &empty_args,
            &mut |n, a| enqueue_enum(n, a, &mut worklist, &mut requested),
        );
    }

    // Drain worklist. Each specialization's payload types may
    // themselves contain `Type::Generic { Enum, ... }` refs (e.g.
    // `Result<Box<i64>, string>`); those go back on the worklist.
    let mut synthesized: HashMap<Symbol, EnumDecl> = HashMap::new();
    while let Some(key) = worklist.pop() {
        let mangled = key.mangled();
        if synthesized.contains_key(&mangled) {
            continue;
        }
        let template = match generic_enums.get(&key.class) {
            Some(e) => e.clone(),
            None => continue,
        };
        if template.type_params.len() != key.args.len() {
            continue;
        }
        let new_enum = specialize_enum(&template, &key.args, mangled.as_str());
        // Walk new variants for further generic enum refs.
        for v in &new_enum.variants {
            match &v.payload {
                VariantPayload::Unit => {}
                VariantPayload::Tuple(tys) => {
                    for t in tys {
                        seed_enums_in_type(t, &mut |n, a| {
                            enqueue_enum(n, a, &mut worklist, &mut requested)
                        });
                    }
                }
                VariantPayload::Struct(fields) => {
                    for f in fields {
                        seed_enums_in_type(&f.ty, &mut |n, a| {
                            enqueue_enum(n, a, &mut worklist, &mut requested)
                        });
                    }
                }
            }
        }
        synthesized.insert(mangled, new_enum);
    }

    // Build output: drop generic-enum templates, rewrite the rest.
    let mut out_items: Vec<Item> = Vec::new();
    for item in &prog.items {
        match item {
            Item::Enum(e) if !e.type_params.is_empty() => { /* drop */ }
            other => out_items.push(rewrite_enum_refs_in_item(
                other,
                &generic_enums,
                enum_ctor_type_args,
                &empty_params,
                &empty_args,
            )),
        }
    }
    for (_, e) in synthesized {
        out_items.push(Item::Enum(e));
    }
    let stmts: Vec<Stmt> = prog
        .stmts
        .iter()
        .map(|s| {
            rewrite_enum_refs_in_stmt(
                s,
                &generic_enums,
                enum_ctor_type_args,
                &empty_params,
                &empty_args,
            )
        })
        .collect();
    let tail = prog.tail.as_ref().map(|e| {
        rewrite_enum_refs_in_expr(
            e,
            &generic_enums,
            enum_ctor_type_args,
            &empty_params,
            &empty_args,
        )
    });
    Program {
        items: out_items,
        stmts,
        tail,
    }
}

pub(super) fn specialize_enum(e: &EnumDecl, args: &[Type], mangled: &str) -> EnumDecl {
    let params = e.type_params.clone();
    // Recursively rewrite nested concrete generics (so a payload
    // type `Box<T>` with T=i64 collapses straight to `Box<i64>`
    // mangled instead of leaking back as Type::Generic).
    let args: Vec<Type> = args.iter().map(rewrite_type).collect();
    EnumDecl {
        is_pub: false,
        name: mangled.into(),
        type_params: Box::new([]),
        repr_ty: e.repr_ty.clone(),
        flags: e.flags,
        variants: e
            .variants
            .iter()
            .map(|v| Variant {
                name: v.name.clone(),
                discriminant: v.discriminant.clone(),
                payload: match &v.payload {
                    VariantPayload::Unit => VariantPayload::Unit,
                    VariantPayload::Tuple(tys) => VariantPayload::Tuple(
                        tys.iter().map(|t| subst_type(t, &params, &args)).collect(),
                    ),
                    VariantPayload::Struct(fields) => VariantPayload::Struct(
                        fields
                            .iter()
                            .map(|f| FieldDecl {
                                is_pub: false,
                                name: f.name.clone(),
                                ty: subst_type(&f.ty, &params, &args),
                                span: f.span, bits: f.bits,
                            })
                            .collect(),
                    ),
                },
                span: v.span,
            })
            .collect(),
        span: e.span,
    }
}

pub(super) fn seed_enums_in_item(item: &Item, visit: &mut dyn FnMut(&str, &[Type])) {
    match item {
        Item::Class(c) => {
            for f in &c.fields {
                seed_enums_in_type(&f.ty, visit);
            }
            for m in &c.methods {
                seed_enums_in_fn(m, visit);
            }
            for m in &c.static_methods {
                seed_enums_in_fn(m, visit);
            }
            for p in c.properties.iter() {
                if let Some(g) = &p.getter {
                    seed_enums_in_fn(g, visit);
                }
                if let Some(s) = &p.setter {
                    seed_enums_in_fn(s, visit);
                }
            }
        }
        Item::Fn(f) => seed_enums_in_fn(f, visit),
        Item::Enum(e) => {
            for v in &e.variants {
                match &v.payload {
                    VariantPayload::Unit => {}
                    VariantPayload::Tuple(tys) => {
                        for t in tys {
                            seed_enums_in_type(t, visit);
                        }
                    }
                    VariantPayload::Struct(fields) => {
                        for f in fields {
                            seed_enums_in_type(&f.ty, visit);
                        }
                    }
                }
            }
        }
        Item::Use(_) | Item::Const(_) => {}
        Item::ExternC(b) => {
            // Walk through `@extern(C) { ... }` so generic enum
            // references inside FFI fn signatures / classes still
            // get seeded for monomorphization. Without this,
            // `Result<bool, _>` (and any other `Result<T, _>`
            // mention) inside an extern block falls back to the
            // built-in `Result<i64, i64>` shape and the coerce
            // step rejects the actual payload type.
            for inner in b.items.iter() {
                match inner {
                    ilang_ast::ExternCItem::FnDecl { params, ret, .. } => {
                        for p in params.iter() {
                            seed_enums_in_type(&p.ty, visit);
                        }
                        if let Some(t) = ret {
                            seed_enums_in_type(t, visit);
                        }
                    }
                    ilang_ast::ExternCItem::FnDef(f) => seed_enums_in_fn(f, visit),
                    ilang_ast::ExternCItem::Class(c) => {
                        for f in c.fields.iter() {
                            seed_enums_in_type(&f.ty, visit);
                        }
                        for m in c.methods.iter() {
                            seed_enums_in_fn(m, visit);
                        }
                        for m in c.static_methods.iter() {
                            seed_enums_in_fn(m, visit);
                        }
                    }
                    ilang_ast::ExternCItem::Struct { fields, .. }
                    | ilang_ast::ExternCItem::Union { fields, .. } => {
                        for f in fields.iter() {
                            seed_enums_in_type(&f.ty, visit);
                        }
                    }
                }
            }
        }
        Item::Interface(_) => {}
    }
}

pub(super) fn seed_enums_in_fn(f: &FnDecl, visit: &mut dyn FnMut(&str, &[Type])) {
    for p in &f.params {
        seed_enums_in_type(&p.ty, visit);
    }
    if let Some(t) = &f.ret {
        seed_enums_in_type(t, visit);
    }
    seed_enums_in_block(&f.body, visit);
}

pub(super) fn seed_enums_in_block(b: &Block, visit: &mut dyn FnMut(&str, &[Type])) {
    for s in &b.stmts {
        seed_enums_in_stmt(s, visit);
    }
    if let Some(t) = &b.tail {
        seed_enums_in_expr(t, visit);
    }
}

pub(super) fn seed_enums_in_stmt(s: &Stmt, visit: &mut dyn FnMut(&str, &[Type])) {
    match &s.kind {
        StmtKind::Let { value, ty, .. } => {
            if let Some(t) = ty {
                seed_enums_in_type(t, visit);
            }
            seed_enums_in_expr(value, visit);
        }
        StmtKind::LetTuple { value, .. }
        | StmtKind::LetStruct { value, .. } => seed_enums_in_expr(value, visit),
        StmtKind::Expr(e) => seed_enums_in_expr(e, visit),
    }
}

pub(super) fn seed_enums_in_expr(e: &Expr, visit: &mut dyn FnMut(&str, &[Type])) {
    if let ExprKind::Cast { ty, .. } = &e.kind {
        seed_enums_in_type(ty, visit);
    }
    walk_expr_children(e, &mut |c| seed_enums_in_expr(c, visit));
}

pub(super) fn seed_enums_in_type(t: &Type, visit: &mut dyn FnMut(&str, &[Type])) {
    match t {
        Type::Generic(g) => {
            visit(g.base.as_str(), &g.args);
            for a in &g.args {
                seed_enums_in_type(a, visit);
            }
        }
        Type::Array { elem, .. } => seed_enums_in_type(elem, visit),
        Type::Optional(inner) | Type::Weak(inner) => seed_enums_in_type(inner, visit),
        Type::Fn(ft) => {
            for p in &ft.params {
                seed_enums_in_type(p, visit);
            }
            seed_enums_in_type(&ft.ret, visit);
        }
        _ => {}
    }
}

pub(super) fn seed_enum_ctors_in_item(
    item: &Item,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    visit: &mut dyn FnMut(&str, &[Type]),
) {
    match item {
        Item::Fn(f) => seed_enum_ctors_in_block(&f.body, table, outer_params, outer_args, visit),
        Item::Class(c) => {
            for m in &c.methods {
                seed_enum_ctors_in_block(&m.body, table, outer_params, outer_args, visit);
            }
            // Static methods were previously skipped here, so any
            // `Result.ok(new Self(...))` inside a static factory
            // was never seeded for monomorphization — the call site
            // then hit "unknown type: Result<...>" at lowering.
            for m in &c.static_methods {
                seed_enum_ctors_in_block(&m.body, table, outer_params, outer_args, visit);
            }
            // Property accessors carry bodies too — sweep them for
            // the same reason.
            for p in c.properties.iter() {
                if let Some(g) = &p.getter {
                    seed_enum_ctors_in_block(&g.body, table, outer_params, outer_args, visit);
                }
                if let Some(s) = &p.setter {
                    seed_enum_ctors_in_block(&s.body, table, outer_params, outer_args, visit);
                }
            }
        }
        Item::ExternC(b) => {
            for inner in b.items.iter() {
                match inner {
                    ilang_ast::ExternCItem::FnDef(f) => {
                        seed_enum_ctors_in_block(
                            &f.body, table, outer_params, outer_args, visit,
                        );
                    }
                    ilang_ast::ExternCItem::Class(c) => {
                        for m in c.methods.iter() {
                            seed_enum_ctors_in_block(
                                &m.body, table, outer_params, outer_args, visit,
                            );
                        }
                        for m in c.static_methods.iter() {
                            seed_enum_ctors_in_block(
                                &m.body, table, outer_params, outer_args, visit,
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
        Item::Enum(_) | Item::Use(_) | Item::Const(_) => {}
        Item::Interface(_) => {}
    }
}

pub(super) fn seed_enum_ctors_in_block(
    b: &Block,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    visit: &mut dyn FnMut(&str, &[Type]),
) {
    for s in &b.stmts {
        seed_enum_ctors_in_stmt(s, table, outer_params, outer_args, visit);
    }
    if let Some(t) = &b.tail {
        seed_enum_ctors_in_expr(t, table, outer_params, outer_args, visit);
    }
}

pub(super) fn seed_enum_ctors_in_stmt(
    s: &Stmt,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    visit: &mut dyn FnMut(&str, &[Type]),
) {
    match &s.kind {
        StmtKind::Let { value, .. }
        | StmtKind::LetTuple { value, .. }
        | StmtKind::LetStruct { value, .. } => {
            seed_enum_ctors_in_expr(value, table, outer_params, outer_args, visit)
        }
        StmtKind::Expr(e) => seed_enum_ctors_in_expr(e, table, outer_params, outer_args, visit),
    }
}

pub(super) fn seed_enum_ctors_in_expr(
    e: &Expr,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    visit: &mut dyn FnMut(&str, &[Type]),
) {
    if let ExprKind::EnumCtor { enum_name, .. } = &e.kind {
        if let Some((name, raw_args)) = table.get(&e.span) {
            if name == enum_name {
                let concrete: Vec<Type> = raw_args
                    .iter()
                    .map(|t| subst_type(t, outer_params, outer_args))
                    .collect();
                visit(enum_name.as_str(), &concrete);
            }
        }
    }
    walk_expr_children(e, &mut |c| {
        seed_enum_ctors_in_expr(c, table, outer_params, outer_args, visit)
    });
}

pub(super) fn rewrite_enum_refs_in_item(
    item: &Item,
    generic_enums: &HashMap<Symbol, EnumDecl>,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
) -> Item {
    match item {
        Item::Fn(f) => Item::Fn(FnDecl {
            is_pub: false,
            attrs: f.attrs.clone(),

            name: f.name.clone(),
            type_params: f.type_params.clone(),
            params: f
                .params
                .iter()
                .map(|p| Param {
                    name: p.name.clone(),
                    ty: rewrite_enum_refs_in_type(&p.ty, generic_enums),
                    span: p.span,
                    default: p.default.clone(),
                })
                .collect(),
            ret: f.ret.as_ref().map(|t| rewrite_enum_refs_in_type(t, generic_enums)),
            body: rewrite_enum_refs_in_block(
                &f.body, generic_enums, table, outer_params, outer_args,
            ),
            span: f.span,
        is_override: f.is_override,
        }),
        Item::Class(c) => Item::Class(ClassDecl {
            is_pub: false,
            extern_lib: c.extern_lib.clone(),
            is_repr_c: c.is_repr_c,
            is_packed: c.is_packed,
            is_union: c.is_union,
            name: c.name.clone(),
            parent: c.parent.clone(),
            interfaces: c.interfaces.clone(),
            type_params: c.type_params.clone(),
            fields: c
                .fields
                .iter()
                .map(|f| FieldDecl {
                    is_pub: false,
                    name: f.name.clone(),
                    ty: rewrite_enum_refs_in_type(&f.ty, generic_enums),
                    span: f.span, bits: f.bits,
                })
                .collect(),
            methods: c
                .methods
                .iter()
                .map(|m| FnDecl {
                    is_pub: false,
                    attrs: m.attrs.clone(),

                    name: m.name.clone(),
                    type_params: m.type_params.clone(),
                    params: m
                        .params
                        .iter()
                        .map(|p| Param {
                            name: p.name.clone(),
                            ty: rewrite_enum_refs_in_type(&p.ty, generic_enums),
                            span: p.span,
                            default: p.default.clone(),
                        })
                        .collect(),
                    ret: m.ret.as_ref().map(|t| rewrite_enum_refs_in_type(t, generic_enums)),
                    body: rewrite_enum_refs_in_block(
                        &m.body, generic_enums, table, outer_params, outer_args,
                    ),
                    span: m.span,
                is_override: m.is_override,
                })
                .collect(),
            static_methods: c
                .static_methods
                .iter()
                .map(|m| FnDecl {
                    is_pub: false,
                    attrs: m.attrs.clone(),

                    name: m.name.clone(),
                    type_params: m.type_params.clone(),
                    params: m.params.iter().map(|p| Param {
                        name: p.name.clone(),
                        ty: rewrite_enum_refs_in_type(&p.ty, generic_enums),
                        span: p.span,
                        default: p.default.clone(),
                    }).collect(),
                    ret: m.ret.as_ref().map(|t| rewrite_enum_refs_in_type(t, generic_enums)),
                    body: rewrite_enum_refs_in_block(
                        &m.body, generic_enums, table, outer_params, outer_args,
                    ),
                    span: m.span,
                is_override: m.is_override,
                })
                .collect(),
            static_fields: c.static_fields.clone(),
            properties: c
                .properties
                .iter()
                .map(|p| ilang_ast::PropertyDecl {
                    is_pub: false,
                    name: p.name.clone(),
                    ty: rewrite_enum_refs_in_type(&p.ty, generic_enums),
                    getter: p.getter.as_ref().map(|g| FnDecl {
                        is_pub: false,
                        attrs: g.attrs.clone(),

                        name: g.name.clone(),
                        type_params: g.type_params.clone(),
                        params: g.params.iter().map(|q| Param {
                            name: q.name.clone(),
                            ty: rewrite_enum_refs_in_type(&q.ty, generic_enums),
                            span: q.span,
                            default: q.default.clone(),
                        }).collect(),
                        ret: g.ret.as_ref().map(|t| rewrite_enum_refs_in_type(t, generic_enums)),
                        body: rewrite_enum_refs_in_block(&g.body, generic_enums, table, outer_params, outer_args),
                        span: g.span,
                    is_override: g.is_override,
                    }),
                    setter: p.setter.as_ref().map(|s| FnDecl {
                        is_pub: false,
                        attrs: s.attrs.clone(),

                        name: s.name.clone(),
                        type_params: s.type_params.clone(),
                        params: s.params.iter().map(|q| Param {
                            name: q.name.clone(),
                            ty: rewrite_enum_refs_in_type(&q.ty, generic_enums),
                            span: q.span,
                            default: q.default.clone(),
                        }).collect(),
                        ret: s.ret.as_ref().map(|t| rewrite_enum_refs_in_type(t, generic_enums)),
                        body: rewrite_enum_refs_in_block(&s.body, generic_enums, table, outer_params, outer_args),
                        span: s.span,
                    is_override: s.is_override,
                    }),
                    span: p.span,
                })
                .collect(),
            span: c.span,
        }),
        Item::Enum(e) => Item::Enum(EnumDecl {
            is_pub: false,
            name: e.name.clone(),
            type_params: e.type_params.clone(),
            repr_ty: e.repr_ty.clone(),
            flags: e.flags,
            variants: e
                .variants
                .iter()
                .map(|v| Variant {
                    name: v.name.clone(),
                    discriminant: v.discriminant.clone(),
                    payload: match &v.payload {
                        VariantPayload::Unit => VariantPayload::Unit,
                        VariantPayload::Tuple(tys) => VariantPayload::Tuple(
                            tys.iter()
                                .map(|t| rewrite_enum_refs_in_type(t, generic_enums))
                                .collect(),
                        ),
                        VariantPayload::Struct(fields) => VariantPayload::Struct(
                            fields
                                .iter()
                                .map(|f| FieldDecl {
                                    is_pub: false,
                                    name: f.name.clone(),
                                    ty: rewrite_enum_refs_in_type(&f.ty, generic_enums),
                                    span: f.span, bits: f.bits,
                                })
                                .collect(),
                        ),
                    },
                    span: v.span,
                })
                .collect(),
            span: e.span,
        }),
        Item::Use(u) => Item::Use(u.clone()),
        Item::Const(c) => Item::Const(c.clone()),
        Item::ExternC(b) => {
            // Rewrite generic enum references inside `@extern(C)`
            // bodies / signatures so e.g. `Result<bool, _>` inside
            // an extern fn picks up the monomorphized variant
            // instead of the built-in `Result<i64, i64>` shape.
            let items: Vec<ilang_ast::ExternCItem> = b
                .items
                .iter()
                .map(|inner| rewrite_enum_refs_in_extern_c_item(
                    inner, generic_enums, table, outer_params, outer_args,
                ))
                .collect();
            Item::ExternC(ilang_ast::ExternCBlock {
                items: items.into_boxed_slice(),
                span: b.span,
            })
        }
        
        Item::Interface(i) => Item::Interface(i.clone()),
    }
}

/// Mirror of `rewrite_enum_refs_in_item` for the inner items of an
/// `@extern(C) { ... }` block. Only `FnDecl` / `FnDef` / `Class`
/// shapes carry signatures / bodies the rewriter needs to walk;
/// `Struct` / `Union` field types pass through unchanged.
fn rewrite_enum_refs_in_extern_c_item(
    item: &ilang_ast::ExternCItem,
    generic_enums: &HashMap<Symbol, EnumDecl>,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
) -> ilang_ast::ExternCItem {
    use ilang_ast::ExternCItem;
    match item {
        ExternCItem::FnDecl {
            is_pub, name, params, ret, libs, optional, variadic, c_symbol, span,
        } => ExternCItem::FnDecl {
            is_pub: *is_pub,
            name: name.clone(),
            params: params
                .iter()
                .map(|p| Param {
                    name: p.name.clone(),
                    ty: rewrite_enum_refs_in_type(&p.ty, generic_enums),
                    span: p.span,
                    default: p.default.clone(),
                })
                .collect(),
            ret: ret.as_ref().map(|t| rewrite_enum_refs_in_type(t, generic_enums)),
            libs: libs.clone(),
            optional: *optional,
            variadic: *variadic,
            c_symbol: *c_symbol,
            span: *span,
        },
        ExternCItem::FnDef(f) => ExternCItem::FnDef(FnDecl {
            is_pub: f.is_pub,
            attrs: f.attrs.clone(),
            name: f.name.clone(),
            type_params: f.type_params.clone(),
            params: f
                .params
                .iter()
                .map(|p| Param {
                    name: p.name.clone(),
                    ty: rewrite_enum_refs_in_type(&p.ty, generic_enums),
                    span: p.span,
                    default: p.default.clone(),
                })
                .collect(),
            ret: f.ret.as_ref().map(|t| rewrite_enum_refs_in_type(t, generic_enums)),
            body: rewrite_enum_refs_in_block(
                &f.body, generic_enums, table, outer_params, outer_args,
            ),
            span: f.span,
            is_override: f.is_override,
        }),
        ExternCItem::Class(c) => {
            // Reuse the regular class path — wrapping classes
            // inside `@extern(C)` share the same shape.
            match rewrite_enum_refs_in_item(
                &Item::Class(c.clone()),
                generic_enums, table, outer_params, outer_args,
            ) {
                Item::Class(rewritten) => ExternCItem::Class(rewritten),
                _ => unreachable!("rewrite_enum_refs_in_item on Class returns Class"),
            }
        }
        ExternCItem::Struct { .. } | ExternCItem::Union { .. } => item.clone(),
    }
}

pub(super) fn rewrite_enum_refs_in_block(
    b: &Block,
    generic_enums: &HashMap<Symbol, EnumDecl>,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
) -> Block {
    Block {
        stmts: b
            .stmts
            .iter()
            .map(|s| {
                rewrite_enum_refs_in_stmt(s, generic_enums, table, outer_params, outer_args)
            })
            .collect(),
        tail: b.tail.as_ref().map(|e| {
            Box::new(rewrite_enum_refs_in_expr(
                e,
                generic_enums,
                table,
                outer_params,
                outer_args,
            ))
        }),
    }
}

pub(super) fn rewrite_enum_refs_in_stmt(
    s: &Stmt,
    generic_enums: &HashMap<Symbol, EnumDecl>,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
) -> Stmt {
    let kind = match &s.kind {
        StmtKind::Let { name, ty, value, .. } => StmtKind::Let {
            is_pub: false,
                is_const: false,
            name: name.clone(),
            ty: ty.as_ref().map(|t| rewrite_enum_refs_in_type(t, generic_enums)),
            value: rewrite_enum_refs_in_expr(
                value,
                generic_enums,
                table,
                outer_params,
                outer_args,
            ),
        },
        StmtKind::LetTuple { elems, value } => StmtKind::LetTuple {
            elems: elems.clone(),
            value: rewrite_enum_refs_in_expr(
                value,
                generic_enums,
                table,
                outer_params,
                outer_args,
            ),
        },
        StmtKind::LetStruct { class, fields, value } => StmtKind::LetStruct {
            class: class.clone(),
            fields: fields.clone(),
            value: rewrite_enum_refs_in_expr(
                value,
                generic_enums,
                table,
                outer_params,
                outer_args,
            ),
        },
        StmtKind::Expr(e) => StmtKind::Expr(rewrite_enum_refs_in_expr(
            e,
            generic_enums,
            table,
            outer_params,
            outer_args,
        )),
    };
    Stmt { kind, span: s.span, source_module: s.source_module.clone() }
}

pub(super) fn rewrite_enum_refs_in_expr(
    e: &Expr,
    generic_enums: &HashMap<Symbol, EnumDecl>,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
) -> Expr {
    let kind = match &e.kind {
        ExprKind::EnumCtor { enum_name, variant, args } => {
            let new_args = match args {
                ilang_ast::CtorArgs::Unit => ilang_ast::CtorArgs::Unit,
                ilang_ast::CtorArgs::Tuple(es) => ilang_ast::CtorArgs::Tuple(
                    es.iter()
                        .map(|x| {
                            rewrite_enum_refs_in_expr(
                                x, generic_enums, table, outer_params, outer_args,
                            )
                        })
                        .collect(),
                ),
                ilang_ast::CtorArgs::Struct(fs) => ilang_ast::CtorArgs::Struct(
                    fs.iter()
                        .map(|(n, x)| {
                            (
                                n.clone(),
                                rewrite_enum_refs_in_expr(
                                    x, generic_enums, table, outer_params, outer_args,
                                ),
                            )
                        })
                        .collect(),
                ),
            };
            let new_name = if generic_enums.contains_key(enum_name) {
                if let Some((tn, raw_args)) = table.get(&e.span) {
                    if tn == enum_name {
                        let concrete: Vec<Type> = raw_args
                            .iter()
                            .map(|t| subst_type(t, outer_params, outer_args))
                            .collect();
                        if !concrete.iter().any(contains_type_var) {
                            mangle_enum(enum_name.as_str(), &concrete)
                        } else {
                            enum_name.clone()
                        }
                    } else {
                        enum_name.clone()
                    }
                } else {
                    enum_name.clone()
                }
            } else {
                enum_name.clone()
            };
            ExprKind::EnumCtor {
                enum_name: new_name,
                variant: variant.clone(),
                args: new_args.into(),
            }
        }
        ExprKind::Cast { expr, ty } => ExprKind::Cast {
            expr: Box::new(rewrite_enum_refs_in_expr(
                expr, generic_enums, table, outer_params, outer_args,
            )),
            ty: rewrite_enum_refs_in_type(ty, generic_enums),
        },
        ExprKind::Match { scrutinee, arms } => {
            // Patterns may carry an explicit `enum_name` (long form
            // `Result.ok(v)`); strip it when it names a now-mangled
            // generic enum so the JIT's match lowering resolves
            // variants from the (already-mangled) scrutinee type.
            let new_scrut = rewrite_enum_refs_in_expr(
                scrutinee, generic_enums, table, outer_params, outer_args,
            );
            let new_arms = arms
                .iter()
                .map(|arm| {
                    let new_pat = match &arm.pattern.kind {
                        ilang_ast::PatternKind::Variant { enum_name, variant, bindings } => {
                            let stripped = enum_name
                                .as_ref()
                                .filter(|n| !generic_enums.contains_key(n))
                                .cloned();
                            ilang_ast::Pattern {
                                kind: ilang_ast::PatternKind::Variant {
                                    enum_name: stripped,
                                    variant: variant.clone(),
                                    bindings: bindings.clone(),
                                },
                                span: arm.pattern.span,
                            }
                        }
                        other => ilang_ast::Pattern {
                            kind: other.clone(),
                            span: arm.pattern.span,
                        },
                    };
                    ilang_ast::MatchArm {
                        pattern: new_pat,
                        body: rewrite_enum_refs_in_expr(
                            &arm.body, generic_enums, table, outer_params, outer_args,
                        ),
                        span: arm.span,
                    }
                })
                .collect();
            ExprKind::Match { scrutinee: Box::new(new_scrut), arms: new_arms }
        }
        _ => map_expr_children(e, &mut |c| {
            rewrite_enum_refs_in_expr(c, generic_enums, table, outer_params, outer_args)
        }),
    };
    Expr { kind, span: e.span }
}

pub(super) fn rewrite_enum_refs_in_type(
    t: &Type,
    generic_enums: &HashMap<Symbol, EnumDecl>,
) -> Type {
    match t {
        Type::Generic(g) => {
            let new_args: Vec<Type> =
                g.args.iter().map(|a| rewrite_enum_refs_in_type(a, generic_enums)).collect();
            if generic_enums.contains_key(&g.base) && !new_args.iter().any(contains_type_var) {
                Type::Object(mangle_enum(g.base.as_str(), &new_args))
            } else {
                Type::generic(g.base.clone(), new_args)
            }
        }
        Type::Array { elem, fixed } => Type::Array {
            elem: Box::new(rewrite_enum_refs_in_type(elem, generic_enums)),
            fixed: *fixed,
        },
        Type::Optional(inner) => {
            Type::Optional(Box::new(rewrite_enum_refs_in_type(inner, generic_enums)))
        }
        Type::Weak(inner) => Type::Weak(Box::new(rewrite_enum_refs_in_type(inner, generic_enums))),
        Type::Fn(ft) => Type::func(
            ft.params.iter().map(|p| rewrite_enum_refs_in_type(p, generic_enums)).collect(),
            rewrite_enum_refs_in_type(&ft.ret, generic_enums),
        ),
        _ => t.clone(),
    }
}

pub(super) fn mangle_enum(name: &str, args: &[Type]) -> Symbol {
    InstKey { class: name.into(), args: args.to_vec() }.mangled()
}

