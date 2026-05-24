//! Hoist anonymous-function expressions out to top-level synthetic
//! fns. Each `fn(...) { ... }` becomes a fresh `Item::Fn` with a
//! generated name like `__anon_fn_0`, and the original `FnExpr` is
//! replaced with a `Var(synth_name)` reference. Driven by
//! `hoist_anon_fns`; per-wrapper metadata (capture layout, mutable
//! flags, lexical-class info) flows out through `ClosureMetaIn`.

use ilang_ast::{
    Block, ClassDecl, Expr, ExprKind, FnDecl, Item, Program, Stmt, StmtKind, Symbol,
};

/// Hoist anonymous-function expressions out to top-level synthetic
/// fns. Each `fn(...) { ... }` becomes a fresh `Item::Fn` with a
/// generated name like `__anon_fn_0`, and the original `FnExpr` is
/// replaced with a `Var(synth_name)` reference. The JIT then sees
/// only named functions — call sites turn into ordinary indirect
/// calls (or direct calls when the var is shadowed by a `let`).
/// Per-closure-wrapper metadata produced by the hoist pass and
/// consumed by the JIT compiler to lay out closure structs.
#[derive(Debug, Clone)]
pub struct ClosureMetaIn {
    pub user_param_tys: Vec<ilang_ast::Type>,
    pub ret_ty: Option<ilang_ast::Type>,
    pub captures: Vec<(Symbol, ilang_ast::Type)>,
    /// `mutable[i]` is true when the wrapper body assigns to
    /// `captures[i].0`. Mutable captures are stored cell-backed
    /// (heap allocation owned by the closure value) so writes
    /// persist across calls; immutable captures stay inline in
    /// the closure struct's env slot. Same length as `captures`.
    pub mutable: Vec<bool>,
    pub span: ilang_ast::Span,
    /// Lexical class when the wrapper was hoisted from inside a
    /// class method body. Used by the JIT to restore
    /// `lc.current_class` at wrapper lower time and by the second
    /// type-check pass to allow `this` / `super` references in the
    /// wrapper body.
    pub this_class: Option<Symbol>,
}

