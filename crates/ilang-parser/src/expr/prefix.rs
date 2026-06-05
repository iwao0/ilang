//! Prefix-position expression parsing — literals, unary operators,
//! and the various compound forms (`new`, `super`, `some`, template
//! literals, paren / tuple / brace / array openers). Control-flow
//! starters (`if`, `for`, `while`, `match`, etc.) dispatch through
//! here but live in `control.rs`.

use ilang_ast::{Expr, ExprKind, Span, Symbol, UnOp};
use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;
use crate::stmt::parse_block;

use super::wrap_numeric_suffix;

impl<'a> Parser<'a> {
    pub(in crate::expr) fn parse_prefix(&mut self) -> Result<Expr, ParseError> {
        let t = self.peek().clone();
        let span = t.span;
        match t.kind {
            // Literals
            TokenKind::Int(n) => {
                let suffix = t.numeric_suffix.clone();
                self.bump();
                Ok(wrap_numeric_suffix(Expr::new(ExprKind::Int(n), span), suffix, span))
            }
            TokenKind::Float(f) => {
                let suffix = t.numeric_suffix.clone();
                self.bump();
                Ok(wrap_numeric_suffix(Expr::new(ExprKind::Float(f), span), suffix, span))
            }
            TokenKind::Str(s) => { self.bump(); Ok(Expr::new(ExprKind::Str(s), span)) }
            TokenKind::TmplStart => self.parse_template_literal(span),
            TokenKind::True => { self.bump(); Ok(Expr::new(ExprKind::Bool(true), span)) }
            TokenKind::False => { self.bump(); Ok(Expr::new(ExprKind::Bool(false), span)) }
            TokenKind::None_ => { self.bump(); Ok(Expr::new(ExprKind::None, span)) }
            TokenKind::This => { self.bump(); Ok(Expr::new(ExprKind::This, span)) }
            TokenKind::Continue => { self.bump(); Ok(Expr::new(ExprKind::Continue, span)) }

            // Compound prefix expressions delegated to per-form helpers.
            TokenKind::Super => self.parse_super_call(span),
            TokenKind::New => self.parse_new_expr(span),
            TokenKind::Some_ => self.parse_some_expr(span),
            TokenKind::Match => self.parse_match_expr(span),
            TokenKind::LParen => self.parse_paren_or_tuple(span),
            TokenKind::LBrace => self.parse_brace_prefix(span),
            TokenKind::LBracket => self.parse_array_literal(span),

            // Control-flow expression starts handled in their own modules.
            TokenKind::If => self.parse_if(),
            TokenKind::Fn => self.parse_fn_expr(),
            TokenKind::While => self.parse_while(),
            TokenKind::Loop => self.parse_loop(),
            TokenKind::For => self.parse_for(),

            // `await expr` — prefix operator. Binds tighter than the
            // top-level binops; takes the next unary expression so
            // `await p + 1` parses as `(await p) + 1`. The desugar
            // pass turns it into a `.then` continuation.
            TokenKind::Await => {
                self.bump();
                // Parse at the same precedence as `!` so chaining
                // `await await p` works and method-call postfix
                // (`.then(...)`) on the awaited value still binds.
                let inner = self.parse_expr(30)?;
                let full = span.to(inner.span);
                Ok(Expr::new(ExprKind::Await(Box::new(inner)), full))
            }

            // `break` / `return` share the optional-operand heuristic.
            TokenKind::Break => {
                self.bump();
                let value = self.parse_optional_operand()?;
                Ok(Expr::new(ExprKind::Break(value), span))
            }
            TokenKind::Return => {
                self.bump();
                let value = self.parse_optional_operand()?;
                Ok(Expr::new(ExprKind::Return(value), span))
            }

            // Unary prefix operators. `-` is special-cased to fold
            // `-<IntLit>` into a single `Int` (see parse_unary_minus).
            TokenKind::Minus => self.parse_unary_minus(span),
            TokenKind::Plus => self.parse_unary_op(UnOp::Pos, span),
            TokenKind::Bang => self.parse_unary_op(UnOp::Not, span),
            TokenKind::Tilde => self.parse_unary_op(UnOp::BitNot, span),
            // `&local` — address-of (FFI). Only allowed inside an
            // `@extern(C)` context; the type checker enforces that.
            // Tokenised as `Amp`, which doubles as the binary
            // bitwise-AND operator in infix position — the Pratt
            // parser keeps the two disambiguated by context.
            TokenKind::Amp => self.parse_unary_op(UnOp::AddrOf, span),

            TokenKind::Ident(name) => {
                self.bump();
                if matches!(self.peek().kind, TokenKind::LParen) {
                    self.bump();
                    let args = self.parse_call_args()?;
                    Ok(Expr::new(ExprKind::Call { callee: name.into(), args: args.into() }, span))
                } else {
                    Ok(Expr::new(ExprKind::Var(name.into()), span))
                }
            }

            other => Err(ParseError::Unexpected {
                found: other,
                expected: "number, identifier, '-', '+' or '('".into(),
                span: t.span,
            }),
        }
    }

