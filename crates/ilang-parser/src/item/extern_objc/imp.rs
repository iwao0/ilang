//! IMP-side machinery for subclass overrides. `build_imp_fn`
//! synthesises the C-ABI IMP that ObjC's runtime calls when our
//! subclass receives a selector — it wraps `self` back into an
//! ilang instance, marshals each `id` arg into its declared ilang
//! type, calls the hidden `__impl_<method>`, and unwraps the
//! return. `build_register_class_fn` emits the per-class
//! `register()` static that runs once at startup to register the
//! subclass with the ObjC runtime and `class_addMethod` every IMP.

use std::collections::HashSet;

use ilang_ast::{
    Attribute, Block, Expr, ExprKind, FnDecl, Param, Span, Stmt, StmtKind, Symbol, Type,
};

use super::model::{is_objc_class_ty, ImpEntry, ObjcCtx, ObjcMethod};
use super::selector::{build_cached_sel_call, build_cstr};

/// Encode an ilang method signature in the Objective-C type
/// encoding format. `<ret>@:<arg1><arg2>...` where `@` is the
/// receiver (id) and `:` is the implicit `_cmd` selector. The
/// encoding covers the primitive types we currently marshal; any
/// type outside this set falls back to `?` so unhandled cases
/// surface as a runtime-side message-signature mismatch rather
/// than silently corrupting the dispatch.
pub(super) fn encode_method_signature(
    params: &[Param],
    ret: Option<&Type>,
    class_names: &HashSet<Symbol>,
) -> String {
    let mut s = String::new();
    s.push_str(encode_type_char(ret, class_names));
    s.push('@'); // self
    s.push(':'); // _cmd
    for p in params {
        s.push_str(encode_type_char(Some(&p.ty), class_names));
    }
    s
}

fn encode_type_char(t: Option<&Type>, class_names: &HashSet<Symbol>) -> &'static str {
    match t {
        None => "v",
        Some(t) => match t {
            Type::Unit => "v",
            Type::Bool | Type::I8 => "c",
            Type::U8 => "C",
            Type::I16 => "s",
            Type::U16 => "S",
            Type::I32 => "i",
            Type::U32 => "I",
            Type::I64 | Type::SSize => "q",
            Type::U64 | Type::Size => "Q",
            Type::F32 => "f",
            Type::F64 => "d",
            Type::Object(n) if class_names.contains(n) => "@",
            Type::RawPtr { .. } => "^v",
            _ => "?",
        },
    }
}

