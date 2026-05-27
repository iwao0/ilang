//! Top-level fn parsing — plain `fn name(...): T { ... }` and the
//! `@intrinsic("symbol")` body-less form that binds an ilang fn
//! name to a runtime-provided implementation.

use ilang_ast::{Attribute, FnDecl, Item, Symbol};
use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;
use crate::stmt::parse_block;

impl<'a> Parser<'a> {
    /// Parse `@intrinsic("...") [pub] fn name(...): T` (declaration
    /// only, no body) at top level. Produces `Item::Fn` with
    /// `intrinsic_name` set to the `$`-prefixed runtime symbol and an
    /// empty body — codegen recognises the marker and lowers the call
    /// site as a cranelift import without going through the
    /// `@lib` / `@symbol` c_symbol path.
    pub(in crate::item) fn parse_intrinsic_fn(
        &mut self,
        is_pub: bool,
        runtime_symbol: Symbol,
        attrs: &[Attribute],
    ) -> Result<Item, ParseError> {
        for a in attrs {
            if a.name.as_str() != "intrinsic" {
                let t = self.peek();
                return Err(ParseError::Unexpected {
                    found: t.kind.clone(),
                    expected: format!(
                        "@intrinsic cannot be combined with other attributes (found @{})",
                        a.name
                    ),
                    span: t.span,
                });
            }
        }
        if !matches!(self.peek().kind, TokenKind::Fn) {
            let t = self.peek();
            return Err(ParseError::Unexpected {
                found: t.kind.clone(),
                expected: "@intrinsic must be followed by a `fn` declaration".into(),
                span: t.span,
            });
        }
        let span = self.peek().span;
        self.expect(&TokenKind::Fn, "'fn'")?;
        let name = self.expect_ident("function name")?;
        if matches!(self.peek().kind, TokenKind::Lt) {
            let t = self.peek();
            return Err(ParseError::Unexpected {
                found: t.kind.clone(),
                expected: "@intrinsic fns do not support generic type parameters".into(),
                span: t.span,
            });
        }
        self.expect(&TokenKind::LParen, "'('")?;
        let params = self.parse_param_list()?;
        self.expect(&TokenKind::RParen, "')'")?;
        let ret = if matches!(self.peek().kind, TokenKind::Colon) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
        // `$`-prefixed runtime symbol. The `$` isn't a legal ilang
        // identifier character so it can't collide with any user fn
        // name; the .il source stays clean.
        let sigil = Symbol::intern(&format!("${}", runtime_symbol.as_str()));
        let empty_body = ilang_ast::Block {
            stmts: Vec::new(),
            tail: None,
        };
        Ok(Item::Fn(ilang_ast::FnDecl {
            attrs: Box::new([]),
            is_pub,
            name,
            type_params: Box::new([]),
            params: params.into(),
            ret,
            body: empty_body,
            span,
            is_override: false,
            is_async: false,
            intrinsic_name: Some(sigil),
        }))
    }

    pub(in crate::item) fn parse_fn_decl(&mut self, attrs: Vec<Attribute>) -> Result<FnDecl, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Fn, "'fn'")?;
        let name = self.expect_ident("function name")?;
        // Optional `<T, U>` type parameters (same shape as classes).
        let type_params = if matches!(self.peek().kind, TokenKind::Lt) {
            self.parse_type_param_list()?
        } else {
            Vec::new()
        };
        self.expect(&TokenKind::LParen, "'('")?;
        let params = self.parse_param_list()?;
        self.expect(&TokenKind::RParen, "')'")?;
        let ret = if matches!(self.peek().kind, TokenKind::Colon) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = parse_block(self)?;
        Ok(FnDecl {
            is_pub: false,
            attrs: attrs.into(),
            name,
            type_params: type_params.into(),
            params: params.into(),
            ret,
            body,
            span,
            is_override: false,
            is_async: false,
            intrinsic_name: None,
        })
    }
}
