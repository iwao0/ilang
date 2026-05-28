//! Numeric literal scanning: decimal / radix integers, floats
//! (including leading-dot and exponent forms), the underscore digit
//! separator, and the trailing numeric type-suffix probe (`1_i32`,
//! `1.5f32`, …) with its speculative-read rollback.

use crate::error::LexError;
use crate::token::{Span, TokenKind};

use super::{is_ident_continue, is_ident_start, Lexer, LexerSnapshot};

impl<'a> Lexer<'a> {
    /// Attempt to consume a numeric type suffix at the current position.
    /// Reads the trailing identifier-like sequence as a whole, optionally
    /// strips one leading `_`, and matches against the known type names.
    ///
    /// - Match → `Ok(Some(type))`.
    /// - No trailing ident-like chars → `Ok(None)`.
    /// - Unknown candidate that started with `_` → roll back and return
    ///   `Ok(None)`, since `_xxx` is also a valid identifier.
    /// - Unknown candidate without a leading `_` → `Err(InvalidNumericSuffix)`,
    ///   so `1foo` / `1u8x` don't silently split into two tokens.
    pub(super) fn try_read_numeric_suffix(&mut self) -> Result<Option<ilang_ast::Type>, LexError> {
        if !matches!(self.peek(), Some(c) if is_ident_start(c)) {
            return Ok(None);
        }
        let snap = LexerSnapshot {
            chars: self.chars.clone(),
            peeked: self.peeked,
            peeked2: self.peeked2,
            line: self.line,
            col: self.col,
            last_line: self.last_line,
            last_col: self.last_col,
            pending_newline: self.pending_newline,
            template_stack: self.template_stack.clone(),
        };
        let suffix_span = Span::new(self.line, self.col);
        // Numeric type suffixes are short, fixed-vocabulary tokens
        // (`i8`..`u64`, `f32`, `f64`, optionally preceded by one `_`).
        // The longest valid suffix has 4 bytes (`_u64`), so a tiny
        // stack buffer is enough — no `String` allocation needed on
        // the hot numeric-literal path.
        let mut buf = [0u8; 8];
        let mut len = 0usize;
        let mut overflowed = false;
        while let Some(c) = self.peek() {
            if !is_ident_continue(c) {
                break;
            }
            // ident chars here are ASCII (`is_ident_continue`).
            if len < buf.len() {
                buf[len] = c as u8;
                len += 1;
            } else {
                overflowed = true;
            }
            self.bump();
        }
        // SAFETY: every byte pushed above came from an ASCII char.
        let full = std::str::from_utf8(&buf[..len]).unwrap();
        let leading_underscore = full.starts_with('_');
        let candidate = if leading_underscore { &full[1..] } else { full };
        let ty = if overflowed {
            None
        } else {
            match candidate {
                "i8" => Some(ilang_ast::Type::I8),
                "i16" => Some(ilang_ast::Type::I16),
                "i32" => Some(ilang_ast::Type::I32),
                "i64" => Some(ilang_ast::Type::I64),
                "u8" => Some(ilang_ast::Type::U8),
                "u16" => Some(ilang_ast::Type::U16),
                "u32" => Some(ilang_ast::Type::U32),
                "u64" => Some(ilang_ast::Type::U64),
                "f32" => Some(ilang_ast::Type::F32),
                "f64" => Some(ilang_ast::Type::F64),
                _ => None,
            }
        };
        if let Some(t) = ty {
            return Ok(Some(t));
        }
        if leading_underscore {
            self.chars = snap.chars;
            self.peeked = snap.peeked;
            self.peeked2 = snap.peeked2;
            self.line = snap.line;
            self.col = snap.col;
            self.last_line = snap.last_line;
            self.last_col = snap.last_col;
            self.pending_newline = snap.pending_newline;
            self.template_stack = snap.template_stack;
            return Ok(None);
        }
        // Unknown suffix that wasn't underscore-prefixed: error out. We
        // have to materialize a String for the diagnostic, but only on
        // the error path (the happy / rollback paths above stay
        // allocation-free).
        let name = if overflowed {
            // Recover the full text past the cache by replaying from
            // the snapshot. Rare path — only triggered by inputs like
            // `1u12345`.
            let mut s = String::new();
            let mut iter = snap.chars.clone();
            if let Some(p) = snap.peeked { s.push(p); }
            while let Some(c) = iter.next() {
                if is_ident_continue(c) { s.push(c); } else { break; }
            }
            s
        } else {
            full.to_string()
        };
        Err(LexError::InvalidNumericSuffix {
            name,
            span: suffix_span,
        })
    }

