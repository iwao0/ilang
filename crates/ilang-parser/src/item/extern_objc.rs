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
            let explicit_pub = matches!(self.peek().kind, TokenKind::Pub);
            if explicit_pub {
                self.bump();
            }

            // Look for @objc(...). Two shapes:
            //   @objc                 → followed by `class Name { ... }`
            //   @objc("selector:")    → followed by `fn name(...)`
            // Visibility follows ilang's normal rule — `pub` is
            // required for cross-module access, no default-pub for
            // @objc items.
            let objc_attr_pos = inner_attrs.iter().position(|a| a.name.as_str() == "objc");
            let item_is_pub = explicit_pub;
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
                    let m = self.parse_objc_method(
                        selector,
                        item_is_pub,
                        /*is_static*/ false,
                        /*require_fn_kw*/ true,
                        /*extra_attrs*/ Vec::new(),
                    )?;
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
        // ilang-defined subclasses (parent set) need the ObjC
        // class-registration helpers — objc_allocateClassPair /
        // objc_registerClassPair — plus objc_getClass for the
        // idempotency check inside `register()`.
        // "Real" subclass = has a parent AND adds at least one
        // method with a body. Plain `: Parent` inheritance with
        // no bodies is just an ilang type-system relationship
        // (no ObjC-runtime registration / IMPs needed); skip the
        // libobjc class-helper extern decls and the per-class
        // `register()` static for those.
        let any_subclass = objc_classes
            .iter()
            .any(|c| c.parent.is_some() && c.methods.iter().any(|m| m.body.is_some()));
        let allocate_pair_name: Symbol = format!("{tag}_allocate_class_pair").into();
        let register_pair_name: Symbol = format!("{tag}_register_class_pair").into();
        let class_add_method_name: Symbol = format!("{tag}_class_add_method").into();
        let dlsym_name: Symbol = format!("{tag}_dlsym").into();
        let retain_name: Symbol = format!("{tag}_objc_retain").into();
        let release_name: Symbol = format!("{tag}_objc_release").into();

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
            // Retain / release helpers — used by the auto-generated
            // deinit on root @objc classes and by the dispatch
            // wrappers' retain-on-autoreleased-return rule.
            if any_class {
                items.push(ilang_ast::ExternCItem::FnDecl {
                    is_pub: false,
                    name: retain_name,
                    params: Box::new([Param {
                        name: Symbol::intern("obj"),
                        ty: Type::RawPtr {
                            is_const: false,
                            inner: Box::new(Type::Object(object_struct_name)),
                        },
                        span: block_span,
                        default: None,
                    }]),
                    ret: Some(Type::RawPtr {
                        is_const: false,
                        inner: Box::new(Type::Object(object_struct_name)),
                    }),
                    libs: Box::new([Symbol::intern("objc")]),
                    optional: false,
                    c_symbol: Some(Symbol::intern("objc_retain")),
                    variadic: false,
                    span: block_span,
                });
                items.push(ilang_ast::ExternCItem::FnDecl {
                    is_pub: false,
                    name: release_name,
                    params: Box::new([Param {
                        name: Symbol::intern("obj"),
                        ty: Type::RawPtr {
                            is_const: false,
                            inner: Box::new(Type::Object(object_struct_name)),
                        },
                        span: block_span,
                        default: None,
                    }]),
                    ret: None,
                    libs: Box::new([Symbol::intern("objc")]),
                    optional: false,
                    c_symbol: Some(Symbol::intern("objc_release")),
                    variadic: false,
                    span: block_span,
                });
            }
            // Class lookup helpers are only injected when at least
            // one class uses a static method (only static dispatch
            // needs `objc_getClass`). Subclass registration also
            // requires objc_getClass for the idempotency check
            // (avoid re-registering on second call).
            if any_static || any_subclass {
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
            // libobjc class-registration helpers — only needed
            // when at least one declared @objc class is an
            // ilang-defined subclass (has a parent set).
            if any_subclass {
                items.push(ilang_ast::ExternCItem::FnDecl {
                    is_pub: false,
                    name: allocate_pair_name,
                    params: Box::new([
                        Param {
                            name: Symbol::intern("parent"),
                            ty: Type::RawPtr {
                                is_const: false,
                                inner: Box::new(Type::Object(class_struct_name)),
                            },
                            span: block_span,
                            default: None,
                        },
                        Param {
                            name: Symbol::intern("name"),
                            ty: Type::RawPtr {
                                is_const: true,
                                inner: Box::new(Type::CChar),
                            },
                            span: block_span,
                            default: None,
                        },
                        Param {
                            name: Symbol::intern("extra_bytes"),
                            ty: Type::Size,
                            span: block_span,
                            default: None,
                        },
                    ]),
                    ret: Some(Type::RawPtr {
                        is_const: false,
                        inner: Box::new(Type::Object(class_struct_name)),
                    }),
                    libs: Box::new([Symbol::intern("objc")]),
                    optional: false,
                    c_symbol: Some(Symbol::intern("objc_allocateClassPair")),
                    variadic: false,
                    span: block_span,
                });
                items.push(ilang_ast::ExternCItem::FnDecl {
                    is_pub: false,
                    name: register_pair_name,
                    params: Box::new([Param {
                        name: Symbol::intern("cls"),
                        ty: Type::RawPtr {
                            is_const: false,
                            inner: Box::new(Type::Object(class_struct_name)),
                        },
                        span: block_span,
                        default: None,
                    }]),
                    ret: None,
                    libs: Box::new([Symbol::intern("objc")]),
                    optional: false,
                    c_symbol: Some(Symbol::intern("objc_registerClassPair")),
                    variadic: false,
                    span: block_span,
                });
                // `class_addMethod(cls, sel, imp, type_encoding)`.
                items.push(ilang_ast::ExternCItem::FnDecl {
                    is_pub: false,
                    name: class_add_method_name,
                    params: Box::new([
                        Param {
                            name: Symbol::intern("cls"),
                            ty: Type::RawPtr {
                                is_const: false,
                                inner: Box::new(Type::Object(class_struct_name)),
                            },
                            span: block_span,
                            default: None,
                        },
                        Param {
                            name: Symbol::intern("sel"),
                            ty: Type::RawPtr {
                                is_const: false,
                                inner: Box::new(Type::Object(sel_struct_name)),
                            },
                            span: block_span,
                            default: None,
                        },
                        Param {
                            name: Symbol::intern("imp"),
                            ty: Type::RawPtr {
                                is_const: false,
                                inner: Box::new(Type::CVoid),
                            },
                            span: block_span,
                            default: None,
                        },
                        Param {
                            name: Symbol::intern("types"),
                            ty: Type::RawPtr {
                                is_const: true,
                                inner: Box::new(Type::CChar),
                            },
                            span: block_span,
                            default: None,
                        },
                    ]),
                    ret: Some(Type::I8),
                    libs: Box::new([Symbol::intern("objc")]),
                    optional: false,
                    c_symbol: Some(Symbol::intern("class_addMethod")),
                    variadic: false,
                    span: block_span,
                });
                // IMP address lookup. AOT links our subclass IMPs
                // with `Linkage::Export` so `dlsym(RTLD_DEFAULT)`
                // would find them; the JIT can't be reached the
                // same way, so we go through an ilang-runtime
                // helper (`__ilang_objc_imp_lookup`) that checks a
                // JIT-populated table first and falls back to
                // dlsym for the AOT path. The first `handle`
                // argument is kept to preserve the dlsym call
                // shape but is ignored by the helper.
                items.push(ilang_ast::ExternCItem::FnDecl {
                    is_pub: false,
                    name: dlsym_name,
                    params: Box::new([
                        Param {
                            name: Symbol::intern("handle"),
                            ty: Type::RawPtr {
                                is_const: false,
                                inner: Box::new(Type::CVoid),
                            },
                            span: block_span,
                            default: None,
                        },
                        Param {
                            name: Symbol::intern("name"),
                            ty: Type::RawPtr {
                                is_const: true,
                                inner: Box::new(Type::CChar),
                            },
                            span: block_span,
                            default: None,
                        },
                    ]),
                    ret: Some(Type::RawPtr {
                        is_const: false,
                        inner: Box::new(Type::CVoid),
                    }),
                    libs: Box::new([Symbol::intern("c")]),
                    optional: false,
                    c_symbol: Some(Symbol::intern("__ilang_objc_imp_lookup")),
                    variadic: false,
                    span: block_span,
                });
            }
        }

        // Top-level @objc fns — same expansion as before.
        for m in objc_fns {
            let (alias, wrapper) =
                build_freefn_dispatch(&m, &tag, sel_struct_name, sel_register_name);
            items.push(alias);
            items.push(wrapper);
        }

        // Names of @objc classes the method-body desugar should
        // treat as "wrapped" — those whose arg/return slots need
        // `.handle` extraction and result re-wrapping. Includes:
        //   1. Classes declared in this block.
        //   2. `@objc class` names imported from already-loaded
        //      dependency modules (populated by the loader). This is
        //      what lets `NSWindow.setTitle(t: NSString)` in
        //      `appkit.il` correctly unwrap a `foundation.NSString`
        //      argument — without (2), the desugar would pass the
        //      ilang wrapper pointer to `objc_msgSend` and crash.
        let class_names: HashSet<Symbol> = objc_classes
            .iter()
            .map(|c| c.name)
            .chain(self.external_objc_classes.iter().copied())
            .collect();

        for c in objc_classes {
            let ctx = ObjcCtx {
                tag: &tag,
                sel_struct: sel_struct_name,
                sel_register: sel_register_name,
                class_struct: class_struct_name,
                get_class: get_class_name,
                object_struct: object_struct_name,
                allocate_pair: allocate_pair_name,
                register_pair: register_pair_name,
                class_add_method: class_add_method_name,
                dlsym: dlsym_name,
                retain: retain_name,
                release: release_name,
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
        require_fn_kw: bool,
        extra_attrs: Vec<Attribute>,
    ) -> Result<ObjcMethod, ParseError> {
        let span = self.peek().span;
        if require_fn_kw {
            self.expect(&TokenKind::Fn, "'fn'")?;
        }
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
        // `{ body }` makes this an ilang-defined IMP (subclass
        // override). Without a body it's a binding-only declaration
        // — the desugar generates an objc_msgSend wrapper but no
        // IMP, matching the (iii) behaviour.
        let body = if matches!(self.peek().kind, TokenKind::LBrace) {
            Some(crate::stmt::parse_block(self)?)
        } else {
            self.consume_stmt_terminator()?;
            None
        };
        Ok(ObjcMethod {
            name,
            selector,
            params: params.into(),
            ret,
            body,
            span,
            is_pub,
            is_static,
            extra_attrs,
        })
    }

    fn parse_objc_class_decl(&mut self, is_pub: bool) -> Result<ObjcClass, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Class, "'class'")?;
        let name = self.expect_ident("class name")?;
        // Optional `: Parent` — when present, this becomes an
        // ilang-defined ObjC subclass. The desugar registers the
        // class with libobjc at startup and exposes each ilang
        // method as a C-ABI IMP that `class_addMethod` can install.
        let parent = if matches!(self.peek().kind, TokenKind::Colon) {
            self.bump();
            Some(self.expect_ident("parent class name")?)
        } else {
            None
        };
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
                false
            };
            // Optional `static` modifier before the method name.
            let is_static = matches!(&self.peek().kind, TokenKind::Ident(n) if n.as_str() == "static");
            if is_static {
                self.bump();
            }
            // `@objc("selector:")` → ObjC dispatch wrapper.
            // No attribute → plain ilang method living inside the
            // @objc class. Useful for static helpers like
            // `pub static wrap(h: i64): Self { __wrap_handle(h) }`
            // that bridge the desugar's internal `__wrap_handle`
            // out to a friendly user-facing name.
            let objc_pos = attrs.iter().position(|a| a.name.as_str() == "objc");
            // Sibling attributes the desugar passes through to the
            // synthesised FnDecl. Currently:
            //   * `@deprecated("reason")` — type-checker warning at
            //     call site.
            // Anything else is rejected to keep typos / unsupported
            // attributes from silently disappearing.
            let split_extra = |attrs: Vec<Attribute>,
                               skip_pos: Option<usize>,
                               err_span: Span|
             -> Result<Vec<Attribute>, ParseError> {
                let mut extra: Vec<Attribute> = Vec::new();
                for (i, a) in attrs.into_iter().enumerate() {
                    if Some(i) == skip_pos {
                        continue;
                    }
                    match a.name.as_str() {
                        "deprecated" => extra.push(a),
                        other => {
                            return Err(ParseError::Unexpected {
                                found: TokenKind::At,
                                expected: format!(
                                    "unsupported attribute `@{other}` on @objc class method (allowed: @objc, @deprecated)"
                                ),
                                span: err_span,
                            });
                        }
                    }
                }
                Ok(extra)
            };
            if let Some(pos) = objc_pos {
                let selector = match &attrs[pos].args[..] {
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
                let err_span = self.peek().span;
                let extra = split_extra(attrs, Some(pos), err_span)?;
                let m = self.parse_objc_method(
                    selector,
                    method_is_pub,
                    is_static,
                    /*require_fn_kw*/ false,
                    extra,
                )?;
                methods.push(m);
            } else {
                let err_span = self.peek().span;
                let extra = split_extra(attrs, None, err_span)?;
                // Plain method: same shape as `parse_objc_method`
                // but flagged so the caller knows to skip the
                // ObjC dispatch wrapper.
                let m = self.parse_objc_method(
                    String::new(),
                    method_is_pub,
                    is_static,
                    /*require_fn_kw*/ false,
                    extra,
                )?;
                let mut plain = m;
                plain.selector = String::new();
                methods.push(plain);
            }
        }
        self.expect(&TokenKind::RBrace, "'}'")?;
        Ok(ObjcClass {
            name,
            is_pub,
            parent,
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
    /// `Some(block)` when the user wrote `{ ... }` after the
    /// signature — the body becomes the ilang-side IMP for an
    /// `@objc class : Parent` subclass override. `None` for plain
    /// declarations that just bind an existing ObjC method (the
    /// (iii) wrapper does the dispatch).
    body: Option<Block>,
    span: Span,
    is_pub: bool,
    is_static: bool,
    /// User-supplied attributes other than `@objc(...)` —
    /// currently just `@deprecated("reason")` for ObjC-side
    /// soft-removal markers. Propagated onto the synthesised
    /// dispatch wrapper so the type checker can warn at call
    /// sites.
    extra_attrs: Vec<Attribute>,
}

struct ObjcClass {
    name: Symbol,
    is_pub: bool,
    /// `Some(parent)` for ilang-defined subclasses (`@objc class Foo : NSObject`).
    /// `None` for plain bindings to existing ObjC classes.
    parent: Option<Symbol>,
    methods: Vec<ObjcMethod>,
    span: Span,
}

/// Apple ARC's NS_RETURNS_RETAINED family rule. The selector's
/// first word (lowercase letters until an uppercase letter, `:`,
/// or end) names the family — `alloc`, `new`, `copy`,
/// `mutableCopy`, `init`, and `retain` return +1. Everything else
/// is autoreleased.
fn returns_retained_selector(selector: &str) -> bool {
    for family in &["alloc", "new", "copy", "mutableCopy", "init", "retain"] {
        if let Some(rest) = selector.strip_prefix(family) {
            let first = rest.chars().next();
            match first {
                None | Some(':') => return true,
                Some(c) if !c.is_lowercase() => return true,
                _ => {}
            }
        }
    }
    false
}

/// Body:
///   if this.__owns != 0 && this.handle != 0 {
///       <release>(this.handle as *objc_object)
///   }
fn build_root_deinit(ctx: &ObjcCtx<'_>, span: Span) -> FnDecl {
    let this_owns = Expr::new(
        ExprKind::Field {
            obj: Box::new(Expr::new(ExprKind::This, span)),
            name: Symbol::intern("__owns"),
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
            name: Symbol::intern("__objc_wrapper"),
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
    }
}

struct ObjcCtx<'a> {
    tag: &'a str,
    sel_struct: Symbol,
    sel_register: Symbol,
    class_struct: Symbol,
    get_class: Symbol,
    object_struct: Symbol,
    allocate_pair: Symbol,
    register_pair: Symbol,
    class_add_method: Symbol,
    dlsym: Symbol,
    retain: Symbol,
    release: Symbol,
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
                name: Symbol::intern("__owns"),
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
                field: Symbol::intern("__owns"),
                value: Box::new(Expr::new(ExprKind::Int(val), span)),
                is_init: true,
            },
            span,
        )
    };
    let init_fn = FnDecl {
        is_pub: true,
        attrs: Box::new([]),
        name: Symbol::intern("__bind_handle"),
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
    };
    let unowned_init_fn = FnDecl {
        is_pub: true,
        attrs: Box::new([]),
        name: Symbol::intern("__bind_handle_unowned"),
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
            tail: Some(Box::new(wrap_body_new("__bind_handle"))),
        },
        span,
        is_override: false,
        is_async: false,
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
            tail: Some(Box::new(wrap_body_new("__bind_handle_unowned"))),
        },
        span,
        is_override: false,
        is_async: false,
    };
    let mut static_methods: Vec<FnDecl> = vec![wrap_fn, wrap_unowned_fn];
    let mut aliases: Vec<ilang_ast::ExternCItem> = Vec::new();
    // Collect bodied methods so the `register()` builder can emit
    // a `class_addMethod` call per IMP. Stored as
    // (method_name, selector, type_encoding, imp_symbol).
    let mut imps_to_attach: Vec<ImpEntry> = Vec::new();

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
            };
            if m.is_static {
                static_methods.push(plain_fn);
            } else {
                methods.push(plain_fn);
            }
            continue;
        }

        let alias_name: Symbol =
            format!("{}_msg_{}_{}", ctx.tag, class_name.as_str(), m.name.as_str()).into();
        let (alias_decl, method_fn) = build_class_method(class_name, m, alias_name, ctx);
        aliases.push(alias_decl);
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
            };
            if m.is_static {
                static_methods.push(impl_fn);
            } else {
                methods.push(impl_fn);
            }

            let imp_symbol: Symbol =
                format!("ilang_objc_imp__{}__{}", class_name.as_str(), m.name.as_str()).into();
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
        is_union: false,
        is_pub: c.is_pub,
        name: class_name,
        // ilang-side inheritance mirrors the declared ObjC parent.
        // Methods declared on the parent (e.g. `hash`, `release`)
        // become callable on the child instance, dispatched as
        // normal ilang method calls through the desugared parent
        // class (which itself does objc_msgSend).
        parent: c.parent,
        interfaces: Box::new([]),
        type_params: Box::new([]),
        fields: fields.into_boxed_slice(),
        methods: methods.into(),
        static_methods: static_methods.into(),
        static_fields: Box::new([]),
        properties: Box::new([]),
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
                        init_method: Some(Symbol::intern("__bind_handle")),
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
            name: Symbol::intern("__objc_wrapper"),
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
        is_override: false,
        is_async: false,
    };
    (alias_decl, method_fn)
}

