//! Class / method / free-fn builders that turn the parsed
//! `ObjcMethod` / `ObjcClass` into desugared ilang `ExternCItem`s.
//! Splits into:
//!
//! - `build_root_deinit` — the auto-injected `deinit` on root
//!   @objc classes that releases the underlying ObjC object when
//!   the ilang wrapper drops.
//! - `build_freefn_dispatch` — top-level `@objc("sel") fn`
//!   alias + wrapper FnDef pair.
//! - `build_class_method` — single instance/static method on an
//!   @objc class (the `objc_msgSend` alias + the ilang body that
//!   marshals args and forwards).
//! - `build_objc_class` — the per-class orchestrator that assembles
//!   the synthesised `ClassDecl` plus every alias / IMP / `register`
//!   the class needs.

use std::collections::HashSet;

use ilang_ast::{
    Attribute, AttrArg, Block, ClassDecl, Expr, ExprKind, FieldDecl, FnDecl, Param, PropertyDecl,
    Span, Stmt, StmtKind, Symbol, Type,
};

use super::imp::{build_imp_fn, build_register_class_fn, encode_method_signature};
use super::model::{
    is_objc_class_ty, ret_class_symbol, returns_retained_selector, simd_array_elem, AccessorKind,
    ImpEntry, ObjcClass, ObjcCtx, ObjcMethod,
};
use super::selector::{build_cached_sel_call, build_cstr};
use super::super_call::{build_super_helper, rewrite_super_in_methods};

/// Body:
///   if this.__owns != 0 && this.handle != 0 {
///       <release>(this.handle as *objc_object)
///   }
pub(super) fn build_root_deinit(ctx: &ObjcCtx<'_>, span: Span) -> FnDecl {
    let this_owns = Expr::new(
        ExprKind::Field {
            obj: Box::new(Expr::new(ExprKind::This, span)),
            name: Symbol::intern("$objc.owns"),
        },
        span,
    );
    let this_handle = Expr::new(
        ExprKind::Field {
            obj: Box::new(Expr::new(ExprKind::This, span)),
            name: Symbol::intern("handle"),
        },
        span,
    );
    let owns_nonzero = Expr::new(
        ExprKind::Binary {
            op: ilang_ast::BinOp::Ne,
            lhs: Box::new(this_owns),
            rhs: Box::new(Expr::new(
                ExprKind::Cast {
                    expr: Box::new(Expr::new(ExprKind::Int(0), span)),
                    ty: Type::I8,
                },
                span,
            )),
        },
        span,
    );
    let handle_nonzero = Expr::new(
        ExprKind::Binary {
            op: ilang_ast::BinOp::Ne,
            lhs: Box::new(this_handle.clone()),
            rhs: Box::new(Expr::new(ExprKind::Int(0), span)),
        },
        span,
    );
    let cond = Expr::new(
        ExprKind::Logical {
            op: ilang_ast::LogicalOp::And,
            lhs: Box::new(owns_nonzero),
            rhs: Box::new(handle_nonzero),
        },
        span,
    );
    let handle_as_ptr = Expr::new(
        ExprKind::Cast {
            expr: Box::new(this_handle),
            ty: Type::RawPtr {
                is_const: false,
                inner: Box::new(Type::Object(ctx.object_struct)),
            },
        },
        span,
    );
    let release_call = Expr::new(
        ExprKind::Call {
            callee: ctx.release,
            args: Box::new([handle_as_ptr]),
        },
        span,
    );
    let then_branch = Block {
        stmts: vec![Stmt::new(StmtKind::Expr(release_call), span)],
        tail: None,
    };
    let if_release = Expr::new(
        ExprKind::If {
            cond: Box::new(cond),
            then_branch,
            else_branch: None,
        },
        span,
    );
    FnDecl {
        is_pub: false,
        // Re-use the wrapper bypass so the *objc_object cast in
        // the body doesn't trip the pointer-in-signature rule the
        // type checker applies to ilang-side helpers.
        attrs: Box::new([Attribute {
            name: Symbol::intern("$objc.wrapper"),
            args: Box::new([]),
        }]),
        name: Symbol::intern("deinit"),
        type_params: Box::new([]),
        params: Box::new([]),
        ret: None,
        body: Block {
            stmts: vec![Stmt::new(StmtKind::Expr(if_release), span)],
            tail: None,
        },
        span,
        is_override: false,
        is_async: false,
        intrinsic_name: None,
    }
}