    /// `super.method(args)` → `SuperCall { method: Some, .. }`,
    /// `super(args)` → `SuperCall { method: None, .. }`.
    pub(in crate::expr) fn parse_super_call(&mut self, span: Span) -> Result<Expr, ParseError> {
        self.bump();
        let (method, args) = match self.peek().kind {
            TokenKind::Dot => {
                self.bump();
                let m = self.expect_ident("method name after `super.`")?;
                self.expect(&TokenKind::LParen, "'('")?;
                let args = self.parse_call_args()?;
                (Some(m), args)
            }
            TokenKind::LParen => {
                self.bump();
                let args = self.parse_call_args()?;
                (None, args)
            }
            _ => {
                let t = self.peek();
                return Err(ParseError::Unexpected {
                    found: t.kind.clone(),
                    expected: "'.' (super.method) or '(' (super(args))".into(),
                    span: t.span,
                });
            }
        };
        Ok(Expr::new(ExprKind::SuperCall { method, args: args.into() }, span))
    }

    /// `new Cls<T, U>(args)`. The class name may be dotted
    /// (`module.Cls`); the optional type-argument list is unambiguous
    /// after the class name because `<` can never start an expression
    /// here.
    pub(in crate::expr) fn parse_new_expr(&mut self, span: Span) -> Result<Expr, ParseError> {
        self.bump();
        let mut class_str = self.expect_ident("class name")?.as_str().to_string();
        while matches!(self.peek().kind, TokenKind::Dot) {
            self.bump();
            let part = self.expect_ident("class name segment")?;
            class_str.push('.');
            class_str.push_str(part.as_str());
        }
        let class: Symbol = class_str.into();
        let type_args = if matches!(self.peek().kind, TokenKind::Lt) {
            self.parse_type_args()?
        } else {
            Vec::new()
        };
        self.expect(&TokenKind::LParen, "'('")?;
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
        Ok(Expr::new(
            ExprKind::New { class, type_args: type_args.into(), args: args.into(), init_method: None },
            span,
        ))
    }

    /// `some(expr)`.
    pub(in crate::expr) fn parse_some_expr(&mut self, span: Span) -> Result<Expr, ParseError> {
        self.bump();
        self.expect(&TokenKind::LParen, "'('")?;
        let inner = self.parse_expr(0)?;
        self.expect(&TokenKind::RParen, "')'")?;
        Ok(Expr::new(ExprKind::Some(Box::new(inner)), span))
    }

