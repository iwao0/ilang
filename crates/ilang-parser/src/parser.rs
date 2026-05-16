use ilang_ast::{Expr, ExprKind};
use ilang_lexer::{Token, TokenKind};

use crate::error::ParseError;

pub(crate) struct Parser<'a> {
    pub(crate) tokens: &'a [Token],
    pub(crate) pos: usize,
    /// When closing nested generics like `Box<Box<i64>>`, the lexer
    /// hands us a single `>>` token. The inner generic consumes one
    /// "virtual" `>` by leaving this counter at 1; the outer's close
    /// then decrements it instead of bumping `pos`.
    pub(crate) pending_close_gt: u32,
}

pub(crate) enum ExprEnd {
    Stmt,
    Tail,
}

pub(crate) fn is_block_like(e: &Expr) -> bool {
    matches!(
        e.kind,
        ExprKind::Block(_)
            | ExprKind::If { .. }
            | ExprKind::While { .. }
            | ExprKind::Loop { .. }
            | ExprKind::ForIn { .. }
            | ExprKind::Match { .. }
            | ExprKind::IfLet { .. }
    )
}

impl<'a> Parser<'a> {
    #[inline]
    pub(crate) fn peek(&self) -> &'a Token {
        &self.tokens[self.pos]
    }

    /// Span of the most recently consumed token. Used by callers that
    /// want to widen a compound expression's span to where the parsing
    /// helper finished (e.g. the `}` consumed inside `parse_block`).
    pub(crate) fn prev_span(&self) -> ilang_ast::Span {
        debug_assert!(self.pos > 0, "prev_span called before any token was consumed");
        self.tokens[self.pos - 1].span
    }

    /// Lookahead to the n-th token from the current position. Returns
    /// `None` if we'd run past the EOF sentinel.
    pub(crate) fn peek_n(&self, n: usize) -> Option<&'a Token> {
        self.tokens.get(self.pos + n)
    }

    #[inline]
    pub(crate) fn bump(&mut self) -> &'a Token {
        let t = &self.tokens[self.pos];
        if !matches!(t.kind, TokenKind::Eof) {
            self.pos += 1;
        }
        t
    }

    /// The non-payload variants of `TokenKind` (the vast majority of
    /// `expect` calls target these) are unit-like, so a single byte
    /// discriminant comparison is enough. `#[inline]` lets LLVM fold
    /// the discriminant lookup into a direct tag compare at each call
    /// site, which is what a literal `matches!(t.kind, TokenKind::Foo)`
    /// would compile to but without needing one bespoke wrapper per
    /// variant.
    #[inline]
    pub(crate) fn expect(
        &mut self,
        expected: &TokenKind,
        label: &str,
    ) -> Result<(), ParseError> {
        let t = self.peek();
        if std::mem::discriminant(&t.kind) == std::mem::discriminant(expected) {
            self.bump();
            Ok(())
        } else {
            Err(ParseError::Unexpected {
                found: t.kind.clone(),
                expected: label.into(),
                span: t.span,
            })
        }
    }

    pub(crate) fn expect_ident(&mut self, label: &str) -> Result<ilang_ast::Symbol, ParseError> {
        // Peek by reference so the Token (and its String payload) is not
        // cloned on the happy path; only the identifier's own String is
        // cloned to be moved into the returned Symbol.
        let t = self.peek();
        if let TokenKind::Ident(n) = &t.kind {
            let s = n.clone();
            self.bump();
            Ok(s.into())
        } else {
            Err(ParseError::Unexpected {
                found: t.kind.clone(),
                expected: label.into(),
                span: t.span,
            })
        }
    }

    /// Like `expect_ident` but additionally accepts a fixed set of
    /// keyword tokens as if they were identifiers. Used in
    /// member-access / enum-variant positions so the user can name a
    /// variant after a C constant like `SDL_HINT_OVERRIDE`.
    ///
    /// The promoted set is the keywords most likely to appear as C
    /// enum members: `class`, `enum`, `fn`, `none`, `override`,
    /// `true`, `false`, `some`, `as`, `in`, `super`, `this`,
    /// `return`. (`static` already lexes as an ident.)
    pub(crate) fn expect_member_name(
        &mut self,
        label: &str,
    ) -> Result<ilang_ast::Symbol, ParseError> {
        // Reference-only inspection so non-identifier branches don't pay
        // for a full Token clone (which would copy the kind's String
        // payload when one is present).
        let t = self.peek();
        let name: Option<&'static str> = match &t.kind {
            TokenKind::Ident(n) => {
                let s = n.clone();
                self.bump();
                return Ok(s.into());
            }
            TokenKind::Class => Some("class"),
            TokenKind::Enum => Some("enum"),
            TokenKind::Fn => Some("fn"),
            TokenKind::None_ => Some("none"),
            TokenKind::Override => Some("override"),
            TokenKind::True => Some("true"),
            TokenKind::False => Some("false"),
            TokenKind::Some_ => Some("some"),
            TokenKind::As => Some("as"),
            TokenKind::In => Some("in"),
            TokenKind::Super => Some("super"),
            TokenKind::This => Some("this"),
            TokenKind::Return => Some("return"),
            _ => None,
        };
        if let Some(s) = name {
            self.bump();
            Ok(s.into())
        } else {
            Err(ParseError::Unexpected {
                found: t.kind.clone(),
                expected: label.into(),
                span: t.span,
            })
        }
    }

    /// After parsing an expression in statement-position, decide whether it
    /// becomes a statement (followed by `;`, JS-style implicit terminator
    /// from a leading newline on the next token, or block-like form) or the
    /// trailing expression (at end of program/block).
    #[inline]
    pub(crate) fn classify_expr_end(
        &mut self,
        expr: &Expr,
        end: TokenKind,
    ) -> Result<ExprEnd, ParseError> {
        if matches!(self.peek().kind, TokenKind::Semicolon) {
            self.bump();
            return Ok(ExprEnd::Stmt);
        }
        if std::mem::discriminant(&self.peek().kind) == std::mem::discriminant(&end) {
            return Ok(ExprEnd::Tail);
        }
        if self.peek().leading_newline {
            return Ok(ExprEnd::Stmt);
        }
        if is_block_like(expr) {
            return Ok(ExprEnd::Stmt);
        }
        let t = self.peek();
        Err(ParseError::Unexpected {
            found: t.kind.clone(),
            expected: "';', newline, or end of block".into(),
            span: t.span,
        })
    }

    /// Consume a statement terminator after a non-expression statement (e.g.
    /// `let`). Accepts an explicit `;`, an implicit newline before the next
    /// token (JS-style ASI), or a closing `}` / EOF (block boundary).
    pub(crate) fn consume_stmt_terminator(&mut self) -> Result<(), ParseError> {
        if matches!(self.peek().kind, TokenKind::Semicolon) {
            self.bump();
            return Ok(());
        }
        if self.peek().leading_newline
            || matches!(self.peek().kind, TokenKind::RBrace | TokenKind::Eof)
        {
            return Ok(());
        }
        let t = self.peek();
        Err(ParseError::Unexpected {
            found: t.kind.clone(),
            expected: "';' or newline".into(),
            span: t.span,
        })
    }
}
