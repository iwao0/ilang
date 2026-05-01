//! Pratt expression parser.
//!
//! Precedence (low → high):
//!
//! | Operator           | l_bp / r_bp | Assoc |
//! |--------------------|-------------|-------|
//! | `=`                | 2 / 1       | right |
//! | `\|\|`             | 3 / 4       | left  |
//! | `&&`               | 5 / 6       | left  |
//! | `==` `!=`          | 7 / 8       | left  |
//! | `<` `<=` `>` `>=`  | 9 / 10      | left  |
//! | `+` `-`            | 10 / 11     | left  |
//! | `*` `/` `%`        | 20 / 21     | left  |
//! | prefix `-` `+` `!` | — / 30      | prefix|

use ilang_ast::{BinOp, Expr, LogicalOp, UnOp};
use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;
use crate::stmt::parse_block;

impl<'a> Parser<'a> {
    pub(crate) fn parse_expr(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_prefix()?;
        loop {
            // Assignment is right-associative; lhs must be a Var.
            if matches!(self.peek().kind, TokenKind::Equals) {
                let l_bp = 2u8;
                let r_bp = 1u8;
                if l_bp < min_bp {
                    break;
                }
                let eq_tok = self.peek().clone();
                self.bump();
                let value = self.parse_expr(r_bp)?;
                let target = match lhs {
                    Expr::Var(name) => name,
                    _ => {
                        return Err(ParseError::InvalidAssignTarget {
                            line: eq_tok.span.line,
                            col: eq_tok.span.col,
                        });
                    }
                };
                lhs = Expr::Assign {
                    target,
                    value: Box::new(value),
                };
                continue;
            }

            // Short-circuit logical operators.
            if let Some((logop, l_bp, r_bp)) = match self.peek().kind {
                TokenKind::PipePipe => Some((LogicalOp::Or, 3u8, 4u8)),
                TokenKind::AmpAmp => Some((LogicalOp::And, 5u8, 6u8)),
                _ => None,
            } {
                if l_bp < min_bp {
                    break;
                }
                self.bump();
                let rhs = self.parse_expr(r_bp)?;
                lhs = Expr::Logical {
                    op: logop,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
                continue;
            }

            // Regular binary operators.
            let (op, l_bp, r_bp) = match &self.peek().kind {
                TokenKind::EqEq => (BinOp::Eq, 7, 8),
                TokenKind::BangEq => (BinOp::Ne, 7, 8),
                TokenKind::Lt => (BinOp::Lt, 9, 10),
                TokenKind::LtEq => (BinOp::Le, 9, 10),
                TokenKind::Gt => (BinOp::Gt, 9, 10),
                TokenKind::GtEq => (BinOp::Ge, 9, 10),
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
        let t = self.peek().clone();
        match t.kind {
            TokenKind::Int(n) => {
                self.bump();
                Ok(Expr::Int(n))
            }
            TokenKind::Float(f) => {
                self.bump();
                Ok(Expr::Float(f))
            }
            TokenKind::True => {
                self.bump();
                Ok(Expr::Bool(true))
            }
            TokenKind::False => {
                self.bump();
                Ok(Expr::Bool(false))
            }
            TokenKind::If => self.parse_if(),
            TokenKind::While => self.parse_while(),
            TokenKind::Bang => {
                self.bump();
                let e = self.parse_expr(30)?;
                Ok(Expr::Unary {
                    op: UnOp::Not,
                    expr: Box::new(e),
                })
            }
            TokenKind::Ident(name) => {
                self.bump();
                if matches!(self.peek().kind, TokenKind::LParen) {
                    self.bump();
                    let mut args = Vec::new();
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
                    Ok(Expr::Call { callee: name, args })
                } else {
                    Ok(Expr::Var(name))
                }
            }
            TokenKind::Minus => {
                self.bump();
                let e = self.parse_expr(30)?;
                Ok(Expr::Unary {
                    op: UnOp::Neg,
                    expr: Box::new(e),
                })
            }
            TokenKind::Plus => {
                self.bump();
                let e = self.parse_expr(30)?;
                Ok(Expr::Unary {
                    op: UnOp::Pos,
                    expr: Box::new(e),
                })
            }
            TokenKind::LParen => {
                self.bump();
                let e = self.parse_expr(0)?;
                self.expect(&TokenKind::RParen, "')'")?;
                Ok(e)
            }
            TokenKind::LBrace => {
                let block = parse_block(self)?;
                Ok(Expr::Block(block))
            }
            other => Err(ParseError::Unexpected {
                found: other,
                expected: "number, identifier, '-', '+' or '('".into(),
                line: t.span.line,
                col: t.span.col,
            }),
        }
    }

    fn parse_if(&mut self) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::If, "'if'")?;
        let cond = self.parse_expr(0)?;
        let then_branch = parse_block(self)?;
        let else_branch = if matches!(self.peek().kind, TokenKind::Else) {
            self.bump();
            // `else if` chains: parse another If expression directly so the
            // structure stays an If with an Else branch that is itself an If.
            if matches!(self.peek().kind, TokenKind::If) {
                let inner = self.parse_if()?;
                Some(Box::new(inner))
            } else {
                let block = parse_block(self)?;
                Some(Box::new(Expr::Block(block)))
            }
        } else {
            None
        };
        Ok(Expr::If {
            cond: Box::new(cond),
            then_branch,
            else_branch,
        })
    }

    fn parse_while(&mut self) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::While, "'while'")?;
        let cond = self.parse_expr(0)?;
        let body = parse_block(self)?;
        Ok(Expr::While {
            cond: Box::new(cond),
            body,
        })
    }
}
