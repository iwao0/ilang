//! Pratt expression parser.
//!
//! Precedence (low → high), C/JS-style:
//!
//! | Operator                | l_bp / r_bp | Assoc  |
//! |-------------------------|-------------|--------|
//! | `=` `+=` …              | 2 / 1       | right  |
//! | `..` `..=` (range)      | 3 / 4       | left   |
//! | `\|\|`                  | 5 / 6       | left   |
//! | `&&`                    | 7 / 8       | left   |
//! | `\|` (bit or)           | 9 / 10      | left   |
//! | `^` (bit xor)           | 11 / 12     | left   |
//! | `&` (bit and)           | 13 / 14     | left   |
//! | `==` `!=`               | 15 / 16     | left   |
//! | `<` `<=` `>` `>=`       | 17 / 18     | left   |
//! | `<<` `>>`               | 19 / 20     | left   |
//! | `+` `-`                 | 21 / 22     | left   |
//! | `*` `/` `%`             | 23 / 24     | left   |
//! | `as` (cast)             | 25 / —      | postfix|
//! | prefix `-` `+` `!` `~`  | — / 30      | prefix |

use ilang_ast::{BinOp, Expr, ExprKind, LogicalOp, UnOp};
use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;
use crate::stmt::parse_block;

/// Truncate an `i64` literal to its declared numeric suffix's width
/// (e.g. `300` with `_u8` becomes `44`). Mirrors the runtime behaviour
/// of `300 as u8 as i64`, so a suffixed pattern literal matches the
/// same bit-pattern the equivalent expression would produce. `None`
/// suffix is a no-op; non-integer suffixes (`f32` / `f64`) are
/// already turned into Float by the lexer and never reach this path.
fn apply_int_suffix(n: i64, suffix: Option<&ilang_ast::Type>) -> i64 {
    match suffix {
        Some(ilang_ast::Type::I8) => (n as i8) as i64,
        Some(ilang_ast::Type::I16) => (n as i16) as i64,
        Some(ilang_ast::Type::I32) => (n as i32) as i64,
        Some(ilang_ast::Type::U8) => (n as u8) as i64,
        Some(ilang_ast::Type::U16) => (n as u16) as i64,
        Some(ilang_ast::Type::U32) => (n as u32) as i64,
        _ => n,
    }
}

