//! `@extern(ObjC) { ... }` block parsing.
//!
//! Two shapes desugar to plain `@extern(C)` items at parse time:
//!
//!   1. **Top-level @objc fn** — a typed `objc_msgSend` alias plus a
//!      thin wrapper that interns the selector and forwards. The
//!      L1 alias path on `objc_msgSend` makes multiple shapes share
//!      one C symbol.
//!
//!   2. **@objc class** — an ilang class with a single `handle: i64`
//!      field plus an `init(h: i64)` constructor. Each declared
//!      instance / static method becomes an ilang method that
//!      extracts handles from arg classes, calls the corresponding
//!      `objc_msgSend` alias, and wraps the result back into an
//!      ilang class instance when the return type names another
//!      `@objc class` from the same block.
//!
//! The block's source position is woven into every synthesised name
//! so multiple `@extern(ObjC)` blocks in the same file coexist.

use std::collections::HashSet;

use ilang_ast::{
    AttrArg, Attribute, Block, ClassDecl, Expr, ExprKind, FieldDecl, FnDecl, Param, Span, Stmt,
    StmtKind, Symbol, Type,
};
use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;

impl<'a> Parser<'a> {
    pub(super) fn parse_extern_objc_block(
        &mut self,
    ) -> Result<ilang_ast::ExternCBlock, ParseError> {
        let block_span = self.peek().span;
        self.expect(&TokenKind::LBrace, "'{'")?;
        let tag = format!("__objc_b{}c{}", block_span.line, block_span.col);
        let sel_struct_name: Symbol = format!("{tag}_sel_t").into();
        let sel_register_name: Symbol = format!("{tag}_sel_register").into();
        let class_struct_name: Symbol = format!("{tag}_class_t").into();
        let get_class_name: Symbol = format!("{tag}_get_class").into();
        let object_struct_name: Symbol = format!("{tag}_object_t").into();

        let mut items: Vec<ilang_ast::ExternCItem> = Vec::new();
        let mut objc_fns: Vec<ObjcMethod> = Vec::new();
        let mut objc_classes: Vec<ObjcClass> = Vec::new();

        loop {
            if matches!(self.peek().kind, TokenKind::RBrace) {
                break;
            }
            let inner_attrs = self.parse_attributes()?;
            let item_is_pub = if matches!(self.peek().kind, TokenKind::Pub) {
                self.bump();
                true
            } else {
                false
            };

            // Look for @objc(...). Two shapes:
            //   @objc                 → followed by `class Name { ... }`
            //   @objc("selector:")    → followed by `fn name(...)`
            let objc_attr_pos = inner_attrs.iter().position(|a| a.name.as_str() == "objc");
            if let Some(pos) = objc_attr_pos {
                if inner_attrs.len() != 1 {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected: "@objc cannot be combined with other attributes inside @extern(ObjC)".into(),
                        span: t.span,
                    });
                }
                let attr = &inner_attrs[pos];
                if attr.args.is_empty() {
                    // @objc class Name { ... }
                    if !matches!(self.peek().kind, TokenKind::Class) {
                        let t = self.peek();
                        return Err(ParseError::Unexpected {
                            found: t.kind.clone(),
                            expected: "`class` after bare @objc (use @objc(\"sel:\") for fns)".into(),
                            span: t.span,
                        });
                    }
                    let parsed = self.parse_objc_class_decl(item_is_pub)?;
                    objc_classes.push(parsed);
                    continue;
                } else {
                    // @objc("sel") fn name(...) — free-fn dispatch
                    let selector = match &attr.args[..] {
                        [AttrArg::Str(s)] => s.clone(),
                        _ => {
                            let t = self.peek();
                            return Err(ParseError::Unexpected {
                                found: t.kind.clone(),
                                expected: "@objc(\"selector:\") takes exactly one string argument".into(),
                                span: t.span,
                            });
                        }
                    };
                    if !matches!(self.peek().kind, TokenKind::Fn) {
                        let t = self.peek();
                        return Err(ParseError::Unexpected {
                            found: t.kind.clone(),
                            expected: "`fn` after @objc(\"...\")".into(),
                            span: t.span,
                        });
                    }
                    let m = self.parse_objc_method(selector, item_is_pub, /*is_static*/ false)?;
                    objc_fns.push(m);
                    continue;
                }
            }

