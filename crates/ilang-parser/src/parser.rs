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
    )
}

impl<'a> Parser<'a> {
    pub(crate) fn peek(&self) -> &'a Token {
        &self.tokens[self.pos]
    }

    /// Lookahead to the n-th token from the current position. Returns
    /// `None` if we'd run past the EOF sentinel.
    pub(crate) fn peek_n(&self, n: usize) -> Option<&'a Token> {
        self.tokens.get(self.pos + n)
    }

    pub(crate) fn bump(&mut self) -> &'a Token {
        let t = &self.tokens[self.pos];
        if !matches!(t.kind, TokenKind::Eof) {
            self.pos += 1;
        }
        t
    }

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

    pub(crate) fn expect_ident(&mut self, label: &str) -> Result<String, ParseError> {
        let t = self.peek().clone();
        if let TokenKind::Ident(n) = t.kind {
            self.bump();
            Ok(n)
        } else {
            Err(ParseError::Unexpected {
                found: t.kind,
                expected: label.into(),
                span: t.span,
            })
        }
    }

    /// After parsing an expression in statement-position, decide whether it
    /// becomes a statement (followed by `;`, JS-style implicit terminator
    /// from a leading newline on the next token, or block-like form) or the
    /// trailing expression (at end of program/block).
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
