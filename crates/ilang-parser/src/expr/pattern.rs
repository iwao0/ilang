//! Pattern parsing for `match` arms and `if let`.
//!
//! Patterns can be literal (`42` / `"hi"` / range like `1..=9`),
//! wildcard (`_`), bool, or a variant — unit / tuple / struct.
//! Variant arms in `match` carry a `{ body }` so the parser has to
//! disambiguate the `{` after a variant name: it belongs to a
//! struct-payload pattern only when the matching `}` is itself
//! followed by another `{` (that second `{` is the arm body).

use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;

use super::apply_int_suffix;

impl<'a> Parser<'a> {
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
        // `Int ..= Int`, with a leading `-` on either side, or a
        // half-open form (`..N`, `..=N`, `N..`).
        // Half-open `..N` / `..=N` (no low):
        let dot_kind = match &self.peek().kind {
            TokenKind::DotDot => Some(false),
            TokenKind::DotDotEq => Some(true),
            _ => None,
        };
        if let Some(inc) = dot_kind {
            if let Some((high, high_len)) = read_signed_int(self, self.pos + 1) {
                self.bump(); // dot-dot token
                for _ in 0..high_len {
                    self.bump();
                }
                return Ok(Some(ilang_ast::Pattern {
                    kind: ilang_ast::PatternKind::IntRange {
                        low: None,
                        high: Some(high),
                        inclusive: inc,
                    },
                    span,
                }));
            }
        }
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
                            low: Some(low),
                            high: Some(high),
                            inclusive: inc,
                        },
                        span,
                    }));
                }
                // `low..` half-open. Only the exclusive form makes
                // sense (`low..=` without high is rejected). Commit
                // when the next token after `..` doesn't start an
                // integer literal — i.e. the arm-body `{`.
                let after_dot = after_low + 1;
                let next_kind = self.tokens.get(after_dot).map(|t| &t.kind);
                if matches!(inc, false) && matches!(next_kind, Some(TokenKind::LBrace)) {
                    for _ in 0..low_len {
                        self.bump();
                    }
                    self.bump(); // dot-dot token
                    return Ok(Some(ilang_ast::Pattern {
                        kind: ilang_ast::PatternKind::IntRange {
                            low: Some(low),
                            high: None,
                            inclusive: false,
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

    pub(super) fn parse_pattern_in_arm(&mut self) -> Result<ilang_ast::Pattern, ParseError> {
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
                    | TokenKind::Enum
                    | TokenKind::Fn
                    | TokenKind::None_
                    | TokenKind::Override
                    | TokenKind::True
                    | TokenKind::False
                    | TokenKind::Some_
                    | TokenKind::As
                    | TokenKind::In
                    | TokenKind::Super
                    | TokenKind::This
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
                ilang_ast::PatternBindings::Tuple(names.into())
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
                ilang_ast::PatternBindings::Struct(fs.into())
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
}
