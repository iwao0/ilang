//! `super.method(args)` rewrite + the per-method `$objc.super.<m>`
//! helper that performs the actual `objc_msgSendSuper` dispatch.
//! `build_super_helper` synthesises the helper FnDecl + its
//! `objc_msgSendSuper` alias, and `rewrite_super_in_methods` walks
//! every user method body replacing `super.X(args)` with
//! `this.$objc.super.X(args)` so the dispatch routes through the
//! synthesised helper.

use ilang_ast::{Block, Expr, ExprKind, FnDecl, Param, Stmt, StmtKind, Symbol, Type};

use super::model::{is_objc_class_ty, ret_class_symbol, ObjcCtx, ObjcMethod};
use super::selector::{build_cached_sel_call, build_cstr};

/// Generate a `__super_<method>(args)` helper method + the
/// `objc_msgSendSuper` alias it forwards to. The helper builds a
/// 2-slot scratch (`receiver=this.handle`, `super_cls=
/// objc_getClass(parent_name)`) on the heap (`i64[]` literal — its
/// data pointer at the FFI boundary matches the `struct objc_super`
/// layout exactly) and calls the alias with that pointer, the
/// selector, and the user's args.
pub(super) fn build_super_helper(
    class_name: Symbol,
    parent_name: Symbol,
    m: &ObjcMethod,
    ctx: &ObjcCtx<'_>,
) -> (FnDecl, ilang_ast::ExternCItem) {
    let helper_name: Symbol = format!("$objc.super.{}", m.name.as_str()).into();
    let alias_name: Symbol = format!(
        "{}_msg_super_{}_{}",
        ctx.tag,
        class_name.as_str(),
        m.name.as_str()
    )
    .into();
    let span = m.span;

    // Alias signature: (super_ptr: *i64, sel: *sel_t, args...) -> ret
    let mut alias_params: Vec<Param> = Vec::with_capacity(m.params.len() + 2);
    alias_params.push(Param {
        name: Symbol::intern("super_ptr"),
        ty: Type::RawPtr {
            is_const: false,
            inner: Box::new(Type::I64),
        },
        span,
        default: None,
    });
    alias_params.push(Param {
        name: Symbol::intern("sel"),
        ty: Type::RawPtr {
            is_const: false,
            inner: Box::new(Type::Object(ctx.sel_struct)),
        },
        span,
        default: None,
    });
    for p in m.params.iter() {
        let p_ty = if is_objc_class_ty(&p.ty, ctx.class_names) {
            Type::RawPtr {
                is_const: false,
                inner: Box::new(Type::Object(ctx.object_struct)),
            }
        } else {
            p.ty.clone()
        };
        alias_params.push(Param {
            name: p.name,
            ty: p_ty,
            span: p.span,
            default: None,
        });
    }
    let alias_ret = match &m.ret {
        Some(t) if is_objc_class_ty(t, ctx.class_names) => Some(Type::RawPtr {
            is_const: false,
            inner: Box::new(Type::Object(ctx.object_struct)),
        }),
        Some(t) => Some(t.clone()),
        None => None,
    };
    let alias_decl = ilang_ast::ExternCItem::FnDecl {
        is_pub: false,
        name: alias_name,
        type_params: Box::new([]),
        params: alias_params.into(),
        ret: alias_ret,
        libs: Box::new([Symbol::intern("objc")]),
        optional: false,
        c_symbol: Some(Symbol::intern("objc_msgSendSuper")),
        intrinsic_name: None,
        variadic: false,
        span,
    };

    // helper body:
    //   let __sup: i64[] = [
    //     this.handle,
    //     __get_class(cstrFromString("ParentName")) as i64
    //   ]
    //   <maybe marshal each @objc-class arg to its raw handle>
    //   <call alias(__sup, __sel_register(cstrFromString("sel")), args...)>
    let this_handle = Expr::new(
        ExprKind::Field {
            obj: Box::new(Expr::new(ExprKind::This, span)),
            name: Symbol::intern("handle"),
        },
        span,
    );
    let parent_cls_call = Expr::new(
        ExprKind::Call {
            callee: ctx.get_class,
            args: Box::new([build_cstr(parent_name.as_str(), span)]),
        },
        span,
    );
    let parent_cls_as_i64 = Expr::new(
        ExprKind::Cast {
            expr: Box::new(parent_cls_call),
            ty: Type::I64,
        },
        span,
    );
    let sup_array = Expr::new(
        ExprKind::Array(Box::new([this_handle, parent_cls_as_i64])),
        span,
    );
    let sup_name = Symbol::intern("__sup");
    let mut stmts = vec![Stmt::new(
        StmtKind::Let {
            is_pub: false,
            is_const: false,
            name: sup_name,
            ty: Some(Type::Array {
                elem: Box::new(Type::I64),
                fixed: None,
            }),
            value: sup_array,
        },
        span,
    )];

    // Selector intern + arg marshalling, then the call.
    let sel_call = build_cached_sel_call(&m.selector, ctx.sel_register, ctx.sel_struct, ctx.sel_cache, span);

    let mut call_args: Vec<Expr> = Vec::with_capacity(m.params.len() + 2);
    // First arg: data pointer of `__sup` array. ilang auto-decays
    // `i64[]` to its data pointer at the FFI boundary — but the
    // alias expects `*i64`, which is exactly that decay.
    call_args.push(Expr::new(ExprKind::Var(sup_name), span));
    call_args.push(sel_call);
    for p in m.params.iter() {
        if is_objc_class_ty(&p.ty, ctx.class_names) {
            // Scope-aware normalize keeps `Field(Var(p.name), "handle")`
            // dispatched on the local param even when p.name shadows
            // a `use`d module — no alias indirection needed.
            let handle = Expr::new(
                ExprKind::Field {
                    obj: Box::new(Expr::new(ExprKind::Var(p.name), p.span)),
                    name: Symbol::intern("handle"),
                },
                p.span,
            );
            let as_ptr = Expr::new(
                ExprKind::Cast {
                    expr: Box::new(handle),
                    ty: Type::RawPtr {
                        is_const: false,
                        inner: Box::new(Type::Object(ctx.object_struct)),
                    },
                },
                p.span,
            );
            call_args.push(as_ptr);
        } else {
            call_args.push(Expr::new(ExprKind::Var(p.name), p.span));
        }
    }
    let call_expr = Expr::new(
        ExprKind::Call {
            callee: alias_name,
            args: call_args.into(),
        },
        span,
    );

    // Tail: if return is @objc class, wrap; else return raw; else
    // statement-call.
    let tail = match &m.ret {
        Some(t) if is_objc_class_ty(t, ctx.class_names) => {
            let raw_as_i64 = Expr::new(
                ExprKind::Cast {
                    expr: Box::new(call_expr),
                    ty: Type::I64,
                },
                span,
            );
            let new_expr = Expr::new(
                ExprKind::New {
                    class: ret_class_symbol(t),
                    type_args: Box::new([]),
                    args: Box::new([raw_as_i64]),
                    init_method: Some(Symbol::intern("$objc.bindHandle")),
                },
                span,
            );
            Some(Box::new(new_expr))
        }
        Some(_) => Some(Box::new(call_expr)),
        None => {
            stmts.push(Stmt::new(StmtKind::Expr(call_expr), span));
            None
        }
    };

    let helper_fn = FnDecl {
        is_pub: true,
        attrs: Box::new([]),
        name: helper_name,
        type_params: Box::new([]),
        params: m.params.clone(),
        ret: m.ret.clone(),
        body: Block { stmts, tail },
        span,
        is_override: false,
        is_async: false,
        intrinsic_name: None,
    };
    (helper_fn, alias_decl)
}