/// Walk an expression tree looking for `Assign { target = name, .. }`
/// (or compound-assignment shapes that desugar to it). Returns true
/// if any branch writes to `name`. Used by the hoist pass to mark
/// each closure capture as mutable when its name appears as an
/// l-value somewhere in the wrapper body.
fn body_writes_to(body: &Block, name: &Symbol) -> bool {
    fn in_block(b: &Block, name: &Symbol) -> bool {
        for s in b.stmts.iter() {
            if in_stmt(s, name) {
                return true;
            }
        }
        if let Some(t) = &b.tail { in_expr(t, name) } else { false }
    }
    fn in_stmt(s: &ilang_ast::Stmt, name: &Symbol) -> bool {
        use ilang_ast::StmtKind::*;
        match &s.kind {
            Let { value, .. } => in_expr(value, name),
            LetTuple { value, .. } => in_expr(value, name),
            LetStruct { value, .. } => in_expr(value, name),
            Expr(e) => in_expr(e, name),
        }
    }
    fn in_expr(e: &Expr, name: &Symbol) -> bool {
        use ilang_ast::CtorArgs;
        use ExprKind::*;
        match &e.kind {
            Assign { target, value } => target == name || in_expr(value, name),
            AssignField { obj, value, .. } => in_expr(obj, name) || in_expr(value, name),
            AssignIndex { obj, index, value } => {
                in_expr(obj, name) || in_expr(index, name) || in_expr(value, name)
            }
            Block(b) => in_block(b, name),
            If { cond, then_branch, else_branch } => {
                in_expr(cond, name)
                    || in_block(then_branch, name)
                    || else_branch.as_deref().is_some_and(|e| in_expr(e, name))
            }
            IfLet { expr, then_branch, else_branch, .. } => {
                in_expr(expr, name)
                    || in_block(then_branch, name)
                    || else_branch.as_deref().is_some_and(|e| in_expr(e, name))
            }
            While { cond, body } => in_expr(cond, name) || in_block(body, name),
            Loop { body } => in_block(body, name),
            ForIn { iter, body, .. } => in_expr(iter, name) || in_block(body, name),
            Range { start, end, .. } => {
                start.as_deref().is_some_and(|e| in_expr(e, name))
                    || end.as_deref().is_some_and(|e| in_expr(e, name))
            }
            Match { scrutinee, arms } => {
                in_expr(scrutinee, name)
                    || arms.iter().any(|a| in_expr(&a.body, name))
            }
            Call { args, .. } => args.iter().any(|a| in_expr(a, name)),
            MethodCall { obj, args, .. } => {
                in_expr(obj, name) || args.iter().any(|a| in_expr(a, name))
            }
            Field { obj, .. } => in_expr(obj, name),
            Index { obj, index } => in_expr(obj, name) || in_expr(index, name),
            Binary { lhs, rhs, .. } => in_expr(lhs, name) || in_expr(rhs, name),
            Logical { lhs, rhs, .. } => in_expr(lhs, name) || in_expr(rhs, name),
            Unary { expr, .. } => in_expr(expr, name),
            Cast { expr, .. } => in_expr(expr, name),
            TypeTest { expr, .. } => in_expr(expr, name),
            TypeDowncast { expr, .. } => in_expr(expr, name),
            Some(inner) | Await(inner) => in_expr(inner, name),
            New { args, .. } => args.iter().any(|a| in_expr(a, name)),
            Return(v) => v.as_deref().is_some_and(|e| in_expr(e, name)),
            Break(v) => v.as_deref().is_some_and(|e| in_expr(e, name)),
            Array(elements) => elements.iter().any(|e| in_expr(e, name)),
            Tuple(elements) => elements.iter().any(|e| in_expr(e, name)),
            MapLit(entries) => {
                entries.iter().any(|(k, v)| in_expr(k, name) || in_expr(v, name))
            }
            StructLit { fields, .. } => fields.iter().any(|(_, v)| in_expr(v, name)),
            EnumCtor { args, .. } => match args {
                CtorArgs::Unit => false,
                CtorArgs::Tuple(es) => es.iter().any(|e| in_expr(e, name)),
                CtorArgs::Struct(fs) => fs.iter().any(|(_, e)| in_expr(e, name)),
            },
            FnExpr { body, params, .. } => {
                // Inner anon-fn shadows `name` if its own param list
                // contains it; otherwise an Assign inside the inner
                // body counts as a write to the outer scope's binding
                // (it'll capture from the same outer slot, which we
                // therefore have to cell-back).
                if params.iter().any(|p| p.name == *name) {
                    false
                } else {
                    in_block(body, name)
                }
            }
            Closure { .. } => false,        // emitted only after this pass
            SuperCall { args, .. } => args.iter().any(|a| in_expr(a, name)),
            // Leaves: no sub-expressions to recurse into.
            Int(_) | Float(_) | Bool(_) | Str(_) | Var(_) | This
            | Continue | None => false,
        }
    }
    in_block(body, name)
}

/// Bundle of state threaded through the hoist walkers.
pub struct HoistCtx<'a> {
    pub counter: &'a mut u32,
    pub hoisted: &'a mut Vec<Item>,
    /// FnExpr span → captured (name, type) list (from typechecker).
    pub captures_map: &'a std::collections::HashMap<
        ilang_ast::Span,
        Vec<(Symbol, ilang_ast::Type)>,
    >,
    /// FnExpr span → enclosing class symbol when the body uses `this`.
    /// Drives the synthetic `this` capture and `ExprKind::This` →
    /// `Var("this")` rewrite below.
    pub this_class_map: &'a std::collections::HashMap<ilang_ast::Span, Symbol>,
    /// wrapper_name → metadata. Filled in as we hoist FnExprs.
    pub closure_meta:
        &'a mut std::collections::HashMap<Symbol, ClosureMetaIn>,
}

