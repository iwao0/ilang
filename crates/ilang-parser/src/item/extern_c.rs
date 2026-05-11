//! `@extern(C) { ... }` block parsing: the block itself, plus the
//! four inner-item variants (`fn` declaration / definition,
//! `struct`, `union`, `class`). `parse_extern_c_block` dispatches on
//! the next token after consuming attributes + `pub`; each sub-
//! parser handles its specific shape.

use ilang_ast::{Attribute, FieldDecl, FnDecl, Symbol};
use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;
use crate::stmt::parse_block;

impl<'a> Parser<'a> {
    pub(super) fn parse_extern_c_block(
        &mut self,
    ) -> Result<ilang_ast::ExternCBlock, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::LBrace, "'{'")?;
        let mut items: Vec<ilang_ast::ExternCItem> = Vec::new();
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
                    let mut it = self.parse_extern_c_fn(inner_attrs)?;
                    match &mut it {
                        ilang_ast::ExternCItem::FnDecl { is_pub, .. } => *is_pub = item_is_pub,
                        ilang_ast::ExternCItem::FnDef(f) => f.is_pub = item_is_pub,
                        _ => {}
                    }
                    it
                }
                TokenKind::Ident(n) if n == "struct" => {
                    let mut it = self.parse_extern_c_struct(inner_attrs)?;
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
                    let mut it = self.parse_extern_c_union()?;
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
                _ => {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected:
                            "fn / struct / union / class declaration inside @extern(C) block"
                                .into(),
                        span: t.span,
                    });
                }
            };
            items.push(item);
        }
        self.expect(&TokenKind::RBrace, "'}'")?;
        Ok(ilang_ast::ExternCBlock { items: items.into(), span })
    }

    fn parse_extern_c_fn(
        &mut self,
        attrs: Vec<Attribute>,
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
                        let t = self.peek();
                        return Err(ParseError::Unexpected {
                            found: t.kind.clone(),
                            expected: "@lib(\"libname\", ...) requires at least one string argument".into(),
                            span: t.span,
                        });
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

    fn parse_extern_c_struct(
        &mut self,
        attrs: Vec<Attribute>,
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
            self.expect(&TokenKind::Colon, "':'")?;
            let f_ty = self.parse_type()?;
            self.consume_stmt_terminator()?;
            fields.push(FieldDecl { is_pub: false, name: f_name, ty: f_ty, span: f_span, bits });
        }
        self.expect(&TokenKind::RBrace, "'}'")?;
        Ok(ilang_ast::ExternCItem::Struct { is_pub: false, name, fields: fields.into(), is_packed, span })
    }

    fn parse_extern_c_union(
        &mut self,
    ) -> Result<ilang_ast::ExternCItem, ParseError> {
        let span = self.peek().span;
        self.bump(); // consume `union`
        let name = self.expect_ident("union name")?;
        self.expect(&TokenKind::LBrace, "'{'")?;
        let mut fields: Vec<FieldDecl> = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBrace) {
            let f_span = self.peek().span;
            let f_name = self.expect_ident("field name")?;
            self.expect(&TokenKind::Colon, "':'")?;
            let f_ty = self.parse_type()?;
            self.consume_stmt_terminator()?;
            fields.push(FieldDecl { is_pub: false, name: f_name, ty: f_ty, span: f_span, bits: None });
        }
        self.expect(&TokenKind::RBrace, "'}'")?;
        Ok(ilang_ast::ExternCItem::Union { is_pub: false, name, fields: fields.into(), span })
    }
}
