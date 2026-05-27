//! `@extern(ObjC) { … }` block parsing. Tokens come in via the
//! `Parser` impl below; the parsed `ObjcMethod` / `ObjcClass` /
//! `InterfaceDecl` lists are handed to `finalize_objc_block` for
//! the actual desugar into `@extern(C)` items.

use ilang_ast::{AttrArg, Attribute, Span, Symbol};
use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;

use super::finalize_objc_block;
use super::model::{AccessorKind, ObjcClass, ObjcMethod};

impl<'a> Parser<'a> {
    pub(in crate::item) fn parse_extern_objc_block(
        &mut self,
        block_libs: Vec<Symbol>,
    ) -> Result<ilang_ast::ExternCBlock, ParseError> {
        let block_span = self.peek().span;
        self.expect(&TokenKind::LBrace, "'{'")?;
        // The libobjc helper-name setup that the desugar phase needs
        // has moved into `finalize_objc_block` so the auto-lift pass
        // can call it with synthesized inputs.
        let mut items: Vec<ilang_ast::ExternCItem> = Vec::new();
        let mut objc_fns: Vec<ObjcMethod> = Vec::new();
        let mut objc_classes: Vec<ObjcClass> = Vec::new();
        let mut objc_interfaces: Vec<ilang_ast::InterfaceDecl> = Vec::new();

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
                    // @objc class Name { ... } OR @objc interface Name { ... }
                    match self.peek().kind {
                        TokenKind::Class => {
                            let parsed = self.parse_objc_class_decl(item_is_pub)?;
                            objc_classes.push(parsed);
                            continue;
                        }
                        TokenKind::Interface => {
                            let mut iface = self.parse_interface_decl(true)?;
                            iface.is_pub = item_is_pub;
                            iface.is_objc = true;
                            // Auto-derive Objective-C selectors for
                            // methods that didn't get an explicit
                            // `@objc("…")` annotation. The rule is
                            // method name + ':' × paramCount —
                            // matches the common ObjC convention.
                            // Methods with intermediate keywords
                            // (`application:openFile:`) must use
                            // explicit `@objc("…")`.
                            let methods = std::mem::take(&mut iface.methods);
                            iface.methods = methods
                                .into_vec()
                                .into_iter()
                                .map(|mut m| {
                                    if m.objc_selector.is_none() {
                                        let mut s = m.name.as_str().to_string();
                                        for _ in 0..m.params.len() {
                                            s.push(':');
                                        }
                                        m.objc_selector =
                                            Some(Symbol::intern(&s));
                                    }
                                    m
                                })
                                .collect();
                            objc_interfaces.push(iface);
                            continue;
                        }
                        _ => {
                            let t = self.peek();
                            return Err(ParseError::Unexpected {
                                found: t.kind.clone(),
                                expected: "`class` or `interface` after bare @objc (use @objc(\"sel:\") for fns)".into(),
                                span: t.span,
                            });
                        }
                    }
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
                        /*is_override*/ false,
                        /*require_fn_kw*/ true,
                        /*extra_attrs*/ Vec::new(),
                        /*accessor*/ None,
                    )?;
                    objc_fns.push(m);
                    continue;
                }
            }

            // Non-@objc — regular @extern(C) item.
            let item = self.parse_extern_c_item_for_objc_block(
                inner_attrs, item_is_pub, &block_libs,
            )?;
            items.push(item);
        }
        self.expect(&TokenKind::RBrace, "'}'")?;

        Ok(finalize_objc_block(
            items,
            objc_fns,
            objc_classes,
            objc_interfaces,
            block_libs,
            block_span,
            self.external_objc_classes,
        ))
    }

    fn parse_objc_method(
        &mut self,
        selector: String,
        is_pub: bool,
        is_static: bool,
        is_override: bool,
        require_fn_kw: bool,
        extra_attrs: Vec<Attribute>,
        accessor: Option<AccessorKind>,
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
        // Accessor shape validation — getter is `(): T`, setter is
        // `(v: T)` with no return. Bodies aren't allowed; the
        // dispatch is auto-synthesised from the @objc selector.
        if let Some(kind) = accessor {
            if body.is_some() {
                return Err(ParseError::Unexpected {
                    found: TokenKind::LBrace,
                    expected: "property accessor inside @objc class takes no body (the @objc(\"selector\") dispatch is auto-synthesised)".into(),
                    span,
                });
            }
            match kind {
                AccessorKind::Getter => {
                    if !params.is_empty() {
                        return Err(ParseError::Unexpected {
                            found: TokenKind::LParen,
                            expected: "@objc getter takes no parameters".into(),
                            span,
                        });
                    }
                    if ret.is_none() {
                        return Err(ParseError::Unexpected {
                            found: TokenKind::RParen,
                            expected: "@objc getter must declare a return type".into(),
                            span,
                        });
                    }
                }
                AccessorKind::Setter => {
                    if params.len() != 1 {
                        return Err(ParseError::Unexpected {
                            found: TokenKind::LParen,
                            expected: "@objc setter takes exactly one parameter".into(),
                            span,
                        });
                    }
                    if ret.is_some() {
                        return Err(ParseError::Unexpected {
                            found: TokenKind::Colon,
                            expected: "@objc setter must not declare a return type".into(),
                            span,
                        });
                    }
                }
            }
        }
        Ok(ObjcMethod {
            name,
            selector,
            params: params.into(),
            ret,
            body,
            span,
            is_pub,
            is_static,
            is_override,
            extra_attrs,
            accessor,
        })
    }

    fn parse_objc_class_decl(&mut self, is_pub: bool) -> Result<ObjcClass, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Class, "'class'")?;
        let name = self.expect_ident("class name")?;
        // Optional `: Parent[, Interface, …]` — parses an
        // Objective-C-style base list. The first entry (if any)
        // is the parent class for the subclass desugar; the rest
        // are interfaces the class is expected to implement (the
        // standard ilang interface-conformance check applies). For
        // the casual delegate case
        //   @objc class MyApp : NSObject, NSApplicationDelegate { … }
        // the parent class object is `NSObject` and `NSApplicationDelegate`
        // is just a contract.
        let (parent, interfaces) = if matches!(self.peek().kind, TokenKind::Colon) {
            self.bump();
            let first = self.expect_ident("parent class name")?;
            let mut ifaces: Vec<Symbol> = Vec::new();
            while matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
                ifaces.push(self.expect_ident("interface name")?);
            }
            (Some(first), ifaces)
        } else {
            (None, Vec::new())
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
            // Optional `override` keyword — matches the plain ilang
            // class grammar. Required by the type checker when this
            // method shadows an inherited slot from the parent
            // @objc class (NSObject's `hash`, `isEqual:`, etc.).
            let is_override = matches!(self.peek().kind, TokenKind::Override);
            if is_override {
                self.bump();
            }
            // Optional `get` / `set` accessor marker. Only valid
            // alongside `@objc("selector")` (the dispatch is
            // auto-synthesised from the selector). The next token
            // must be an ident (the property name) — otherwise
            // `get` / `set` is treated as a regular method name.
            let accessor = if let TokenKind::Ident(n) = &self.peek().kind {
                let next_is_ident = matches!(
                    self.peek_n(1).map(|t| &t.kind),
                    Some(TokenKind::Ident(_))
                );
                if next_is_ident && (n == "get" || n == "set") {
                    let kind = if n == "get" {
                        AccessorKind::Getter
                    } else {
                        AccessorKind::Setter
                    };
                    self.bump();
                    Some(kind)
                } else {
                    None
                }
            } else {
                None
            };
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
            //   * `@since("version")` — documentation-only marker
            //     for the minimum OS version. Carried through so
            //     future hover / completion surfaces can show it.
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
                        "deprecated" | "since" => extra.push(a),
                        other => {
                            return Err(ParseError::Unexpected {
                                found: TokenKind::At,
                                expected: format!(
                                    "unsupported attribute `@{other}` on @objc class method (allowed: @objc, @deprecated, @since)"
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
                    is_override,
                    /*require_fn_kw*/ false,
                    extra,
                    accessor,
                )?;
                methods.push(m);
            } else {
                if accessor.is_some() {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected: "`pub get` / `pub set` inside an @objc class needs an @objc(\"selector\") attribute — the accessor body is the objc_msgSend dispatch".into(),
                        span: t.span,
                    });
                }
                let err_span = self.peek().span;
                let extra = split_extra(attrs, None, err_span)?;
                // Plain method: same shape as `parse_objc_method`
                // but flagged so the caller knows to skip the
                // ObjC dispatch wrapper.
                let m = self.parse_objc_method(
                    String::new(),
                    method_is_pub,
                    is_static,
                    is_override,
                    /*require_fn_kw*/ false,
                    extra,
                    /*accessor*/ None,
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
            interfaces,
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
        block_libs: &[Symbol],
    ) -> Result<ilang_ast::ExternCItem, ParseError> {
        match &self.peek().kind {
            TokenKind::Fn => {
                let mut it = self.parse_extern_c_fn_with_default_libs(
                    inner_attrs, block_libs,
                )?;
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
