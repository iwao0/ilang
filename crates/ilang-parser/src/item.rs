use ilang_ast::{AttrArg, Attribute, ClassDecl, FieldDecl, FnDecl, Item, Param, Type};
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
            _ => {
                let t = self.peek();
                Err(ParseError::Unexpected {
                    found: t.kind.clone(),
                    expected: "'fn' or 'class' after attributes".into(),
                    span: t.span,
                })
            }
        }
    }

    fn parse_class_decl(&mut self) -> Result<ClassDecl, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Class, "'class'")?;
        let name = self.expect_ident("class name")?;
        self.expect(&TokenKind::LBrace, "'{'")?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        loop {
            match self.peek().kind {
                TokenKind::RBrace => break,
                TokenKind::Hash => {
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
            fields,
            methods,
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

    fn parse_attributes(&mut self) -> Result<Vec<Attribute>, ParseError> {
        let mut out = Vec::new();
        while matches!(self.peek().kind, TokenKind::Hash) {
            self.bump();
            self.expect(&TokenKind::LBracket, "'['")?;
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
            self.expect(&TokenKind::RBracket, "']'")?;
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

    pub(crate) fn parse_type(&mut self) -> Result<Type, ParseError> {
        let t = self.peek().clone();
        match t.kind {
            TokenKind::Ident(n) => {
                self.bump();
                match n.as_str() {
                    "i32" => Ok(Type::I32),
                    "i64" => Ok(Type::I64),
                    "f32" => Ok(Type::F32),
                    "f64" => Ok(Type::F64),
                    "bool" => Ok(Type::Bool),
                    _ => Ok(Type::Object(n)),
                }
            }
            other => Err(ParseError::Unexpected {
                found: other,
                expected: "type name".into(),
                span: t.span,
            }),
        }
    }
}