    /// Backtick-quoted template literal. The lexer emits an
    /// alternating stream of `TmplLit(text)` chunks and
    /// `TmplExprStart expr ... TmplExprEnd` runs between the opening
    /// `TmplStart` (already at `peek()` when this is called) and the
    /// closing `TmplEnd`. We stitch them into the `Template { parts }`
    /// shape the rest of the pipeline expects.
    pub(in crate::expr) fn parse_template_literal(&mut self, span: Span) -> Result<Expr, ParseError> {
        self.bump(); // TmplStart
        let mut parts: Vec<ilang_ast::TemplatePart> = Vec::new();
        loop {
            let tok = self.peek().clone();
            match tok.kind {
                TokenKind::TmplLit(s) => {
                    self.bump();
                    parts.push(ilang_ast::TemplatePart::Str(s));
                }
                TokenKind::TmplExprStart => {
                    self.bump();
                    let inner = self.parse_expr(0)?;
                    let end_tok = self.peek().clone();
                    match end_tok.kind {
                        TokenKind::TmplExprEnd => {
                            self.bump();
                        }
                        _ => {
                            return Err(ParseError::Unexpected {
                                found: end_tok.kind,
                                expected: "'}' closing template interpolation".into(),
                                span: end_tok.span,
                            });
                        }
                    }
                    parts.push(ilang_ast::TemplatePart::Expr(inner));
                }
                TokenKind::TmplEnd => {
                    self.bump();
                    let full_span = span.to(tok.span);
                    return Ok(Expr::new(
                        ExprKind::Template { parts: parts.into() },
                        full_span,
                    ));
                }
                _ => {
                    return Err(ParseError::Unexpected {
                        found: tok.kind,
                        expected: "template literal chunk or '${' / '`'".into(),
                        span: tok.span,
                    });
                }
            }
        }
    }

