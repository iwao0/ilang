use ilang_ast::{Block, Stmt, StmtKind};
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
            TokenKind::Const => {
                let s = parse_local_const_stmt(p)?;
                stmts.push(s);
            }
            _ => {
                let e = p.parse_expr(0)?;
                let span = e.span;
                match p.classify_expr_end(&e, TokenKind::RBrace)? {
                    ExprEnd::Stmt => stmts.push(Stmt::new(StmtKind::Expr(e), span)),
                    ExprEnd::Tail => {
                        tail = Some(Box::new(e));
                        break;
                    }
                }
            }
        }
    }
    p.expect(&TokenKind::RBrace, "'}'")?;
    Ok(Block { stmts: stmts.into(), tail })
}

/// Top-level entrypoint that allows an optional `pub` prefix —
/// `pub let X = ...` exposes the binding as `module.X`. Inside fn /
/// class bodies, `parse_let_stmt` is called directly without `pub`.
pub(crate) fn parse_top_level_let(p: &mut Parser) -> Result<Stmt, ParseError> {
    let is_pub = if matches!(p.peek().kind, TokenKind::Pub) {
        p.bump();
        true
    } else {
        false
    };
    let mut s = parse_let_stmt(p)?;
    if is_pub {
        if let StmtKind::Let { is_pub: out,
                is_const: false, .. } = &mut s.kind {
            *out = true;
        } else {
            // `pub let (a, b) = ...` / `pub let X { ... } = ...` —
            // tuple/struct destructures aren't a single named export,
            // so reject them.
            return Err(ParseError::Unexpected {
                found: TokenKind::Pub,
                expected: "`pub let` only supports a single-name binding".into(),
                span: s.span,
            });
        }
    }
    Ok(s)
}

pub(crate) fn parse_let_stmt(p: &mut Parser) -> Result<Stmt, ParseError> {
    let let_span = p.peek().span;
    p.expect(&TokenKind::Let, "'let'")?;
    // Tuple destructure: `let (a, b, ...) = expr`. Each slot is
    // `Ident` or `_` (wildcard). Disambiguates from the leading
    // `let Ident ...` form by the LParen after `let`.
    if matches!(p.peek().kind, TokenKind::LParen) {
        p.bump(); // consume `(`
        let mut elems: Vec<Option<ilang_ast::Symbol>> = Vec::new();
        loop {
            match &p.peek().kind {
                TokenKind::RParen => break,
                TokenKind::Ident(name) if name == "_" => {
                    elems.push(None);
                    p.bump();
                }
                TokenKind::Ident(_) => {
                    elems.push(Some(p.expect_ident("binding name")?));
                }
                other => {
                    let span = p.peek().span;
                    return Err(ParseError::Unexpected {
                        found: other.clone(),
                        expected: "destructuring binding name or `_`".into(),
                        span,
                    });
                }
            }
            if matches!(p.peek().kind, TokenKind::Comma) {
                p.bump();
            } else {
                break;
            }
        }
        p.expect(&TokenKind::RParen, "')'")?;
        if elems.len() < 2 {
            let t = p.peek();
            return Err(ParseError::Unexpected {
                found: t.kind.clone(),
                expected: "tuple destructure needs at least 2 slots".into(),
                span: t.span,
            });
        }
        p.expect(&TokenKind::Equals, "'='")?;
        let value = p.parse_expr(0)?;
        p.consume_stmt_terminator()?;
        return Ok(Stmt::new(
            StmtKind::LetTuple { elems: elems.into(), value },
            let_span,
        ));
    }
    let name = p.expect_ident("variable name")?;
    // Struct destructure: `let ClassName { f1, f2, ... } = expr`.
    // The leading ident in `let ClassName {` reads like the start
    // of a regular binding, so we have to peek past it.
    if matches!(p.peek().kind, TokenKind::LBrace) {
        p.bump(); // consume `{`
        let mut fields: Vec<ilang_ast::Symbol> = Vec::new();
        loop {
            match &p.peek().kind {
                TokenKind::RBrace => break,
                TokenKind::Ident(_) => {
                    fields.push(p.expect_ident("destructure field name")?);
                }
                other => {
                    let span = p.peek().span;
                    return Err(ParseError::Unexpected {
                        found: other.clone(),
                        expected: "field name".into(),
                        span,
                    });
                }
            }
            if matches!(p.peek().kind, TokenKind::Comma) {
                p.bump();
            } else {
                break;
            }
        }
        p.expect(&TokenKind::RBrace, "'}'")?;
        p.expect(&TokenKind::Equals, "'='")?;
        let value = p.parse_expr(0)?;
        p.consume_stmt_terminator()?;
        return Ok(Stmt::new(
            StmtKind::LetStruct { class: name, fields: fields.into(), value },
            let_span,
        ));
    }
    let ty = if matches!(p.peek().kind, TokenKind::Colon) {
        p.bump();
        Some(p.parse_type()?)
    } else {
        None
    };
    p.expect(&TokenKind::Equals, "'='")?;
    let value = p.parse_expr(0)?;
    p.consume_stmt_terminator()?;
    Ok(Stmt::new(StmtKind::Let { is_pub: false,
                is_const: false, name, ty, value }, let_span))
}

/// `const x [: T] = expr` inside a block — a one-time-assigned
/// local binding. Same shape as `let`, but the type checker
/// rejects subsequent assignments. Tuple / struct destructure
/// forms are not supported (consts are single-name only).
pub(crate) fn parse_local_const_stmt(p: &mut Parser) -> Result<Stmt, ParseError> {
    let const_span = p.peek().span;
    p.expect(&TokenKind::Const, "'const'")?;
    let name = p.expect_ident("constant name")?;
    let ty = if matches!(p.peek().kind, TokenKind::Colon) {
        p.bump();
        Some(p.parse_type()?)
    } else {
        None
    };
    if !matches!(p.peek().kind, TokenKind::Equals) {
        let t = p.peek();
        return Err(ParseError::Unexpected {
            found: t.kind.clone(),
            expected: "`=` — local `const` must have an initializer expression".into(),
            span: t.span,
        });
    }
    p.bump();
    let value = p.parse_expr(0)?;
    p.consume_stmt_terminator()?;
    Ok(Stmt::new(
        StmtKind::Let { is_pub: false, is_const: true, name, ty, value },
        const_span,
    ))
}
