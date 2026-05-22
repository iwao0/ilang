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

use ilang_ast::{BinOp, Expr, ExprKind, LogicalOp, Span, UnOp, Symbol};
use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;
use crate::stmt::parse_block;

mod pattern;

/// Truncate an `i64` literal to its declared numeric suffix's width
/// (e.g. `300` with `_u8` becomes `44`). Mirrors the runtime behaviour
/// of `300 as u8 as i64`, so a suffixed pattern literal matches the
/// same bit-pattern the equivalent expression would produce. `None`
/// suffix is a no-op; non-integer suffixes (`f32` / `f64`) are
/// already turned into Float by the lexer and never reach this path.
pub(super) fn apply_int_suffix(n: i64, suffix: Option<&ilang_ast::Type>) -> i64 {
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

/// Wrap a numeric literal in an `as Ty` cast when the lexer attached a
/// numeric suffix (e.g. `1u8`, `3.14_f32`). When no suffix is present
/// the literal is returned unchanged.
fn wrap_numeric_suffix(lit: Expr, suffix: Option<ilang_ast::Type>, span: Span) -> Expr {
    match suffix {
        Some(ty) => Expr::new(ExprKind::Cast { expr: Box::new(lit), ty }, span),
        None => lit,
    }
}

/// Returns `Some("a.b.Foo")` for a chain of `Var`/`Field` whose root
/// is a `Var`, otherwise `None`. Used by struct-literal disambiguation
/// so `module.Foo { x: 1 }` is recognised the same way `Foo { x: 1 }`
/// is — the parser emits the dotted name as the `class` of the
/// `StructLit`, leaving module-prefix resolution to the loader.
fn flatten_var_dot_chain(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Var(n) => Some(n.as_str().to_string()),
        ExprKind::Field { obj, name } => {
            let base = flatten_var_dot_chain(obj)?;
            Some(format!("{base}.{name}"))
        }
        _ => None,
    }
}

/// Map a compound-assignment token to its underlying binary op
/// (`+=` → `Add`). Returns `None` for plain `=` and unrelated tokens.
fn compound_assign_op(k: &TokenKind) -> Option<BinOp> {
    Some(match k {
        TokenKind::PlusEq => BinOp::Add,
        TokenKind::MinusEq => BinOp::Sub,
        TokenKind::StarEq => BinOp::Mul,
        TokenKind::SlashEq => BinOp::Div,
        TokenKind::PercentEq => BinOp::Rem,
        TokenKind::AmpEq => BinOp::BitAnd,
        TokenKind::PipeEq => BinOp::BitOr,
        TokenKind::CaretEq => BinOp::BitXor,
        TokenKind::LtLtEq => BinOp::Shl,
        TokenKind::GtGtEq => BinOp::Shr,
        _ => return None,
    })
}

/// Recognise short-circuit logical operators and their (l_bp, r_bp).
fn logical_op_bp(k: &TokenKind) -> Option<(LogicalOp, u8, u8)> {
    match k {
        TokenKind::PipePipe => Some((LogicalOp::Or, 5, 6)),
        TokenKind::AmpAmp => Some((LogicalOp::And, 7, 8)),
        _ => None,
    }
}

/// Precedence table for the standard left-associative binary
/// operators. Returns `(op, l_bp, r_bp)` for each recognised token.
fn binary_op_bp(k: &TokenKind) -> Option<(BinOp, u8, u8)> {
    Some(match k {
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
        _ => return None,
    })
}