/// Returns `Some("a.b.Foo")` for a chain of `Var`/`Field` whose root
/// is a `Var`, otherwise `None`. Used by struct-literal disambiguation
/// so `module.Foo { x: 1 }` is recognised the same way `Foo { x: 1 }`
/// is — the parser emits the dotted name as the `class` of the
/// `StructLit`, leaving module-prefix resolution to the loader.
fn flatten_var_dot_chain(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Var(n) => Some(n.clone()),
        ExprKind::Field { obj, name } => {
            let base = flatten_var_dot_chain(obj)?;
            Some(format!("{base}.{name}"))
        }
        _ => None,
    }
}

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

            // Range: `a..b` (exclusive) / `a..=b` (inclusive). Lowest
            // precedence among non-assignment operators so `1..n+1`
            // parses as `1..(n+1)` and `for i in 1..len(xs)` works.
            // Standalone use (anything outside a `for-in` iter slot)
            // is rejected by the type checker.
            if matches!(self.peek().kind, TokenKind::DotDot | TokenKind::DotDotEq) {
                let l_bp = 3u8;
                let r_bp = 4u8;
                if l_bp < min_bp {
                    break;
                }
                let inclusive = matches!(self.peek().kind, TokenKind::DotDotEq);
                let r_span = self.peek().span;
                self.bump();
                let rhs = self.parse_expr(r_bp)?;
                lhs = Expr::new(
                    ExprKind::Range {
                        start: Box::new(lhs),
                        end: Box::new(rhs),
                        inclusive,
                    },
                    r_span,
                );
                continue;
            }

            // Short-circuit logical operators.
            if let Some((logop, l_bp, r_bp)) = match self.peek().kind {
                TokenKind::PipePipe => Some((LogicalOp::Or, 5u8, 6u8)),
                TokenKind::AmpAmp => Some((LogicalOp::And, 7u8, 8u8)),
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
                let l_bp = 25u8;
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
                TokenKind::Pipe => (BinOp::BitOr, 9, 10),
                TokenKind::Caret => (BinOp::BitXor, 11, 12),
                TokenKind::Amp => (BinOp::BitAnd, 13, 14),
                TokenKind::EqEq => (BinOp::Eq, 15, 16),
                TokenKind::BangEq => (BinOp::Ne, 15, 16),
                TokenKind::Lt => (BinOp::Lt, 17, 18),
                TokenKind::LtEq => (BinOp::Le, 17, 18),
                TokenKind::Gt => (BinOp::Gt, 17, 18),
                TokenKind::GtEq => (BinOp::Ge, 17, 18),
                TokenKind::LtLt => (BinOp::Shl, 19, 20),
                TokenKind::GtGt => (BinOp::Shr, 19, 20),
                TokenKind::Plus => (BinOp::Add, 21, 22),
                TokenKind::Minus => (BinOp::Sub, 21, 22),
                TokenKind::Star => (BinOp::Mul, 23, 24),
                TokenKind::Slash => (BinOp::Div, 23, 24),
                TokenKind::Percent => (BinOp::Rem, 23, 24),
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
                // Struct literal: `Foo { f1: v1, f2: v2 }`. Only
                // accepted when the receiver is a bare class name
                // (`Var(_)`) and the body has the `{ ident :` shape
                // — disambiguating from blocks and map literals.
                // Lowered as `new Foo()` plus a sequence of field
                // assignments at the type-checker / codegen stage.
                TokenKind::LBrace
                    if flatten_var_dot_chain(&expr).is_some()
                        && matches!(
                            self.peek_n(1).map(|t| &t.kind),
                            Some(TokenKind::Ident(_))
                        )
                        && matches!(
                            self.peek_n(2).map(|t| &t.kind),
                            Some(TokenKind::Colon)
                        ) =>
                {
                    self.bump();
                    let mut fs: Vec<(String, Expr)> = Vec::new();
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
                    let class_name = flatten_var_dot_chain(&expr)
                        .expect("matched in the guard above");
                    let span = expr.span;
                    expr = Expr::new(
                        ExprKind::StructLit {
                            class: class_name,
                            fields: fs,
                        },
                        span,
                    );
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
            let name = self.expect_member_name("field or method name")?;
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
            TokenKind::Super => {
                // `super.method(args)` → SuperCall { method: Some, args }
                // `super(args)` → SuperCall { method: None, args }
                self.bump();
                let (method, args) = match self.peek().kind {
                    TokenKind::Dot => {
                        self.bump();
                        let m = self.expect_ident("method name after `super.`")?;
                        self.expect(&TokenKind::LParen, "'('")?;
                        let args = self.parse_call_args()?;
                        (Some(m), args)
                    }
                    TokenKind::LParen => {
                        self.bump();
                        let args = self.parse_call_args()?;
                        (None, args)
                    }
                    _ => {
                        let t = self.peek();
                        return Err(ParseError::Unexpected {
                            found: t.kind.clone(),
                            expected: "'.' (super.method) or '(' (super(args))".into(),
                            span: t.span,
                        });
                    }
                };
                Ok(Expr::new(ExprKind::SuperCall { method, args }, span))
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
                // `break` alone vs `break expr`. Same heuristic as `return`:
                // operand is omitted when the next token is a statement
                // terminator or starts a new logical line.
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
                Ok(Expr::new(ExprKind::Break(value), span))
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
                    let args = self.parse_call_args()?;
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
                // Fold `-<IntLit>` into a single `Int` so that minimum
                // values (`i64::MIN`, `i32::MIN`, ...) are writable as
                // `-N`. The suffixed form (`-128_i8`) shows up as
                // `Cast{Int(n), ty}`, so peel that wrapper too.
                if let ExprKind::Int(n) = e.kind {
                    return Ok(Expr::new(ExprKind::Int(n.wrapping_neg()), span));
                }
                if let ExprKind::Cast { expr: inner, ty } = &e.kind {
                    if let ExprKind::Int(n) = inner.kind {
                        let neg = Expr::new(ExprKind::Int(n.wrapping_neg()), inner.span);
                        return Ok(Expr::new(
                            ExprKind::Cast {
                                expr: Box::new(neg),
                                ty: ty.clone(),
                            },
                            span,
                        ));
                    }
                }
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
                let first = self.parse_expr(0)?;
                if matches!(self.peek().kind, TokenKind::Comma) {
                    let mut elems = vec![first];
                    while matches!(self.peek().kind, TokenKind::Comma) {
                        self.bump();
                        if matches!(self.peek().kind, TokenKind::RParen) {
                            break;
                        }
                        elems.push(self.parse_expr(0)?);
                    }
                    self.expect(&TokenKind::RParen, "')'")?;
                    return Ok(Expr::new(ExprKind::Tuple(elems), span));
                }
                self.expect(&TokenKind::RParen, "')'")?;
                Ok(first)
            }
            TokenKind::LBrace => {
                // Map literal vs. block disambiguation. A `{` followed by
                // a key token (string / int / bool literal) and then `:`
                // is a map literal; otherwise it's a block. The tokens
                // that can start a key never form a valid statement
                // followed by `:`, so this rule has no false positives
                // against existing programs.
                // `{ -1: ... }` is also a map: the key starts with a
                // unary minus, so look one further for `Int(_)` and
                // shift the `:` check by one slot.
                let neg_int_key = matches!(
                    self.peek_n(1).map(|t| &t.kind),
                    Some(TokenKind::Minus)
                ) && matches!(
                    self.peek_n(2).map(|t| &t.kind),
                    Some(TokenKind::Int(_))
                ) && matches!(
                    self.peek_n(3).map(|t| &t.kind),
                    Some(TokenKind::Colon)
                );
                let positive_key = matches!(
                    self.peek_n(1).map(|t| &t.kind),
                    Some(TokenKind::Str(_) | TokenKind::Int(_) | TokenKind::True | TokenKind::False)
                ) && matches!(
                    self.peek_n(2).map(|t| &t.kind),
                    Some(TokenKind::Colon)
                );
                let is_map = positive_key || neg_int_key;
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
    /// Recognise integer / string literal patterns in pattern
    /// position. `42`, `-7`, `"hi"`. Returns `None` when the next
    /// token isn't a literal pattern (caller falls through to the
    /// variant / wildcard path). `true` / `false` are deliberately
    /// not handled here — they're parsed as `Variant` so that an
    /// enum with a `true` / `false` variant still works; the type
    /// checker rewrites them to `BoolLit` when the scrutinee is a
    /// bool.
    fn try_parse_literal_pattern(&mut self) -> Result<Option<ilang_ast::Pattern>, ParseError> {
        let span = self.peek().span;
        // Read an optional leading `-` then an Int. Returns the
        // signed value and how many tokens it consumed (1 for plain
        // Int, 2 for `-Int`).
        // `wrapping_neg` so the absolute value of `i64::MIN`
        // (`9223372036854775808u64`) round-trips through `-` without
        // overflowing — `-9223372036854775808` is a valid `i64::MIN`
        // literal.
        let read_signed_int = |this: &Self, start: usize| -> Option<(i64, usize)> {
            match &this.tokens.get(start)?.kind {
                TokenKind::Int(n) => {
                    let raw = *n as i64;
                    let suffix = this.tokens.get(start)?.numeric_suffix.as_ref();
                    Some((apply_int_suffix(raw, suffix), 1))
                }
                TokenKind::Minus => match &this.tokens.get(start + 1)?.kind {
                    TokenKind::Int(n) => {
                        let raw = (*n as i64).wrapping_neg();
                        let suffix = this.tokens.get(start + 1)?.numeric_suffix.as_ref();
                        Some((apply_int_suffix(raw, suffix), 2))
                    }
                    _ => None,
                },
                _ => None,
            }
        };
        // Look ahead: is this a range pattern? Either `Int .. Int`,
        // `Int ..= Int`, or with a leading `-` on either side.
        if let Some((low, low_len)) = read_signed_int(self, self.pos) {
            let after_low = self.pos + low_len;
            let dotdot = self.tokens.get(after_low).map(|t| &t.kind);
            let inclusive = match dotdot {
                Some(TokenKind::DotDot) => Some(false),
                Some(TokenKind::DotDotEq) => Some(true),
                _ => None,
            };
            if let Some(inc) = inclusive {
                if let Some((high, high_len)) = read_signed_int(self, after_low + 1) {
                    // Commit: consume low (`-`?+Int), `..` / `..=`,
                    // high (`-`?+Int).
                    for _ in 0..low_len {
                        self.bump();
                    }
                    self.bump(); // dot-dot token
                    for _ in 0..high_len {
                        self.bump();
                    }
                    return Ok(Some(ilang_ast::Pattern {
                        kind: ilang_ast::PatternKind::IntRange {
                            low,
                            high,
                            inclusive: inc,
                        },
                        span,
                    }));
                }
            }
        }
        match &self.peek().kind {
            TokenKind::Int(n) => {
                let raw = *n as i64;
                let suffix = self.peek().numeric_suffix.clone();
                let v = apply_int_suffix(raw, suffix.as_ref());
                self.bump();
                Ok(Some(ilang_ast::Pattern {
                    kind: ilang_ast::PatternKind::IntLit(v),
                    span,
                }))
            }
            TokenKind::Minus => {
                // `-N` integer pattern. Only consume when the next
                // token is actually an Int literal.
                if let Some(next) = self.tokens.get(self.pos + 1) {
                    if let TokenKind::Int(n) = next.kind {
                        let raw = (n as i64).wrapping_neg();
                        let v = apply_int_suffix(raw, next.numeric_suffix.as_ref());
                        self.bump(); // -
                        self.bump(); // Int
                        return Ok(Some(ilang_ast::Pattern {
                            kind: ilang_ast::PatternKind::IntLit(v),
                            span,
                        }));
                    }
                }
                Ok(None)
            }
            TokenKind::Str(s) => {
                let v = s.clone();
                self.bump();
                Ok(Some(ilang_ast::Pattern {
                    kind: ilang_ast::PatternKind::StrLit(v),
                    span,
                }))
            }
            _ => Ok(None),
        }
    }

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
        // Variant names accept ident plus the promoted keywords.
        // Mirrors `Parser::expect_member_name`.
        let is_name = |k: &TokenKind| {
            matches!(
                k,
                TokenKind::Ident(_)
                    | TokenKind::Class
                    | TokenKind::None_
                    | TokenKind::Override
                    | TokenKind::True
                    | TokenKind::False
                    | TokenKind::Some_
                    | TokenKind::As
                    | TokenKind::In
                    | TokenKind::Super
                    | TokenKind::This
                    | TokenKind::Extends
                    | TokenKind::Return
            )
        };
        let t0 = &self.tokens[self.pos].kind;
        if !is_name(t0) {
            return false;
        }
        let t1 = self.tokens.get(self.pos + 1).map(|t| &t.kind);
        let brace_pos = if matches!(t1, Some(TokenKind::Dot)) {
            // long form `Enum.Variant`
            let t2 = self.tokens.get(self.pos + 2).map(|t| &t.kind);
            match t2 {
                Some(k) if is_name(k) => self.pos + 3,
                _ => return false,
            }
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
        // Literal patterns (int / string). `true` / `false` parse as
        // Variant — the type checker rewrites them when the scrutinee
        // is a bool, otherwise they're enum-variant patterns.
        if let Some(p) = self.try_parse_literal_pattern()? {
            return Ok(p);
        }
        let first = self.expect_member_name("variant name")?;
        let (enum_name, variant) = if matches!(self.peek().kind, TokenKind::Dot) {
            self.bump();
            let v = self.expect_member_name("variant name")?;
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
        // Literal patterns (int / string). `true` / `false` parse as
        // Variant — the type checker rewrites them when the scrutinee
        // is a bool, otherwise they're enum-variant patterns.
        if let Some(p) = self.try_parse_literal_pattern()? {
            return Ok(p);
        }
        // Long form `EnumName.Variant` vs. short form `Variant` (the
        // checker fills in the enum name from the scrutinee). Detect
        // by looking for `.` after the first ident.
        let first = self.expect_member_name("pattern (variant or `_`)")?;
        let (enum_name, variant) = if matches!(self.peek().kind, TokenKind::Dot) {
            self.bump();
            let v = self.expect_member_name("variant name")?;
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
                    } else if !matches!(self.peek().kind, TokenKind::RBrace)
                        && !self.peek().leading_newline
                    {
                        let p = self.peek();
                        return Err(ParseError::Unexpected {
                            found: p.kind.clone(),
                            expected: "',' or newline between struct-pattern fields".into(),
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
        let span = self.peek().span;
        self.expect(&TokenKind::Fn, "'fn'")?;
        self.expect(&TokenKind::LParen, "'('")?;
        let params = self.parse_param_list()?;
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

    /// Parse a comma-separated `expr, expr, ...)` list. The opening
    /// `(` must have been consumed; the closing `)` is consumed
    /// here. Trailing comma is allowed (matches the rest of the
    /// language's punctuation flexibility).
    fn parse_call_args(&mut self) -> Result<Vec<Expr>, ParseError> {
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
        Ok(args)
    }
}