/// Generate a `__super_<method>(args)` helper method + the
/// `objc_msgSendSuper` alias it forwards to. The helper builds a
/// 2-slot scratch (`receiver=this.handle`, `super_cls=
/// objc_getClass(parent_name)`) on the heap (`i64[]` literal — its
/// data pointer at the FFI boundary matches the `struct objc_super`
/// layout exactly) and calls the alias with that pointer, the
/// selector, and the user's args.
fn build_super_helper(
    class_name: Symbol,
    parent_name: Symbol,
    m: &ObjcMethod,
    ctx: &ObjcCtx<'_>,
) -> (FnDecl, ilang_ast::ExternCItem) {
    let helper_name: Symbol = format!("__super_{}", m.name.as_str()).into();
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
        params: alias_params.into(),
        ret: alias_ret,
        libs: Box::new([Symbol::intern("objc")]),
        optional: false,
        c_symbol: Some(Symbol::intern("objc_msgSendSuper")),
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
    let sel_call = build_sel_register_call(&m.selector, ctx.sel_register, span);

    let mut call_args: Vec<Expr> = Vec::with_capacity(m.params.len() + 2);
    // First arg: data pointer of `__sup` array. ilang auto-decays
    // `i64[]` to its data pointer at the FFI boundary — but the
    // alias expects `*i64`, which is exactly that decay.
    call_args.push(Expr::new(ExprKind::Var(sup_name), span));
    call_args.push(sel_call);
    for p in m.params.iter() {
        if is_objc_class_ty(&p.ty, ctx.class_names) {
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
                    init_method: Some(Symbol::intern("__bind_handle")),
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
    };
    (helper_fn, alias_decl)
}

/// Walk every method body in `methods` and replace any
/// `SuperCall { method: Some(name), args }` with a method call
/// `this.__super_<name>(args)` so the generated helper carries
/// out the actual `objc_msgSendSuper` dispatch.
fn rewrite_super_in_methods(methods: &mut [FnDecl]) {
    for m in methods.iter_mut() {
        // Skip generated helpers / register / impl methods that
        // we ourselves emitted — they don't contain user `super`
        // calls (and self-rewriting would infinite-loop).
        let name = m.name.as_str();
        if name.starts_with("__super_") || name == "register" || name == "init" {
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
    // Recurse first so nested super calls inside args are
    // rewritten too.
    match &mut expr.kind {
        ExprKind::SuperCall { method: Some(name), args } => {
            for a in args.iter_mut() {
                rewrite_super_in_expr(a);
            }
            let helper_name: Symbol = format!("__super_{}", name.as_str()).into();
            let new_expr = Expr::new(
                ExprKind::MethodCall {
                    obj: Box::new(Expr::new(ExprKind::This, expr.span)),
                    method: helper_name,
                    args: std::mem::take(args),
                },
                expr.span,
            );
            *expr = new_expr;
        }
        ExprKind::Unary { expr: inner, .. } | ExprKind::Cast { expr: inner, .. } => {
            rewrite_super_in_expr(inner);
        }
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            rewrite_super_in_expr(lhs);
            rewrite_super_in_expr(rhs);
        }
        ExprKind::Call { args, .. } => {
            for a in args.iter_mut() {
                rewrite_super_in_expr(a);
            }
        }
        ExprKind::MethodCall { obj, args, .. } => {
            rewrite_super_in_expr(obj);
            for a in args.iter_mut() {
                rewrite_super_in_expr(a);
            }
        }
        ExprKind::Field { obj, .. } => rewrite_super_in_expr(obj),
        ExprKind::Block(b) => rewrite_super_in_block(b),
        ExprKind::If { cond, then_branch, else_branch } => {
            rewrite_super_in_expr(cond);
            rewrite_super_in_block(then_branch);
            if let Some(e) = else_branch.as_mut() {
                rewrite_super_in_expr(e);
            }
        }
        ExprKind::While { cond, body } => {
            rewrite_super_in_expr(cond);
            rewrite_super_in_block(body);
        }
        ExprKind::ForIn { iter, body, .. } => {
            rewrite_super_in_expr(iter);
            rewrite_super_in_block(body);
        }
        ExprKind::Loop { body } => rewrite_super_in_block(body),
        ExprKind::Return(opt) | ExprKind::Break(opt) => {
            if let Some(e) = opt.as_mut() {
                rewrite_super_in_expr(e);
            }
        }
        ExprKind::Array(items) | ExprKind::Tuple(items) => {
            for it in items.iter_mut() {
                rewrite_super_in_expr(it);
            }
        }
        ExprKind::Assign { value, .. } => rewrite_super_in_expr(value),
        ExprKind::AssignField { obj, value, .. } => {
            rewrite_super_in_expr(obj);
            rewrite_super_in_expr(value);
        }
        ExprKind::AssignIndex { obj, index, value } => {
            rewrite_super_in_expr(obj);
            rewrite_super_in_expr(index);
            rewrite_super_in_expr(value);
        }
        ExprKind::Index { obj, index } => {
            rewrite_super_in_expr(obj);
            rewrite_super_in_expr(index);
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start.as_mut() {
                rewrite_super_in_expr(s);
            }
            if let Some(e) = end.as_mut() {
                rewrite_super_in_expr(e);
            }
        }
        _ => {} // leaves and unhandled exotic cases
    }
}

/// Records what the `register()` method needs to know about each
/// bodied @objc method so it can attach the corresponding IMP via
/// `class_addMethod`. The IMP itself is generated as a separate
/// C-ABI function (see `build_imp_fn`) and resolved at runtime
/// through `dlsym`.
struct ImpEntry {
    selector: String,
    encoding: String,
    imp_symbol: Symbol,
}

/// Encode an ilang method signature in the Objective-C type
/// encoding format. `<ret>@:<arg1><arg2>...` where `@` is the
/// receiver (id) and `:` is the implicit `_cmd` selector. The
/// encoding covers the primitive types we currently marshal; any
/// type outside this set falls back to `?` so unhandled cases
/// surface as a runtime-side message-signature mismatch rather
/// than silently corrupting the dispatch.
fn encode_method_signature(
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
fn build_imp_fn(
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
            init_method: Some(Symbol::intern("__bind_handle_unowned")),
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
                    init_method: Some(Symbol::intern("__bind_handle_unowned")),
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
            name: Symbol::intern("__objc_wrapper"),
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
    };
    ilang_ast::ExternCItem::FnDef(fndecl)
}

/// Build a `pub static register()` method that registers the
/// subclass with the ObjC runtime on first call and attaches
/// every IMP via `class_addMethod`. Idempotent through the
/// `objc_getClass` probe at the top.
fn build_register_class_fn(
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

    let get_parent = Expr::new(
        ExprKind::Call {
            callee: ctx.get_class,
            args: Box::new([build_cstr(parent_name.as_str(), span)]),
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
        let sel_call = build_sel_register_call(&imp.selector, ctx.sel_register, span);
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
    }
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