pub fn hoist_anon_fns(
    prog: &Program,
    fn_expr_captures: &std::collections::HashMap<
        ilang_ast::Span,
        Vec<(Symbol, ilang_ast::Type)>,
    >,
    fn_expr_this_class: &std::collections::HashMap<ilang_ast::Span, Symbol>,
) -> (Program, std::collections::HashMap<Symbol, ClosureMetaIn>) {
    let mut counter: u32 = 0;
    let mut hoisted: Vec<Item> = Vec::new();
    let mut closure_meta: std::collections::HashMap<Symbol, ClosureMetaIn> =
        std::collections::HashMap::new();
    let mut ctx = HoistCtx {
        counter: &mut counter,
        hoisted: &mut hoisted,
        captures_map: fn_expr_captures,
        this_class_map: fn_expr_this_class,
        closure_meta: &mut closure_meta,
    };
    let new_items: Vec<Item> = prog
        .items
        .iter()
        .map(|i| hoist_in_item(i, &mut ctx))
        .collect();
    let new_stmts: Vec<Stmt> = prog
        .stmts
        .iter()
        .map(|s| hoist_in_stmt(s, &mut ctx))
        .collect();
    let new_tail = prog
        .tail
        .as_ref()
        .map(|e| hoist_in_expr(e, &mut ctx));
    let mut items = new_items;
    items.extend(hoisted);
    (
        Program {
            items,
            stmts: new_stmts.into(),
            tail: new_tail,
        },
        closure_meta,
    )
}

fn fresh_anon_name(counter: &mut u32) -> Symbol {
    let n = *counter;
    *counter += 1;
    Symbol::intern(&format!("$anon.fn_{n}"))
}

fn hoist_in_item(item: &Item, ctx: &mut HoistCtx) -> Item {
    match item {
        Item::Fn(f) => Item::Fn(FnDecl {
            is_pub: false,
            attrs: f.attrs.clone(),

            name: f.name.clone(),
            type_params: f.type_params.clone(),
            params: f.params.clone(),
            ret: f.ret.clone(),
            body: hoist_in_block(&f.body, ctx),
            span: f.span,
        is_override: f.is_override,
            is_async: false,
        }),
        Item::Class(c) => Item::Class(ClassDecl {
            is_pub: false,
            extern_lib: c.extern_lib.clone(),
            is_repr_c: c.is_repr_c,
            is_packed: c.is_packed,
        is_handle: c.is_handle,
            is_union: c.is_union,
            name: c.name.clone(),
            parent: c.parent.clone(),
            interfaces: c.interfaces.clone(),
            type_params: c.type_params.clone(),
            fields: c.fields.clone(),
            methods: c
                .methods
                .iter()
                .map(|m| FnDecl {
                    is_pub: false,
                    attrs: m.attrs.clone(),

                    name: m.name.clone(),
                    type_params: m.type_params.clone(),
                    params: m.params.clone(),
                    ret: m.ret.clone(),
                    body: hoist_in_block(&m.body, ctx),
                    span: m.span,
                is_override: m.is_override,
            is_async: false,
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
                    params: m.params.clone(),
                    ret: m.ret.clone(),
                    body: hoist_in_block(&m.body, ctx),
                    span: m.span,
                is_override: m.is_override,
            is_async: false,
                })
                .collect(),
            static_fields: c.static_fields.clone(),
            properties: c
                .properties
                .iter()
                .map(|p| ilang_ast::PropertyDecl { is_static: p.is_static,
                    is_pub: false,
                    name: p.name.clone(),
                    ty: p.ty.clone(),
                    getter: p.getter.as_ref().map(|g| FnDecl {
                        is_pub: false,
                        attrs: g.attrs.clone(),

                        name: g.name.clone(),
                        type_params: g.type_params.clone(),
                        params: g.params.clone(),
                        ret: g.ret.clone(),
                        body: hoist_in_block(&g.body, ctx),
                        span: g.span,
                    is_override: g.is_override,
            is_async: false,
                    }),
                    setter: p.setter.as_ref().map(|s| FnDecl {
                        is_pub: false,
                        attrs: s.attrs.clone(),

                        name: s.name.clone(),
                        type_params: s.type_params.clone(),
                        params: s.params.clone(),
                        ret: s.ret.clone(),
                        body: hoist_in_block(&s.body, ctx),
                        span: s.span,
                    is_override: s.is_override,
            is_async: false,
                    }),
                    span: p.span,
                })
                .collect(),
            attrs: c.attrs.clone(),
            span: c.span,
        }),
        Item::Enum(e) => Item::Enum(e.clone()),
        Item::Use(u) => Item::Use(u.clone()),
        Item::Const(c) => Item::Const(c.clone()),
        Item::ExternC(b) => Item::ExternC(b.clone()),
        
        Item::Interface(i) => Item::Interface(i.clone()),
    }
}

