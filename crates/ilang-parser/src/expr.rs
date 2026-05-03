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
//! | `as` (cast)             | 23 / —      | postfix|
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
                    ExprKind::Index { obj, index } => Expr::new(
                        ExprKind::AssignIndex {
                            obj,
                            index,
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

            // `as`-cast: postfix-style infix that takes a type instead of
            // an expression. Tighter than any binary op so `1 + 2 as f64`
            // parses as `1 + ((2) as f64)`.
            if matches!(self.peek().kind, TokenKind::As) {
                let l_bp = 23u8;
                if l_bp < min_bp {
                    break;
                }
                self.bump();
                let target = self.parse_type()?;
                let span = lhs.span;
                lhs = Expr::new(
                    ExprKind::Cast {
                        expr: Box::new(lhs),
                        ty: target,
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
        loop {
            match self.peek().kind {
                TokenKind::LBracket => {
                    self.bump();
                    let index = self.parse_expr(0)?;
                    self.expect(&TokenKind::RBracket, "']'")?;
                    let span = expr.span;
                    expr = Expr::new(
                        ExprKind::Index {
                            obj: Box::new(expr),
                            index: Box::new(index),
                        },
                        span,
                    );
                }
                TokenKind::Dot => {
                    expr = self.parse_dot_postfix(expr)?;
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    /// One iteration of `.field` / `.method(args)`. Caller's loop drives
    /// repetition so dot and index chains can interleave (`a[0].x`).
    fn parse_dot_postfix(&mut self, mut expr: Expr) -> Result<Expr, ParseError> {
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
            } else if matches!(self.peek().kind, TokenKind::LBrace)
                && matches!(
                    self.peek_n(1).map(|t| &t.kind),
                    Some(TokenKind::Ident(_))
                )
                && matches!(
                    self.peek_n(2).map(|t| &t.kind),
                    Some(TokenKind::Colon)
                )
                && matches!(&expr.kind, ExprKind::Var(_))
            {
                // `EnumName.Variant { field: value, ... }` —
                // struct-payload enum constructor. Lookahead
                // `{ Ident :` distinguishes from a stray block.
                // Only accepted when the receiver is a bare Var
                // (i.e. `EnumName`); chained access like `a.b.c { ... }`
                // is not enum-ctor.
                self.bump();
                let mut fs = Vec::new();
                while !matches!(self.peek().kind, TokenKind::RBrace) {
                    let fname = self.expect_ident("field name")?;
                    self.expect(&TokenKind::Colon, "':'")?;
                    let fval = self.parse_expr(0)?;
                    fs.push((fname, fval));
                    if matches!(self.peek().kind, TokenKind::Comma) {
                        self.bump();
                    } else if !matches!(self.peek().kind, TokenKind::RBrace)
                        && !self.peek().leading_newline
                    {
                        let p = self.peek();
                        return Err(ParseError::Unexpected {
                            found: p.kind.clone(),
                            expected: "',' or newline between fields".into(),
                            span: p.span,
                        });
                    }
                }
                self.expect(&TokenKind::RBrace, "'}'")?;
                let enum_name = match expr.kind {
                    ExprKind::Var(n) => n,
                    _ => unreachable!(),
                };
                expr = Expr::new(
                    ExprKind::EnumCtor {
                        enum_name,
                        variant: name,
                        args: ilang_ast::CtorArgs::Struct(fs),
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
                let suffix = t.numeric_suffix.clone();
                self.bump();
                let lit = Expr::new(ExprKind::Int(n), span);
                Ok(match suffix {
                    Some(ty) => Expr::new(
                        ExprKind::Cast {
                            expr: Box::new(lit),
                            ty,
                        },
                        span,
                    ),
                    None => lit,
                })
            }
            TokenKind::Float(f) => {
                let suffix = t.numeric_suffix.clone();
                self.bump();
                let lit = Expr::new(ExprKind::Float(f), span);
                Ok(match suffix {
                    Some(ty) => Expr::new(
                        ExprKind::Cast {
                            expr: Box::new(lit),
                            ty,
                        },
                        span,
                    ),
                    None => lit,
                })
            }
            TokenKind::Str(s) => {
                self.bump();
                Ok(Expr::new(ExprKind::Str(s), span))
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
                // Class name is either bare `Counter` or
                // `module.Counter` (whole-module imported class).
                let mut class = self.expect_ident("class name")?;
                while matches!(self.peek().kind, TokenKind::Dot) {
                    self.bump();
                    let part = self.expect_ident("class name segment")?;
                    class.push('.');
                    class.push_str(&part);
                }
                // Optional `<T, U>` type arguments before the constructor
                // arg list. Unambiguous after `new ClassName` since `<`
                // can never be the start of an expression here.
                let type_args = if matches!(self.peek().kind, TokenKind::Lt) {
                    self.parse_type_args()?
                } else {
                    Vec::new()
                };
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
                Ok(Expr::new(ExprKind::New { class, type_args, args, init_method: None }, span))
            }
            TokenKind::If => self.parse_if(),
            TokenKind::Fn => self.parse_fn_expr(),
            TokenKind::None_ => {
                self.bump();
                Ok(Expr::new(ExprKind::None, span))
            }
            TokenKind::Some_ => {
                self.bump();
                self.expect(&TokenKind::LParen, "'('")?;
                let inner = self.parse_expr(0)?;
                self.expect(&TokenKind::RParen, "')'")?;
                Ok(Expr::new(ExprKind::Some(Box::new(inner)), span))
            }
            TokenKind::While => self.parse_while(),
            TokenKind::Loop => self.parse_loop(),
            TokenKind::For => self.parse_for(),
            TokenKind::Break => {
                self.bump();
                Ok(Expr::new(ExprKind::Break, span))
            }
            TokenKind::Continue => {
                self.bump();
                Ok(Expr::new(ExprKind::Continue, span))
            }
            TokenKind::Return => {
                self.bump();
                // `return` alone vs `return expr`. The operand is
                // omitted when the next token is a stmt terminator
                // (`;`, `}`, EOF) or starts a new logical line (ASI).
                let next = self.peek();
                let no_value = matches!(
                    next.kind,
                    TokenKind::Semicolon | TokenKind::RBrace | TokenKind::Eof
                ) || next.leading_newline;
                let value = if no_value {
                    None
                } else {
                    Some(Box::new(self.parse_expr(0)?))
                };
                Ok(Expr::new(ExprKind::Return(value), span))
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
            TokenKind::Match => {
                self.bump();
                let scrutinee = self.parse_expr(0)?;
                self.expect(&TokenKind::LBrace, "'{'")?;
                let mut arms = Vec::new();
                while !matches!(self.peek().kind, TokenKind::RBrace) {
                    let arm_span = self.peek().span;
                    let pattern = self.parse_pattern_in_arm()?;
                    // Arm body is a brace-delimited block — no `=>`.
                    let body_span = self.peek().span;
                    let body_block = parse_block(self)?;
                    let body = Expr::new(ExprKind::Block(body_block), body_span);
                    arms.push(ilang_ast::MatchArm {
                        pattern,
                        body,
                        span: arm_span,
                    });
                    if matches!(self.peek().kind, TokenKind::Comma) {
                        self.bump();
                    } else if !matches!(self.peek().kind, TokenKind::RBrace)
                        && !self.peek().leading_newline
                    {
                        let p = self.peek();
                        return Err(ParseError::Unexpected {
                            found: p.kind.clone(),
                            expected: "',' or newline between match arms".into(),
                            span: p.span,
                        });
                    }
                }
                self.expect(&TokenKind::RBrace, "'}'")?;
                Ok(Expr::new(
                    ExprKind::Match {
                        scrutinee: Box::new(scrutinee),
                        arms,
                    },
                    span,
                ))
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
                // Map literal vs. block disambiguation. A `{` followed by
                // a key token (string / int / bool literal) and then `:`
                // is a map literal; otherwise it's a block. The tokens
                // that can start a key never form a valid statement
                // followed by `:`, so this rule has no false positives
                // against existing programs.
                let is_map = matches!(
                    self.peek_n(1).map(|t| &t.kind),
                    Some(TokenKind::Str(_) | TokenKind::Int(_) | TokenKind::True | TokenKind::False)
                ) && matches!(
                    self.peek_n(2).map(|t| &t.kind),
                    Some(TokenKind::Colon)
                );
                if is_map {
                    self.parse_map_literal(span)
                } else {
                    let block = parse_block(self)?;
                    Ok(Expr::new(ExprKind::Block(block), span))
                }
            }
            TokenKind::LBracket => {
                self.bump();
                let mut elements = Vec::new();
                while !matches!(self.peek().kind, TokenKind::RBracket) {
                    elements.push(self.parse_expr(0)?);
                    // Trailing comma is allowed: stop the loop if the next
                    // token is `]`, regardless of whether we consumed a `,`.
                    if matches!(self.peek().kind, TokenKind::Comma) {
                        self.bump();
                    } else {
                        break;
                    }
                }
                self.expect(&TokenKind::RBracket, "']'")?;
                Ok(Expr::new(ExprKind::Array(elements), span))
            }
            other => Err(ParseError::Unexpected {
                found: other,
                expected: "number, identifier, '-', '+' or '('".into(),
                span: t.span,
            }),
        }
    }

    /// Continue an `if` chain at the `elif` keyword. Produces an
    /// `If` expression whose else branch is itself another `If` if
    /// further `elif` follows.
    fn parse_elif_chain(&mut self) -> Result<Expr, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Elif, "'elif'")?;
        let cond = self.parse_expr(0)?;
        let then_branch = parse_block(self)?;
        let else_branch = match self.peek().kind {
            TokenKind::Elif => Some(Box::new(self.parse_elif_chain()?)),
            TokenKind::Else => {
                self.bump();
                if matches!(self.peek().kind, TokenKind::If) {
                    let p = self.peek();
                    return Err(ParseError::Unexpected {
                        found: p.kind.clone(),
                        expected: "'elif' (use `elif` for chained conditions, not `else if`)"
                            .into(),
                        span: p.span,
                    });
                }
                let block_span = self.peek().span;
                let block = parse_block(self)?;
                Some(Box::new(Expr::new(ExprKind::Block(block), block_span)))
            }
            _ => None,
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

    fn parse_if(&mut self) -> Result<Expr, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::If, "'if'")?;
        // `if let some(name) = expr { ... } else { ... }` — the only
        // pattern form supported (so far). Anything else after `if let`
        // is a syntax error to avoid promising more pattern matching
        // than is implemented.
        if matches!(self.peek().kind, TokenKind::Let) {
            return self.parse_if_let(span);
        }
        let cond = self.parse_expr(0)?;
        let then_branch = parse_block(self)?;
        // `elif cond { ... }` chains as the else branch (a nested if).
        // `else if` is rejected with a hint pointing to `elif`.
        let else_branch = match self.peek().kind {
            TokenKind::Elif => {
                let inner = self.parse_elif_chain()?;
                Some(Box::new(inner))
            }
            TokenKind::Else => {
                self.bump();
                if matches!(self.peek().kind, TokenKind::If) {
                    let p = self.peek();
                    return Err(ParseError::Unexpected {
                        found: p.kind.clone(),
                        expected: "'elif' (use `elif` for chained conditions, not `else if`)"
                            .into(),
                        span: p.span,
                    });
                }
                let block_span = self.peek().span;
                let block = parse_block(self)?;
                Some(Box::new(Expr::new(ExprKind::Block(block), block_span)))
            }
            _ => None,
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

    /// Parse a pattern in match-arm position, where the pattern is
    /// always followed by a `{ body }` arm body. Disambiguates the
    /// `{` after a variant name: it's a struct-pattern only when the
    /// matching `}` is itself followed by another `{` (that second
    /// `{` is the arm body). Otherwise the `{` belongs to the arm
    /// body and the pattern is a unit variant.
    fn parse_pattern_in_arm(&mut self) -> Result<ilang_ast::Pattern, ParseError> {
        // Wildcard / Result short forms / variant with explicit
        // `(...)` are unambiguous — fall through to the normal path
        // for those. The only ambiguity is when, after a variant name,
        // we see `{`. We handle that by peeking through to the
        // matching `}` and checking what follows.
        if self.lookahead_is_unit_variant_in_arm() {
            // Parse just `EnumName::Variant` (no payload), leave the
            // arm-body `{` alone.
            return self.parse_pattern_unit_only();
        }
        self.parse_pattern()
    }

    /// True when the upcoming pattern is a variant name optionally
    /// followed by a `{ ... }` whose closing `}` is NOT followed by
    /// another `{`. In that case the `{` belongs to the arm body —
    /// the pattern is a bare unit variant.
    fn lookahead_is_unit_variant_in_arm(&self) -> bool {
        // Pattern shapes covered:
        //   long  form: `Ident :: Ident { ... }`     (variant_pos = 2, brace_pos = 3)
        //   short form: `Ident { ... }`              (variant_pos = 0, brace_pos = 1)
        // For the short form we still want the same disambiguation.
        // (Wildcard `_` and `ok`/`err` Result short forms never end up
        // here — they're handled inside parse_pattern.)
        let t0 = &self.tokens[self.pos].kind;
        if !matches!(t0, TokenKind::Ident(_)) {
            return false;
        }
        let t1 = self.tokens.get(self.pos + 1).map(|t| &t.kind);
        let brace_pos = if matches!(t1, Some(TokenKind::Dot)) {
            // long form `Enum.Variant`
            let t2 = self.tokens.get(self.pos + 2).map(|t| &t.kind);
            if !matches!(t2, Some(TokenKind::Ident(_))) {
                return false;
            }
            self.pos + 3
        } else if matches!(t1, Some(TokenKind::LBrace)) {
            // short form `Ident { ... }`
            self.pos + 1
        } else {
            return false;
        };
        if !matches!(self.tokens.get(brace_pos).map(|t| &t.kind), Some(TokenKind::LBrace)) {
            return false;
        }
        // Walk from brace_pos to find the matching `}`. Then check if
        // the next token is `{` (struct pattern + arm body) or
        // anything else (the `{` is actually the arm body).
        let mut depth: i32 = 0;
        let mut i = brace_pos;
        while i < self.tokens.len() {
            match &self.tokens[i].kind {
                TokenKind::LBrace => depth += 1,
                TokenKind::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        // Found the matching `}`. Look at next token.
                        let after = self.tokens.get(i + 1).map(|t| &t.kind);
                        return !matches!(after, Some(TokenKind::LBrace));
                    }
                }
                TokenKind::Eof => break,
                _ => {}
            }
            i += 1;
        }
        // Unbalanced — bail to the regular parser, which will error.
        false
    }

    /// Parse `EnumName::Variant` only — used when the lookahead has
    /// determined the `{` that follows belongs to the arm body, not
    /// a struct payload pattern.
    fn parse_pattern_unit_only(&mut self) -> Result<ilang_ast::Pattern, ParseError> {
        let span = self.peek().span;
        // Wildcard arm `_ { ... }`.
        if let TokenKind::Ident(n) = &self.peek().kind {
            if n == "_" {
                self.bump();
                return Ok(ilang_ast::Pattern {
                    kind: ilang_ast::PatternKind::Wildcard,
                    span,
                });
            }
        }
        let first = self.expect_ident("variant name")?;
        let (enum_name, variant) = if matches!(self.peek().kind, TokenKind::Dot) {
            self.bump();
            let v = self.expect_ident("variant name")?;
            (Some(first), v)
        } else {
            (None, first)
        };
        Ok(ilang_ast::Pattern {
            kind: ilang_ast::PatternKind::Variant {
                enum_name,
                variant,
                bindings: ilang_ast::PatternBindings::Unit,
            },
            span,
        })
    }

    fn parse_pattern(&mut self) -> Result<ilang_ast::Pattern, ParseError> {
        let span = self.peek().span;
        // `_` wildcard.
        if let TokenKind::Ident(name) = &self.peek().kind {
            if name == "_" {
                self.bump();
                return Ok(ilang_ast::Pattern {
                    kind: ilang_ast::PatternKind::Wildcard,
                    span,
                });
            }
        }
        // Long form `EnumName.Variant` vs. short form `Variant` (the
        // checker fills in the enum name from the scrutinee). Detect
        // by looking for `::` after the first ident.
        let first = self.expect_ident("pattern (variant or `_`)")?;
        let (enum_name, variant) = if matches!(self.peek().kind, TokenKind::Dot) {
            self.bump();
            let v = self.expect_ident("variant name")?;
            (Some(first), v)
        } else {
            (None, first)
        };
        let bindings = match self.peek().kind {
            TokenKind::LParen => {
                self.bump();
                let mut names = Vec::new();
                if !matches!(self.peek().kind, TokenKind::RParen) {
                    loop {
                        let n = self.expect_ident("binding name (or `_`)")?;
                        names.push(n);
                        if matches!(self.peek().kind, TokenKind::Comma) {
                            self.bump();
                        } else {
                            break;
                        }
                    }
                }
                self.expect(&TokenKind::RParen, "')'")?;
                ilang_ast::PatternBindings::Tuple(names)
            }
            TokenKind::LBrace => {
                self.bump();
                let mut fs = Vec::new();
                while !matches!(self.peek().kind, TokenKind::RBrace) {
                    let fname = self.expect_ident("field name")?;
                    // Shorthand: `{ side }` is `{ side: side }`.
                    let bname = if matches!(self.peek().kind, TokenKind::Colon) {
                        self.bump();
                        self.expect_ident("binding name")?
                    } else {
                        fname.clone()
                    };
                    fs.push((fname, bname));
                    if matches!(self.peek().kind, TokenKind::Comma) {
                        self.bump();
                    } else if !matches!(self.peek().kind, TokenKind::RBrace) {
                        let p = self.peek();
                        return Err(ParseError::Unexpected {
                            found: p.kind.clone(),
                            expected: "',' between struct-pattern fields".into(),
                            span: p.span,
                        });
                    }
                }
                self.expect(&TokenKind::RBrace, "'}'")?;
                ilang_ast::PatternBindings::Struct(fs)
            }
            _ => ilang_ast::PatternBindings::Unit,
        };
        Ok(ilang_ast::Pattern {
            kind: ilang_ast::PatternKind::Variant {
                enum_name,
                variant,
                bindings,
            },
            span,
        })
    }

    fn parse_if_let(&mut self, span: ilang_ast::Span) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::Let, "'let'")?;
        self.expect(&TokenKind::Some_, "'some' (only pattern supported)")?;
        self.expect(&TokenKind::LParen, "'('")?;
        let name = self.expect_ident("variable name")?;
        self.expect(&TokenKind::RParen, "')'")?;
        self.expect(&TokenKind::Equals, "'='")?;
        let scrut = self.parse_expr(0)?;
        let then_branch = parse_block(self)?;
        let else_branch = if matches!(self.peek().kind, TokenKind::Else) {
            self.bump();
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
            ExprKind::IfLet {
                name,
                expr: Box::new(scrut),
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

    /// Map literal: `{ key1: value1, key2: value2, ... }`. Trailing
    /// comma allowed. Empty maps `{}` are not produced here — that
    /// path is handled by the block-vs-map lookahead above (an empty
    /// `{}` is parsed as a unit-block; for an empty Map use
    /// `new Map<K, V>()`).
    fn parse_map_literal(&mut self, span: ilang_ast::Span) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::LBrace, "'{'")?;
        let mut entries = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBrace) {
            let key = self.parse_expr(0)?;
            self.expect(&TokenKind::Colon, "':'")?;
            let value = self.parse_expr(0)?;
            entries.push((key, value));
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        self.expect(&TokenKind::RBrace, "'}'")?;
        Ok(Expr::new(ExprKind::MapLit(entries), span))
    }

    /// Anonymous function expression: `fn(p: T, ...): R { body }`. The
    /// shape mirrors `fn name(...) { ... }` minus the name.
    fn parse_fn_expr(&mut self) -> Result<Expr, ParseError> {
        use ilang_ast::Param;
        let span = self.peek().span;
        self.expect(&TokenKind::Fn, "'fn'")?;
        self.expect(&TokenKind::LParen, "'('")?;
        let mut params = Vec::new();
        if !matches!(self.peek().kind, TokenKind::RParen) {
            loop {
                let pspan = self.peek().span;
                let pname = self.expect_ident("parameter name")?;
                self.expect(&TokenKind::Colon, "':'")?;
                let pty = self.parse_type()?;
                params.push(Param {
                    name: pname,
                    ty: pty,
                    span: pspan,
                });
                if matches!(self.peek().kind, TokenKind::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RParen, "')'")?;
        let ret = if matches!(self.peek().kind, TokenKind::Colon) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = crate::stmt::parse_block(self)?;
        Ok(Expr::new(
            ExprKind::FnExpr { params, ret, body },
            span,
        ))
    }

    fn parse_for(&mut self) -> Result<Expr, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::For, "'for'")?;
        let var_tok = self.bump().clone();
        let var = match &var_tok.kind {
            TokenKind::Ident(n) => n.clone(),
            _ => {
                return Err(ParseError::Unexpected {
                    found: var_tok.kind.clone(),
                    expected: "identifier after 'for'".into(),
                    span: var_tok.span,
                });
            }
        };
        self.expect(&TokenKind::In, "'in'")?;
        let iter = self.parse_expr(0)?;
        let body = parse_block(self)?;
        Ok(Expr::new(
            ExprKind::ForIn {
                var,
                iter: Box::new(iter),
                body,
            },
            span,
        ))
    }
}
