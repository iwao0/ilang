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
//!
//! The driver here owns the Pratt loop (`parse_expr`) plus the
//! continuation parsers (`assignment` / `range` / `cast` / `is`).
//! Per-position dispatch lives in siblings — `prefix` (primary
//! forms + unary), `postfix` (`.field` / `.method` / `[idx]` /
//! struct lit / `?`), `control` (`if` / `match` / `for` / `while`
//! / `fn` / `map` literal), and `pattern` (match-arm patterns).

use ilang_ast::{BinOp, Expr, ExprKind, LogicalOp, Span};
use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;

mod control;
mod pattern;
mod postfix;
mod prefix;

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
pub(in crate::expr) fn wrap_numeric_suffix(
    lit: Expr,
    suffix: Option<ilang_ast::Type>,
    span: Span,
) -> Expr {
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
pub(in crate::expr) fn flatten_var_dot_chain(e: &Expr) -> Option<String> {
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
}