/// Generate the C-ABI IMP that ObjC calls when our subclass
/// receives the given selector. It wraps `self` into a fresh
/// ilang class instance, marshals each argument from raw
/// pointer / scalar form into the ilang method's declared
/// parameter shape, calls the hidden `__impl_<method>` method,
/// then unwraps the return.
pub(super) fn build_imp_fn(
    class_name: Symbol,
    m: &ObjcMethod,
    imp_symbol: Symbol,
    impl_method_name: Symbol,
    ctx: &ObjcCtx<'_>,
) -> ilang_ast::ExternCItem {
    let mut params: Vec<Param> = Vec::with_capacity(m.params.len() + 2);
    params.push(Param {
        name: Symbol::intern("__self"),
        ty: Type::RawPtr {
            is_const: false,
            inner: Box::new(Type::Object(ctx.object_struct)),
        },
        span: m.span,
        default: None,
    });
    params.push(Param {
        name: Symbol::intern("__cmd"),
        ty: Type::RawPtr {
            is_const: false,
            inner: Box::new(Type::Object(ctx.sel_struct)),
        },
        span: m.span,
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
        params.push(Param {
            name: p.name,
            ty: p_ty,
            span: p.span,
            default: None,
        });
    }

    let imp_ret = match m.ret.as_ref() {
        Some(t) if is_objc_class_ty(t, ctx.class_names) => Some(Type::RawPtr {
            is_const: false,
            inner: Box::new(Type::Object(ctx.object_struct)),
        }),
        Some(t) => Some(t.clone()),
        None => None,
    };

    // Body:
    //   let me = new MyClass(__self as i64)
    //   <marshal args>
    //   <call me.__impl_<method>(args)>
    //   <unwrap return if any>
    let me_name = Symbol::intern("__me");
    let self_as_i64 = Expr::new(
        ExprKind::Cast {
            expr: Box::new(Expr::new(ExprKind::Var(Symbol::intern("__self")), m.span)),
            ty: Type::I64,
        },
        m.span,
    );
    let new_me = Expr::new(
        ExprKind::New {
            class: class_name,
            type_args: Box::new([]),
            args: Box::new([self_as_i64]),
            // AppKit gave us this `self`; it owns the reference.
            // The IMP-side wrapper is non-owning so its deinit
            // doesn't double-release when the IMP returns.
            init_method: Some(Symbol::intern("$objc.bindHandleUnowned")),
        },
        m.span,
    );
    let me_let = Stmt::new(
        StmtKind::Let {
            is_pub: false,
            is_const: false,
            name: me_name,
            ty: None,
            value: new_me,
        },
        m.span,
    );

    let mut stmts = vec![me_let];

    // Per-arg marshalling: @objc class args arrive as raw
    // `*objc_object`; wrap into `new ArgClass(p as i64)` before
    // calling the ilang method. Scalars pass through unchanged.
    let mut call_args: Vec<Expr> = Vec::with_capacity(m.params.len());
    for p in m.params.iter() {
        if is_objc_class_ty(&p.ty, ctx.class_names) {
            let arg_as_i64 = Expr::new(
                ExprKind::Cast {
                    expr: Box::new(Expr::new(ExprKind::Var(p.name), p.span)),
                    ty: Type::I64,
                },
                p.span,
            );
            let class = match &p.ty {
                Type::Object(n) => *n,
                _ => unreachable!("checked by is_objc_class_ty"),
            };
            let wrapped = Expr::new(
                ExprKind::New {
                    class,
                    type_args: Box::new([]),
                    args: Box::new([arg_as_i64]),
                    // Same reasoning as `__me` above — the sender
                    // is owned by AppKit, not by us.
                    init_method: Some(Symbol::intern("$objc.bindHandleUnowned")),
                },
                p.span,
            );
            let wname: Symbol = format!("__w_{}", p.name.as_str()).into();
            stmts.push(Stmt::new(
                StmtKind::Let {
                    is_pub: false,
                    is_const: false,
                    name: wname,
                    ty: None,
                    value: wrapped,
                },
                p.span,
            ));
            call_args.push(Expr::new(ExprKind::Var(wname), p.span));
        } else {
            call_args.push(Expr::new(ExprKind::Var(p.name), p.span));
        }
    }

    let impl_call = Expr::new(
        ExprKind::MethodCall {
            obj: Box::new(Expr::new(ExprKind::Var(me_name), m.span)),
            method: impl_method_name,
            args: call_args.into(),
        },
        m.span,
    );

    let tail = match m.ret.as_ref() {
        Some(t) if is_objc_class_ty(t, ctx.class_names) => {
            // Unwrap the ilang return value back to its handle as
            // a raw `*objc_object` so the ObjC runtime gets what
            // it expects.
            let r_name = Symbol::intern("__r");
            stmts.push(Stmt::new(
                StmtKind::Let {
                    is_pub: false,
                    is_const: false,
                    name: r_name,
                    ty: None,
                    value: impl_call,
                },
                m.span,
            ));
            let handle_field = Expr::new(
                ExprKind::Field {
                    obj: Box::new(Expr::new(ExprKind::Var(r_name), m.span)),
                    name: Symbol::intern("handle"),
                },
                m.span,
            );
            let handle_as_ptr = Expr::new(
                ExprKind::Cast {
                    expr: Box::new(handle_field),
                    ty: Type::RawPtr {
                        is_const: false,
                        inner: Box::new(Type::Object(ctx.object_struct)),
                    },
                },
                m.span,
            );
            let _ = t;
            Some(Box::new(handle_as_ptr))
        }
        Some(_) => Some(Box::new(impl_call)),
        None => {
            stmts.push(Stmt::new(StmtKind::Expr(impl_call), m.span));
            None
        }
    };

    let fndecl = FnDecl {
        is_pub: false,
        // Re-use the wrapper bypass — the IMP's signature
        // intentionally carries `*objc_object` etc. (it has to,
        // by ObjC's calling convention) so the raw-pointer
        // rejection isn't appropriate.
        attrs: Box::new([Attribute {
            name: Symbol::intern("$objc.wrapper"),
            args: Box::new([]),
        }]),
        name: imp_symbol,
        type_params: Box::new([]),
        params: params.into(),
        ret: imp_ret,
        body: Block { stmts, tail },
        span: m.span,
        is_override: false,
        is_async: false,
        intrinsic_name: None,
    };
    ilang_ast::ExternCItem::FnDef(fndecl)
}

/// Build a `pub static register()` method that registers the
/// subclass with the ObjC runtime on first call and attaches
/// every IMP via `class_addMethod`. Idempotent through the
/// `objc_getClass` probe at the top.
pub(super) fn build_register_class_fn(
    class_name: Symbol,
    parent_name: Symbol,
    imps: &[ImpEntry],
    ctx: &ObjcCtx<'_>,
    span: Span,
) -> FnDecl {
    // let existing = __get_class(cstrFromString("ClassName"))
    // if (existing as i64) != 0 { return }
    // let parent = __get_class(cstrFromString("ParentName"))
    // let cls = __allocate_class_pair(parent, cstrFromString("ClassName"), 0)
    // __register_class_pair(cls)
    let existing_name = Symbol::intern("__existing");
    let parent_var = Symbol::intern("__parent");
    let cls_var = Symbol::intern("__cls");

    let get_existing = Expr::new(
        ExprKind::Call {
            callee: ctx.get_class,
            args: Box::new([build_cstr(class_name.as_str(), span)]),
        },
        span,
    );
    let existing_let = Stmt::new(
        StmtKind::Let {
            is_pub: false,
            is_const: false,
            name: existing_name,
            ty: None,
            value: get_existing,
        },
        span,
    );

    // `if (existing as i64) != 0 { return }`
    let existing_as_i64 = Expr::new(
        ExprKind::Cast {
            expr: Box::new(Expr::new(ExprKind::Var(existing_name), span)),
            ty: Type::I64,
        },
        span,
    );
    let cond = Expr::new(
        ExprKind::Binary {
            op: ilang_ast::BinOp::Ne,
            lhs: Box::new(existing_as_i64),
            rhs: Box::new(Expr::new(ExprKind::Int(0), span)),
        },
        span,
    );
    let return_block = ilang_ast::Block {
        stmts: Vec::new(),
        tail: Some(Box::new(Expr::new(ExprKind::Return(None), span))),
    };
    let early_return = Stmt::new(
        StmtKind::Expr(Expr::new(
            ExprKind::If {
                cond: Box::new(cond),
                then_branch: return_block,
                else_branch: None,
            },
            span,
        )),
        span,
    );

    // The loader's merge prefixes class names by module
    // (`NSObject` → `foundation.NSObject`); `objc_getClass`
    // needs the bare Objective-C name, so strip everything up
    // to the last `.`.
    let parent_objc_name = parent_name
        .as_str()
        .rsplit_once('.')
        .map(|(_, tail)| tail)
        .unwrap_or_else(|| parent_name.as_str());
    let get_parent = Expr::new(
        ExprKind::Call {
            callee: ctx.get_class,
            args: Box::new([build_cstr(parent_objc_name, span)]),
        },
        span,
    );
    let parent_let = Stmt::new(
        StmtKind::Let {
            is_pub: false,
            is_const: false,
            name: parent_var,
            ty: None,
            value: get_parent,
        },
        span,
    );

    let allocate_call = Expr::new(
        ExprKind::Call {
            callee: ctx.allocate_pair,
            args: Box::new([
                Expr::new(ExprKind::Var(parent_var), span),
                build_cstr(class_name.as_str(), span),
                Expr::new(
                    ExprKind::Cast {
                        expr: Box::new(Expr::new(ExprKind::Int(0), span)),
                        ty: Type::Size,
                    },
                    span,
                ),
            ]),
        },
        span,
    );
    let cls_let = Stmt::new(
        StmtKind::Let {
            is_pub: false,
            is_const: false,
            name: cls_var,
            ty: None,
            value: allocate_call,
        },
        span,
    );

    // class_addMethod calls — one per IMP. Look up the IMP's
    // address via dlsym(RTLD_DEFAULT, "ilang_objc_imp__..."). On
    // Apple platforms RTLD_DEFAULT is encoded as -2; passing 0
    // gives non-deterministic behaviour per Apple's manpage.
    let mut stmts = vec![existing_let, early_return, parent_let, cls_let];
    for imp in imps {
        let rtld_default = Expr::new(
            ExprKind::Cast {
                expr: Box::new(Expr::new(ExprKind::Int(-2), span)),
                ty: Type::RawPtr {
                    is_const: false,
                    inner: Box::new(Type::CVoid),
                },
            },
            span,
        );
        let imp_addr = Expr::new(
            ExprKind::Call {
                callee: ctx.dlsym,
                args: Box::new([rtld_default, build_cstr(imp.imp_symbol.as_str(), span)]),
            },
            span,
        );
        // `register()` is itself idempotent (`__existing != 0` guard
        // returns early), but route the selector through the shared
        // cache anyway so every selector in the block has a single
        // slot — keeps the cache class membership uniform.
        let sel_call = build_cached_sel_call(&imp.selector, ctx.sel_register, ctx.sel_struct, ctx.sel_cache, span);
        let encoding_cstr = build_cstr(&imp.encoding, span);
        let add_method = Expr::new(
            ExprKind::Call {
                callee: ctx.class_add_method,
                args: Box::new([
                    Expr::new(ExprKind::Var(cls_var), span),
                    sel_call,
                    imp_addr,
                    encoding_cstr,
                ]),
            },
            span,
        );
        stmts.push(Stmt::new(StmtKind::Expr(add_method), span));
    }

    let register_call = Expr::new(
        ExprKind::Call {
            callee: ctx.register_pair,
            args: Box::new([Expr::new(ExprKind::Var(cls_var), span)]),
        },
        span,
    );
    stmts.push(Stmt::new(StmtKind::Expr(register_call), span));

    let body = Block {
        stmts,
        tail: None,
    };

    FnDecl {
        is_pub: true,
        attrs: Box::new([]),
        name: Symbol::intern("register"),
        type_params: Box::new([]),
        params: Box::new([]),
        ret: None,
        body,
        span,
        is_override: false,
        is_async: false,
        intrinsic_name: None,
    }
}
