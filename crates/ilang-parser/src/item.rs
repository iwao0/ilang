use ilang_ast::{AttrArg, Attribute, FnDecl, Item, Param, Type};
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
            _ => {
                let t = self.peek();
                Err(ParseError::Unexpected {
                    found: t.kind.clone(),
                    expected: "'fn' after attributes".into(),
                    line: t.span.line,
                    col: t.span.col,
                })
            }
        }
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
        self.expect(&TokenKind::Fn, "'fn'")?;
        let name = self.expect_ident("function name")?;
        self.expect(&TokenKind::LParen, "'('")?;
        let mut params = Vec::new();
        if !matches!(self.peek().kind, TokenKind::RParen) {
            loop {
                let pname = self.expect_ident("parameter name")?;
                self.expect(&TokenKind::Colon, "':'")?;
                let pty = self.parse_type()?;
                params.push(Param {
                    name: pname,
                    ty: pty,
                });
                if matches!(self.peek().kind, TokenKind::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RParen, "')'")?;
        let ret = if matches!(self.peek().kind, TokenKind::Arrow) {
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
        })
    }

    pub(crate) fn parse_type(&mut self) -> Result<Type, ParseError> {
        let t = self.peek().clone();
        match t.kind {
            TokenKind::Ident(n) => {
                self.bump();
                match n.as_str() {
                    "i64" => Ok(Type::I64),
                    "f64" => Ok(Type::F64),
                    "bool" => Ok(Type::Bool),
                    _ => Err(ParseError::UnknownType {
                        name: n,
                        line: t.span.line,
                        col: t.span.col,
                    }),
                }
            }
            other => Err(ParseError::Unexpected {
                found: other,
                expected: "type name".into(),
                line: t.span.line,
                col: t.span.col,
            }),
        }
    }
}