            // Non-@objc — regular @extern(C) item.
            let item = self.parse_extern_c_item_for_objc_block(inner_attrs, item_is_pub)?;
            items.push(item);
        }
        self.expect(&TokenKind::RBrace, "'}'")?;

        let any_objc = !objc_fns.is_empty() || !objc_classes.is_empty();
        let any_static = objc_classes
            .iter()
            .any(|c| c.methods.iter().any(|m| m.is_static));
        let any_class = !objc_classes.is_empty();

        if any_objc {
            // Selector type + sel_registerName alias.
            items.insert(
                0,
                ilang_ast::ExternCItem::Struct {
                    is_pub: false,
                    name: sel_struct_name,
                    fields: Box::new([]),
                    is_packed: false,
                    restrict_c_types: false,
                    span: block_span,
                },
            );
            items.insert(
                1,
                ilang_ast::ExternCItem::FnDecl {
                    is_pub: false,
                    name: sel_register_name,
                    params: Box::new([Param {
                        name: Symbol::intern("name"),
                        ty: Type::RawPtr {
                            is_const: true,
                            inner: Box::new(Type::CChar),
                        },
                        span: block_span,
                        default: None,
                    }]),
                    ret: Some(Type::RawPtr {
                        is_const: false,
                        inner: Box::new(Type::Object(sel_struct_name)),
                    }),
                    libs: Box::new([Symbol::intern("objc")]),
                    optional: false,
                    c_symbol: Some(Symbol::intern("sel_registerName")),
                    variadic: false,
                    span: block_span,
                },
            );
            // Opaque ObjC `id` placeholder — used as the receiver
            // type on instance-method aliases and as the value of
            // `arg.handle as *...` casts. Only injected when the
            // block actually declares an @objc class (top-level
            // @objc fns already use the user-named opaque types
            // in their declared signatures).
            if any_class {
                items.insert(
                    2,
                    ilang_ast::ExternCItem::Struct {
                        is_pub: false,
                        name: object_struct_name,
                        fields: Box::new([]),
                        is_packed: false,
                        restrict_c_types: false,
                        span: block_span,
                    },
                );
            }
            // Class lookup helpers are only injected when at least
            // one class uses a static method (only static dispatch
            // needs `objc_getClass`).
            if any_static {
                items.insert(
                    2,
                    ilang_ast::ExternCItem::Struct {
                        is_pub: false,
                        name: class_struct_name,
                        fields: Box::new([]),
                        is_packed: false,
                        restrict_c_types: false,
                        span: block_span,
                    },
                );
                items.insert(
                    3,
                    ilang_ast::ExternCItem::FnDecl {
                        is_pub: false,
                        name: get_class_name,
                        params: Box::new([Param {
                            name: Symbol::intern("name"),
                            ty: Type::RawPtr {
                                is_const: true,
                                inner: Box::new(Type::CChar),
                            },
                            span: block_span,
                            default: None,
                        }]),
                        ret: Some(Type::RawPtr {
                            is_const: false,
                            inner: Box::new(Type::Object(class_struct_name)),
                        }),
                        libs: Box::new([Symbol::intern("objc")]),
                        optional: false,
                        c_symbol: Some(Symbol::intern("objc_getClass")),
                        variadic: false,
                        span: block_span,
                    },
                );
            }
        }

        // Top-level @objc fns — same expansion as before.
        for m in objc_fns {
            let (alias, wrapper) =
                build_freefn_dispatch(&m, &tag, sel_struct_name, sel_register_name);
            items.push(alias);
            items.push(wrapper);
        }

        // Names of @objc classes declared in this block — used by
        // method-body desugar to decide which arg / return types
        // get handle-extracted vs passed straight through.
        let class_names: HashSet<Symbol> =
            objc_classes.iter().map(|c| c.name).collect();

        for c in objc_classes {
            let ctx = ObjcCtx {
                tag: &tag,
                sel_struct: sel_struct_name,
                sel_register: sel_register_name,
                class_struct: class_struct_name,
                get_class: get_class_name,
                object_struct: object_struct_name,
                class_names: &class_names,
            };
            let (class_item, aliases) = build_objc_class(c, &ctx);
            items.push(class_item);
            items.extend(aliases);
        }

        Ok(ilang_ast::ExternCBlock { items: items.into(), span: block_span })
    }

    fn parse_objc_method(
        &mut self,
        selector: String,
        is_pub: bool,
        is_static: bool,
    ) -> Result<ObjcMethod, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Fn, "'fn'")?;
        let name = self.expect_ident("function name")?;
        self.expect(&TokenKind::LParen, "'('")?;
        let params = self.parse_param_list()?;
        self.expect(&TokenKind::RParen, "')'")?;
        let ret = if matches!(self.peek().kind, TokenKind::Colon) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
        self.consume_stmt_terminator()?;
        // Free fns need a receiver as first arg; class methods get
        // one synthesised from `this` (or no receiver for static).
        if !is_static && params.is_empty() {
            // Class method without static + zero params: still
            // legal (e.g., a no-arg instance method). The class
            // method desugar inserts `this` as the receiver.
        }
        Ok(ObjcMethod {
            name,
            selector,
            params: params.into(),
            ret,
            span,
            is_pub,
            is_static,
        })
    }

    fn parse_objc_class_decl(&mut self, is_pub: bool) -> Result<ObjcClass, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Class, "'class'")?;
        let name = self.expect_ident("class name")?;
        self.expect(&TokenKind::LBrace, "'{'")?;
        let mut methods: Vec<ObjcMethod> = Vec::new();
        loop {
            if matches!(self.peek().kind, TokenKind::RBrace) {
                break;
            }
            let attrs = self.parse_attributes()?;
            let method_is_pub = if matches!(self.peek().kind, TokenKind::Pub) {
                self.bump();
                true
            } else {
                // Default class methods to `pub` so the desugared
                // ilang class methods can be called from outside
                // the block without the user having to spell `pub`
                // on every line.
                true
            };
            // Every method must carry an @objc("selector:") attr.
            let objc_pos = attrs
                .iter()
                .position(|a| a.name.as_str() == "objc")
                .ok_or_else(|| ParseError::Unexpected {
                    found: self.peek().kind.clone(),
                    expected: "every method inside @objc class needs @objc(\"selector:\")".into(),
                    span: self.peek().span,
                })?;
            if attrs.len() != 1 {
                let t = self.peek();
                return Err(ParseError::Unexpected {
                    found: t.kind.clone(),
                    expected: "@objc(...) cannot be combined with other attributes on a method".into(),
                    span: t.span,
                });
            }
            let selector = match &attrs[objc_pos].args[..] {
                [AttrArg::Str(s)] => s.clone(),
                _ => {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected: "@objc(\"selector:\") takes exactly one string argument".into(),
                        span: t.span,
                    });
                }
            };
            // Optional `static` modifier before `fn`.
            let is_static = matches!(&self.peek().kind, TokenKind::Ident(n) if n.as_str() == "static");
            if is_static {
                self.bump();
            }
            let m = self.parse_objc_method(selector, method_is_pub, is_static)?;
            methods.push(m);
        }
        self.expect(&TokenKind::RBrace, "'}'")?;
        Ok(ObjcClass {
            name,
            is_pub,
            methods,
            span,
        })
    }

    /// Catch-all for non-`@objc` items inside an `@extern(ObjC)`
    /// block — forwards to the regular `@extern(C)` parsers.
    fn parse_extern_c_item_for_objc_block(
        &mut self,
        inner_attrs: Vec<Attribute>,
        item_is_pub: bool,
    ) -> Result<ilang_ast::ExternCItem, ParseError> {
        match &self.peek().kind {
            TokenKind::Fn => {
                let mut it = self.parse_extern_c_fn(inner_attrs)?;
                match &mut it {
                    ilang_ast::ExternCItem::FnDecl { is_pub, .. } => *is_pub = item_is_pub,
                    ilang_ast::ExternCItem::FnDef(f) => f.is_pub = item_is_pub,
                    _ => {}
                }
                Ok(it)
            }
            TokenKind::Ident(n) if n.as_str() == "struct" => {
                let mut it = self.parse_struct_decl(inner_attrs, false)?;
                if let ilang_ast::ExternCItem::Struct { is_pub, .. } = &mut it {
                    *is_pub = item_is_pub;
                }
                Ok(it)
            }
            TokenKind::Ident(n) if n.as_str() == "union" => {
                if !inner_attrs.is_empty() {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected: "no attributes are supported on `union` inside @extern(ObjC)"
                            .into(),
                        span: t.span,
                    });
                }
                let mut it = self.parse_union_decl(false)?;
                if let ilang_ast::ExternCItem::Union { is_pub, .. } = &mut it {
                    *is_pub = item_is_pub;
                }
                Ok(it)
            }
            _ => {
                let t = self.peek();
                Err(ParseError::Unexpected {
                    found: t.kind.clone(),
                    expected: "@objc(\"...\") fn, @objc class, fn, struct, or union inside @extern(ObjC) block".into(),
                    span: t.span,
                })
            }
        }
    }
}

