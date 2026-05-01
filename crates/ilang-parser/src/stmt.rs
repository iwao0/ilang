use ilang_ast::{Block, Stmt};
use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::{ExprEnd, Parser};

/// `parse_block` is a free function rather than a method so that
/// `expr.rs` can call it without a circular `impl` dependency.
pub(crate) fn parse_block(p: &mut Parser) -> Result<Block, ParseError> {
    p.expect(&TokenKind::LBrace, "'{'")?;
    let mut stmts = Vec::new();
    let mut tail = None;
    loop {
        match &p.peek().kind {
            TokenKind::RBrace => break,
            TokenKind::Let => {
                let s = parse_let_stmt(p)?;
                stmts.push(s);
            }
            _ => {
                let e = p.parse_expr(0)?;
                match p.classify_expr_end(&e, TokenKind::RBrace)? {
                    ExprEnd::Stmt => stmts.push(Stmt::Expr(e)),
                    ExprEnd::Tail => {
                        tail = Some(Box::new(e));
                        break;
                    }
                }
            }
        }
    }
    p.expect(&TokenKind::RBrace, "'}'")?;
    Ok(Block { stmts, tail })
}

pub(crate) fn parse_let_stmt(p: &mut Parser) -> Result<Stmt, ParseError> {
    p.expect(&TokenKind::Let, "'let'")?;
    let name = p.expect_ident("variable name")?;
    let ty = if matches!(p.peek().kind, TokenKind::Colon) {
        p.bump();
        Some(p.parse_type()?)
    } else {
        None
    };
    p.expect(&TokenKind::Equals, "'='")?;
    let value = p.parse_expr(0)?;
    p.expect(&TokenKind::Semicolon, "';'")?;
    Ok(Stmt::Let { name, ty, value })
}