    pub(super) fn read_number(&mut self, span: Span) -> Result<TokenKind, LexError> {
        // Hex / binary prefix: only valid right after a leading `0`. We've
        // already verified the first char is an ASCII digit; check the
        // second to decide which radix to use.
        if self.peek() == Some('0') {
            let mut lookahead = self.chars.clone();
            // peeked may already hold a char; lookahead is the iterator state
            // *after* the peeked one would be consumed.
            if let Some(prefix) = lookahead.next() {
                if prefix == 'x' || prefix == 'X' {
                    self.bump(); // consume '0'
                    self.bump(); // consume 'x' / 'X'
                    return self.read_radix_int(span, 16, "hex");
                }
                if prefix == 'b' || prefix == 'B' {
                    self.bump(); // '0'
                    self.bump(); // 'b' / 'B'
                    return self.read_radix_int(span, 2, "binary");
                }
                if prefix == 'o' || prefix == 'O' {
                    self.bump(); // '0'
                    self.bump(); // 'o' / 'O'
                    return self.read_radix_int(span, 8, "octal");
                }
            }
        }

        let mut buf = String::new();
        let mut is_float = false;

        // Integer part. `_` is only consumed when it sits between digits;
        // otherwise it stays for the next token (so `1_foo` is `Int(1)`
        // followed by `_foo`, not `Int(1)` with `_` silently dropped).
        Self::scan_digits(self, &mut buf, 10);

        // Only treat `.` as the start of a fractional part when a digit
        // immediately follows. This keeps `1..5` (range), `1.method()`
        // (method call on int), and `1.foo` (field access) out of the
        // float body. A bare trailing `.` (`1.` at EOF or before a
        // non-digit) becomes `Int(1) Dot`, so write `1.0` for the float.
        if self.peek() == Some('.')
            && matches!(self.peek_second(), Some(c) if c.is_ascii_digit())
        {
            is_float = true;
            buf.push('.');
            self.bump();
            Self::scan_digits(self, &mut buf, 10);
        }

        if let Some(c) = self.peek() {
            if c == 'e' || c == 'E' {
                is_float = true;
                buf.push(c);
                self.bump();
                if let Some(sign) = self.peek() {
                    if sign == '+' || sign == '-' {
                        buf.push(sign);
                        self.bump();
                    }
                }
                let before = buf.len();
                Self::scan_digits(self, &mut buf, 10);
                if buf.len() == before {
                    return Err(LexError::InvalidNumber {
                        text: buf,
                        span,
                        reason: "exponent has no digits".into(),
                    });
                }
            }
        }

        if is_float {
            buf.parse::<f64>()
                .map(TokenKind::Float)
                .map_err(|e| LexError::InvalidNumber {
                    text: buf.clone(),
                    span,
                    reason: e.to_string(),
                })
        } else {
            // Try i64 first; fall back to u64 (reinterpreted) so values
            // in `(i64::MAX, u64::MAX]` round-trip via the `_u64` suffix.
            match buf.parse::<i64>() {
                Ok(n) => Ok(TokenKind::Int(n)),
                Err(_) => match buf.parse::<u64>() {
                    Ok(n) => Ok(TokenKind::Int(n as i64)),
                    Err(e) => Err(LexError::InvalidNumber {
                        text: buf,
                        span,
                        reason: e.to_string(),
                    }),
                },
            }
        }
    }