struct ObjcMethod {
    name: Symbol,
    selector: String,
    params: Box<[Param]>,
    ret: Option<Type>,
    span: Span,
    is_pub: bool,
    is_static: bool,
}

struct ObjcClass {
    name: Symbol,
    is_pub: bool,
    methods: Vec<ObjcMethod>,
    span: Span,
}

struct ObjcCtx<'a> {
    tag: &'a str,
    sel_struct: Symbol,
    sel_register: Symbol,
    class_struct: Symbol,
    get_class: Symbol,
    object_struct: Symbol,
    class_names: &'a HashSet<Symbol>,
}

// ─── Builders ─────────────────────────────────────────────────────

/// Construct a top-level @objc fn's alias + wrapper FnDef.
fn build_freefn_dispatch(
    m: &ObjcMethod,
    tag: &str,
    sel_struct: Symbol,
    sel_register: Symbol,
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
        params: alias_params.into(),
        ret: m.ret.clone(),
        libs: Box::new([Symbol::intern("objc")]),
        optional: false,
        c_symbol: Some(Symbol::intern("objc_msgSend")),
        variadic: false,
        span: m.span,
    };

    // body: alias(receiver, sel_register(cstrFromString("sel")), args)
    let receiver_var = Expr::new(ExprKind::Var(m.params[0].name), m.span);
    let sel_call = build_sel_register_call(&m.selector, sel_register, m.span);
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
            name: Symbol::intern("__objc_wrapper"),
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
    });
    (alias, wrapper)
}

