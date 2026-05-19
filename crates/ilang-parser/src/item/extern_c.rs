//! `@extern(C) { ... }` block parsing: the block itself, plus the
//! four inner-item variants (`fn` declaration / definition,
//! `struct`, `union`, `class`). `parse_extern_c_block_with_default_libs` dispatches on
//! the next token after consuming attributes + `pub`; each sub-
//! parser handles its specific shape.

use ilang_ast::{Attribute, FieldDecl, FnDecl, Symbol};
use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;
use crate::stmt::parse_block;

impl<'a> Parser<'a> {
    /// `@extern(C, "libname", ...) { ... }` form. The trailing
    /// strings become the default `@lib(...)` for any plain `pub
    /// fn` inside the block — write `@lib` with no args to opt
    /// into them, or `@lib("other")` to override per-fn. Without
    /// a `@lib` attribute the fn still errors (a function with
    /// no library resolution can't be dlsym'd); the bare-`@lib`
    /// marker stays explicit so a fn that wanted the default
    /// reads obvious.
    pub(super) fn parse_extern_c_block_with_default_libs(
        &mut self,
        default_libs: &[ilang_ast::Symbol],
    ) -> Result<ilang_ast::ExternCBlock, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::LBrace, "'{'")?;
        let mut items: Vec<ilang_ast::ExternCItem> = Vec::new();
        let mut extern_c_interfaces: Vec<ilang_ast::InterfaceDecl> = Vec::new();
        loop {
            // Skip leading newlines / blank lines inside the block.
            if matches!(self.peek().kind, TokenKind::RBrace) {
                break;
            }
            let inner_attrs = self.parse_attributes()?;
            // Optional `pub` modifier on the next item inside the block.
            let item_is_pub = if matches!(self.peek().kind, TokenKind::Pub) {
                self.bump();
                true
            } else {
                false
            };
            let item = match &self.peek().kind {
                TokenKind::Fn => {
                    let mut it = self.parse_extern_c_fn_with_default_libs(
                        inner_attrs, default_libs,
                    )?;
                    match &mut it {
                        ilang_ast::ExternCItem::FnDecl { is_pub, .. } => *is_pub = item_is_pub,
                        ilang_ast::ExternCItem::FnDef(f) => f.is_pub = item_is_pub,
                        _ => {}
                    }
                    it
                }
                TokenKind::Ident(n) if n == "struct" => {
                    let mut it = self.parse_struct_decl(inner_attrs, false)?;
                    if let ilang_ast::ExternCItem::Struct { is_pub, .. } = &mut it {
                        *is_pub = item_is_pub;
                    }
                    it
                }
                TokenKind::Ident(n) if n == "union" => {
                    if !inner_attrs.is_empty() {
                        let t = self.peek();
                        return Err(ParseError::Unexpected {
                            found: t.kind.clone(),
                            expected:
                                "no attributes are supported on `union` inside @extern(C)"
                                    .into(),
                            span: t.span,
                        });
                    }
                    let mut it = self.parse_union_decl(false)?;
                    if let ilang_ast::ExternCItem::Union { is_pub, .. } = &mut it {
                        *is_pub = item_is_pub;
                    }
                    it
                }
                TokenKind::Class => {
                    if !inner_attrs.is_empty() {
                        let t = self.peek();
                        return Err(ParseError::Unexpected {
                            found: t.kind.clone(),
                            expected:
                                "no attributes are supported on `class` inside @extern(C)"
                                    .into(),
                            span: t.span,
                        });
                    }
                    let mut c = self.parse_class_decl()?;
                    c.is_pub = item_is_pub;
                    ilang_ast::ExternCItem::Class(c)
                }
                TokenKind::Interface => {
                    // `@com interface I { ... }` — COM vtable
                    // contract. Lives inside @extern(C) so its
                    // signatures can reference raw pointers /
                    // C-only types just like fn / struct decls.
                    let mut is_com = false;
                    for a in &inner_attrs {
                        match a.name.as_str() {
                            "com" if a.args.is_empty() => {
                                is_com = true;
                            }
                            _ => {
                                let t = self.peek();
                                return Err(ParseError::Unexpected {
                                    found: t.kind.clone(),
                                    expected:
                                        "only `@com` is supported on interface declarations inside @extern(C)"
                                            .into(),
                                    span: t.span,
                                });
                            }
                        }
                    }
                    let mut iface = self.parse_interface_decl()?;
                    iface.is_pub = item_is_pub;
                    iface.is_com = is_com;
                    extern_c_interfaces.push(iface);
                    continue;
                }
                _ => {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected:
                            "fn / struct / union / class / interface declaration inside @extern(C) block"
                                .into(),
                        span: t.span,
                    });
                }
            };
            items.push(item);
        }
        self.expect(&TokenKind::RBrace, "'}'")?;
        Ok(ilang_ast::ExternCBlock {
            items: items.into(),
            interfaces: extern_c_interfaces.into(),
            span,
        })
    }

    /// Parse a single fn declaration inside an `@extern(C)` /
    /// `@extern(ObjC)` block. `default_libs` is the block-level
    /// library list: a bare `@lib` (no args) on the fn picks it
    /// up. Pass an empty slice for plain `@extern(C) { ... }`
    /// (the legacy form) — bare `@lib` then errors with a "no
    /// default library" message.
    pub(super) fn parse_extern_c_fn_with_default_libs(
        &mut self,
        attrs: Vec<Attribute>,
        default_libs: &[Symbol],
    ) -> Result<ilang_ast::ExternCItem, ParseError> {
        // Accepted: `@lib("name", "fallback", ...)` (one or more strings),
        // `@optional` and `@symbol("c_name")`. Anything else is rejected
        // so users notice the legacy flags are gone.
        let mut libs: Vec<Symbol> = Vec::new();
        let mut optional = false;
        let mut c_symbol: Option<Symbol> = None;
        for a in &attrs {
            match a.name.as_str() {
                "lib" => {
                    if a.args.is_empty() {
                        if default_libs.is_empty() {
                            let t = self.peek();
                            return Err(ParseError::Unexpected {
                                found: t.kind.clone(),
                                expected: "bare `@lib` (no args) is only valid inside `@extern(C, \"name\", ...)` or `@extern(ObjC, \"path\", ...)`; an @extern(C) block with no default library name needs `@lib(\"libname\", ...)` per fn".into(),
                                span: t.span,
                            });
                        }
                        libs.extend(default_libs.iter().cloned());
                        continue;
                    }
                    for arg in &a.args {
                        match arg {
                            ilang_ast::AttrArg::Str(s) => libs.push(s.as_str().into()),
                            _ => {
                                let t = self.peek();
                                return Err(ParseError::Unexpected {
                                    found: t.kind.clone(),
                                    expected: "@lib(...) takes string arguments only".into(),
                                    span: t.span,
                                });
                            }
                        }
                    }
                }
                "optional" if a.args.is_empty() => {
                    optional = true;
                }
                "symbol" => {
                    let t = self.peek();
                    let bad = ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected: "@symbol(\"c_name\") takes exactly one string argument".into(),
                        span: t.span,
                    };
                    if a.args.len() != 1 {
                        return Err(bad);
                    }
                    match &a.args[0] {
                        ilang_ast::AttrArg::Str(s) => c_symbol = Some(s.as_str().into()),
                        _ => return Err(bad),
                    }
                }
                _ => {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected: "@lib(\"libname\", ...), @optional, or @symbol(\"c_name\") (no other attributes accepted on extern(C) fn)".into(),
                        span: t.span,
                    });
                }
            }
        }
        // Parse the fn signature manually so we can distinguish
        // declaration-only (no `{` after the return type, dlsym'd
        // through @lib) from definition (has a `{ body }`).
        let span = self.peek().span;
        self.expect(&TokenKind::Fn, "'fn'")?;
        let name = self.expect_ident("function name")?;
        self.expect(&TokenKind::LParen, "'('")?;
        let params = self.parse_param_list()?;
        // Trailing `...` after the last fixed param marks a C
        // variadic. `parse_param_list` already consumed the trailing
        // `,` if there was one, so we just need to check for `...`.
        let mut variadic = false;
        if matches!(self.peek().kind, TokenKind::DotDotDot) {
            self.bump();
            variadic = true;
        }
        self.expect(&TokenKind::RParen, "')'")?;
        let ret = if matches!(self.peek().kind, TokenKind::Colon) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
        if matches!(self.peek().kind, TokenKind::LBrace) {
            if variadic {
                let t = self.peek();
                return Err(ParseError::Unexpected {
                    found: t.kind.clone(),
                    expected: "variadic `...` is only allowed on extern fn declarations, not definitions".into(),
                    span: t.span,
                });
            }
            // Definition: ilang body, C ABI.
            let body = parse_block(self)?;
            let fn_decl = FnDecl {
                is_pub: false,
                attrs: Box::new([]),
                name,
                type_params: Box::new([]),
                params: params.into(),
                ret,
                body,
                span,
                is_override: false,
            is_async: false,
            };
            Ok(ilang_ast::ExternCItem::FnDef(fn_decl))
        } else {
            // Declaration: terminator and we're done.
            self.consume_stmt_terminator()?;
            Ok(ilang_ast::ExternCItem::FnDecl {
                is_pub: false,
                name,
                params: params.into(),
                ret,
                libs: libs.into(),
                optional,
                variadic,
                c_symbol,
                span,
            })
        }
    }

    /// Parse a `struct Name { fields }` declaration.
    ///
    /// `restrict_c_types` is set to `true` when the declaration sits at
    /// the top level of a module (outside any `@extern(C) { ... }`
    /// block). The flag is propagated into the AST node and consumed by
    /// a later validation pass that rejects C-only field types
    /// (`char`, `void`, `size_t`, `ssize_t`, raw pointers).
    pub(super) fn parse_struct_decl(
        &mut self,
        attrs: Vec<Attribute>,
        restrict_c_types: bool,
    ) -> Result<ilang_ast::ExternCItem, ParseError> {
        // Inside `@extern(C) {}` the C layout is implicit, so
        // `@extern(C) struct` is redundant (and rejected). Only `@packed` is
        // accepted, marking the layout as packed (no padding).
        let mut is_packed = false;
        for a in &attrs {
            match (a.name.as_str(), &*a.args) {
                ("packed", []) => {
                    is_packed = true;
                }
                _ => {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected:
                            "@packed only (no other attributes on struct inside @extern(C))".into(),
                        span: t.span,
                    });
                }
            }
        }
        let span = self.peek().span;
        self.bump(); // consume `struct`
        let name = self.expect_ident("struct name")?;
        self.expect(&TokenKind::LBrace, "'{'")?;
        let mut fields: Vec<FieldDecl> = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBrace) {
            let f_attrs = self.parse_attributes()?;
            let mut bits: Option<u32> = None;
            for a in &f_attrs {
                match (a.name.as_str(), &*a.args) {
                    ("bits", [ilang_ast::AttrArg::Int(n)]) if *n >= 1 && *n <= 64 => {
                        bits = Some(*n as u32);
                    }
                    _ => {
                        let t = self.peek();
                        return Err(ParseError::Unexpected {
                            found: t.kind.clone(),
                            expected: "@bits(N) (1..=64)".into(),
                            span: t.span,
                        });
                    }
                }
            }
            let f_span = self.peek().span;
            let f_name = self.expect_ident("field name")?;
            // `name(...)` reads like a method declaration. `struct`
            // inside `@extern(C) {}` is C-layout-only: methods belong
            // on a `class`, so give a targeted error instead of the
            // generic "expected ':'".
            if matches!(self.peek().kind, TokenKind::LParen) {
                let t = self.peek();
                return Err(ParseError::Unexpected {
                    found: t.kind.clone(),
                    expected: format!(
                        "':' after field name — `struct {name}` cannot define methods (move `{f_name}` onto a `class` if you need a method)"
                    ),
                    span: t.span,
                });
            }
            self.expect(&TokenKind::Colon, "':'")?;
            let f_ty = self.parse_type()?;
            self.consume_stmt_terminator()?;
            fields.push(FieldDecl { is_pub: false, name: f_name, ty: f_ty, span: f_span, bits });
        }
        self.expect(&TokenKind::RBrace, "'}'")?;
        Ok(ilang_ast::ExternCItem::Struct {
            is_pub: false,
            name,
            fields: fields.into(),
            is_packed,
            restrict_c_types,
            span,
        })
    }

    /// Parse a `union Name { fields }` declaration. See
    /// [`parse_struct_decl`] for the meaning of `restrict_c_types`.
    pub(super) fn parse_union_decl(
        &mut self,
        restrict_c_types: bool,
    ) -> Result<ilang_ast::ExternCItem, ParseError> {
        let span = self.peek().span;
        self.bump(); // consume `union`
        let name = self.expect_ident("union name")?;
        self.expect(&TokenKind::LBrace, "'{'")?;
        let mut fields: Vec<FieldDecl> = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBrace) {
            let f_span = self.peek().span;
            let f_name = self.expect_ident("field name")?;
            if matches!(self.peek().kind, TokenKind::LParen) {
                let t = self.peek();
                return Err(ParseError::Unexpected {
                    found: t.kind.clone(),
                    expected: format!(
                        "':' after field name — `union {name}` cannot define methods (move `{f_name}` onto a `class` if you need a method)"
                    ),
                    span: t.span,
                });
            }
            self.expect(&TokenKind::Colon, "':'")?;
            let f_ty = self.parse_type()?;
            self.consume_stmt_terminator()?;
            fields.push(FieldDecl { is_pub: false, name: f_name, ty: f_ty, span: f_span, bits: None });
        }
        self.expect(&TokenKind::RBrace, "'}'")?;
        Ok(ilang_ast::ExternCItem::Union {
            is_pub: false,
            name,
            fields: fields.into(),
            restrict_c_types,
            span,
        })
    }
}