/// Construct a top-level @objc fn's alias + wrapper FnDef.
pub(super) fn build_freefn_dispatch(
    m: &ObjcMethod,
    tag: &str,
    sel_struct: Symbol,
    sel_register: Symbol,
    sel_cache: &super::selector::SelectorCache,
) -> (ilang_ast::ExternCItem, ilang_ast::ExternCItem) {
    let alias_name: Symbol = format!("{tag}_msg_{}", m.name.as_str()).into();
    let mut alias_params: Vec<Param> = Vec::with_capacity(m.params.len() + 1);
    alias_params.push(m.params[0].clone());
    alias_params.push(Param {
        name: Symbol::intern("_sel"),
        ty: Type::RawPtr {
            is_const: false,
            inner: Box::new(Type::Object(sel_struct)),
        },
        span: m.span,
        default: None,
    });
    for p in &m.params[1..] {
        alias_params.push(p.clone());
    }
    let alias = ilang_ast::ExternCItem::FnDecl {
        is_pub: false,
        name: alias_name,
        type_params: Box::new([]),
        params: alias_params.into(),
        ret: m.ret.clone(),
        libs: Box::new([Symbol::intern("objc")]),
        optional: false,
        c_symbol: Some(Symbol::intern("objc_msgSend")),
        intrinsic_name: None,
        variadic: false,
        span: m.span,
    };

    // body: alias(receiver, <cached sel>, args)
    let receiver_var = Expr::new(ExprKind::Var(m.params[0].name), m.span);
    let sel_call = build_cached_sel_call(&m.selector, sel_register, sel_struct, sel_cache, m.span);
    let mut call_args: Vec<Expr> = Vec::with_capacity(m.params.len() + 1);
    call_args.push(receiver_var);
    call_args.push(sel_call);
    for p in &m.params[1..] {
        call_args.push(Expr::new(ExprKind::Var(p.name), p.span));
    }
    let body_call = Expr::new(
        ExprKind::Call {
            callee: alias_name,
            args: call_args.into(),
        },
        m.span,
    );
    let wrapper = ilang_ast::ExternCItem::FnDef(FnDecl {
        is_pub: m.is_pub,
        attrs: Box::new([Attribute {
            name: Symbol::intern("$objc.wrapper"),
            args: Box::new([]),
        }]),
        name: m.name,
        type_params: Box::new([]),
        params: m.params.clone(),
        ret: m.ret.clone(),
        body: Block {
            stmts: Vec::new(),
            tail: Some(Box::new(body_call)),
        },
        span: m.span,
        is_override: false,
        is_async: false,
        intrinsic_name: None,
    });
    (alias, wrapper)
}

