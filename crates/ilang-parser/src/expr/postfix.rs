//! Postfix expression parsing — `.field` / `.method(args)` / `[idx]`
//! / `Name { ... }` struct literals / `?` Result-shortcircuit. Runs
//! after `parse_prefix` and binds tighter than any infix operator.

use ilang_ast::{Expr, ExprKind, Span, Symbol};
use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;

use super::flatten_var_dot_chain;

impl<'a> Parser<'a> {
    /// Apply postfix `.field` / `.method(args)` chains, repeatedly, to a
    /// parsed primary expression.
    pub(in crate::expr) fn parse_postfix(&mut self, mut expr: Expr) -> Result<Expr, ParseError> {
        loop {
            match self.peek().kind {
                TokenKind::LBracket => {
                    self.bump();
                    let index = self.parse_expr(0)?;
                    let close_span = self.peek().span;
                    self.expect(&TokenKind::RBracket, "']'")?;
                    let span = expr.span.to(close_span);
                    expr = Expr::new(
                        ExprKind::Index {
                            obj: Box::new(expr),
                            index: Box::new(index),
                        },
                        span,
                    );
                }
                TokenKind::Dot => {
                    expr = self.parse_dot_postfix(expr)?;
                }
                // Struct literal: `Foo { f1: v1, f2: v2 }`. Only
                // accepted when the receiver is a bare class name
                // (`Var(_)`) and the body has the `{ ident :` shape
                // — disambiguating from blocks and map literals.
                // Lowered as `new Foo()` plus a sequence of field
                // assignments at the type-checker / codegen stage.
                TokenKind::LBrace
                    if flatten_var_dot_chain(&expr).is_some()
                        && matches!(
                            self.peek_n(1).map(|t| &t.kind),
                            Some(TokenKind::Ident(_))
                        )
                        && matches!(
                            self.peek_n(2).map(|t| &t.kind),
                            Some(TokenKind::Colon)
                        ) =>
                {
                    self.bump();
                    // Struct literals typically have a handful of fields;
                    // prealloc avoids the 0→4→8 reallocation pair for
                    // the common 3–4-field case.
                    let mut fs: Vec<(Symbol, Expr)> = Vec::with_capacity(4);
                    let mut name_spans: Vec<Span> = Vec::with_capacity(4);
                    while !matches!(self.peek().kind, TokenKind::RBrace) {
                        let fname_span = self.peek().span;
                        let fname = self.expect_ident("field name")?;
                        self.expect(&TokenKind::Colon, "':'")?;
                        let fval = self.parse_expr(0)?;
                        fs.push((fname, fval));
                        name_spans.push(fname_span);
                        if matches!(self.peek().kind, TokenKind::Comma) {
                            self.bump();
                        } else if !matches!(self.peek().kind, TokenKind::RBrace)
                            && !self.peek().leading_newline
                        {
                            let p = self.peek();
                            return Err(ParseError::Unexpected {
                                found: p.kind.clone(),
                                expected: "',' or newline between fields".into(),
                                span: p.span,
                            });
                        }
                    }
                    let close_span = self.peek().span;
                    self.expect(&TokenKind::RBrace, "'}'")?;
                    let class_name = flatten_var_dot_chain(&expr)
                        .expect("matched in the guard above");
                    let span = expr.span.to(close_span);
                    expr = Expr::new(
                        ExprKind::StructLit {
                            class: class_name.into(),
                            fields: fs.into(),
                            field_name_spans: name_spans.into(),
                        },
                        span,
                    );
                }
                // Call on an arbitrary callee expression — `arr[0]()`,
                // `(f)(x)`, `obj.make()()`. A bare `name(...)` is
                // already turned into `Call` by the primary parser, so
                // this only fires when `(` follows a non-name postfix
                // expression. The AST's `Call` is name-based, so
                // desugar to a block that binds the callee to a
                // positional temp and calls that, reusing the
                // closure-call machinery the named case already has
                // (mirrors the `?`-operator desugar below). The
                // newline guard keeps a `(...)` that starts the next
                // statement from being read as a call.
                TokenKind::LParen if !self.peek().leading_newline => {
                    let lp_span = self.peek().span;
                    self.bump();
                    let args = self.parse_call_args()?;
                    let span = expr.span.to(lp_span);
                    let tmp: Symbol =
                        format!("__call_{}_{}", lp_span.line, lp_span.col)
                            .as_str()
                            .into();
                    let call = Expr::new(
                        ExprKind::Call { callee: tmp, args: args.into() },
                        span,
                    );
                    let block = ilang_ast::Block {
                        stmts: vec![ilang_ast::Stmt {
                            kind: ilang_ast::StmtKind::Let {
                                is_pub: false,
                                is_const: false,
                                name: tmp,
                                ty: None,
                                value: expr,
                            },
                            span,
                            source_module: None,
                        }],
                        tail: Some(Box::new(call)),
                    };
                    expr = Expr::new(ExprKind::Block(block), span);
                }
                // Postfix `?` — Result short-circuit. `e?` evaluates
                // to the `ok` payload when `e` is `Result.ok(v)`, or
                // early-returns `Result.err(err)` from the enclosing
                // fn when `e` is `Result.err(err)`.
                //
                // Desugars in-place to
                //   { let __try_L_C = <e>
                //     match __try_L_C {
                //         ok(__try_v_L_C) { __try_v_L_C }
                //         err(__try_e_L_C) { return Result.err(__try_e_L_C) }
                //     } }
                // where L_C tags the names with the `?`'s source
                // position so multiple uses in one fn don't collide.
                TokenKind::Question => {
                    let q_span = self.peek().span;
                    self.bump();
                    let span = expr.span.to(q_span);
                    let line = q_span.line;
                    let col = q_span.col;
                    let tmp: Symbol =
                        format!("__try_{line}_{col}").as_str().into();
                    let ok_v: Symbol =
                        format!("__try_v_{line}_{col}").as_str().into();
                    let err_e: Symbol =
                        format!("__try_e_{line}_{col}").as_str().into();
                    let ok_body = Expr::new(ExprKind::Var(ok_v), span);
                    let err_payload = Expr::new(
                        ExprKind::EnumCtor {
                            enum_name: Symbol::intern("Result"),
                            variant: Symbol::intern("err"),
                            args: ilang_ast::CtorArgs::Tuple(Box::new([
                                Expr::new(ExprKind::Var(err_e), span),
                            ])),
                        },
                        span,
                    );
                    let err_body = Expr::new(
                        ExprKind::Return(Some(Box::new(err_payload))),
                        span,
                    );
                    let arms = vec![
                        ilang_ast::MatchArm {
                            pattern: ilang_ast::Pattern {
                                kind: ilang_ast::PatternKind::Variant {
                                    enum_name: None,
                                    variant: Symbol::intern("ok"),
                                    bindings: ilang_ast::PatternBindings::Tuple(
                                        Box::new([ok_v]),
                                    ),
                                },
                                span,
                            },
                            body: ok_body,
                            span,
                        },
                        ilang_ast::MatchArm {
                            pattern: ilang_ast::Pattern {
                                kind: ilang_ast::PatternKind::Variant {
                                    enum_name: None,
                                    variant: Symbol::intern("err"),
                                    bindings: ilang_ast::PatternBindings::Tuple(
                                        Box::new([err_e]),
                                    ),
                                },
                                span,
                            },
                            body: err_body,
                            span,
                        },
                    ];
                    let match_expr = Expr::new(
                        ExprKind::Match {
                            scrutinee: Box::new(Expr::new(
                                ExprKind::Var(tmp),
                                span,
                            )),
                            arms: arms.into_boxed_slice(),
                        },
                        span,
                    );
                    let block = ilang_ast::Block {
                        stmts: vec![ilang_ast::Stmt {
                            kind: ilang_ast::StmtKind::Let {
                                is_pub: false,
                                is_const: false,
                                name: tmp,
                                ty: None,
                                value: expr,
                            },
                            span,
                            source_module: None,
                        }],
                        tail: Some(Box::new(match_expr)),
                    };
                    expr = Expr::new(ExprKind::Block(block), span);
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    /// One iteration of `.field` / `.method(args)`. Caller's loop drives
    /// repetition so dot and index chains can interleave (`a[0].x`).
    pub(in crate::expr) fn parse_dot_postfix(&mut self, mut expr: Expr) -> Result<Expr, ParseError> {
        while matches!(self.peek().kind, TokenKind::Dot) {
            self.bump();
            let name_span = self.peek().span;
            let name = self.expect_member_name("field or method name")?;
            if matches!(self.peek().kind, TokenKind::LParen) {
                self.bump();
                let mut args = Vec::with_capacity(4);
                if !matches!(self.peek().kind, TokenKind::RParen) {
                    loop {
                        args.push(self.parse_expr(0)?);
                        if matches!(self.peek().kind, TokenKind::Comma) {
                            self.bump();
                        } else {
                            break;
                        }
                    }
                }
                let close_span = self.peek().span;
                self.expect(&TokenKind::RParen, "')'")?;
                let span = expr.span.to(close_span);
                expr = Expr::new(
                    ExprKind::MethodCall {
                        obj: Box::new(expr),
                        method: name,
                        args: args.into(),
                    },
                    span,
                );
            } else if matches!(self.peek().kind, TokenKind::LBrace)
                && matches!(
                    self.peek_n(1).map(|t| &t.kind),
                    Some(TokenKind::Ident(_))
                )
                && matches!(
                    self.peek_n(2).map(|t| &t.kind),
                    Some(TokenKind::Colon)
                )
                && matches!(&expr.kind, ExprKind::Var(_))
            {
                // `EnumName.Variant { field: value, ... }` —
                // struct-payload enum constructor. Lookahead
                // `{ Ident :` distinguishes from a stray block.
                // Only accepted when the receiver is a bare Var
                // (i.e. `EnumName`); chained access like `a.b.c { ... }`
                // is not enum-ctor.
                self.bump();
                let mut fs = Vec::with_capacity(4);
                while !matches!(self.peek().kind, TokenKind::RBrace) {
                    let fname = self.expect_ident("field name")?;
                    self.expect(&TokenKind::Colon, "':'")?;
                    let fval = self.parse_expr(0)?;
                    fs.push((fname, fval));
                    if matches!(self.peek().kind, TokenKind::Comma) {
                        self.bump();
                    } else if !matches!(self.peek().kind, TokenKind::RBrace)
                        && !self.peek().leading_newline
                    {
                        let p = self.peek();
                        return Err(ParseError::Unexpected {
                            found: p.kind.clone(),
                            expected: "',' or newline between fields".into(),
                            span: p.span,
                        });
                    }
                }
                let close_span = self.peek().span;
                self.expect(&TokenKind::RBrace, "'}'")?;
                let span = expr.span.to(close_span);
                let enum_name = match expr.kind {
                    ExprKind::Var(n) => n,
                    _ => unreachable!(),
                };
                expr = Expr::new(
                    ExprKind::EnumCtor {
                        enum_name,
                        variant: name,
                        args: ilang_ast::CtorArgs::Struct(fs.into()),
                    },
                    span,
                );
            } else {
                let span = expr.span.to(name_span);
                expr = Expr::new(
                    ExprKind::Field {
                        obj: Box::new(expr),
                        name,
                    },
                    span,
                );
            }
        }
        Ok(expr)
    }

    /// Parse a comma-separated `expr, expr, ...)` list. The opening
    /// `(` must have been consumed; the closing `)` is consumed
    /// here. Trailing comma is allowed (matches the rest of the
    /// language's punctuation flexibility).
    pub(in crate::expr) fn parse_call_args(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut args = Vec::with_capacity(4);
        if !matches!(self.peek().kind, TokenKind::RParen) {
            loop {
                args.push(self.parse_expr(0)?);
                if matches!(self.peek().kind, TokenKind::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RParen, "')'")?;
        Ok(args)
    }
}
