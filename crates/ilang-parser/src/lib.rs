mod error;
mod expr;
mod item;
pub mod loader;
mod normalize;
mod parser;
mod stmt;
mod visibility;

use ilang_ast::{Expr, Item, Program, Stmt, StmtKind};
use ilang_lexer::{Token, TokenKind};

pub use error::ParseError;

use parser::{ExprEnd, Parser};
use stmt::{parse_let_stmt, parse_top_level_let};

pub fn parse(tokens: &[Token]) -> Result<Program, ParseError> {
    let mut p = Parser { tokens, pos: 0, pending_close_gt: 0 };
    let prog = parse_program(&mut p)?;
    normalize::normalize(prog)
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
    let mut items: Vec<Item> = Vec::new();
    let mut stmts: Vec<Stmt> = Vec::new();
    let mut tail: Option<Expr> = None;
    loop {
        match &p.peek().kind {
            TokenKind::Eof => break,
            TokenKind::Pub
                if matches!(
                    p.tokens.get(p.pos + 1).map(|t| &t.kind),
                    Some(TokenKind::Let)
                ) =>
            {
                let s = parse_top_level_let(p)?;
                stmts.push(s);
            }
            TokenKind::At
            | TokenKind::Fn
            | TokenKind::Class
            | TokenKind::Interface
            | TokenKind::Enum
            | TokenKind::Use
            | TokenKind::Pub
            | TokenKind::Const => {
                let item = p.parse_item()?;
                items.push(item);
            }
            // Top-level `struct` / `union` (outside any `@extern(C) {}`
            // block). They share the C-layout / value-type semantics
            // of `@extern(C) struct`, but their fields are restricted
            // by a later validation pass — no `char` / `void` /
            // `size_t` / raw pointers.
            TokenKind::Ident(n) if n == "struct" || n == "union" => {
                let item = p.parse_item()?;
                items.push(item);
            }
            TokenKind::Let => {
                let s = parse_let_stmt(p)?;
                stmts.push(s);
            }
            _ => {
                let e = p.parse_expr(0)?;
                let span = e.span;
                match p.classify_expr_end(&e, TokenKind::Eof)? {
                    ExprEnd::Stmt => {
                        stmts.push(Stmt::new(StmtKind::Expr(e), span));
                    }
                    ExprEnd::Tail => {
                        tail = Some(e);
                        break;
                    }
                }
            }
        }
    }
    Ok(Program { items, stmts, tail })
}
