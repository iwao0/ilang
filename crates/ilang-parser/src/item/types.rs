//! Type / generic-parameter / parameter-list parsing.
//!
//! Pulled out of `item.rs` so the class / enum / fn / extern-c
//! decl parsers can share these helpers without re-reading them
//! at the top of every other parser file.

use ilang_ast::{Param, Symbol, Type};
use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;

impl<'a> Parser<'a> {
    /// Parse `<T, U, ...>` after a class name in declaration position.
    /// Returns the bare identifier names; uniqueness is checked downstream.
    pub(crate) fn parse_type_param_list(&mut self) -> Result<Vec<Symbol>, ParseError> {
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

    /// Parse a comma-separated parameter list `name: T` or
    /// `name: T = default_expr`. The opening `(` and closing `)` are
    /// expected to be handled by the caller. Validates that defaults
    /// only appear on trailing parameters (once one parameter has a
    /// default, every later one must too).
    pub(crate) fn parse_param_list(&mut self) -> Result<Vec<Param>, ParseError> {
        let mut params = Vec::new();
        if matches!(self.peek().kind, TokenKind::RParen) {
            return Ok(params);
        }
        let mut seen_default_at: Option<ilang_ast::Span> = None;
        loop {
            let pspan = self.peek().span;
            let pname = self.expect_ident("parameter name")?;
            self.expect(&TokenKind::Colon, "':'")?;
            let pty = self.parse_type()?;
            let default = if matches!(self.peek().kind, TokenKind::Equals) {
                self.bump();
                let expr = self.parse_expr(0)?;
                seen_default_at = Some(pspan);
                Some(expr)
            } else {
                if let Some(_first) = seen_default_at {
                    return Err(ParseError::Unexpected {
                        found: self.peek().kind.clone(),
                        expected: "'=' (parameter without default cannot follow one with a default)"
                            .into(),
                        span: pspan,
                    });
                }
                None
            };
            params.push(Param {
                name: pname,
                ty: pty,
                span: pspan,
                default,
            });
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
                // Allow trailing `...` after the comma (variadic
                // marker on `@extern(C)` fns). Stop here and let the
                // caller consume it; `parse_param_list` itself stays
                // unaware of variadics.
                if matches!(self.peek().kind, TokenKind::DotDotDot) {
                    break;
                }
            } else {
                break;
            }
        }
        Ok(params)
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
        // Raw C pointer: `*T` or `*const T`. Only nameable inside an
        // `@extern(C) { ... }` block (the type checker enforces that).
        if matches!(t.kind, TokenKind::Star) {
            self.bump();
            let is_const = matches!(self.peek().kind, TokenKind::Const);
            if is_const {
                self.bump();
            }
            let inner = self.parse_type()?;
            return Ok(Type::RawPtr {
                is_const,
                inner: Box::new(inner),
            });
        }
        // Function type: `fn(T1, T2): R` (or `fn(): R` / `fn(T)` for
        // unit ret). Falls through to the postfix `[]` / `?` /
        // `.weak` loop below so callers can write `fn(T)[]` (an
        // array of listeners) or `fn(T)?` (an optional callback).
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
            let ty = Type::func(params, ret);
            return self.apply_type_postfix(ty);
        }
        // Tuple type: `(T1, T2, ...)`. A single `(T)` is grouping and
        // returns `T` itself; `()` would be unit but is not currently
        // emitted by the type parser.
        if matches!(t.kind, TokenKind::LParen) {
            self.bump();
            let first = self.parse_type()?;
            if matches!(self.peek().kind, TokenKind::Comma) {
                let mut elems = vec![first];
                while matches!(self.peek().kind, TokenKind::Comma) {
                    self.bump();
                    if matches!(self.peek().kind, TokenKind::RParen) {
                        break;
                    }
                    elems.push(self.parse_type()?);
                }
                self.expect(&TokenKind::RParen, "')'")?;
                return self.apply_type_postfix(Type::Tuple(elems.into()));
            }
            self.expect(&TokenKind::RParen, "')'")?;
            return self.apply_type_postfix(first);
        }
        let ty = match t.kind {
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
                    "void" => Type::CVoid,
                    "char" => Type::CChar,
                    "size_t" => Type::Size,
                    "ssize_t" => Type::SSize,
                    _ => {
                        // Module-qualified type names: `module.Type`
                        // (or even deeper). Each segment is an
                        // identifier separated by `.`. The reserved
                        // postfix `.weak` (weak-reference modifier)
                        // is left for the postfix loop below.
                        let mut full_name = n;
                        while matches!(self.peek().kind, TokenKind::Dot)
                            && !matches!(
                                self.peek_n(1).map(|t| &t.kind),
                                Some(TokenKind::Ident(n)) if n == "weak"
                            )
                        {
                            self.bump();
                            let next = self.peek().clone();
                            match next.kind {
                                TokenKind::Ident(seg) => {
                                    self.bump();
                                    full_name.push('.');
                                    full_name.push_str(&seg);
                                }
                                other => {
                                    return Err(ParseError::Unexpected {
                                        found: other,
                                        expected: "identifier after `.`".into(),
                                        span: next.span,
                                    });
                                }
                            }
                        }
                        // `simd.<elem>x<lanes>` — first-class SIMD
                        // vector type. Lift out of the generic
                        // dotted-name path so call sites get a
                        // typed `Type::Simd { elem, lanes }` they
                        // can route through cranelift's native
                        // F32X4 / etc. representation.
                        if let Some(simd) = parse_simd_suffix(&full_name) {
                            simd
                        } else if matches!(self.peek().kind, TokenKind::Lt) {
                            // After a class-like name, accept optional
                            // `<T, U>` for generic instantiations:
                            //   Box<i64>          → Generic { Box, [i64] }
                            //   Pair<string, i64> → Generic { Pair, [Str, I64] }
                            let args = self.parse_type_args()?;
                            Type::generic(full_name, args)
                        } else {
                            Type::Object(full_name.into())
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
        self.apply_type_postfix(ty)
    }

    /// Apply the postfix type modifiers — array `T[]` / `T[N]`,
    /// optional `T?`, and weak `T.weak` — to an already-parsed base
    /// type. They can chain (`T[]?`, `T?[]`). Shared by every branch
    /// of `parse_type` so tuple / function / grouped types accept the
    /// same suffixes a named type does (e.g. `(i32, f32)[]`).
    pub(crate) fn apply_type_postfix(&mut self, mut ty: Type) -> Result<Type, ParseError> {
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

/// Recognise `simd.<elem><N>` shapes:
///   `simd.f32x4`, `simd.f32x2`, `simd.f64x2`, `simd.i32x4`, …
/// Returns the matching `Type::Simd` when both halves parse,
/// `None` otherwise so the caller falls back to `Type::Object`.
fn parse_simd_suffix(full_name: &str) -> Option<ilang_ast::Type> {
    let rest = full_name.strip_prefix("simd.")?;
    // Element name is the prefix up to (but not including) `x`.
    let x_idx = rest.find('x')?;
    let (elem_str, lanes_str) = rest.split_at(x_idx);
    let lanes_str = &lanes_str[1..]; // skip the `x`
    let elem = match elem_str {
        "f32" => ilang_ast::SimdElem::F32,
        "f64" => ilang_ast::SimdElem::F64,
        "i8" => ilang_ast::SimdElem::I8,
        "i16" => ilang_ast::SimdElem::I16,
        "i32" => ilang_ast::SimdElem::I32,
        "i64" => ilang_ast::SimdElem::I64,
        _ => return None,
    };
    let lanes: u32 = lanes_str.parse().ok()?;
    if lanes == 0 {
        return None;
    }
    Some(ilang_ast::Type::Simd { elem, lanes })
}