fn hoist_in_block(b: &Block, ctx: &mut HoistCtx) -> Block {
    Block {
        stmts: b
            .stmts
            .iter()
            .map(|s| hoist_in_stmt(s, ctx))
            .collect(),
        tail: b
            .tail
            .as_ref()
            .map(|e| Box::new(hoist_in_expr(e, ctx))),
    }
}

fn hoist_in_stmt(s: &Stmt, ctx: &mut HoistCtx) -> Stmt {
    let kind = match &s.kind {
        StmtKind::Let { name, ty, value, .. } => StmtKind::Let {
            is_pub: false,
                is_const: false,
            name: name.clone(),
            ty: ty.clone(),
            value: hoist_in_expr(value, ctx),
        },
        StmtKind::LetTuple { elems, value } => StmtKind::LetTuple {
            elems: elems.clone(),
            value: hoist_in_expr(value, ctx),
        },
        StmtKind::LetStruct { class, fields, value } => StmtKind::LetStruct {
            class: class.clone(),
            fields: fields.clone(),
            value: hoist_in_expr(value, ctx),
        },
        StmtKind::Expr(e) => StmtKind::Expr(hoist_in_expr(e, ctx)),
    };
    Stmt {
        kind,
        span: s.span,
        source_module: s.source_module.clone(),
    }
}

