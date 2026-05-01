//! Pratt expression parser.
//!
//! Precedence (low → high), C/JS-style:
//!
//! | Operator                | l_bp / r_bp | Assoc  |
//! |-------------------------|-------------|--------|
//! | `=` `+=` …              | 2 / 1       | right  |
//! | `\|\|`                  | 3 / 4       | left   |
//! | `&&`                    | 5 / 6       | left   |
//! | `\|` (bit or)           | 7 / 8       | left   |
//! | `^` (bit xor)           | 9 / 10      | left   |
//! | `&` (bit and)           | 11 / 12     | left   |
//! | `==` `!=`               | 13 / 14     | left   |
//! | `<` `<=` `>` `>=`       | 15 / 16     | left   |
//! | `<<` `>>`               | 17 / 18     | left   |
//! | `+` `-`                 | 19 / 20     | left   |
//! | `*` `/` `%`             | 21 / 22     | left   |
//! | prefix `-` `+` `!` `~`  | — / 30      | prefix |

use ilang_ast::{BinOp, Expr, ExprKind, LogicalOp, UnOp};
use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;
use crate::stmt::parse_block;

impl<'a> Parser<'a> {
    pub(crate) fn parse_expr(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_prefix()?;
        // Postfix: `.field` and `.method(args)` chains. These bind tighter
        // than any infix operator.
        lhs = self.parse_postfix(lhs)?;
        loop {
            // Assignment is right-associative; lhs must be Var or Field.
            // Compound forms (`+=`, `-=`, ...) are desugared here into the
            // equivalent `lhs = lhs <op> rhs` so the rest of the pipeline
            // (type checker, evaluator) needs no new cases.
            let compound_op = match self.peek().kind {
                TokenKind::PlusEq => Some(BinOp::Add),
                TokenKind::MinusEq => Some(BinOp::Sub),
                TokenKind::StarEq => Some(BinOp::Mul),
                TokenKind::SlashEq => Some(BinOp::Div),
                TokenKind::PercentEq => Some(BinOp::Rem),
                TokenKind::AmpEq => Some(BinOp::BitAnd),
                TokenKind::PipeEq => Some(BinOp::BitOr),
                TokenKind::CaretEq => Some(BinOp::BitXor),
                TokenKind::LtLtEq => Some(BinOp::Shl),
                TokenKind::GtGtEq => Some(BinOp::Shr),
                _ => None,
            };
            if matches!(self.peek().kind, TokenKind::Equals) || compound_op.is_some() {
                let l_bp = 2u8;
                let r_bp = 1u8;
                if l_bp < min_bp {
                    break;
                }
                let eq_tok = self.peek().clone();
                self.bump();
                let rhs = self.parse_expr(r_bp)?;
                let lhs_span = lhs.span;
                let value = match compound_op {
                    Some(op) => Expr::new(
                        ExprKind::Binary {
                            op,
                            lhs: Box::new(lhs.clone()),
                            rhs: Box::new(rhs),
                        },
                        lhs_span,
                    ),
                    None => rhs,
                };
                lhs = match lhs.kind {
                    ExprKind::Var(name) => Expr::new(
                        ExprKind::Assign {
                            target: name,
                            value: Box::new(value),
                        },
                        lhs_span,
                    ),
                    ExprKind::Field { obj, name } => Expr::new(
                        ExprKind::AssignField {
                            obj,
                            field: name,
                            value: Box::new(value),
                        },
                        lhs_span,
                    ),
                    _ => {
                        return Err(ParseError::InvalidAssignTarget { span: eq_tok.span });
                    }
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
                let span = lhs.span;
                lhs = Expr::new(
                    ExprKind::Logical {
                        op: logop,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    },
                    span,
                );
                continue;
            }

            // Regular binary operators.
            let (op, l_bp, r_bp) = match &self.peek().kind {
                TokenKind::Pipe => (BinOp::BitOr, 7, 8),
                TokenKind::Caret => (BinOp::BitXor, 9, 10),
                TokenKind::Amp => (BinOp::BitAnd, 11, 12),
                TokenKind::EqEq => (BinOp::Eq, 13, 14),
                TokenKind::BangEq => (BinOp::Ne, 13, 14),
                TokenKind::Lt => (BinOp::Lt, 15, 16),
                TokenKind::LtEq => (BinOp::Le, 15, 16),
                TokenKind::Gt => (BinOp::Gt, 15, 16),
                TokenKind::GtEq => (BinOp::Ge, 15, 16),
                TokenKind::LtLt => (BinOp::Shl, 17, 18),
                TokenKind::GtGt => (BinOp::Shr, 17, 18),
                TokenKind::Plus => (BinOp::Add, 19, 20),
                TokenKind::Minus => (BinOp::Sub, 19, 20),
                TokenKind::Star => (BinOp::Mul, 21, 22),
                TokenKind::Slash => (BinOp::Div, 21, 22),
                TokenKind::Percent => (BinOp::Rem, 21, 22),
                _ => break,
            };
            if l_bp < min_bp {
                break;
            }
            self.bump();
            let rhs = self.parse_expr(r_bp)?;
            let span = lhs.span;
            lhs = Expr::new(
                ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                span,
            );
        }
        Ok(lhs)
    }

    /// Apply postfix `.field` / `.method(args)` chains, repeatedly, to a
    /// parsed primary expression.
    fn parse_postfix(&mut self, mut expr: Expr) -> Result<Expr, ParseError> {
        while matches!(self.peek().kind, TokenKind::Dot) {
            self.bump();
            let name = self.expect_ident("field or method name")?;
            let span = expr.span;
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
                expr = Expr::new(
                    ExprKind::MethodCall {
                        obj: Box::new(expr),
                        method: name,
                        args,
                    },
                    span,
                );
            } else {
                expr = Expr::new(
                    ExprKind::Field {
                        obj: Box::new(expr),
                        name,
                    },
                    span,
                );
            }
        }
        Ok(expr)
    }

