//! Control-flow and structured expression parsing — `if` / `elif`
//! / `if let` / `while` / `loop` / `for` / `fn` expressions / `match`
//! arms / map literals. All of these dispatch through `parse_prefix`
//! based on the leading keyword.

use ilang_ast::{Expr, ExprKind, Span};
use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;
use crate::stmt::parse_block;

impl<'a> Parser<'a> {
    /// `match scrutinee { pattern { body } [,|newline] ... }`.
    /// Arm bodies are brace-delimited blocks (no `=>`).
    pub(in crate::expr) fn parse_match_expr(&mut self, span: Span) -> Result<Expr, ParseError> {
        self.bump();
        let scrutinee = self.parse_expr(0)?;
        self.expect(&TokenKind::LBrace, "'{'")?;
        let mut arms = Vec::with_capacity(4);
        while !matches!(self.peek().kind, TokenKind::RBrace) {
            let arm_span = self.peek().span;
            let pattern = self.parse_pattern_in_arm()?;
            let body_span = self.peek().span;
            let body_block = parse_block(self)?;
            let body = Expr::new(ExprKind::Block(body_block), body_span);
            arms.push(ilang_ast::MatchArm { pattern, body, span: arm_span });
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
            } else if !matches!(self.peek().kind, TokenKind::RBrace)
                && !self.peek().leading_newline
            {
                let p = self.peek();
                return Err(ParseError::Unexpected {
                    found: p.kind.clone(),
                    expected: "',' or newline between match arms".into(),
                    span: p.span,
                });
            }
        }
        self.expect(&TokenKind::RBrace, "'}'")?;
        Ok(Expr::new(
            ExprKind::Match { scrutinee: Box::new(scrutinee), arms: arms.into() },
            span,
        ))
    }

    /// Continue an `if` chain at the `elif` keyword. Produces an
    /// `If` expression whose else branch is itself another `If` if
    /// further `elif` follows.
    pub(in crate::expr) fn parse_elif_chain(&mut self) -> Result<Expr, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Elif, "'elif'")?;
        let cond = self.parse_expr(0)?;
        let then_branch = parse_block(self)?;
        let else_branch = match self.peek().kind {
            TokenKind::Elif => Some(Box::new(self.parse_elif_chain()?)),
            TokenKind::Else => {
                self.bump();
                if matches!(self.peek().kind, TokenKind::If) {
                    let p = self.peek();
                    return Err(ParseError::Unexpected {
                        found: p.kind.clone(),
                        expected: "'elif' (use `elif` for chained conditions, not `else if`)"
                            .into(),
                        span: p.span,
                    });
                }
                let block_span = self.peek().span;
                let block = parse_block(self)?;
                Some(Box::new(Expr::new(ExprKind::Block(block), block_span)))
            }
            _ => None,
        };
        Ok(Expr::new(
            ExprKind::If {
                cond: Box::new(cond),
                then_branch,
                else_branch,
            },
            span,
        ))
    }

    pub(in crate::expr) fn parse_if(&mut self) -> Result<Expr, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::If, "'if'")?;
        // `if let some(name) = expr { ... } else { ... }` — the only
        // pattern form supported (so far). Anything else after `if let`
        // is a syntax error to avoid promising more pattern matching
        // than is implemented.
        if matches!(self.peek().kind, TokenKind::Let) {
            return self.parse_if_let(span);
        }
        let cond = self.parse_expr(0)?;
        let then_branch = parse_block(self)?;
        // `elif cond { ... }` chains as the else branch (a nested if).
        // `else if` is rejected with a hint pointing to `elif`.
        let else_branch = match self.peek().kind {
            TokenKind::Elif => {
                let inner = self.parse_elif_chain()?;
                Some(Box::new(inner))
            }
            TokenKind::Else => {
                self.bump();
                if matches!(self.peek().kind, TokenKind::If) {
                    let p = self.peek();
                    return Err(ParseError::Unexpected {
                        found: p.kind.clone(),
                        expected: "'elif' (use `elif` for chained conditions, not `else if`)"
                            .into(),
                        span: p.span,
                    });
                }
                let block_span = self.peek().span;
                let block = parse_block(self)?;
                Some(Box::new(Expr::new(ExprKind::Block(block), block_span)))
            }
            _ => None,
        };
        Ok(Expr::new(
            ExprKind::If {
                cond: Box::new(cond),
                then_branch,
                else_branch,
            },
            span,
        ))
    }

    pub(in crate::expr) fn parse_if_let(&mut self, span: ilang_ast::Span) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::Let, "'let'")?;
        self.expect(&TokenKind::Some_, "'some' (only pattern supported)")?;
        self.expect(&TokenKind::LParen, "'('")?;
        let name = self.expect_ident("variable name")?;
        self.expect(&TokenKind::RParen, "')'")?;
        self.expect(&TokenKind::Equals, "'='")?;
        let scrut = self.parse_expr(0)?;
        let then_branch = parse_block(self)?;
        let else_branch = match self.peek().kind {
            TokenKind::Elif => {
                let inner = self.parse_elif_chain()?;
                Some(Box::new(inner))
            }
            TokenKind::Else => {
                self.bump();
                if matches!(self.peek().kind, TokenKind::If) {
                    let p = self.peek();
                    return Err(ParseError::Unexpected {
                        found: p.kind.clone(),
                        expected: "'elif' (use `elif` for chained conditions, not `else if`)"
                            .into(),
                        span: p.span,
                    });
                }
                let block_span = self.peek().span;
                let block = parse_block(self)?;
                Some(Box::new(Expr::new(ExprKind::Block(block), block_span)))
            }
            _ => None,
        };
        Ok(Expr::new(
            ExprKind::IfLet {
                name,
                expr: Box::new(scrut),
                then_branch,
                else_branch,
            },
            span,
        ))
    }

    pub(in crate::expr) fn parse_while(&mut self) -> Result<Expr, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::While, "'while'")?;
        let cond = self.parse_expr(0)?;
        let body = parse_block(self)?;
        Ok(Expr::new(
            ExprKind::While {
                cond: Box::new(cond),
                body,
            },
            span,
        ))
    }

    pub(in crate::expr) fn parse_loop(&mut self) -> Result<Expr, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Loop, "'loop'")?;
        let body = parse_block(self)?;
        Ok(Expr::new(ExprKind::Loop { body }, span))
    }

    /// Map literal: `{ key1: value1, key2: value2, ... }`. Trailing
    /// comma allowed. Empty maps `{}` are not produced here — that
    /// path is handled by the block-vs-map lookahead above (an empty
    /// `{}` parses as a unit-block). An empty map is written either as
    /// `new Map<K, V>()`, or as `{}` in a position with a `Map<K, V>`
    /// type annotation: the type checker reads an empty block against a
    /// Map target as the empty-map shorthand (see `empty_block_as_map`).
    pub(in crate::expr) fn parse_map_literal(&mut self, span: ilang_ast::Span) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::LBrace, "'{'")?;
        let mut entries = Vec::with_capacity(4);
        while !matches!(self.peek().kind, TokenKind::RBrace) {
            let key = self.parse_expr(0)?;
            self.expect(&TokenKind::Colon, "':'")?;
            let value = self.parse_expr(0)?;
            entries.push((key, value));
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        let close_span = self.peek().span;
        self.expect(&TokenKind::RBrace, "'}'")?;
        Ok(Expr::new(ExprKind::MapLit(entries.into()), span.to(close_span)))
    }

    /// Anonymous function expression: `fn(p: T, ...): R { body }`. The
    /// shape mirrors `fn name(...) { ... }` minus the name.
    pub(in crate::expr) fn parse_fn_expr(&mut self) -> Result<Expr, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Fn, "'fn'")?;
        self.expect(&TokenKind::LParen, "'('")?;
        let params = self.parse_param_list()?;
        self.expect(&TokenKind::RParen, "')'")?;
        let ret = if matches!(self.peek().kind, TokenKind::Colon) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = crate::stmt::parse_block(self)?;
        let close_span = self.prev_span();
        Ok(Expr::new(
            ExprKind::FnExpr { params: params.into(), ret, body },
            span.to(close_span),
        ))
    }

    pub(in crate::expr) fn parse_for(&mut self) -> Result<Expr, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::For, "'for'")?;
        let var_tok = self.bump().clone();
        let var = match &var_tok.kind {
            TokenKind::Ident(n) => n.clone(),
            _ => {
                return Err(ParseError::Unexpected {
                    found: var_tok.kind.clone(),
                    expected: "identifier after 'for'".into(),
                    span: var_tok.span,
                });
            }
        };
        self.expect(&TokenKind::In, "'in'")?;
        let iter = self.parse_expr(0)?;
        let body = parse_block(self)?;
        let close_span = self.prev_span();
        Ok(Expr::new(
            ExprKind::ForIn {
                var: var.into(),
                iter: Box::new(iter),
                body,
            },
            span.to(close_span),
        ))
    }
}