/// Build an @objc class into a desugared ilang ClassDecl plus the
/// `objc_msgSend` aliases its methods need.
pub(super) fn build_objc_class(
    c: ObjcClass,
    ctx: &ObjcCtx<'_>,
) -> (ilang_ast::ExternCItem, Vec<ilang_ast::ExternCItem>) {
    let class_name = c.name;
    let span = c.span;

    // Root @objc class: declare a fresh `handle: i64` slot to
    // carry the underlying ObjC `id`.
    // Subclass: rely on the inherited slot — declaring our own
    // `handle` again would shadow it, give the child two
    // independent fields (parent's at HEADER+0, ours at
    // HEADER+N), and the wrapper code's `this.handle` would set
    // the child's while a sibling cast via the parent would read
    // the parent's empty slot (this manifested as
    // `[contentView addSubview:nil]` for NSButton instances).
    let has_parent = c.parent.is_some();
    let fields: Vec<FieldDecl> = if has_parent {
        Vec::new()
    } else {
        // `__owns` tags whether this wrapper is responsible for
        // releasing the underlying ObjC object when its ilang
        // refcount drops. `__bind_handle` sets it to 1; the IMP-side
        // `__bind_handle_unowned` (used for the AppKit-supplied
        // `__self` / sender args) leaves it 0 so the auto-deinit
        // skips them.
        vec![
            FieldDecl {
                is_pub: true,
                name: Symbol::intern("handle"),
                ty: Type::I64,
                span,
                bits: None,
            },
            FieldDecl {
                is_pub: true,
                name: Symbol::intern("$objc.owns"),
                ty: Type::I8,
                span,
                bits: None,
            },
        ]
    };

    // `__bind_handle(h: i64)` — every @objc class's owning handle
    // binder. Sets `handle = h` and `__owns = 1`. The matching
    // `__bind_handle_unowned` variant zeroes `__owns` so the auto
    // deinit skips release — used by IMP wrappers for the
    // AppKit-supplied `__self` / `sender` args we don't own.
    //
    // It used to be named `init`, but the @objc `init` selector
    // is too useful at user level — keeping the binder under a
    // reserved internal name leaves `init` free for user
    // `@objc("init") pub init(): Self` declarations. Subclasses
    // inherit the slots through the normal ilang vtable (declaring
    // our own here would trip the "method hides parent without
    // override" check).
    let bind_h_param = || Param {
        name: Symbol::intern("h"),
        ty: Type::I64,
        span,
        default: None,
    };
    let assign_handle = || {
        Expr::new(
            ExprKind::AssignField {
                obj: Box::new(Expr::new(ExprKind::This, span)),
                field: Symbol::intern("handle"),
                value: Box::new(Expr::new(ExprKind::Var(Symbol::intern("h")), span)),
                is_init: true,
            },
            span,
        )
    };
    let assign_owns = |val: i64| {
        Expr::new(
            ExprKind::AssignField {
                obj: Box::new(Expr::new(ExprKind::This, span)),
                field: Symbol::intern("$objc.owns"),
                value: Box::new(Expr::new(ExprKind::Int(val), span)),
                is_init: true,
            },
            span,
        )
    };
    let init_fn = FnDecl {
        is_pub: true,
        attrs: Box::new([]),
        name: Symbol::intern("$objc.bindHandle"),
        type_params: Box::new([]),
        params: Box::new([bind_h_param()]),
        ret: None,
        body: Block {
            stmts: vec![
                Stmt::new(StmtKind::Expr(assign_handle()), span),
                Stmt::new(StmtKind::Expr(assign_owns(1)), span),
            ],
            tail: None,
        },
        span,
        is_override: false,
        is_async: false,
        intrinsic_name: None,
    };
    let unowned_init_fn = FnDecl {
        is_pub: true,
        attrs: Box::new([]),
        name: Symbol::intern("$objc.bindHandleUnowned"),
        type_params: Box::new([]),
        params: Box::new([bind_h_param()]),
        ret: None,
        body: Block {
            stmts: vec![
                Stmt::new(StmtKind::Expr(assign_handle()), span),
                Stmt::new(StmtKind::Expr(assign_owns(0)), span),
            ],
            tail: None,
        },
        span,
        is_override: false,
        is_async: false,
        intrinsic_name: None,
    };

    // Auto-deinit on the root @objc class: release the underlying
    // ObjC object iff this wrapper owns it and the handle is set.
    // Subclasses inherit the deinit unchanged.
    let deinit_fn = build_root_deinit(ctx, span);

    // Only the root @objc class declares the binders + deinit —
    // children inherit them through the normal ilang vtable.
    let mut methods: Vec<FnDecl> = if has_parent {
        Vec::new()
    } else {
        vec![init_fn, unowned_init_fn, deinit_fn]
    };
    // `pub static __wrap_handle(h: i64): Self` — internal helper
    // the @objc desugar leans on to wrap a raw ObjC id into an
    // ilang instance. Hidden from LSP through the `__` prefix
    // filter; user code in cocoa.il references it explicitly to
    // expose a friendly `wrap(h: i64): NSObject` on top of it.
    let wrap_param = || Param {
        name: Symbol::intern("h"),
        ty: Type::I64,
        span,
        default: None,
    };
    let wrap_body_new = |init: &'static str| {
        Expr::new(
            ExprKind::New {
                class: class_name,
                type_args: Box::new([]),
                args: Box::new([Expr::new(
                    ExprKind::Var(Symbol::intern("h")),
                    span,
                )]),
                init_method: Some(Symbol::intern(init)),
            },
            span,
        )
    };
    let wrap_fn = FnDecl {
        is_pub: true,
        attrs: Box::new([]),
        name: Symbol::intern("__wrap_handle"),
        type_params: Box::new([]),
        params: Box::new([wrap_param()]),
        ret: Some(Type::Object(class_name)),
        body: Block {
            stmts: Vec::new(),
            tail: Some(Box::new(wrap_body_new("$objc.bindHandle"))),
        },
        span,
        is_override: false,
        is_async: false,
        intrinsic_name: None,
    };
    // Mirror of `__wrap_handle` for the non-owning case — wraps a
    // handle whose retain count we don't manage. Block callback
    // bodies use this to view an AppKit-owned id (NSEvent etc.)
    // without our deinit double-releasing it.
    let wrap_unowned_fn = FnDecl {
        is_pub: true,
        attrs: Box::new([]),
        name: Symbol::intern("__wrap_handle_unowned"),
        type_params: Box::new([]),
        params: Box::new([wrap_param()]),
        ret: Some(Type::Object(class_name)),
        body: Block {
            stmts: Vec::new(),
            tail: Some(Box::new(wrap_body_new("$objc.bindHandleUnowned"))),
        },
        span,
        is_override: false,
        is_async: false,
        intrinsic_name: None,
    };
    let mut static_methods: Vec<FnDecl> = vec![wrap_fn, wrap_unowned_fn];
    let mut aliases: Vec<ilang_ast::ExternCItem> = Vec::new();
    // Track which `<tag>_msg_<class>_<name>` alias declarations
    // we've already emitted in this class. Methods sharing an ObjC
    // selector (typed-vs-NSObject `setDelegate:` overloads, etc.)
    // would otherwise generate duplicate extern decls with the
    // same signature, which the ilang type checker flags as an
    // ambiguous overload.
    let mut emitted_alias_names: HashSet<Symbol> = HashSet::new();
    // Collect bodied methods so the `register()` builder can emit
    // a `class_addMethod` call per IMP. Stored as
    // (method_name, selector, type_encoding, imp_symbol).
    let mut imps_to_attach: Vec<ImpEntry> = Vec::new();
    // `@objc("sel") pub get / pub set` accessors collect here;
    // installed on the synthesised ClassDecl's `properties` slot
    // below so `obj.name` / `obj.name = v` dispatch through
    // ilang's property machinery onto the @objc-msgSend body.
    let mut properties: Vec<PropertyDecl> = Vec::new();

    let is_subclass = c.parent.is_some();
    for m in &c.methods {
        if m.body.is_some() && !is_subclass {
            // Body without a parent: the user wanted to override
            // something but didn't declare what they inherited
            // from. Surface this as an error rather than silently
            // dropping the body.
            // (Parsing already accepted the body; we just don't
            // emit an IMP. Future: report this through the
            // ParseError channel.)
        }

        // Plain (non-@objc) method living inside the @objc class:
        // pass it through as a regular ilang method, no
        // `objc_msgSend` wrapper. The parser flags these with an
        // empty selector. Used for static helpers like
        // `pub static wrap(h: i64): NSObject { __wrap_handle(h) }`.
        if m.selector.is_empty() {
            let plain_fn = FnDecl {
                is_pub: m.is_pub,
                attrs: Box::new([]),
                name: m.name,
                type_params: Box::new([]),
                params: m.params.clone(),
                ret: m.ret.clone(),
                body: m.body.clone().unwrap_or(Block {
                    stmts: Vec::new(),
                    tail: None,
                }),
                span: m.span,
                is_override: false,
                is_async: false,
                intrinsic_name: None,
            };
            if m.is_static {
                static_methods.push(plain_fn);
            } else {
                methods.push(plain_fn);
            }
            continue;
        }

        // Property accessors share `m.name` (the bare property
        // name) between getter and setter, so include the
        // accessor kind in the alias name to keep their
        // signatures distinct (`…msg_X_size_get` vs
        // `…msg_X_size_set`). Plain methods stay
        // `…msg_X_<name>` for backwards compatibility.
        let alias_name: Symbol = match m.accessor {
            Some(AccessorKind::Getter) => format!(
                "{}_msg_{}_{}_get",
                ctx.tag, class_name.as_str(), m.name.as_str()
            ).into(),
            Some(AccessorKind::Setter) => format!(
                "{}_msg_{}_{}_set",
                ctx.tag, class_name.as_str(), m.name.as_str()
            ).into(),
            None => format!(
                "{}_msg_{}_{}",
                ctx.tag, class_name.as_str(), m.name.as_str()
            ).into(),
        };
        let (alias_decl, method_fn) = build_class_method(class_name, m, alias_name, ctx);
        if emitted_alias_names.insert(alias_name) {
            aliases.push(alias_decl);
        }
        // `pub get / pub set` inside an @objc class: install the
        // synthesised dispatch FnDecl as the property's accessor
        // instead of a regular method. The existing
        // `class.properties` machinery handles `obj.name` /
        // `obj.name = v` call sites; the field type is the
        // getter's return type (or the setter's sole param type
        // if the getter is declared later).
        if let Some(kind) = m.accessor {
            // Static `pub static get blackColor(): NSColor` etc.
            // are common in Cocoa (`+[NSColor blackColor]`,
            // `+[NSApplication sharedApplication]`, ...). The
            // synthesised getter / setter FnDecl is treated as a
            // class-level method by the rest of the desugar; we
            // just mark the resulting PropertyDecl as static so
            // the type checker / MIR dispatch reads / writes via
            // `ClassName.name` instead of `obj.name`.
            let prop_ty = match kind {
                AccessorKind::Getter => method_fn
                    .ret
                    .clone()
                    .expect("getter ret checked at parse"),
                AccessorKind::Setter => method_fn.params[0].ty.clone(),
            };
            if let Some(existing) =
                properties.iter_mut().find(|p: &&mut PropertyDecl| p.name == m.name)
            {
                match kind {
                    AccessorKind::Getter => existing.getter = Some(method_fn),
                    AccessorKind::Setter => existing.setter = Some(method_fn),
                }
                if m.is_pub {
                    existing.is_pub = true;
                }
            } else {
                let (getter, setter) = match kind {
                    AccessorKind::Getter => (Some(method_fn), None),
                    AccessorKind::Setter => (None, Some(method_fn)),
                };
                properties.push(PropertyDecl {
                    is_pub: m.is_pub,
                    is_static: m.is_static,
                    name: m.name,
                    ty: prop_ty,
                    getter,
                    setter,
                    span: m.span,
                });
            }
            continue;
        }
        if m.is_static {
            static_methods.push(method_fn);
        } else {
            methods.push(method_fn);
        }

        // When the user supplied a `{ body }` and we have a
        // parent, emit a hidden `__impl_<method>` method (the
        // actual ilang implementation) and a C-ABI IMP function
        // that wraps `self` into an ilang instance and calls
        // `__impl_<method>`. `register()` then dlsym's the IMP
        // and class_addMethod's it onto the subclass.
        if let (Some(user_body), true) = (m.body.as_ref(), is_subclass) {
            let impl_name: Symbol = format!("_ilang_impl_{}", m.name.as_str()).into();
            let impl_fn = FnDecl {
                is_pub: true,
                attrs: Box::new([]),
                name: impl_name,
                type_params: Box::new([]),
                params: m.params.clone(),
                ret: m.ret.clone(),
                body: user_body.clone(),
                span: m.span,
                is_override: false,
                is_async: false,
                intrinsic_name: None,
            };
            if m.is_static {
                static_methods.push(impl_fn);
            } else {
                methods.push(impl_fn);
            }

            let imp_symbol: Symbol =
                format!("$objc.imp.{}.{}", class_name.as_str(), m.name.as_str()).into();
            let imp_fn = build_imp_fn(class_name, m, imp_symbol, impl_name, ctx);
            // The IMP itself is a top-level @extern(C) FnDef that
            // we'll push into the block's items list. Return it
            // through `aliases` (the caller appends everything).
            aliases.push(imp_fn);

            let encoding = encode_method_signature(&m.params, m.ret.as_ref(), ctx.class_names);
            imps_to_attach.push(ImpEntry {
                selector: m.selector.clone(),
                encoding,
                imp_symbol,
            });
        }
    }

    // Only emit the subclass machinery (super helpers, register
    // static, libobjc dispatch) when this class actually overrides
    // at least one method. A bare `@objc class A : B { }` is a
    // pure ilang-type-system inheritance and needs no runtime
    // registration — the parent class already exists in libobjc.
    let is_real_subclass = c.parent.is_some() && !imps_to_attach.is_empty();
    if is_real_subclass {
        let parent_name = c.parent.unwrap();
        for m in &c.methods {
            if m.is_static {
                continue;
            }
            let (helper_fn, super_alias) =
                build_super_helper(class_name, parent_name, m, ctx);
            methods.push(helper_fn);
            aliases.push(super_alias);
        }
        rewrite_super_in_methods(&mut methods);
        rewrite_super_in_methods(&mut static_methods);
        static_methods.push(build_register_class_fn(
            class_name, parent_name, &imps_to_attach, ctx, span,
        ));
    }

    let class_decl = ClassDecl {
        extern_lib: None,
        is_repr_c: false,
        is_packed: false,
        is_handle: false,
        is_union: false,
        is_pub: c.is_pub,
        name: class_name,
        // ilang-side inheritance mirrors the declared ObjC parent.
        // Methods declared on the parent (e.g. `hash`, `release`)
        // become callable on the child instance, dispatched as
        // normal ilang method calls through the desugared parent
        // class (which itself does objc_msgSend).
        parent: c.parent,
        interfaces: c.interfaces.clone().into_boxed_slice(),
        type_params: Box::new([]),
        fields: fields.into_boxed_slice(),
        methods: methods.into(),
        static_methods: static_methods.into(),
        static_fields: Box::new([]),
        properties: properties.into_boxed_slice(),
        // Preserve `@objc` on the synthesised class so LSP hover
        // can render it. No args — `@objc class` carries no
        // selector at the class level.
        attrs: Box::new([Attribute {
            name: Symbol::intern("objc"),
            args: Box::new([]),
        }]),
        span,
    };
    (ilang_ast::ExternCItem::Class(class_decl), aliases)
}