    fn parse_prefix(&mut self) -> Result<Expr, ParseError> {
        let t = self.peek().clone();
        let span = t.span;
        match t.kind {
            TokenKind::Int(n) => {
                self.bump();
                Ok(Expr::new(ExprKind::Int(n), span))
            }
            TokenKind::Float(f) => {
                self.bump();
                Ok(Expr::new(ExprKind::Float(f), span))
            }
            TokenKind::True => {
                self.bump();
                Ok(Expr::new(ExprKind::Bool(true), span))
            }
            TokenKind::False => {
                self.bump();
                Ok(Expr::new(ExprKind::Bool(false), span))
            }
            TokenKind::This => {
                self.bump();
                Ok(Expr::new(ExprKind::This, span))
            }
            TokenKind::New => {
                self.bump();
                let class = self.expect_ident("class name")?;
                self.expect(&TokenKind::LParen, "'('")?;
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
                Ok(Expr::new(ExprKind::New { class, args }, span))
            }
            TokenKind::If => self.parse_if(),
            TokenKind::While => self.parse_while(),
            TokenKind::Loop => self.parse_loop(),
            TokenKind::Break => {
                self.bump();
                Ok(Expr::new(ExprKind::Break, span))
            }
            TokenKind::Continue => {
                self.bump();
                Ok(Expr::new(ExprKind::Continue, span))
            }
            TokenKind::Bang => {
                self.bump();
                let e = self.parse_expr(30)?;
                Ok(Expr::new(
                    ExprKind::Unary {
                        op: UnOp::Not,
                        expr: Box::new(e),
                    },
                    span,
                ))
            }
            TokenKind::Tilde => {
                self.bump();
                let e = self.parse_expr(30)?;
                Ok(Expr::new(
                    ExprKind::Unary {
                        op: UnOp::BitNot,
                        expr: Box::new(e),
                    },
                    span,
                ))
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
                    Ok(Expr::new(ExprKind::Call { callee: name, args }, span))
                } else {
                    Ok(Expr::new(ExprKind::Var(name), span))
                }
            }
            TokenKind::Minus => {
                self.bump();
                let e = self.parse_expr(30)?;
                Ok(Expr::new(
                    ExprKind::Unary {
                        op: UnOp::Neg,
                        expr: Box::new(e),
                    },
                    span,
                ))
            }
            TokenKind::Plus => {
                self.bump();
                let e = self.parse_expr(30)?;
                Ok(Expr::new(
                    ExprKind::Unary {
                        op: UnOp::Pos,
                        expr: Box::new(e),
                    },
                    span,
                ))
            }
            TokenKind::LParen => {
                self.bump();
                let e = self.parse_expr(0)?;
                self.expect(&TokenKind::RParen, "')'")?;
                Ok(e)
            }
            TokenKind::LBrace => {
                let block = parse_block(self)?;
                Ok(Expr::new(ExprKind::Block(block), span))
            }
            other => Err(ParseError::Unexpected {
                found: other,
                expected: "number, identifier, '-', '+' or '('".into(),
                span: t.span,
            }),
        }
    }

    fn parse_if(&mut self) -> Result<Expr, ParseError> {
        let span = self.peek().span;
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
                let block_span = self.peek().span;
                let block = parse_block(self)?;
                Some(Box::new(Expr::new(ExprKind::Block(block), block_span)))
            }
        } else {
            None
        };
        Ok(Expr::new(
            ExprKind::If {
                cond: Box::new(cond),
                then_branch,
                else_branch,
            },
            span,
        ))
    }

    fn parse_while(&mut self) -> Result<Expr, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::While, "'while'")?;
        let cond = self.parse_expr(0)?;
        let body = parse_block(self)?;
        Ok(Expr::new(
            ExprKind::While {
                cond: Box::new(cond),
                body,
            },
            span,
        ))
    }

    fn parse_loop(&mut self) -> Result<Expr, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Loop, "'loop'")?;
        let body = parse_block(self)?;
        Ok(Expr::new(ExprKind::Loop { body }, span))
    }
}
