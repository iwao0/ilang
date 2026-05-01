use ilang_ast::{BinOp, Expr, UnOp};
use ilang_lexer::{Span, Token, TokenKind};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ParseError {
    #[error("unexpected token {found:?} at line {line}, col {col} (expected {expected})")]
    Unexpected {
        found: TokenKind,
        expected: String,
        line: u32,
        col: u32,
    },
    #[error("trailing tokens after expression at line {line}, col {col}")]
    TrailingTokens { line: u32, col: u32 },
}

pub fn parse(tokens: &[Token]) -> Result<Expr, ParseError> {
    let mut p = Parser { tokens, pos: 0 };
    let expr = p.parse_expr(0)?;
    let next = p.peek();
    if !matches!(next.kind, TokenKind::Eof) {
        return Err(ParseError::TrailingTokens {
            line: next.span.line,
            col: next.span.col,
        });
    }
    Ok(expr)
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> &'a Token {
        &self.tokens[self.pos]
    }

    fn bump(&mut self) -> &'a Token {
        let t = &self.tokens[self.pos];
        if !matches!(t.kind, TokenKind::Eof) {
            self.pos += 1;
        }
        t
    }

    fn parse_expr(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_prefix()?;

        loop {
            let (op, l_bp, r_bp) = match &self.peek().kind {
                TokenKind::Plus => (BinOp::Add, 10, 11),
                TokenKind::Minus => (BinOp::Sub, 10, 11),
                TokenKind::Star => (BinOp::Mul, 20, 21),
                TokenKind::Slash => (BinOp::Div, 20, 21),
                TokenKind::Percent => (BinOp::Rem, 20, 21),
                _ => break,
            };
            if l_bp < min_bp {
                break;
            }
            self.bump();
            let rhs = self.parse_expr(r_bp)?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }

        Ok(lhs)
    }

    fn parse_prefix(&mut self) -> Result<Expr, ParseError> {
        let tok = self.peek().clone();
        match tok.kind {
            TokenKind::Int(n) => {
                self.bump();
                Ok(Expr::Int(n))
            }
            TokenKind::Float(f) => {
                self.bump();
                Ok(Expr::Float(f))
            }
            TokenKind::Minus => {
                self.bump();
                let expr = self.parse_expr(30)?;
                Ok(Expr::Unary {
                    op: UnOp::Neg,
                    expr: Box::new(expr),
                })
            }
            TokenKind::Plus => {
                self.bump();
                let expr = self.parse_expr(30)?;
                Ok(Expr::Unary {
                    op: UnOp::Pos,
                    expr: Box::new(expr),
                })
            }
            TokenKind::LParen => {
                self.bump();
                let expr = self.parse_expr(0)?;
                self.expect(&TokenKind::RParen, "')'")?;
                Ok(expr)
            }
            other => {
                let Span { line, col } = tok.span;
                Err(ParseError::Unexpected {
                    found: other,
                    expected: "number, '-', '+' or '('".into(),
                    line,
                    col,
                })
            }
        }
    }

    fn expect(&mut self, expected: &TokenKind, label: &str) -> Result<(), ParseError> {
        let tok = self.peek();
        if std::mem::discriminant(&tok.kind) == std::mem::discriminant(expected) {
            self.bump();
            Ok(())
        } else {
            Err(ParseError::Unexpected {
                found: tok.kind.clone(),
                expected: label.into(),
                line: tok.span.line,
                col: tok.span.col,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ilang_lexer::tokenize;

    fn parse_str(src: &str) -> Expr {
        let toks = tokenize(src).unwrap();
        parse(&toks).unwrap()
    }

    #[test]
    fn precedence() {
        let e = parse_str("1 + 2 * 3");
        assert_eq!(
            e,
            Expr::Binary {
                op: BinOp::Add,
                lhs: Box::new(Expr::Int(1)),
                rhs: Box::new(Expr::Binary {
                    op: BinOp::Mul,
                    lhs: Box::new(Expr::Int(2)),
                    rhs: Box::new(Expr::Int(3)),
                }),
            }
        );
    }

    #[test]
    fn left_assoc() {
        let e = parse_str("1 - 2 - 3");
        assert_eq!(
            e,
            Expr::Binary {
                op: BinOp::Sub,
                lhs: Box::new(Expr::Binary {
                    op: BinOp::Sub,
                    lhs: Box::new(Expr::Int(1)),
                    rhs: Box::new(Expr::Int(2)),
                }),
                rhs: Box::new(Expr::Int(3)),
            }
        );
    }

    #[test]
    fn parens_override() {
        let e = parse_str("(1 + 2) * 3");
        assert_eq!(
            e,
            Expr::Binary {
                op: BinOp::Mul,
                lhs: Box::new(Expr::Binary {
                    op: BinOp::Add,
                    lhs: Box::new(Expr::Int(1)),
                    rhs: Box::new(Expr::Int(2)),
                }),
                rhs: Box::new(Expr::Int(3)),
            }
        );
    }

    #[test]
    fn unary_minus() {
        let e = parse_str("-2 + 1");
        assert_eq!(
            e,
            Expr::Binary {
                op: BinOp::Add,
                lhs: Box::new(Expr::Unary {
                    op: UnOp::Neg,
                    expr: Box::new(Expr::Int(2)),
                }),
                rhs: Box::new(Expr::Int(1)),
            }
        );
    }

    #[test]
    fn trailing_error() {
        let toks = tokenize("1 2").unwrap();
        assert!(matches!(parse(&toks), Err(ParseError::TrailingTokens { .. })));
    }
}