    /// `(e)` — parenthesised expression — or `(a, b, ...)` — tuple.
    /// A trailing comma is accepted inside the tuple form.
    pub(in crate::expr) fn parse_paren_or_tuple(&mut self, span: Span) -> Result<Expr, ParseError> {
        self.bump();
        let first = self.parse_expr(0)?;
        if matches!(self.peek().kind, TokenKind::Comma) {
            let mut elems = vec![first];
            while matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
                if matches!(self.peek().kind, TokenKind::RParen) {
                    break;
                }
                elems.push(self.parse_expr(0)?);
            }
            let close_span = self.peek().span;
            self.expect(&TokenKind::RParen, "')'")?;
            return Ok(Expr::new(ExprKind::Tuple(elems.into()), span.to(close_span)));
        }
        self.expect(&TokenKind::RParen, "')'")?;
        Ok(first)
    }

    /// `{ ... }` in expression position. Disambiguates map literal vs.
    /// block: a `{` followed by a key token (string / int / bool /
    /// `-Int`) and then `:` is a map literal; otherwise it's a block.
    /// The tokens that can start a key never form a valid statement
    /// followed by `:`, so this rule has no false positives.
    pub(in crate::expr) fn parse_brace_prefix(&mut self, span: Span) -> Result<Expr, ParseError> {
        let neg_int_key = matches!(
            self.peek_n(1).map(|t| &t.kind),
            Some(TokenKind::Minus)
        ) && matches!(
            self.peek_n(2).map(|t| &t.kind),
            Some(TokenKind::Int(_))
        ) && matches!(
            self.peek_n(3).map(|t| &t.kind),
            Some(TokenKind::Colon)
        );
        let positive_key = matches!(
            self.peek_n(1).map(|t| &t.kind),
            Some(TokenKind::Str(_) | TokenKind::Int(_) | TokenKind::True | TokenKind::False)
        ) && matches!(
            self.peek_n(2).map(|t| &t.kind),
            Some(TokenKind::Colon)
        );
        if positive_key || neg_int_key {
            self.parse_map_literal(span)
        } else {
            let block = parse_block(self)?;
            Ok(Expr::new(ExprKind::Block(block), span))
        }
    }

    /// `[a, b, ...]` (trailing comma allowed) or `[expr; N]` repeat
    /// form, which the parser expands eagerly into N clones of `expr`
    /// so downstream stages only see the plain list form.
    pub(in crate::expr) fn parse_array_literal(&mut self, span: Span) -> Result<Expr, ParseError> {
        self.bump();
        if matches!(self.peek().kind, TokenKind::RBracket) {
            self.bump();
            return Ok(Expr::new(ExprKind::Array(Box::new([])), span));
        }
        let first = self.parse_expr(0)?;
        if matches!(self.peek().kind, TokenKind::Semicolon) {
            self.bump();
            let count_tok = self.peek().clone();
            let count = match count_tok.kind {
                TokenKind::Int(n) if n >= 0 => {
                    self.bump();
                    n as usize
                }
                _ => {
                    return Err(ParseError::Unexpected {
                        span: count_tok.span,
                        expected: "non-negative integer literal repeat count".into(),
                        found: count_tok.kind,
                    });
                }
            };
            self.expect(&TokenKind::RBracket, "']'")?;
            let mut elements = Vec::with_capacity(count);
            for _ in 0..count {
                elements.push(first.clone());
            }
            return Ok(Expr::new(ExprKind::Array(elements.into()), span));
        }
        let mut elements = Vec::with_capacity(4);
        elements.push(first);
        while matches!(self.peek().kind, TokenKind::Comma) {
            self.bump();
            if matches!(self.peek().kind, TokenKind::RBracket) {
                break;
            }
            elements.push(self.parse_expr(0)?);
        }
        self.expect(&TokenKind::RBracket, "']'")?;
        Ok(Expr::new(ExprKind::Array(elements.into()), span))
    }

    /// Shared body for the trivial unary prefix operators (`!`, `~`,
    /// `&`, `+`). `-` has its own helper for literal-folding.
    pub(in crate::expr) fn parse_unary_op(&mut self, op: UnOp, span: Span) -> Result<Expr, ParseError> {
        self.bump();
        let e = self.parse_expr(30)?;
        let full = span.to(e.span);
        Ok(Expr::new(ExprKind::Unary { op, expr: Box::new(e) }, full))
    }

    /// Prefix `-`. Folds `-<IntLit>` into a single `Int` so that the
    /// minimum signed values (`i64::MIN`, `i32::MIN`, ...) are writable
    /// as `-N`. The suffixed form (`-128_i8`) shows up as
    /// `Cast{Int(n), ty}`, so peel that wrapper too.
    pub(in crate::expr) fn parse_unary_minus(&mut self, span: Span) -> Result<Expr, ParseError> {
        self.bump();
        let e = self.parse_expr(30)?;
        let full = span.to(e.span);
        if let ExprKind::Int(n) = e.kind {
            return Ok(Expr::new(ExprKind::Int(n.wrapping_neg()), full));
        }
        if let ExprKind::Cast { expr: inner, ty } = &e.kind {
            if let ExprKind::Int(n) = inner.kind {
                let neg = Expr::new(ExprKind::Int(n.wrapping_neg()), inner.span);
                return Ok(Expr::new(
                    ExprKind::Cast { expr: Box::new(neg), ty: ty.clone() },
                    full,
                ));
            }
        }
        Ok(Expr::new(ExprKind::Unary { op: UnOp::Neg, expr: Box::new(e) }, full))
    }

    /// Operand-presence heuristic shared by `break` and `return`.
    /// The operand is omitted when the next token is a statement
    /// terminator (`;`, `}`, EOF) or starts a new logical line (ASI).
    pub(in crate::expr) fn parse_optional_operand(&mut self) -> Result<Option<Box<Expr>>, ParseError> {
        let next = self.peek();
        let no_value = matches!(
            next.kind,
            TokenKind::Semicolon | TokenKind::RBrace | TokenKind::Eof
        ) || next.leading_newline;
        if no_value {
            Ok(None)
        } else {
            Ok(Some(Box::new(self.parse_expr(0)?)))
        }
    }
}
