//! `enum` declaration parsing — type parameters, optional underlying
//! repr, variants (unit / tuple / struct payload), and explicit
//! discriminants (`name = 1` / `name = "a"`).

use ilang_ast::{EnumDecl, FieldDecl, Variant, VariantPayload};
use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;

impl<'a> Parser<'a> {
    pub(super) fn parse_enum_decl(&mut self) -> Result<EnumDecl, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Enum, "'enum'")?;
        let name = self.expect_ident("enum name")?;
        // Optional `<T, U>` type parameters — same shape as classes.
        let type_params = if matches!(self.peek().kind, TokenKind::Lt) {
            self.parse_type_param_list()?
        } else {
            Vec::new()
        };
        // Optional `: <numeric-type>` underlying repr —
        // `enum Flag: u32 { ... }`. Only allowed for fieldless
        // (unit-only) enums; the type checker enforces that and
        // numeric-primitive-only.
        let repr_ty = if matches!(self.peek().kind, TokenKind::Colon) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(&TokenKind::LBrace, "'{'")?;
        let mut variants = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBrace) {
            let v_span = self.peek().span;
            let v_name = self.expect_member_name("variant name")?;
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
                        VariantPayload::Tuple(tys.into())
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
                                is_pub: false,
                                name: f_name,
                                ty: f_ty,
                                span: f_span,
                                bits: None,
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
                        VariantPayload::Struct(fields.into())
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
            // Optional explicit discriminant: `name = <int>`. Only
            // valid on unit variants — payloaded variants don't have
            // a single integer tag the user can pin (and would
            // mostly conflict with the auto-assigned slot index).
            let discriminant = if matches!(self.peek().kind, TokenKind::Equals) {
                if !matches!(payload, VariantPayload::Unit) {
                    let p = self.peek();
                    return Err(ParseError::Unexpected {
                        found: p.kind.clone(),
                        expected: "explicit `= value` only allowed on payload-less variants".into(),
                        span: p.span,
                    });
                }
                self.bump();
                let neg = if matches!(self.peek().kind, TokenKind::Minus) {
                    self.bump();
                    true
                } else {
                    false
                };
                let lit = self.peek().clone();
                match lit.kind {
                    TokenKind::Int(n) => {
                        self.bump();
                        let v = if neg { n.wrapping_neg() } else { n };
                        Some(ilang_ast::DiscriminantLit::Int(v))
                    }
                    TokenKind::Str(s) => {
                        if neg {
                            return Err(ParseError::Unexpected {
                                found: TokenKind::Str(s),
                                expected: "integer literal after `-`".into(),
                                span: lit.span,
                            });
                        }
                        self.bump();
                        Some(ilang_ast::DiscriminantLit::Str(s))
                    }
                    other => {
                        return Err(ParseError::Unexpected {
                            found: other,
                            expected: "integer or string literal after `=`".into(),
                            span: lit.span,
                        });
                    }
                }
            } else {
                None
            };
            variants.push(Variant {
                name: v_name,
                payload,
                discriminant,
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
            is_pub: false,
            name,
            type_params: type_params.into(),
            repr_ty,
            flags: false,
            variants: variants.into(),
            span,
        })
    }
}
