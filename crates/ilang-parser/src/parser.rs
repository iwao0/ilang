use ilang_ast::Expr;
use ilang_lexer::{Token, TokenKind};

use crate::error::ParseError;

pub(crate) struct Parser<'a> {
    pub(crate) tokens: &'a [Token],
    pub(crate) pos: usize,
}

pub(crate) enum ExprEnd {
    Stmt,
    Tail,
}

pub(crate) fn is_block_like(e: &Expr) -> bool {
    matches!(e, Expr::Block(_) | Expr::If { .. } | Expr::While { .. })
}

impl<'a> Parser<'a> {
    pub(crate) fn peek(&self) -> &'a Token {
        &self.tokens[self.pos]
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
                line: t.span.line,
                col: t.span.col,
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
                line: t.span.line,
                col: t.span.col,
            })
        }
    }

    /// After parsing an expression in statement-position, decide whether it
    /// becomes a statement (followed by `;`, or block-like and more tokens
    /// follow) or the trailing expression (at end of program/block).
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
        if is_block_like(expr) {
            return Ok(ExprEnd::Stmt);
        }
        let t = self.peek();
        Err(ParseError::Unexpected {
            found: t.kind.clone(),
            expected: "';' or end of block".into(),
            line: t.span.line,
            col: t.span.col,
        })
    }
}