fn hoist_in_expr(e: &Expr, ctx: &mut HoistCtx) -> Expr {
    let kind = match &e.kind {
        ExprKind::FnExpr { params, ret, body } => {
            // Capture-mutability scan must run on the ORIGINAL body
            // (before recursive hoisting). Nested FnExpr nodes inside
            // `body` get rewritten to opaque `Closure` references by
            // `hoist_in_block`, after which `body_writes_to` can no
            // longer see the inner assigns — so an outer capture that's
            // only mutated through a nested closure would look
            // read-only and end up inline-stored, breaking the
            // share-cell path between nested closures.
            let captures_pre: Vec<(Symbol, ilang_ast::Type)> = ctx
                .captures_map
                .get(&e.span)
                .cloned()
                .unwrap_or_default();
            let mut mutable_pre: Vec<bool> = captures_pre
                .iter()
                .map(|(n, _)| body_writes_to(body, n))
                .collect();
            // Now hoist any nested anon fns inside this body.
            let body = hoist_in_block(body, ctx);
            let name = fresh_anon_name(ctx.counter);
            // Reuse the captures list collected above; the (rare)
            // synthetic `this` entry is prepended below and never
            // mutable.
            let mut captures = captures_pre;
            // If this closure was built inside a class method body
            // and refers to `this`, prepend a synthetic `this`
            // capture so codegen has somewhere to read the
            // pointer from. The wrapper itself is lowered with
            // `lc.this` populated from this capture (compiler.rs),
            // which makes both `ExprKind::This` and
            // `super.method(...)` work transparently in the body —
            // no AST rewrite needed. The class symbol is also
            // stashed on the closure meta so the second TC and the
            // SuperCall lowerer can restore the lexical class.
            let this_class = ctx.this_class_map.get(&e.span).cloned();
            if let Some(class_name) = this_class.clone() {
                let this_sym: Symbol = "this".into();
                if !captures.iter().any(|(n, _)| *n == this_sym) {
                    captures.insert(
                        0,
                        (this_sym, ilang_ast::Type::Object(class_name)),
                    );
                    // Keep `mutable_pre` in lockstep with `captures`.
                    mutable_pre.insert(0, false);
                }
            }
            // Wrapper takes a hidden env_ptr first param so the same
            // calling convention applies to capture-free anon fns and
            // closures alike. Inside the body, captured Var(name)
            // references are resolved to env loads at lower-time
            // (see lower_expr's Var handler).
            let mut wrapper_params = Vec::with_capacity(params.len() + 1);
            wrapper_params.push(ilang_ast::Param {
                name: "__env".into(),
                ty: ilang_ast::Type::I64,
                span: e.span,
                default: None,
            });
            wrapper_params.extend(params.iter().cloned());
            // `mutable_pre` was computed against the original body so
            // it sees writes that nested closures would otherwise
            // hide.
            let mutable = mutable_pre;
            ctx.hoisted.push(Item::Fn(FnDecl {
                is_pub: false,
                attrs: Box::new([]),

                name: name.clone(),
                type_params: Box::new([]),
                params: wrapper_params.into(),
                ret: ret.clone(),
                body,
                span: e.span,
                is_override: false,
            is_async: false,
            }));
            ctx.closure_meta.insert(
                name.clone(),
                ClosureMetaIn {
                    user_param_tys: params.iter().map(|p| p.ty.clone()).collect(),
                    ret_ty: ret.clone(),
                    captures: captures.clone(),
                    mutable,
                    span: e.span,
                    this_class,
                },
            );
            ExprKind::Closure { fn_name: name, captures: captures.into() }
        }
        // Recurse through anything that might contain expressions.
        ExprKind::Some(inner) => {
            ExprKind::Some(Box::new(hoist_in_expr(inner, ctx)))
        }
        ExprKind::Await(inner) => {
            ExprKind::Await(Box::new(hoist_in_expr(inner, ctx)))
        }
        ExprKind::Unary { op, expr } => ExprKind::Unary {
            op: *op,
            expr: Box::new(hoist_in_expr(expr, ctx)),
        },
        ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
            op: *op,
            lhs: Box::new(hoist_in_expr(lhs, ctx)),
            rhs: Box::new(hoist_in_expr(rhs, ctx)),
        },
        ExprKind::Logical { op, lhs, rhs } => ExprKind::Logical {
            op: *op,
            lhs: Box::new(hoist_in_expr(lhs, ctx)),
            rhs: Box::new(hoist_in_expr(rhs, ctx)),
        },
        ExprKind::Cast { expr, ty } => ExprKind::Cast {
            expr: Box::new(hoist_in_expr(expr, ctx)),
            ty: ty.clone(),
        },
        ExprKind::TypeTest { expr, ty } => ExprKind::TypeTest {
            expr: Box::new(hoist_in_expr(expr, ctx)),
            ty: ty.clone(),
        },
        ExprKind::TypeDowncast { expr, ty } => ExprKind::TypeDowncast {
            expr: Box::new(hoist_in_expr(expr, ctx)),
            ty: ty.clone(),
        },
        ExprKind::Call { callee, args } => ExprKind::Call {
            callee: callee.clone(),
            args: args.iter().map(|a| hoist_in_expr(a, ctx)).collect(),
        },
        ExprKind::SuperCall { method, args } => ExprKind::SuperCall {
            method: method.clone(),
            args: args.iter().map(|a| hoist_in_expr(a, ctx)).collect(),
        },
        ExprKind::Closure { fn_name, captures } => ExprKind::Closure {
            fn_name: fn_name.clone(),
            captures: captures.clone(),
        },
        ExprKind::Field { obj, name } => ExprKind::Field {
            obj: Box::new(hoist_in_expr(obj, ctx)),
            name: name.clone(),
        },
        ExprKind::MethodCall { obj, method, args } => ExprKind::MethodCall {
            obj: Box::new(hoist_in_expr(obj, ctx)),
            method: method.clone(),
            args: args.iter().map(|a| hoist_in_expr(a, ctx)).collect(),
        },
        ExprKind::New { class, type_args, args, init_method } => ExprKind::New {
            class: class.clone(),
            type_args: type_args.clone(),
            args: args.iter().map(|a| hoist_in_expr(a, ctx)).collect(), init_method: init_method.clone(),
        },
        ExprKind::Block(b) => ExprKind::Block(hoist_in_block(b, ctx)),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => ExprKind::If {
            cond: Box::new(hoist_in_expr(cond, ctx)),
            then_branch: hoist_in_block(then_branch, ctx),
            else_branch: else_branch
                .as_ref()
                .map(|e| Box::new(hoist_in_expr(e, ctx))),
        },
        ExprKind::IfLet {
            name,
            expr,
            then_branch,
            else_branch,
        } => ExprKind::IfLet {
                name: name.clone(),
            expr: Box::new(hoist_in_expr(expr, ctx)),
            then_branch: hoist_in_block(then_branch, ctx),
            else_branch: else_branch
                .as_ref()
                .map(|e| Box::new(hoist_in_expr(e, ctx))),
        },
        ExprKind::While { cond, body } => ExprKind::While {
            cond: Box::new(hoist_in_expr(cond, ctx)),
            body: hoist_in_block(body, ctx),
        },
        ExprKind::Loop { body } => ExprKind::Loop {
            body: hoist_in_block(body, ctx),
        },
        ExprKind::ForIn { var, iter, body } => ExprKind::ForIn {
            var: var.clone(),
            iter: Box::new(hoist_in_expr(iter, ctx)),
            body: hoist_in_block(body, ctx),
        },
        ExprKind::Range { start, end, inclusive } => ExprKind::Range {
            start: start.as_ref().map(|s| Box::new(hoist_in_expr(s, ctx))),
            end: end.as_ref().map(|e| Box::new(hoist_in_expr(e, ctx))),
            inclusive: *inclusive,
        },
        ExprKind::Return(opt) => ExprKind::Return(
            opt.as_ref().map(|e| Box::new(hoist_in_expr(e, ctx))),
        ),
        ExprKind::Break(opt) => ExprKind::Break(
            opt.as_ref().map(|e| Box::new(hoist_in_expr(e, ctx))),
        ),
        ExprKind::Assign { target, value } => ExprKind::Assign {
            target: target.clone(),
            value: Box::new(hoist_in_expr(value, ctx)),
        },
        ExprKind::AssignField { obj, field, value, is_init } => ExprKind::AssignField {
            obj: obj.clone(),
            field: field.clone(),
            value: Box::new(hoist_in_expr(value, ctx)), is_init: *is_init },
        ExprKind::AssignIndex { obj, index, value } => ExprKind::AssignIndex {
            obj: obj.clone(),
            index: index.clone(),
            value: Box::new(hoist_in_expr(value, ctx)),
        },
        ExprKind::Array(items) => ExprKind::Array(
            items.iter().map(|i| hoist_in_expr(i, ctx)).collect(),
        ),
        ExprKind::Tuple(items) => ExprKind::Tuple(
            items.iter().map(|i| hoist_in_expr(i, ctx)).collect(),
        ),
        ExprKind::StructLit { class, fields, field_name_spans } => ExprKind::StructLit {
            class: class.clone(),
            fields: fields
                .iter()
                .map(|(n, e)| (n.clone(), hoist_in_expr(e, ctx)))
                .collect(),
            field_name_spans: field_name_spans.clone(),
        },
        ExprKind::MapLit(entries) => ExprKind::MapLit(
            entries
                .iter()
                .map(|(k, v)| {
                    (
                        hoist_in_expr(k, ctx),
                        hoist_in_expr(v, ctx),
                    )
                })
                .collect(),
        ),
        ExprKind::Index { obj, index } => ExprKind::Index {
            obj: Box::new(hoist_in_expr(obj, ctx)),
            index: Box::new(hoist_in_expr(index, ctx)),
        },
        ExprKind::EnumCtor {
            enum_name,
            variant,
            args,
        } => ExprKind::EnumCtor {
            enum_name: enum_name.clone(),
            variant: variant.clone(),
            args: match args {
                ilang_ast::CtorArgs::Unit => ilang_ast::CtorArgs::Unit,
                ilang_ast::CtorArgs::Tuple(es) => ilang_ast::CtorArgs::Tuple(
                    es.iter().map(|e| hoist_in_expr(e, ctx)).collect(),
                ),
                ilang_ast::CtorArgs::Struct(fs) => ilang_ast::CtorArgs::Struct(
                    fs.iter()
                        .map(|(n, e)| (n.clone(), hoist_in_expr(e, ctx)))
                        .collect(),
                ),
            },
        },
        ExprKind::Match { scrutinee, arms } => ExprKind::Match {
            scrutinee: Box::new(hoist_in_expr(scrutinee, ctx)),
            arms: arms
                .iter()
                .map(|arm| ilang_ast::MatchArm {
                    pattern: arm.pattern.clone(),
                    body: hoist_in_expr(&arm.body, ctx),
                    span: arm.span,
                })
                .collect(),
        },
        ExprKind::Int(n) => ExprKind::Int(*n),
        ExprKind::Float(f) => ExprKind::Float(*f),
        ExprKind::Bool(b) => ExprKind::Bool(*b),
        ExprKind::Str(s) => ExprKind::Str(s.clone()),
        ExprKind::Var(n) => ExprKind::Var(n.clone()),
        ExprKind::This => ExprKind::This,
        ExprKind::None => ExprKind::None,
        ExprKind::Continue => ExprKind::Continue,
    };
    Expr {
        kind,
        span: e.span,
    }
}
