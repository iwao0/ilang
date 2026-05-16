//! `@extern(ObjC) { ... }` block parsing.
//!
//! Each `@objc("selector:") fn name(receiver, args): ret` declared
//! inside the block desugars at parse time into three plain
//! `@extern(C)` items:
//!
//!   1. An opaque `__objc_b<N>_sel_t` struct (synthesised once per
//!      block) — the typed handle for an Objective-C selector.
//!   2. An alias of `objc_msgSend` with this fn's exact signature
//!      *plus* a `sel: *__objc_b<N>_sel_t` second parameter. Driven
//!      by the L1 alias path so multiple shapes all share the C
//!      `objc_msgSend` symbol.
//!   3. The user-visible wrapper `fn <name>(receiver, args): ret`
//!      whose body interns the selector via `sel_registerName` and
//!      forwards the call.
//!
//! The block's byte offset is woven into every synthesised name so
//! multiple `@extern(ObjC)` blocks in the same file can coexist
//! without colliding.

use ilang_ast::{AttrArg, Attribute, Block, Expr, ExprKind, FnDecl, Param, Span, Symbol, Type};
use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;

impl<'a> Parser<'a> {
    pub(super) fn parse_extern_objc_block(
        &mut self,
    ) -> Result<ilang_ast::ExternCBlock, ParseError> {
        let block_span = self.peek().span;
        self.expect(&TokenKind::LBrace, "'{'")?;
        // Tag generated names with the block's start offset so two
        // ObjC blocks in the same file can both expand cleanly.
        // Lines/cols make a unique-ish tag without depending on a
        // raw byte offset (which `Span` doesn't carry).
        let tag = format!("__objc_b{}c{}", block_span.line, block_span.col);
        let sel_struct_name: Symbol = format!("{tag}_sel_t").into();
        let sel_register_name: Symbol = format!("{tag}_sel_register").into();

        let mut items: Vec<ilang_ast::ExternCItem> = Vec::new();
        let mut objc_methods: Vec<ObjcMethod> = Vec::new();

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

            // Pick out an `@objc("...")` attribute if present. If
            // anything else is stacked alongside it, reject up-front
            // — the user can't mix `@lib`/`@symbol` with `@objc`.
            let objc_attr_pos = inner_attrs.iter().position(|a| a.name.as_str() == "objc");
            if let Some(pos) = objc_attr_pos {
                if inner_attrs.len() != 1 {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected: "@objc(\"selector:\") cannot be combined with other attributes on a fn inside @extern(ObjC)".into(),
                        span: t.span,
                    });
                }
                let selector = match &inner_attrs[pos].args[..] {
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
                // Only fn declarations get the @objc treatment.
                if !matches!(self.peek().kind, TokenKind::Fn) {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected: "`fn` after @objc(\"...\")".into(),
                        span: t.span,
                    });
                }
                let m = self.parse_objc_method(selector, item_is_pub)?;
                objc_methods.push(m);
                continue;
            }

            // No @objc — fall back to the regular extern(C) items.
            // Reusing the @extern(C) sub-parsers keeps struct /
            // union / class / non-@objc fn declarations interpretable
            // exactly as if they appeared in an @extern(C) block.
            let item = self.parse_extern_c_item_for_objc_block(inner_attrs, item_is_pub)?;
            items.push(item);
        }
        self.expect(&TokenKind::RBrace, "'}'")?;

        // If any @objc fn was declared, synthesise the per-block
        // setup items (selector type + sel_registerName alias) up
        // front, then emit one alias-decl + one wrapper-def per
        // method. `cstrFromString` is a built-in ilang helper so
        // the wrapper body needs no extra declaration.
        if !objc_methods.is_empty() {
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

            for m in objc_methods {
                let alias_name: Symbol =
                    format!("{tag}_msg_{}", m.name.as_str()).into();
                // Build the alias parameter list: original params
                // with a synthetic `sel: *__objc_b<N>_sel_t` slot
                // inserted right after the receiver.
                let mut alias_params: Vec<Param> = Vec::with_capacity(m.params.len() + 1);
                alias_params.push(m.params[0].clone());
                alias_params.push(Param {
                    name: Symbol::intern("_sel"),
                    ty: Type::RawPtr {
                        is_const: false,
                        inner: Box::new(Type::Object(sel_struct_name)),
                    },
                    span: m.span,
                    default: None,
                });
                for p in &m.params[1..] {
                    alias_params.push(p.clone());
                }
                items.push(ilang_ast::ExternCItem::FnDecl {
                    is_pub: false,
                    name: alias_name,
                    params: alias_params.into(),
                    ret: m.ret.clone(),
                    libs: Box::new([Symbol::intern("objc")]),
                    optional: false,
                    c_symbol: Some(Symbol::intern("objc_msgSend")),
                    variadic: false,
                    span: m.span,
                });

                // Wrapper body: `alias(receiver, sel_register(cstrFromString("sel")), args...)`
                let receiver_var = Expr::new(
                    ExprKind::Var(m.params[0].name),
                    m.span,
                );
                let sel_str = Expr::new(
                    ExprKind::Str(m.selector.clone()),
                    m.span,
                );
                let cstr_call = Expr::new(
                    ExprKind::Call {
                        callee: Symbol::intern("cstrFromString"),
                        args: Box::new([sel_str]),
                    },
                    m.span,
                );
                let sel_call = Expr::new(
                    ExprKind::Call {
                        callee: sel_register_name,
                        args: Box::new([cstr_call]),
                    },
                    m.span,
                );
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
                let body = Block {
                    stmts: Vec::new(),
                    tail: Some(Box::new(body_call)),
                };
                items.push(ilang_ast::ExternCItem::FnDef(FnDecl {
                    is_pub: m.is_pub,
                    // `@__objc_wrapper` flags this FnDef as a
                    // parser-synthesised objc dispatch wrapper. The
                    // type checker skips its raw-pointer-in-signature
                    // rejection so the wrapper can expose `*objc_*`
                    // params/returns just like the source @objc
                    // declaration did.
                    attrs: Box::new([Attribute {
                        name: Symbol::intern("__objc_wrapper"),
                        args: Box::new([]),
                    }]),
                    name: m.name,
                    type_params: Box::new([]),
                    params: m.params,
                    ret: m.ret,
                    body,
                    span: m.span,
                    is_override: false,
                    is_async: false,
                }));
            }
        }

        Ok(ilang_ast::ExternCBlock { items: items.into(), span: block_span })
    }

    /// Parse a `@objc(...) fn` declaration body (signature only —
    /// `@objc` fns never have a user-written body; the wrapper is
    /// synthesised). The receiver is the first parameter and must
    /// be a raw-pointer type (`*objc_object` / `*objc_class` /
    /// any user-declared opaque handle).
    fn parse_objc_method(
        &mut self,
        selector: String,
        is_pub: bool,
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
        if params.is_empty() {
            return Err(ParseError::Unexpected {
                found: TokenKind::RParen,
                expected: "@objc fn must take a receiver (id / Class / ...) as its first parameter".into(),
                span,
            });
        }
        Ok(ObjcMethod {
            name,
            selector,
            params: params.into(),
            ret,
            span,
            is_pub,
        })
    }

    /// Catch-all for non-`@objc` items inside an `@extern(ObjC)`
    /// block — forwards to the regular `@extern(C)` parsers so
    /// users can declare opaque structs (`struct objc_object {}`)
    /// and even ordinary C fns side-by-side with their ObjC ones.
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
                    expected: "@objc(\"...\") fn, fn, struct, or union inside @extern(ObjC) block"
                        .into(),
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
}
