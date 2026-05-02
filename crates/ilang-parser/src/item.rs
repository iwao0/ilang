use ilang_ast::{
    AttrArg, Attribute, ClassDecl, EnumDecl, FieldDecl, FnDecl, Item, Param, Type, Variant,
    VariantPayload,
};
use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;
use crate::stmt::parse_block;

impl<'a> Parser<'a> {
    pub(crate) fn parse_item(&mut self) -> Result<Item, ParseError> {
        let attrs = self.parse_attributes()?;
        match self.peek().kind {
            TokenKind::Fn => {
                let fn_decl = self.parse_fn_decl(attrs)?;
                Ok(Item::Fn(fn_decl))
            }
            TokenKind::Class => {
                if !attrs.is_empty() {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected: "'fn' (attributes on classes are not supported yet)".into(),
                        span: t.span,
                    });
                }
                let c = self.parse_class_decl()?;
                Ok(Item::Class(c))
            }
            TokenKind::Enum => {
                if !attrs.is_empty() {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected: "'fn' (attributes on enums are not supported)".into(),
                        span: t.span,
                    });
                }
                let e = self.parse_enum_decl()?;
                Ok(Item::Enum(e))
            }
            _ => {
                let t = self.peek();
                Err(ParseError::Unexpected {
                    found: t.kind.clone(),
                    expected: "'fn', 'class', or 'enum' after attributes".into(),
                    span: t.span,
                })
            }
        }
    }

    fn parse_class_decl(&mut self) -> Result<ClassDecl, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Class, "'class'")?;
        let name = self.expect_ident("class name")?;
        // Optional `<T, U>` type parameters. Always unambiguous after a
        // class name in declaration position.
        let type_params = if matches!(self.peek().kind, TokenKind::Lt) {
            self.parse_type_param_list()?
        } else {
            Vec::new()
        };
        self.expect(&TokenKind::LBrace, "'{'")?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        loop {
            match self.peek().kind {
                TokenKind::RBrace => break,
                TokenKind::At => {
                    let attrs = self.parse_attributes()?;
                    let m = self.parse_method(attrs)?;
                    methods.push(m);
                }
                TokenKind::Ident(_) => {
                    let next_kind = self.tokens[(self.pos + 1).min(self.tokens.len() - 1)]
                        .kind
                        .clone();
                    match next_kind {
                        TokenKind::Colon => {
                            let f = self.parse_field()?;
                            fields.push(f);
                        }
                        TokenKind::LParen => {
                            let m = self.parse_method(Vec::new())?;
                            methods.push(m);
                        }
                        other => {
                            let t = self.peek();
                            return Err(ParseError::Unexpected {
                                found: other,
                                expected: "':' (field) or '(' (method)".into(),
                                span: t.span,
                            });
                        }
                    }
                }
                _ => {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected: "field, method, or '}'".into(),
                        span: t.span,
                    });
                }
            }
        }
        self.expect(&TokenKind::RBrace, "'}'")?;
        Ok(ClassDecl {
            name,
            type_params,
            fields,
            methods,
            span,
        })
    }

    fn parse_enum_decl(&mut self) -> Result<EnumDecl, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Enum, "'enum'")?;
        let name = self.expect_ident("enum name")?;
        // Optional `<T, U>` type parameters — same shape as classes.
        let type_params = if matches!(self.peek().kind, TokenKind::Lt) {
            self.parse_type_param_list()?
        } else {
            Vec::new()
        };
        self.expect(&TokenKind::LBrace, "'{'")?;
        let mut variants = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBrace) {
            let v_span = self.peek().span;
            let v_name = self.expect_ident("variant name")?;
            // Payload is introduced by `:` — either `: (Ty, ...)` for
            // tuple or `: { name: Ty, ... }` for struct. Without a `:`
            // the variant is a unit (no payload).
            let payload = if matches!(self.peek().kind, TokenKind::Colon) {
                self.bump();
                match self.peek().kind {
                    TokenKind::LParen => {
                        self.bump();
                        let mut tys = Vec::new();
                        if !matches!(self.peek().kind, TokenKind::RParen) {
                            loop {
                                tys.push(self.parse_type()?);
                                if matches!(self.peek().kind, TokenKind::Comma) {
                                    self.bump();
                                } else {
                                    break;
                                }
                            }
                        }
                        self.expect(&TokenKind::RParen, "')'")?;
                        VariantPayload::Tuple(tys)
                    }
                    TokenKind::LBrace => {
                        self.bump();
                        let mut fields = Vec::new();
                        while !matches!(self.peek().kind, TokenKind::RBrace) {
                            let f_span = self.peek().span;
                            let f_name = self.expect_ident("field name")?;
                            self.expect(&TokenKind::Colon, "':'")?;
                            let f_ty = self.parse_type()?;
                            fields.push(FieldDecl {
                                name: f_name,
                                ty: f_ty,
                                span: f_span,
                            });
                            if matches!(self.peek().kind, TokenKind::Comma) {
                                self.bump();
                            } else if !matches!(self.peek().kind, TokenKind::RBrace)
                                && !self.peek().leading_newline
                            {
                                let p = self.peek();
                                return Err(ParseError::Unexpected {
                                    found: p.kind.clone(),
                                    expected: "',' or newline between struct fields".into(),
                                    span: p.span,
                                });
                            }
                        }
                        self.expect(&TokenKind::RBrace, "'}'")?;
                        VariantPayload::Struct(fields)
                    }
                    _ => {
                        let p = self.peek();
                        return Err(ParseError::Unexpected {
                            found: p.kind.clone(),
                            expected: "'(' (tuple payload) or '{' (struct payload) after ':'"
                                .into(),
                            span: p.span,
                        });
                    }
                }
            } else {
                VariantPayload::Unit
            };
            variants.push(Variant {
                name: v_name,
                payload,
                span: v_span,
            });
            // Variants separated by commas or newlines.
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
            } else if !matches!(self.peek().kind, TokenKind::RBrace)
                && !self.peek().leading_newline
            {
                let p = self.peek();
                return Err(ParseError::Unexpected {
                    found: p.kind.clone(),
                    expected: "',' or newline between variants".into(),
                    span: p.span,
                });
            }
        }
        self.expect(&TokenKind::RBrace, "'}'")?;
        Ok(EnumDecl {
            name,
            type_params,
            variants,
            span,
        })
    }

    fn parse_field(&mut self) -> Result<FieldDecl, ParseError> {
        let span = self.peek().span;
        let name = self.expect_ident("field name")?;
        self.expect(&TokenKind::Colon, "':'")?;
        let ty = self.parse_type()?;
        self.consume_stmt_terminator()?;
        Ok(FieldDecl { name, ty, span })
    }

    fn parse_method(&mut self, attrs: Vec<Attribute>) -> Result<FnDecl, ParseError> {
        let span = self.peek().span;
        let name = self.expect_ident("method name")?;
        self.expect(&TokenKind::LParen, "'('")?;
        let mut params = Vec::new();
        if !matches!(self.peek().kind, TokenKind::RParen) {
            loop {
                let pspan = self.peek().span;
                let pname = self.expect_ident("parameter name")?;
                self.expect(&TokenKind::Colon, "':'")?;
                let pty = self.parse_type()?;
                params.push(Param {
                    name: pname,
                    ty: pty,
                    span: pspan,
                });
                if matches!(self.peek().kind, TokenKind::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RParen, "')'")?;
        let ret = if matches!(self.peek().kind, TokenKind::Colon) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = parse_block(self)?;
        Ok(FnDecl {
            attrs,
            name,
            params,
            ret,
            body,
            span,
        })
    }

    /// Parse a sequence of `@name(args)` attributes (TS / Java / Python
    /// decorator style). Each `@` introduces one attribute; chain them
    /// for multiple. The argument list is required for now — bare `@x`
    /// without parens is a parse error so the syntax stays predictable.
    fn parse_attributes(&mut self) -> Result<Vec<Attribute>, ParseError> {
        let mut out = Vec::new();
        while matches!(self.peek().kind, TokenKind::At) {
            self.bump();
            let name = self.expect_ident("attribute name")?;
            self.expect(&TokenKind::LParen, "'('")?;
            let mut args = Vec::new();
            if !matches!(self.peek().kind, TokenKind::RParen) {
                loop {
                    let path = self.parse_attr_path()?;
                    args.push(AttrArg::Path(path));
                    if matches!(self.peek().kind, TokenKind::Comma) {
                        self.bump();
                    } else {
                        break;
                    }
                }
            }
            self.expect(&TokenKind::RParen, "')'")?;
            out.push(Attribute { name, args });
        }
        Ok(out)
    }

    fn parse_attr_path(&mut self) -> Result<Vec<String>, ParseError> {
        let mut parts = vec![self.expect_ident("capability name")?];
        while matches!(self.peek().kind, TokenKind::ColonColon) {
            self.bump();
            parts.push(self.expect_ident("capability segment")?);
        }
        Ok(parts)
    }

    fn parse_fn_decl(&mut self, attrs: Vec<Attribute>) -> Result<FnDecl, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Fn, "'fn'")?;
        let name = self.expect_ident("function name")?;
        self.expect(&TokenKind::LParen, "'('")?;
        let mut params = Vec::new();
        if !matches!(self.peek().kind, TokenKind::RParen) {
            loop {
                let pspan = self.peek().span;
                let pname = self.expect_ident("parameter name")?;
                self.expect(&TokenKind::Colon, "':'")?;
                let pty = self.parse_type()?;
                params.push(Param {
                    name: pname,
                    ty: pty,
                    span: pspan,
                });
                if matches!(self.peek().kind, TokenKind::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RParen, "')'")?;
        let ret = if matches!(self.peek().kind, TokenKind::Colon) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = parse_block(self)?;
        Ok(FnDecl {
            attrs,
            name,
            params,
            ret,
            body,
            span,
        })
    }

    /// Parse `<T, U, ...>` after a class name in declaration position.
    /// Returns the bare identifier names; uniqueness is checked downstream.
    fn parse_type_param_list(&mut self) -> Result<Vec<String>, ParseError> {
        self.expect(&TokenKind::Lt, "'<'")?;
        let mut names = Vec::new();
        loop {
            names.push(self.expect_ident("type parameter name")?);
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        self.expect_close_gt()?;
        Ok(names)
    }

    /// Parse `<T, U, ...>` of concrete type arguments (used in generic
    /// type references and `new Foo<T>(args)`).
    pub(crate) fn parse_type_args(&mut self) -> Result<Vec<Type>, ParseError> {
        self.expect(&TokenKind::Lt, "'<'")?;
        let mut args = Vec::new();
        loop {
            args.push(self.parse_type()?);
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        self.expect_close_gt()?;
        Ok(args)
    }

    /// Consume a closing `>` for a generic. Handles the `>>` case by
    /// splitting it: the inner generic registers one "virtual" `>` via
    /// `pending_close_gt` so the outer can close without re-tokenizing.
    fn expect_close_gt(&mut self) -> Result<(), ParseError> {
        // Outer close after a previously-split `>>`.
        if self.pending_close_gt > 0 {
            self.pending_close_gt -= 1;
            self.bump(); // consume the `>>` token now that both halves used
            return Ok(());
        }
        let peeked = self.peek().clone();
        match peeked.kind {
            TokenKind::Gt => {
                self.bump();
                Ok(())
            }
            TokenKind::GtGt => {
                // Take the first `>` here; leave the token in place so the
                // surrounding generic's close picks up the second.
                self.pending_close_gt += 1;
                Ok(())
            }
            other => Err(ParseError::Unexpected {
                found: other,
                expected: "'>'".into(),
                span: peeked.span,
            }),
        }
    }

    pub(crate) fn parse_type(&mut self) -> Result<Type, ParseError> {
        let t = self.peek().clone();
        // Function type: `fn(T1, T2): R` (or `fn(): R` / `fn(T)` for unit ret).
        if matches!(t.kind, TokenKind::Fn) {
            self.bump();
            self.expect(&TokenKind::LParen, "'('")?;
            let mut params = Vec::new();
            if !matches!(self.peek().kind, TokenKind::RParen) {
                loop {
                    params.push(self.parse_type()?);
                    if matches!(self.peek().kind, TokenKind::Comma) {
                        self.bump();
                    } else {
                        break;
                    }
                }
            }
            self.expect(&TokenKind::RParen, "')'")?;
            let ret = if matches!(self.peek().kind, TokenKind::Colon) {
                self.bump();
                self.parse_type()?
            } else {
                Type::Unit
            };
            return Ok(Type::Fn {
                params,
                ret: Box::new(ret),
            });
        }
        let mut ty = match t.kind {
            TokenKind::Ident(n) => {
                self.bump();
                match n.as_str() {
                    "i8" => Type::I8,
                    "i16" => Type::I16,
                    "i32" => Type::I32,
                    "i64" => Type::I64,
                    "u8" => Type::U8,
                    "u16" => Type::U16,
                    "u32" => Type::U32,
                    "u64" => Type::U64,
                    "f32" => Type::F32,
                    "f64" => Type::F64,
                    "bool" => Type::Bool,
                    "string" => Type::Str,
                    _ => {
                        // After a class-like name, accept optional
                        // `<T, U>` for generic instantiations:
                        //   Box<i64>          → Generic { Box, [i64] }
                        //   Pair<string, i64> → Generic { Pair, [Str, I64] }
                        if matches!(self.peek().kind, TokenKind::Lt) {
                            let args = self.parse_type_args()?;
                            Type::Generic { base: n, args }
                        } else {
                            Type::Object(n)
                        }
                    }
                }
            }
            other => {
                return Err(ParseError::Unexpected {
                    found: other,
                    expected: "type name".into(),
                    span: t.span,
                });
            }
        };
        // Postfix modifiers: array `T[]` / `T[N]` and optional `T?`.
        // Both can chain (`T[]?`, `T?[]`, `T??` though redundant).
        loop {
            match self.peek().kind {
                TokenKind::LBracket => {
                    self.bump();
                    let fixed = match self.peek().kind {
                        TokenKind::RBracket => None,
                        TokenKind::Int(n) if n >= 0 => {
                            self.bump();
                            Some(n as usize)
                        }
                        _ => {
                            let p = self.peek();
                            return Err(ParseError::Unexpected {
                                found: p.kind.clone(),
                                expected: "']' or non-negative integer literal".into(),
                                span: p.span,
                            });
                        }
                    };
                    self.expect(&TokenKind::RBracket, "']'")?;
                    ty = Type::Array {
                        elem: Box::new(ty),
                        fixed,
                    };
                }
                TokenKind::Question => {
                    self.bump();
                    ty = Type::Optional(Box::new(ty));
                }
                TokenKind::Dot => {
                    // `.weak` postfix — only valid form at the moment.
                    // We snapshot the position so an unrelated dot
                    // sequence after a type wouldn't accidentally be
                    // consumed (no such case today, but safe-guarded).
                    if matches!(
                        self.peek_n(1).map(|t| &t.kind),
                        Some(TokenKind::Ident(n)) if n == "weak"
                    ) {
                        self.bump(); // .
                        self.bump(); // weak
                        ty = Type::Weak(Box::new(ty));
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }
        Ok(ty)
    }
}