/// Walk every method body in `methods` and replace any
/// `SuperCall { method: Some(name), args }` with a method call
/// `this.__super_<name>(args)` so the generated helper carries
/// out the actual `objc_msgSendSuper` dispatch.
pub(super) fn rewrite_super_in_methods(methods: &mut [FnDecl]) {
    for m in methods.iter_mut() {
        // Skip generated helpers / register / impl methods that
        // we ourselves emitted — they don't contain user `super`
        // calls (and self-rewriting would infinite-loop).
        let name = m.name.as_str();
        if name.starts_with("$objc.super.") || name == "register" || name == "init" {
            continue;
        }
        rewrite_super_in_block(&mut m.body);
    }
}

fn rewrite_super_in_block(block: &mut ilang_ast::Block) {
    for stmt in block.stmts.iter_mut() {
        rewrite_super_in_stmt(stmt);
    }
    if let Some(tail) = block.tail.as_mut() {
        rewrite_super_in_expr(tail);
    }
}

fn rewrite_super_in_stmt(stmt: &mut ilang_ast::Stmt) {
    match &mut stmt.kind {
        ilang_ast::StmtKind::Let { value, .. } => rewrite_super_in_expr(value),
        ilang_ast::StmtKind::LetTuple { value, .. } => rewrite_super_in_expr(value),
        ilang_ast::StmtKind::LetStruct { value, .. } => rewrite_super_in_expr(value),
        ilang_ast::StmtKind::Expr(e) => rewrite_super_in_expr(e),
    }
}

fn rewrite_super_in_expr(expr: &mut Expr) {
    // `super.method(args)` → `this.__super_method(args)`. The shared
    // `walk_expr_children_mut` handles every other variant's child
    // traversal, so a `super` call buried inside e.g. an `await`,
    // `match` arm body, struct-literal field, or closure body now
    // gets rewritten too (the previous hand-rolled match's
    // `_ => {}` arm silently missed those positions).
    if let ExprKind::SuperCall { method: Some(name), args } = &mut expr.kind {
        for a in args.iter_mut() {
            rewrite_super_in_expr(a);
        }
        let helper_name: Symbol = format!("$objc.super.{}", name.as_str()).into();
        let new_expr = Expr::new(
            ExprKind::MethodCall {
                obj: Box::new(Expr::new(ExprKind::This, expr.span)),
                method: helper_name,
                args: std::mem::take(args),
            },
            expr.span,
        );
        *expr = new_expr;
        return;
    }
    ilang_ast::walk::walk_expr_children_mut(
        expr,
        &mut rewrite_super_in_expr,
        &mut rewrite_super_in_block,
    );
}