/// Construct one method (instance or static) of an @objc class:
/// the `objc_msgSend` alias declaration + the ilang method that
/// marshals args and forwards.
fn build_class_method(
    class_name: Symbol,
    m: &ObjcMethod,
    alias_name: Symbol,
    ctx: &ObjcCtx<'_>,
) -> (ilang_ast::ExternCItem, FnDecl) {
    // Alias parameter list:
    //   [receiver_ptr, sel, ...args_as_raw_pointers_or_scalars]
    // Receiver is `*objc_object` for instance methods, `*class_t`
    // for static methods.
    let receiver_ty = if m.is_static {
        Type::RawPtr {
            is_const: false,
            inner: Box::new(Type::Object(ctx.class_struct)),
        }
    } else {
        Type::RawPtr {
            is_const: false,
            inner: Box::new(Type::Object(ctx.object_struct)),
        }
    };
    let mut alias_params: Vec<Param> = Vec::new();
    alias_params.push(Param {
        name: Symbol::intern("_recv"),
        ty: receiver_ty,
        span: m.span,
        default: None,
    });
    alias_params.push(Param {
        name: Symbol::intern("_sel"),
        ty: Type::RawPtr {
            is_const: false,
            inner: Box::new(Type::Object(ctx.sel_struct)),
        },
        span: m.span,
        default: None,
    });
    for p in m.params.iter() {
        let p_ty = if is_objc_class_ty(&p.ty, ctx.class_names) {
            // @objc class arg passes its underlying id (raw ptr).
            Type::RawPtr {
                is_const: false,
                inner: Box::new(Type::Object(ctx.object_struct)),
            }
        } else if let Some(elem) = simd_array_elem(&p.ty) {
            // `simd.f32x2[]` etc. — Apple's factories take a
            // `const vector_floatN *`. Marshal as a const raw
            // pointer to the SIMD elem; the wrapper body emits
            // an `arr as *const simd.fNxM` cast which lowers to
            // a `__array_data_ptr` extract.
            Type::RawPtr {
                is_const: true,
                inner: Box::new(elem),
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
        c_symbol: Some(Symbol::intern("objc_msgSend")),
        intrinsic_name: None,
        variadic: false,
        span: m.span,
    };

    // ilang method body. We funnel everything through a `Block`
    // with let statements + a tail expression so the AST stays
    // straightforward.
    let mut stmts: Vec<Stmt> = Vec::new();

    // 1. Receiver value.
    //    Instance: this.handle as *objc_object
    //    Static:   __get_class(cstrFromString("ClassName"))
    let receiver_value = if m.is_static {
        Expr::new(
            ExprKind::Call {
                callee: ctx.get_class,
                args: Box::new([build_cstr(class_name.as_str(), m.span)]),
            },
            m.span,
        )
    } else {
        let handle_field = Expr::new(
            ExprKind::Field {
                obj: Box::new(Expr::new(ExprKind::This, m.span)),
                name: Symbol::intern("handle"),
            },
            m.span,
        );
        Expr::new(
            ExprKind::Cast {
                expr: Box::new(handle_field),
                ty: Type::RawPtr {
                    is_const: false,
                    inner: Box::new(Type::Object(ctx.object_struct)),
                },
            },
            m.span,
        )
    };
    let recv_name = Symbol::intern("__recv");
    stmts.push(Stmt::new(
        StmtKind::Let {
            is_pub: false,
            is_const: false,
            name: recv_name,
            ty: None,
            value: receiver_value,
        },
        m.span,
    ));

    // 2. Selector intern.
    let sel_name = Symbol::intern("__sel");
    stmts.push(Stmt::new(
        StmtKind::Let {
            is_pub: false,
            is_const: false,
            name: sel_name,
            ty: None,
            value: build_cached_sel_call(&m.selector, ctx.sel_register, ctx.sel_struct, ctx.sel_cache, m.span),
        },
        m.span,
    ));

    // 3. Build call args: receiver, sel, and one per declared param
    //    (extracting `.handle as *objc_object` for @objc class args).
    let mut call_args: Vec<Expr> = Vec::with_capacity(m.params.len() + 2);
    call_args.push(Expr::new(ExprKind::Var(recv_name), m.span));
    call_args.push(Expr::new(ExprKind::Var(sel_name), m.span));
    for p in m.params.iter() {
        let arg_expr = if is_objc_class_ty(&p.ty, ctx.class_names) {
            // `Field(Var(p.name), "handle")` is safe even when p.name
            // collides with a `use`d module — normalize's scope-aware
            // rewrite suppresses the module-name collapse on locals.
            let field = Expr::new(
                ExprKind::Field {
                    obj: Box::new(Expr::new(ExprKind::Var(p.name), p.span)),
                    name: Symbol::intern("handle"),
                },
                p.span,
            );
            Expr::new(
                ExprKind::Cast {
                    expr: Box::new(field),
                    ty: Type::RawPtr {
                        is_const: false,
                        inner: Box::new(Type::Object(ctx.object_struct)),
                    },
                },
                p.span,
            )
        } else if let Some(elem) = simd_array_elem(&p.ty) {
            // SIMD array → const raw pointer. The cast lowers
            // to a `__array_data_ptr` extract (offset +16 of
            // the ilang array header), matching the C ABI for
            // `const vector_floatN *`.
            Expr::new(
                ExprKind::Cast {
                    expr: Box::new(Expr::new(ExprKind::Var(p.name), p.span)),
                    ty: Type::RawPtr {
                        is_const: true,
                        inner: Box::new(elem),
                    },
                },
                p.span,
            )
        } else {
            Expr::new(ExprKind::Var(p.name), p.span)
        };
        call_args.push(arg_expr);
    }
    let call_expr = Expr::new(
        ExprKind::Call {
            callee: alias_name,
            args: call_args.into(),
        },
        m.span,
    );

    // 4. Tail expression: wrap an @objc-class return into `new T(raw as i64)`,
    //    otherwise just return the raw value (or no tail for unit).
    //
    // Memory rules (mirrors Apple's ARC NS_RETURNS_RETAINED family):
    //   * `alloc*` / `new*` / `copy*` / `mutableCopy*` / `init*` /
    //     `retain` return +1 — wrap as-is. The instance `init*` case
    //     additionally needs to consume `self`: set `this.handle = 0`
    //     so the (now-stale, same-pointer) outer wrapper's deinit
    //     doesn't double-release the object init just gave us back.
    //   * Anything else is autoreleased — `objc_retain` first so our
    //     wrapper holds a stable +1 once the pool drains.
    let retained_return = returns_retained_selector(&m.selector);
    // `init`-family instance methods are special: ilang's MIR
    // ignores the body's tail value for any method literally named
    // `init` and synthesises a return of `this` instead (so that
    // `new C(args)` callers get the constructed instance back).
    // We therefore don't wrap a fresh `new T(...)`; we overwrite
    // `this.handle` with the pointer ObjC's `init` returned (same
    // pointer in the common case, a substituted one for the rare
    // `self = [super init]` pattern). The +1 alloc'd into `this`
    // is now balanced by `this`'s own deinit when it drops.
    let init_returns_this = !m.is_static && m.name.as_str() == "init";
    let tail = match &m.ret {
        Some(t) if is_objc_class_ty(t, ctx.class_names) => {
            let result_name = Symbol::intern("__raw");
            // let __raw = (alias_call) as i64
            let call_as_i64 = Expr::new(
                ExprKind::Cast {
                    expr: Box::new(call_expr),
                    ty: Type::I64,
                },
                m.span,
            );
            stmts.push(Stmt::new(
                StmtKind::Let {
                    is_pub: false,
                    is_const: false,
                    name: result_name,
                    ty: None,
                    value: call_as_i64,
                },
                m.span,
            ));
            // For init-family instance methods, zero out this.handle
            // so our outer wrapper's deinit doesn't release the
            // pointer init just returned to us (same +1 the caller
            // now owns through the new wrapper).
            if init_returns_this {
                // `init` family: adopt the returned pointer onto
                // `this.handle` (typically a no-op since the pointer
                // matches the alloc'd one). MIR will synthesise
                // `return this` for us, so building a fresh
                // wrapper here would be both wasteful and wrong —
                // the caller never sees it.
                let assign_handle = Expr::new(
                    ExprKind::AssignField {
                        obj: Box::new(Expr::new(ExprKind::This, m.span)),
                        field: Symbol::intern("handle"),
                        value: Box::new(Expr::new(ExprKind::Var(result_name), m.span)),
                        is_init: false,
                    },
                    m.span,
                );
                stmts.push(Stmt::new(StmtKind::Expr(assign_handle), m.span));
                // Tail = `this` so the type checker sees the
                // declared `: Self` return type satisfied. MIR
                // would have synthesised a return-this anyway for
                // any method literally named `init`.
                Some(Box::new(Expr::new(ExprKind::This, m.span)))
            } else {
                // If the selector returns autoreleased, retain the
                // raw pointer before wrapping so it survives the
                // next pool drain.
                let raw_for_wrap = if retained_return {
                    Expr::new(ExprKind::Var(result_name), m.span)
                } else {
                    let raw_as_ptr = Expr::new(
                        ExprKind::Cast {
                            expr: Box::new(Expr::new(ExprKind::Var(result_name), m.span)),
                            ty: Type::RawPtr {
                                is_const: false,
                                inner: Box::new(Type::Object(ctx.object_struct)),
                            },
                        },
                        m.span,
                    );
                    let retain_call = Expr::new(
                        ExprKind::Call {
                            callee: ctx.retain,
                            args: Box::new([raw_as_ptr]),
                        },
                        m.span,
                    );
                    Expr::new(
                        ExprKind::Cast {
                            expr: Box::new(retain_call),
                            ty: Type::I64,
                        },
                        m.span,
                    )
                };
                let new_expr = Expr::new(
                    ExprKind::New {
                        class: ret_class_symbol(t),
                        type_args: Box::new([]),
                        args: Box::new([raw_for_wrap]),
                        init_method: Some(Symbol::intern("$objc.bindHandle")),
                    },
                    m.span,
                );
                Some(Box::new(new_expr))
            }
        }
        Some(_) => Some(Box::new(call_expr)),
        None => {
            // Statement-level call, no tail.
            stmts.push(Stmt::new(StmtKind::Expr(call_expr), m.span));
            None
        }
    };

    // Base attrs the synthesised dispatch wrapper always carries,
    // plus any user-supplied passthrough attrs (`@deprecated`, …).
    let mut wrapper_attrs: Vec<Attribute> = vec![
        Attribute {
            name: Symbol::intern("$objc.wrapper"),
            args: Box::new([]),
        },
        Attribute {
            name: Symbol::intern("objc"),
            args: Box::new([AttrArg::Str(m.selector.clone())]),
        },
    ];
    wrapper_attrs.extend(m.extra_attrs.iter().cloned());
    let method_fn = FnDecl {
        is_pub: m.is_pub,
        // The wrapper's signature may reference raw `*objc_*`
        // types when the user's @objc method declared them; flag
        // so the type checker's pointer-in-signature rejection
        // doesn't trip on us. The trailing `@objc("selector:")`
        // is purely informational — kept so LSP hover renders it.
        attrs: wrapper_attrs.into_boxed_slice(),
        name: m.name,
        type_params: Box::new([]),
        params: m.params.clone(),
        ret: m.ret.clone(),
        body: Block { stmts, tail },
        span: m.span,
        // Propagate the user-written `override` so the type
        // checker accepts a subclass shadowing an inherited slot
        // (the IMP installed by `register()` is exactly the
        // override mechanism — `class_addMethod` swaps the
        // selector's implementation at registration time).
        is_override: m.is_override,
        is_async: false,
        intrinsic_name: None,
    };
    (alias_decl, method_fn)
}