    /// `.5`-style float — entered when the main scanner sees `.` followed
    /// by a digit. The integer part is implicitly `0`.
    pub(super) fn read_leading_dot_float(&mut self, span: Span) -> Result<TokenKind, LexError> {
        self.bump(); // consume `.`
        let mut buf = String::from(".");
        self.scan_digits(&mut buf, 10);
        if let Some(c) = self.peek() {
            if c == 'e' || c == 'E' {
                buf.push(c);
                self.bump();
                if let Some(s) = self.peek() {
                    if s == '+' || s == '-' {
                        buf.push(s);
                        self.bump();
                    }
                }
                let before = buf.len();
                self.scan_digits(&mut buf, 10);
                if buf.len() == before {
                    return Err(LexError::InvalidNumber {
                        text: buf,
                        span,
                        reason: "exponent has no digits".into(),
                    });
                }
            }
        }
        buf.parse::<f64>()
            .map(TokenKind::Float)
            .map_err(|e| LexError::InvalidNumber {
                text: buf.clone(),
                span,
                reason: e.to_string(),
            })
    }

    /// Append digits in the given radix to `buf`, treating `_` as a
    /// separator only when it sits strictly between two digits.
    fn scan_digits(&mut self, buf: &mut String, radix: u32) {
        loop {
            match self.peek() {
                Some(c) if c.is_digit(radix) => {
                    buf.push(c);
                    self.bump();
                }
                Some('_') if matches!(self.peek_second(), Some(n) if n.is_digit(radix)) => {
                    // For radix 16, `_f32` / `_f64` are float type
                    // suffixes whose first char also happens to be a
                    // hex digit. Stop the digit scan in that case so
                    // the suffix handler can pick them up.
                    if radix == 16 && self.underscore_starts_float_suffix() {
                        break;
                    }
                    self.bump();
                }
                _ => break,
            }
        }
    }

    /// Inspect (without consuming) the chars after the current `_`. Returns
    /// `true` when they form exactly `f32` or `f64` followed by a non-ident
    /// char — i.e., a complete numeric type suffix that would otherwise be
    /// swallowed as more hex digits.
    fn underscore_starts_float_suffix(&self) -> bool {
        let mut iter = self.chars.clone();
        let mut s = String::new();
        for _ in 0..3 {
            match iter.next() {
                Some(c) if c.is_ascii_alphanumeric() => s.push(c),
                _ => break,
            }
        }
        if s != "f32" && s != "f64" {
            return false;
        }
        // The next char must terminate the ident-like run, otherwise this
        // is something like `_f32x` and not actually a clean suffix.
        match iter.next() {
            Some(c) if c.is_ascii_alphanumeric() || c == '_' => false,
            _ => true,
        }
    }

    /// Read the digit body of a non-decimal integer literal. The `0x` or
    /// `0b` prefix has already been consumed. Underscores are accepted as
    /// digit separators and stripped before parsing.
    fn read_radix_int(
        &mut self,
        span: Span,
        radix: u32,
        label: &str,
    ) -> Result<TokenKind, LexError> {
        let prefix = match radix {
            16 => "0x",
            2 => "0b",
            8 => "0o",
            _ => "0?",
        };
        let mut digits = String::new();
        Self::scan_digits(self, &mut digits, radix);
        // Catch out-of-range digits like `0b102` or `0o78`. Without this,
        // the scan stops at the first invalid digit and the next call
        // to `next_token` would silently lex it as a new integer.
        if let Some(c) = self.peek() {
            if c.is_ascii_digit() && !c.is_digit(radix) {
                let bad_span = Span::new(self.line, self.col);
                return Err(LexError::InvalidNumber {
                    text: format!("{prefix}{digits}{c}"),
                    span: bad_span,
                    reason: format!("{c:?} is not a valid {label} digit"),
                });
            }
        }
        if digits.is_empty() {
            return Err(LexError::InvalidNumber {
                text: prefix.into(),
                span,
                reason: format!("{label} literal needs at least one digit"),
            });
        }
        // Parse as u64 to allow the full bit pattern (e.g. `0xFFFFFFFFFFFFFFFF`)
        // and reinterpret as i64. The user can `as u64` to recover the
        // original unsigned value.
        u64::from_str_radix(&digits, radix)
            .map(|n| TokenKind::Int(n as i64))
            .map_err(|e| LexError::InvalidNumber {
                text: format!("{prefix}{digits}"),
                span,
                reason: e.to_string(),
            })
    }
}
