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

use ilang_ast::{BinOp, Expr, ExprKind, LogicalOp, UnOp, Symbol};
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
                            is_init: false,
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
                // Postfix `1..` form: when the next token can't
                // start an expression (block, separator, or
                // closing bracket), treat as an open-ended
                // RangeFrom (`end = None`). Anything else is the
                // usual `1..end` parse.
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
                    Some(Box::new(self.parse_expr(r_bp)?))
                };
                if inclusive && end.is_none() {
                    return Err(ParseError::Unexpected {
                        found: self.peek().kind.clone(),
                        expected: "`..=` requires an upper bound (try `..` for open-ended)".into(),
                        span: r_span,
                    });
                }
                lhs = Expr::new(
                    ExprKind::Range {
                        start: Some(Box::new(lhs)),
                        end,
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
                let span = lhs.span.to(rhs.span);
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

            // `as`-cast / `as?`-downcast: postfix-style infix that
            // takes a type instead of an expression. Tighter than any
            // binary op so `1 + 2 as f64` parses as `1 + ((2) as f64)`.
            if matches!(self.peek().kind, TokenKind::As) {
                let l_bp = 25u8;
                if l_bp < min_bp {
                    break;
                }
                self.bump();
                let is_downcast = matches!(self.peek().kind, TokenKind::Question);
                if is_downcast {
                    self.bump();
                }
                let target = self.parse_type()?;
                let span = lhs.span;
                lhs = Expr::new(
                    if is_downcast {
                        ExprKind::TypeDowncast {
                            expr: Box::new(lhs),
                            ty: target,
                        }
                    } else {
                        ExprKind::Cast {
                            expr: Box::new(lhs),
                            ty: target,
                        }
                    },
                    span,
                );
                continue;
            }
            // `is T` — runtime type test, returns bool. Same precedence
            // as `as` (postfix-style on the lhs).
            if matches!(self.peek().kind, TokenKind::Is) {
                let l_bp = 25u8;
                if l_bp < min_bp {
                    break;
                }
                self.bump();
                let target = self.parse_type()?;
                let span = lhs.span;
                lhs = Expr::new(
                    ExprKind::TypeTest {
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
            let span = lhs.span.to(rhs.span);
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
                    let class_name = flatten_var_dot_chain(&expr)
                        .expect("matched in the guard above");
                    let span = expr.span.to(close_span);
                    expr = Expr::new(
                        ExprKind::StructLit {
                            class: class_name.into(),
                            fields: fs.into(),
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
                Ok(Expr::new(ExprKind::SuperCall { method, args: args.into() }, span))
            }
            TokenKind::New => {
                self.bump();
                // Class name is either bare `Counter` or
                // `module.Counter` (whole-module imported class).
                let mut class_str = self.expect_ident("class name")?.as_str().to_string();
                while matches!(self.peek().kind, TokenKind::Dot) {
                    self.bump();
                    let part = self.expect_ident("class name segment")?;
                    class_str.push('.');
                    class_str.push_str(part.as_str());
                }
                let class: Symbol = class_str.into();
                // Optional `<T, U>` type arguments before the constructor
                // arg list. Unambiguous after `new ClassName` since `<`
                // can never be the start of an expression here.
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
                Ok(Expr::new(ExprKind::New { class, type_args: type_args.into(), args: args.into(), init_method: None }, span))
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
                let full = span.to(e.span);
                Ok(Expr::new(
                    ExprKind::Unary {
                        op: UnOp::Not,
                        expr: Box::new(e),
                    },
                    full,
                ))
            }
            TokenKind::Tilde => {
                self.bump();
                let e = self.parse_expr(30)?;
                let full = span.to(e.span);
                Ok(Expr::new(
                    ExprKind::Unary {
                        op: UnOp::BitNot,
                        expr: Box::new(e),
                    },
                    full,
                ))
            }
            // `&local` — address-of (FFI). Only allowed inside an
            // `@extern(C)` context; the type checker enforces that.
            // Tokenised as `Amp`, which doubles as the binary
            // bitwise-AND operator in infix position — the Pratt
            // parser keeps the two disambiguated by context.
            TokenKind::Amp => {
                self.bump();
                let e = self.parse_expr(30)?;
                let full = span.to(e.span);
                Ok(Expr::new(
                    ExprKind::Unary {
                        op: UnOp::AddrOf,
                        expr: Box::new(e),
                    },
                    full,
                ))
            }
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
            TokenKind::Match => {
                self.bump();
                let scrutinee = self.parse_expr(0)?;
                self.expect(&TokenKind::LBrace, "'{'")?;
                let mut arms = Vec::with_capacity(4);
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
                        arms: arms.into(),
                    },
                    span,
                ))
            }
            TokenKind::Minus => {
                self.bump();
                let e = self.parse_expr(30)?;
                let full = span.to(e.span);
                // Fold `-<IntLit>` into a single `Int` so that minimum
                // values (`i64::MIN`, `i32::MIN`, ...) are writable as
                // `-N`. The suffixed form (`-128_i8`) shows up as
                // `Cast{Int(n), ty}`, so peel that wrapper too.
                if let ExprKind::Int(n) = e.kind {
                    return Ok(Expr::new(ExprKind::Int(n.wrapping_neg()), full));
                }
                if let ExprKind::Cast { expr: inner, ty } = &e.kind {
                    if let ExprKind::Int(n) = inner.kind {
                        let neg = Expr::new(ExprKind::Int(n.wrapping_neg()), inner.span);
                        return Ok(Expr::new(
                            ExprKind::Cast {
                                expr: Box::new(neg),
                                ty: ty.clone(),
                            },
                            full,
                        ));
                    }
                }
                Ok(Expr::new(
                    ExprKind::Unary {
                        op: UnOp::Neg,
                        expr: Box::new(e),
                    },
                    full,
                ))
            }
            TokenKind::Plus => {
                self.bump();
                let e = self.parse_expr(30)?;
                let full = span.to(e.span);
                Ok(Expr::new(
                    ExprKind::Unary {
                        op: UnOp::Pos,
                        expr: Box::new(e),
                    },
                    full,
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
                    let close_span = self.peek().span;
                    self.expect(&TokenKind::RParen, "')'")?;
                    return Ok(Expr::new(ExprKind::Tuple(elems.into()), span.to(close_span)));
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
                let mut elements = Vec::with_capacity(4);
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
                Ok(Expr::new(ExprKind::Array(elements.into()), span))
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