impl<'a> Parser<'a> {
    pub(crate) fn parse_expr(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_prefix()?;
        // Postfix: `.field` and `.method(args)` chains. These bind tighter
        // than any infix operator.
        lhs = self.parse_postfix(lhs)?;
        loop {
            // Assignment (`=` / `+=` / `-=` / ...): bp 2/1, right-assoc.
            let compound = compound_assign_op(&self.peek().kind);
            if matches!(self.peek().kind, TokenKind::Equals) || compound.is_some() {
                if 2u8 < min_bp {
                    break;
                }
                lhs = self.parse_assignment_continuation(lhs, compound)?;
                continue;
            }

            // Range (`..` / `..=`): bp 3/4. Lowest non-assignment so
            // `1..n+1` parses as `1..(n+1)` and `for i in 1..len(xs)`
            // works.
            if matches!(self.peek().kind, TokenKind::DotDot | TokenKind::DotDotEq) {
                if 3u8 < min_bp {
                    break;
                }
                lhs = self.parse_range_continuation(lhs)?;
                continue;
            }

            // Short-circuit logical (`||` / `&&`).
            if let Some((logop, l_bp, r_bp)) = logical_op_bp(&self.peek().kind) {
                if l_bp < min_bp {
                    break;
                }
                self.bump();
                let rhs = self.parse_expr(r_bp)?;
                let span = lhs.span.to(rhs.span);
                lhs = Expr::new(
                    ExprKind::Logical { op: logop, lhs: Box::new(lhs), rhs: Box::new(rhs) },
                    span,
                );
                continue;
            }

            // Cast (`as T` / `as? T`): bp 25, postfix-style infix.
            if matches!(self.peek().kind, TokenKind::As) {
                if 25u8 < min_bp {
                    break;
                }
                lhs = self.parse_cast_continuation(lhs)?;
                continue;
            }
            // Type test (`is T`): bp 25, postfix-style infix.
            if matches!(self.peek().kind, TokenKind::Is) {
                if 25u8 < min_bp {
                    break;
                }
                lhs = self.parse_is_continuation(lhs)?;
                continue;
            }

            // Standard left-assoc binary (`+`, `*`, `==`, ...).
            let Some((op, l_bp, r_bp)) = binary_op_bp(&self.peek().kind) else {
                break;
            };
            if l_bp < min_bp {
                break;
            }
            self.bump();
            let rhs = self.parse_expr(r_bp)?;
            let span = lhs.span.to(rhs.span);
            lhs = Expr::new(
                ExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) },
                span,
            );
        }
        Ok(lhs)
    }

    /// `=` (or a compound `+=`/`-=`/...) is at `peek`. Consume it,
    /// parse the rhs at the assignment right-bp, desugar compound
    /// forms into `lhs <op> rhs`, and validate the target is one of
    /// `Var` / `Field` / `Index`.
    fn parse_assignment_continuation(
        &mut self,
        lhs: Expr,
        compound: Option<BinOp>,
    ) -> Result<Expr, ParseError> {
        let eq_span = self.peek().span;
        self.bump();
        let rhs = self.parse_expr(1)?;
        let lhs_span = lhs.span;
        // `lhs += rhs` ⇒ `lhs = lhs <op> rhs`. The desugar happens
        // here so the type checker / evaluator only see plain Assign.
        let value = match compound {
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
        Ok(match lhs.kind {
            ExprKind::Var(name) => Expr::new(
                ExprKind::Assign { target: name, value: Box::new(value) },
                lhs_span,
            ),
            ExprKind::Field { obj, name } => Expr::new(
                ExprKind::AssignField {
                    obj,
                    field: name,
                    value: Box::new(value),
                    is_init: false,
                },
                lhs_span,
            ),
            ExprKind::Index { obj, index } => Expr::new(
                ExprKind::AssignIndex { obj, index, value: Box::new(value) },
                lhs_span,
            ),
            _ => return Err(ParseError::InvalidAssignTarget { span: eq_span }),
        })
    }

    /// `..` or `..=` is at `peek`. Consume it, then parse the upper
    /// bound — or treat as open-ended `1..` when the next token can't
    /// start an expression. `..=` requires an upper bound (open-ended
    /// inclusive ranges are nonsensical).
    fn parse_range_continuation(&mut self, lhs: Expr) -> Result<Expr, ParseError> {
        let inclusive = matches!(self.peek().kind, TokenKind::DotDotEq);
        let r_span = self.peek().span;
        self.bump();
        let end = if matches!(
            self.peek().kind,
            TokenKind::LBrace
                | TokenKind::Comma
                | TokenKind::Semicolon
                | TokenKind::RParen
                | TokenKind::RBracket
                | TokenKind::Eof
        ) {
            None
        } else {
            Some(Box::new(self.parse_expr(4)?))
        };
        if inclusive && end.is_none() {
            return Err(ParseError::Unexpected {
                found: self.peek().kind.clone(),
                expected: "`..=` requires an upper bound (try `..` for open-ended)".into(),
                span: r_span,
            });
        }
        Ok(Expr::new(
            ExprKind::Range { start: Some(Box::new(lhs)), end, inclusive },
            r_span,
        ))
    }

    /// `as` is at `peek`. Distinguish `as T` (Cast) from `as? T`
    /// (TypeDowncast) by the optional `?` that follows.
    fn parse_cast_continuation(&mut self, lhs: Expr) -> Result<Expr, ParseError> {
        self.bump();
        let is_downcast = matches!(self.peek().kind, TokenKind::Question);
        if is_downcast {
            self.bump();
        }
        let target = self.parse_type()?;
        let span = lhs.span;
        let kind = if is_downcast {
            ExprKind::TypeDowncast { expr: Box::new(lhs), ty: target }
        } else {
            ExprKind::Cast { expr: Box::new(lhs), ty: target }
        };
        Ok(Expr::new(kind, span))
    }

    /// `is` is at `peek`. Parse `is T` into a runtime type test
    /// returning `bool`.
    fn parse_is_continuation(&mut self, lhs: Expr) -> Result<Expr, ParseError> {
        self.bump();
        let target = self.parse_type()?;
        let span = lhs.span;
        Ok(Expr::new(
            ExprKind::TypeTest { expr: Box::new(lhs), ty: target },
            span,
        ))
    }

    /// Apply postfix `.field` / `.method(args)` chains, repeatedly, to a
    /// parsed primary expression.
    fn parse_postfix(&mut self, mut expr: Expr) -> Result<Expr, ParseError> {
        loop {
            match self.peek().kind {
                TokenKind::LBracket => {
                    self.bump();
                    let index = self.parse_expr(0)?;
                    let close_span = self.peek().span;
                    self.expect(&TokenKind::RBracket, "']'")?;
                    let span = expr.span.to(close_span);
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
                    // Struct literals typically have a handful of fields;
                    // prealloc avoids the 0→4→8 reallocation pair for
                    // the common 3–4-field case.
                    let mut fs: Vec<(Symbol, Expr)> = Vec::with_capacity(4);
                    let mut name_spans: Vec<Span> = Vec::with_capacity(4);
                    while !matches!(self.peek().kind, TokenKind::RBrace) {
                        let fname_span = self.peek().span;
                        let fname = self.expect_ident("field name")?;
                        self.expect(&TokenKind::Colon, "':'")?;
                        let fval = self.parse_expr(0)?;
                        fs.push((fname, fval));
                        name_spans.push(fname_span);
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
                    let close_span = self.peek().span;
                    self.expect(&TokenKind::RBrace, "'}'")?;
                    let class_name = flatten_var_dot_chain(&expr)
                        .expect("matched in the guard above");
                    let span = expr.span.to(close_span);
                    expr = Expr::new(
                        ExprKind::StructLit {
                            class: class_name.into(),
                            fields: fs.into(),
                            field_name_spans: name_spans.into(),
                        },
                        span,
                    );
                }
                // Postfix `?` — Result short-circuit. `e?` evaluates
                // to the `ok` payload when `e` is `Result.ok(v)`, or
                // early-returns `Result.err(err)` from the enclosing
                // fn when `e` is `Result.err(err)`.
                //
                // Desugars in-place to
                //   { let __try_L_C = <e>
                //     match __try_L_C {
                //         ok(__try_v_L_C) { __try_v_L_C }
                //         err(__try_e_L_C) { return Result.err(__try_e_L_C) }
                //     } }
                // where L_C tags the names with the `?`'s source
                // position so multiple uses in one fn don't collide.
                TokenKind::Question => {
                    let q_span = self.peek().span;
                    self.bump();
                    let span = expr.span.to(q_span);
                    let line = q_span.line;
                    let col = q_span.col;
                    let tmp: Symbol =
                        format!("__try_{line}_{col}").as_str().into();
                    let ok_v: Symbol =
                        format!("__try_v_{line}_{col}").as_str().into();
                    let err_e: Symbol =
                        format!("__try_e_{line}_{col}").as_str().into();
                    let ok_body = Expr::new(ExprKind::Var(ok_v), span);
                    let err_payload = Expr::new(
                        ExprKind::EnumCtor {
                            enum_name: Symbol::intern("Result"),
                            variant: Symbol::intern("err"),
                            args: ilang_ast::CtorArgs::Tuple(Box::new([
                                Expr::new(ExprKind::Var(err_e), span),
                            ])),
                        },
                        span,
                    );
                    let err_body = Expr::new(
                        ExprKind::Return(Some(Box::new(err_payload))),
                        span,
                    );
                    let arms = vec![
                        ilang_ast::MatchArm {
                            pattern: ilang_ast::Pattern {
                                kind: ilang_ast::PatternKind::Variant {
                                    enum_name: None,
                                    variant: Symbol::intern("ok"),
                                    bindings: ilang_ast::PatternBindings::Tuple(
                                        Box::new([ok_v]),
                                    ),
                                },
                                span,
                            },
                            body: ok_body,
                            span,
                        },
                        ilang_ast::MatchArm {
                            pattern: ilang_ast::Pattern {
                                kind: ilang_ast::PatternKind::Variant {
                                    enum_name: None,
                                    variant: Symbol::intern("err"),
                                    bindings: ilang_ast::PatternBindings::Tuple(
                                        Box::new([err_e]),
                                    ),
                                },
                                span,
                            },
                            body: err_body,
                            span,
                        },
                    ];
                    let match_expr = Expr::new(
                        ExprKind::Match {
                            scrutinee: Box::new(Expr::new(
                                ExprKind::Var(tmp),
                                span,
                            )),
                            arms: arms.into_boxed_slice(),
                        },
                        span,
                    );
                    let block = ilang_ast::Block {
                        stmts: vec![ilang_ast::Stmt {
                            kind: ilang_ast::StmtKind::Let {
                                is_pub: false,
                                is_const: false,
                                name: tmp,
                                ty: None,
                                value: expr,
                            },
                            span,
                            source_module: None,
                        }],
                        tail: Some(Box::new(match_expr)),
                    };
                    expr = Expr::new(ExprKind::Block(block), span);
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
            let name_span = self.peek().span;
            let name = self.expect_member_name("field or method name")?;
            if matches!(self.peek().kind, TokenKind::LParen) {
                self.bump();
                let mut args = Vec::with_capacity(4);
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
                let close_span = self.peek().span;
                self.expect(&TokenKind::RParen, "')'")?;
                let span = expr.span.to(close_span);
                expr = Expr::new(
                    ExprKind::MethodCall {
                        obj: Box::new(expr),
                        method: name,
                        args: args.into(),
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
                let mut fs = Vec::with_capacity(4);
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
                let close_span = self.peek().span;
                self.expect(&TokenKind::RBrace, "'}'")?;
                let span = expr.span.to(close_span);
                let enum_name = match expr.kind {
                    ExprKind::Var(n) => n,
                    _ => unreachable!(),
                };
                expr = Expr::new(
                    ExprKind::EnumCtor {
                        enum_name,
                        variant: name,
                        args: ilang_ast::CtorArgs::Struct(fs.into()),
                    },
                    span,
                );
            } else {
                let span = expr.span.to(name_span);
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
            // Literals
            TokenKind::Int(n) => {
                let suffix = t.numeric_suffix.clone();
                self.bump();
                Ok(wrap_numeric_suffix(Expr::new(ExprKind::Int(n), span), suffix, span))
            }
            TokenKind::Float(f) => {
                let suffix = t.numeric_suffix.clone();
                self.bump();
                Ok(wrap_numeric_suffix(Expr::new(ExprKind::Float(f), span), suffix, span))
            }
            TokenKind::Str(s) => { self.bump(); Ok(Expr::new(ExprKind::Str(s), span)) }
            TokenKind::True => { self.bump(); Ok(Expr::new(ExprKind::Bool(true), span)) }
            TokenKind::False => { self.bump(); Ok(Expr::new(ExprKind::Bool(false), span)) }
            TokenKind::None_ => { self.bump(); Ok(Expr::new(ExprKind::None, span)) }
            TokenKind::This => { self.bump(); Ok(Expr::new(ExprKind::This, span)) }
            TokenKind::Continue => { self.bump(); Ok(Expr::new(ExprKind::Continue, span)) }

            // Compound prefix expressions delegated to per-form helpers.
            TokenKind::Super => self.parse_super_call(span),
            TokenKind::New => self.parse_new_expr(span),
            TokenKind::Some_ => self.parse_some_expr(span),
            TokenKind::Match => self.parse_match_expr(span),
            TokenKind::LParen => self.parse_paren_or_tuple(span),
            TokenKind::LBrace => self.parse_brace_prefix(span),
            TokenKind::LBracket => self.parse_array_literal(span),

            // Control-flow expression starts handled in their own modules.
            TokenKind::If => self.parse_if(),
            TokenKind::Fn => self.parse_fn_expr(),
            TokenKind::While => self.parse_while(),
            TokenKind::Loop => self.parse_loop(),
            TokenKind::For => self.parse_for(),

            // `await expr` — prefix operator. Binds tighter than the
            // top-level binops; takes the next unary expression so
            // `await p + 1` parses as `(await p) + 1`. The desugar
            // pass turns it into a `.then` continuation.
            TokenKind::Await => {
                self.bump();
                // Parse at the same precedence as `!` so chaining
                // `await await p` works and method-call postfix
                // (`.then(...)`) on the awaited value still binds.
                let inner = self.parse_expr(30)?;
                let full = span.to(inner.span);
                Ok(Expr::new(ExprKind::Await(Box::new(inner)), full))
            }

            // `break` / `return` share the optional-operand heuristic.
            TokenKind::Break => {
                self.bump();
                let value = self.parse_optional_operand()?;
                Ok(Expr::new(ExprKind::Break(value), span))
            }
            TokenKind::Return => {
                self.bump();
                let value = self.parse_optional_operand()?;
                Ok(Expr::new(ExprKind::Return(value), span))
            }

            // Unary prefix operators. `-` is special-cased to fold
            // `-<IntLit>` into a single `Int` (see parse_unary_minus).
            TokenKind::Minus => self.parse_unary_minus(span),
            TokenKind::Plus => self.parse_unary_op(UnOp::Pos, span),
            TokenKind::Bang => self.parse_unary_op(UnOp::Not, span),
            TokenKind::Tilde => self.parse_unary_op(UnOp::BitNot, span),
            // `&local` — address-of (FFI). Only allowed inside an
            // `@extern(C)` context; the type checker enforces that.
            // Tokenised as `Amp`, which doubles as the binary
            // bitwise-AND operator in infix position — the Pratt
            // parser keeps the two disambiguated by context.
            TokenKind::Amp => self.parse_unary_op(UnOp::AddrOf, span),

            TokenKind::Ident(name) => {
                self.bump();
                if matches!(self.peek().kind, TokenKind::LParen) {
                    self.bump();
                    let args = self.parse_call_args()?;
                    Ok(Expr::new(ExprKind::Call { callee: name.into(), args: args.into() }, span))
                } else {
                    Ok(Expr::new(ExprKind::Var(name.into()), span))
                }
            }

            other => Err(ParseError::Unexpected {
                found: other,
                expected: "number, identifier, '-', '+' or '('".into(),
                span: t.span,
            }),
        }
    }

    /// `super.method(args)` → `SuperCall { method: Some, .. }`,
    /// `super(args)` → `SuperCall { method: None, .. }`.
    fn parse_super_call(&mut self, span: Span) -> Result<Expr, ParseError> {
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
        Ok(Expr::new(ExprKind::SuperCall { method, args: args.into() }, span))
    }

    /// `new Cls<T, U>(args)`. The class name may be dotted
    /// (`module.Cls`); the optional type-argument list is unambiguous
    /// after the class name because `<` can never start an expression
    /// here.
    fn parse_new_expr(&mut self, span: Span) -> Result<Expr, ParseError> {
        self.bump();
        let mut class_str = self.expect_ident("class name")?.as_str().to_string();
        while matches!(self.peek().kind, TokenKind::Dot) {
            self.bump();
            let part = self.expect_ident("class name segment")?;
            class_str.push('.');
            class_str.push_str(part.as_str());
        }
        let class: Symbol = class_str.into();
        let type_args = if matches!(self.peek().kind, TokenKind::Lt) {
            self.parse_type_args()?
        } else {
            Vec::new()
        };
        self.expect(&TokenKind::LParen, "'('")?;
        let mut args = Vec::with_capacity(4);
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
        Ok(Expr::new(
            ExprKind::New { class, type_args: type_args.into(), args: args.into(), init_method: None },
            span,
        ))
    }

    /// `some(expr)`.
    fn parse_some_expr(&mut self, span: Span) -> Result<Expr, ParseError> {
        self.bump();
        self.expect(&TokenKind::LParen, "'('")?;
        let inner = self.parse_expr(0)?;
        self.expect(&TokenKind::RParen, "')'")?;
        Ok(Expr::new(ExprKind::Some(Box::new(inner)), span))
    }

    /// `match scrutinee { pattern { body } [,|newline] ... }`.
    /// Arm bodies are brace-delimited blocks (no `=>`).
    fn parse_match_expr(&mut self, span: Span) -> Result<Expr, ParseError> {
        self.bump();
        let scrutinee = self.parse_expr(0)?;
        self.expect(&TokenKind::LBrace, "'{'")?;
        let mut arms = Vec::with_capacity(4);
        while !matches!(self.peek().kind, TokenKind::RBrace) {
            let arm_span = self.peek().span;
            let pattern = self.parse_pattern_in_arm()?;
            let body_span = self.peek().span;
            let body_block = parse_block(self)?;
            let body = Expr::new(ExprKind::Block(body_block), body_span);
            arms.push(ilang_ast::MatchArm { pattern, body, span: arm_span });
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
            ExprKind::Match { scrutinee: Box::new(scrutinee), arms: arms.into() },
            span,
        ))
    }

    /// `(e)` — parenthesised expression — or `(a, b, ...)` — tuple.
    /// A trailing comma is accepted inside the tuple form.
    fn parse_paren_or_tuple(&mut self, span: Span) -> Result<Expr, ParseError> {
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
            let close_span = self.peek().span;
            self.expect(&TokenKind::RParen, "')'")?;
            return Ok(Expr::new(ExprKind::Tuple(elems.into()), span.to(close_span)));
        }
        self.expect(&TokenKind::RParen, "')'")?;
        Ok(first)
    }

    /// `{ ... }` in expression position. Disambiguates map literal vs.
    /// block: a `{` followed by a key token (string / int / bool /
    /// `-Int`) and then `:` is a map literal; otherwise it's a block.
    /// The tokens that can start a key never form a valid statement
    /// followed by `:`, so this rule has no false positives.
    fn parse_brace_prefix(&mut self, span: Span) -> Result<Expr, ParseError> {
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
        if positive_key || neg_int_key {
            self.parse_map_literal(span)
        } else {
            let block = parse_block(self)?;
            Ok(Expr::new(ExprKind::Block(block), span))
        }
    }

    /// `[a, b, ...]`. Trailing comma allowed.
    fn parse_array_literal(&mut self, span: Span) -> Result<Expr, ParseError> {
        self.bump();
        let mut elements = Vec::with_capacity(4);
        while !matches!(self.peek().kind, TokenKind::RBracket) {
            elements.push(self.parse_expr(0)?);
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        self.expect(&TokenKind::RBracket, "']'")?;
        Ok(Expr::new(ExprKind::Array(elements.into()), span))
    }

    /// Shared body for the trivial unary prefix operators (`!`, `~`,
    /// `&`, `+`). `-` has its own helper for literal-folding.
    fn parse_unary_op(&mut self, op: UnOp, span: Span) -> Result<Expr, ParseError> {
        self.bump();
        let e = self.parse_expr(30)?;
        let full = span.to(e.span);
        Ok(Expr::new(ExprKind::Unary { op, expr: Box::new(e) }, full))
    }

    /// Prefix `-`. Folds `-<IntLit>` into a single `Int` so that the
    /// minimum signed values (`i64::MIN`, `i32::MIN`, ...) are writable
    /// as `-N`. The suffixed form (`-128_i8`) shows up as
    /// `Cast{Int(n), ty}`, so peel that wrapper too.
    fn parse_unary_minus(&mut self, span: Span) -> Result<Expr, ParseError> {
        self.bump();
        let e = self.parse_expr(30)?;
        let full = span.to(e.span);
        if let ExprKind::Int(n) = e.kind {
            return Ok(Expr::new(ExprKind::Int(n.wrapping_neg()), full));
        }
        if let ExprKind::Cast { expr: inner, ty } = &e.kind {
            if let ExprKind::Int(n) = inner.kind {
                let neg = Expr::new(ExprKind::Int(n.wrapping_neg()), inner.span);
                return Ok(Expr::new(
                    ExprKind::Cast { expr: Box::new(neg), ty: ty.clone() },
                    full,
                ));
            }
        }
        Ok(Expr::new(ExprKind::Unary { op: UnOp::Neg, expr: Box::new(e) }, full))
    }

    /// Operand-presence heuristic shared by `break` and `return`.
    /// The operand is omitted when the next token is a statement
    /// terminator (`;`, `}`, EOF) or starts a new logical line (ASI).
    fn parse_optional_operand(&mut self) -> Result<Option<Box<Expr>>, ParseError> {
        let next = self.peek();
        let no_value = matches!(
            next.kind,
            TokenKind::Semicolon | TokenKind::RBrace | TokenKind::Eof
        ) || next.leading_newline;
        if no_value {
            Ok(None)
        } else {
            Ok(Some(Box::new(self.parse_expr(0)?)))
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
        let mut entries = Vec::with_capacity(4);
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
        let close_span = self.peek().span;
        self.expect(&TokenKind::RBrace, "'}'")?;
        Ok(Expr::new(ExprKind::MapLit(entries.into()), span.to(close_span)))
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
        let close_span = self.prev_span();
        Ok(Expr::new(
            ExprKind::FnExpr { params: params.into(), ret, body },
            span.to(close_span),
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
        let close_span = self.prev_span();
        Ok(Expr::new(
            ExprKind::ForIn {
                var: var.into(),
                iter: Box::new(iter),
                body,
            },
            span.to(close_span),
        ))
    }

    /// Parse a comma-separated `expr, expr, ...)` list. The opening
    /// `(` must have been consumed; the closing `)` is consumed
    /// here. Trailing comma is allowed (matches the rest of the
    /// language's punctuation flexibility).
    fn parse_call_args(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut args = Vec::with_capacity(4);
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
