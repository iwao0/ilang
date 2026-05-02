mod error;
mod expr;
mod item;
pub mod loader;
mod normalize;
mod parser;
mod stmt;

use ilang_ast::{Expr, Program, Stmt, StmtKind};
use ilang_lexer::{Token, TokenKind};

pub use error::ParseError;

use parser::{ExprEnd, Parser};
use stmt::parse_let_stmt;

pub fn parse(tokens: &[Token]) -> Result<Program, ParseError> {
    let mut p = Parser { tokens, pos: 0, pending_close_gt: 0 };
    let prog = parse_program(&mut p)?;
    Ok(normalize::normalize(prog))
}

/// Parse a single expression — used by tests that want to inspect expression
/// trees directly without wrapping in a program.
pub fn parse_expr_only(tokens: &[Token]) -> Result<Expr, ParseError> {
    let mut p = Parser { tokens, pos: 0, pending_close_gt: 0 };
    let e = p.parse_expr(0)?;
    if !matches!(p.peek().kind, TokenKind::Eof) {
        let t = p.peek();
        return Err(ParseError::Unexpected {
            found: t.kind.clone(),
            expected: "end of input".into(),
            span: t.span,
        });
    }
    Ok(e)
}

fn parse_program(p: &mut Parser) -> Result<Program, ParseError> {
    let mut prog = Program::default();
    loop {
        match &p.peek().kind {
            TokenKind::Eof => break,
            TokenKind::At
            | TokenKind::Fn
            | TokenKind::Class
            | TokenKind::Enum
            | TokenKind::Use => {
                let item = p.parse_item()?;
                prog.items.push(item);
            }
            TokenKind::Let => {
                let s = parse_let_stmt(p)?;
                prog.stmts.push(s);
            }
            _ => {
                let e = p.parse_expr(0)?;
                let span = e.span;
                match p.classify_expr_end(&e, TokenKind::Eof)? {
                    ExprEnd::Stmt => {
                        prog.stmts.push(Stmt::new(StmtKind::Expr(e), span));
                    }
                    ExprEnd::Tail => {
                        prog.tail = Some(e);
                        break;
                    }
                }
            }
        }
    }
    Ok(prog)
}