/// Build an @objc class into a desugared ilang ClassDecl plus the
/// `objc_msgSend` aliases its methods need.
fn build_objc_class(
    c: ObjcClass,
    ctx: &ObjcCtx<'_>,
) -> (ilang_ast::ExternCItem, Vec<ilang_ast::ExternCItem>) {
    let class_name = c.name;
    let span = c.span;

    // `pub handle: i64` field carries the underlying ObjC `id`.
    let handle_field = FieldDecl {
        is_pub: true,
        name: Symbol::intern("handle"),
        ty: Type::I64,
        span,
        bits: None,
    };

    // `pub init(h: i64) { this.handle = h }` — wraps a raw id.
    let init_param = Param {
        name: Symbol::intern("h"),
        ty: Type::I64,
        span,
        default: None,
    };
    let init_body_assign = Expr::new(
        ExprKind::AssignField {
            obj: Box::new(Expr::new(ExprKind::This, span)),
            field: Symbol::intern("handle"),
            value: Box::new(Expr::new(ExprKind::Var(Symbol::intern("h")), span)),
            is_init: true,
        },
        span,
    );
    let init_fn = FnDecl {
        is_pub: true,
        attrs: Box::new([]),
        name: Symbol::intern("init"),
        type_params: Box::new([]),
        params: Box::new([init_param]),
        ret: None,
        body: Block {
            stmts: vec![Stmt::new(StmtKind::Expr(init_body_assign), span)],
            tail: None,
        },
        span,
        is_override: false,
        is_async: false,
    };

    let mut methods: Vec<FnDecl> = vec![init_fn];
    let mut static_methods: Vec<FnDecl> = Vec::new();
    let mut aliases: Vec<ilang_ast::ExternCItem> = Vec::new();

    for m in &c.methods {
        let alias_name: Symbol =
            format!("{}_msg_{}_{}", ctx.tag, class_name.as_str(), m.name.as_str()).into();
        let (alias_decl, method_fn) = build_class_method(class_name, m, alias_name, ctx);
        aliases.push(alias_decl);
        if m.is_static {
            static_methods.push(method_fn);
        } else {
            methods.push(method_fn);
        }
    }

    let class_decl = ClassDecl {
        extern_lib: None,
        is_repr_c: false,
        is_packed: false,
        is_union: false,
        is_pub: c.is_pub,
        name: class_name,
        parent: None,
        interfaces: Box::new([]),
        type_params: Box::new([]),
        fields: Box::new([handle_field]),
        methods: methods.into(),
        static_methods: static_methods.into(),
        static_fields: Box::new([]),
        properties: Box::new([]),
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
        params: alias_params.into(),
        ret: alias_ret,
        libs: Box::new([Symbol::intern("objc")]),
        optional: false,
        c_symbol: Some(Symbol::intern("objc_msgSend")),
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
            value: build_sel_register_call(&m.selector, ctx.sel_register, m.span),
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
    let tail = match &m.ret {
        Some(t) if is_objc_class_ty(t, ctx.class_names) => {
            // new ReturnClass(call_expr as i64)
            let raw_as_i64 = Expr::new(
                ExprKind::Cast {
                    expr: Box::new(call_expr),
                    ty: Type::I64,
                },
                m.span,
            );
            let new_expr = Expr::new(
                ExprKind::New {
                    class: ret_class_symbol(t),
                    type_args: Box::new([]),
                    args: Box::new([raw_as_i64]),
                    init_method: None,
                },
                m.span,
            );
            Some(Box::new(new_expr))
        }
        Some(_) => Some(Box::new(call_expr)),
        None => {
            // Statement-level call, no tail.
            stmts.push(Stmt::new(StmtKind::Expr(call_expr), m.span));
            None
        }
    };

    let method_fn = FnDecl {
        is_pub: m.is_pub,
        attrs: Box::new([]),
        name: m.name,
        type_params: Box::new([]),
        params: m.params.clone(),
        ret: m.ret.clone(),
        body: Block { stmts, tail },
        span: m.span,
        is_override: false,
        is_async: false,
    };
    (alias_decl, method_fn)
}

fn is_objc_class_ty(t: &Type, class_names: &HashSet<Symbol>) -> bool {
    match t {
        Type::Object(name) => class_names.contains(name),
        _ => false,
    }
}

fn ret_class_symbol(t: &Type) -> Symbol {
    match t {
        Type::Object(n) => *n,
        _ => unreachable!("caller checks is_objc_class_ty first"),
    }
}

fn build_cstr(s: &str, span: Span) -> Expr {
    Expr::new(
        ExprKind::Call {
            callee: Symbol::intern("cstrFromString"),
            args: Box::new([Expr::new(ExprKind::Str(s.to_string()), span)]),
        },
        span,
    )
}

fn build_sel_register_call(selector: &str, sel_register: Symbol, span: Span) -> Expr {
    Expr::new(
        ExprKind::Call {
            callee: sel_register,
            args: Box::new([build_cstr(selector, span)]),
        },
        span,
    )
}

