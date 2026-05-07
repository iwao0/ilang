use crate::error::LexError;
use crate::token::{Span, Token, TokenKind};

pub fn tokenize(src: &str) -> Result<Vec<Token>, LexError> {
    let mut lexer = Lexer::new(src);
    let mut tokens = Vec::new();
    loop {
        let tok = lexer.next_token()?;
        let is_eof = matches!(tok.kind, TokenKind::Eof);
        tokens.push(tok);
        if is_eof {
            break;
        }
    }
    Ok(tokens)
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

struct Lexer<'a> {
    chars: std::str::Chars<'a>,
    peeked: Option<char>,
    line: u32,
    col: u32,
    /// Position of the most recently bumped character (1-based, inclusive).
    /// Used to populate the end of a token's `Span` after reading.
    last_line: u32,
    last_col: u32,
    /// Becomes `true` whenever whitespace contains at least one `\n`; the
    /// next token consumes this flag so it knows a newline preceded it.
    pending_newline: bool,
}

/// Snapshot of all mutable lexer state — used to roll back when a
/// speculative read (numeric type suffix) turns out not to match.
struct LexerSnapshot<'a> {
    chars: std::str::Chars<'a>,
    peeked: Option<char>,
    line: u32,
    col: u32,
    last_line: u32,
    last_col: u32,
    pending_newline: bool,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        // Skip a leading UTF-8 BOM (U+FEFF) if present — many editors
        // insert one and we don't want it surfacing as an unexpected
        // character on the first token.
        let src = src.strip_prefix('\u{FEFF}').unwrap_or(src);
        Self {
            chars: src.chars(),
            peeked: None,
            line: 1,
            col: 1,
            last_line: 1,
            last_col: 1,
            pending_newline: false,
        }
    }

    fn peek(&mut self) -> Option<char> {
        if self.peeked.is_none() {
            self.peeked = self.chars.next();
        }
        self.peeked
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peeked.take().or_else(|| self.chars.next())?;
        // Remember the position of *this* char so that token spans can
        // record their last char (inclusive end position).
        self.last_line = self.line;
        self.last_col = self.col;
        // Treat both `\n` and a lone `\r` (old-Mac line ending) as line
        // breaks. CRLF is handled by letting the `\n` advance the line —
        // the preceding `\r` only bumps `col`.
        if c == '\n' || (c == '\r' && self.peek() != Some('\n')) {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn skip_whitespace(&mut self) -> Result<(), LexError> {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    if c == '\n' || c == '\r' {
                        self.pending_newline = true;
                    }
                    self.bump();
                }
                // Line comment: `// ...` to end of line. The `\n` itself
                // (if any) is left for the whitespace branch above so it
                // still sets `pending_newline`, preserving JS-style ASI.
                Some('/') if self.peek_second() == Some('/') => {
                    self.bump();
                    self.bump();
                    while let Some(c) = self.peek() {
                        if c == '\n' || c == '\r' {
                            break;
                        }
                        self.bump();
                    }
                }
                // Block comment: `/* ... */`. Nestable (Rust-style) so
                // commenting out a region that already contains
                // `/* ... */` works. Newlines inside still set
                // `pending_newline` to keep ASI behavior unsurprising.
                Some('/') if self.peek_second() == Some('*') => {
                    let start_span = Span::new(self.line, self.col);
                    self.bump(); // /
                    self.bump(); // *
                    let mut depth: u32 = 1;
                    while depth > 0 {
                        match self.peek() {
                            None => {
                                return Err(LexError::UnterminatedBlockComment {
                                    span: start_span,
                                });
                            }
                            Some('/') if self.peek_second() == Some('*') => {
                                self.bump();
                                self.bump();
                                depth += 1;
                            }
                            Some('*') if self.peek_second() == Some('/') => {
                                self.bump();
                                self.bump();
                                depth -= 1;
                            }
                            Some('\n') | Some('\r') => {
                                self.pending_newline = true;
                                self.bump();
                            }
                            Some(_) => {
                                self.bump();
                            }
                        }
                    }
                }
                _ => break,
            }
        }
        Ok(())
    }

    fn peek_second(&mut self) -> Option<char> {
        // Need to materialize the first peeked char first so the underlying
        // iterator advances to the second one.
        let _ = self.peek();
        self.chars.clone().next()
    }

    fn next_token(&mut self) -> Result<Token, LexError> {
        self.skip_whitespace()?;
        let leading_newline = std::mem::take(&mut self.pending_newline);
        let line = self.line;
        let col = self.col;
        // Inner read sites use this as a point span (start position) for
        // any error they raise. The token's final span is widened below
        // to cover the last char actually consumed.
        let span = Span::new(line, col);

        let Some(c) = self.peek() else {
            return Ok(Token {
                kind: TokenKind::Eof,
                span,
                leading_newline,
                numeric_suffix: None,
            });
        };

        let kind = match c {
            '+' => {
                self.bump();
                if matches!(self.peek(), Some('=')) {
                    self.bump();
                    TokenKind::PlusEq
                } else {
                    TokenKind::Plus
                }
            }
            '-' => {
                self.bump();
                if matches!(self.peek(), Some('=')) {
                    self.bump();
                    TokenKind::MinusEq
                } else {
                    TokenKind::Minus
                }
            }
            '*' => {
                self.bump();
                if matches!(self.peek(), Some('=')) {
                    self.bump();
                    TokenKind::StarEq
                } else {
                    TokenKind::Star
                }
            }
            '/' => {
                self.bump();
                if matches!(self.peek(), Some('=')) {
                    self.bump();
                    TokenKind::SlashEq
                } else {
                    TokenKind::Slash
                }
            }
            '%' => {
                self.bump();
                if matches!(self.peek(), Some('=')) {
                    self.bump();
                    TokenKind::PercentEq
                } else {
                    TokenKind::Percent
                }
            }
            '(' => {
                self.bump();
                TokenKind::LParen
            }
            ')' => {
                self.bump();
                TokenKind::RParen
            }
            '{' => {
                self.bump();
                TokenKind::LBrace
            }
            '}' => {
                self.bump();
                TokenKind::RBrace
            }
            '[' => {
                self.bump();
                TokenKind::LBracket
            }
            ']' => {
                self.bump();
                TokenKind::RBracket
            }
            ';' => {
                self.bump();
                TokenKind::Semicolon
            }
            ',' => {
                self.bump();
                TokenKind::Comma
            }
            ':' => {
                self.bump();
                if matches!(self.peek(), Some(':')) {
                    self.bump();
                    TokenKind::ColonColon
                } else {
                    TokenKind::Colon
                }
            }
            '=' => {
                self.bump();
                match self.peek() {
                    Some('=') => {
                        self.bump();
                        TokenKind::EqEq
                    }
                    Some('>') => {
                        self.bump();
                        TokenKind::FatArrow
                    }
                    _ => TokenKind::Equals,
                }
            }
            '!' => {
                self.bump();
                if matches!(self.peek(), Some('=')) {
                    self.bump();
                    TokenKind::BangEq
                } else {
                    TokenKind::Bang
                }
            }
            '<' => {
                self.bump();
                match self.peek() {
                    Some('=') => {
                        self.bump();
                        TokenKind::LtEq
                    }
                    Some('<') => {
                        self.bump();
                        if matches!(self.peek(), Some('=')) {
                            self.bump();
                            TokenKind::LtLtEq
                        } else {
                            TokenKind::LtLt
                        }
                    }
                    _ => TokenKind::Lt,
                }
            }
            '>' => {
                self.bump();
                match self.peek() {
                    Some('=') => {
                        self.bump();
                        TokenKind::GtEq
                    }
                    Some('>') => {
                        self.bump();
                        if matches!(self.peek(), Some('=')) {
                            self.bump();
                            TokenKind::GtGtEq
                        } else {
                            TokenKind::GtGt
                        }
                    }
                    _ => TokenKind::Gt,
                }
            }
            '&' => {
                self.bump();
                match self.peek() {
                    Some('&') => {
                        self.bump();
                        TokenKind::AmpAmp
                    }
                    Some('=') => {
                        self.bump();
                        TokenKind::AmpEq
                    }
                    _ => TokenKind::Amp,
                }
            }
            '|' => {
                self.bump();
                match self.peek() {
                    Some('|') => {
                        self.bump();
                        TokenKind::PipePipe
                    }
                    Some('=') => {
                        self.bump();
                        TokenKind::PipeEq
                    }
                    _ => TokenKind::Pipe,
                }
            }
            '^' => {
                self.bump();
                if matches!(self.peek(), Some('=')) {
                    self.bump();
                    TokenKind::CaretEq
                } else {
                    TokenKind::Caret
                }
            }
            '~' => {
                self.bump();
                TokenKind::Tilde
            }
            '.' if matches!(self.peek_second(), Some(d) if d.is_ascii_digit()) => {
                // Leading-dot float (`.5`). Only enter here when the next
                // char is a digit, so `.foo` and `..` keep their meaning.
                self.read_leading_dot_float(span)?
            }
            '.' => {
                self.bump();
                if self.peek() == Some('.') {
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        TokenKind::DotDotEq
                    } else if self.peek() == Some('.') {
                        self.bump();
                        TokenKind::DotDotDot
                    } else {
                        TokenKind::DotDot
                    }
                } else {
                    TokenKind::Dot
                }
            }
            '@' => {
                self.bump();
                TokenKind::At
            }
            '?' => {
                self.bump();
                TokenKind::Question
            }
            '"' => self.read_string(span)?,
            c if c.is_ascii_digit() => self.read_number(span)?,
            c if is_ident_start(c) => self.read_ident_or_keyword(),
            other => {
                return Err(LexError::UnexpectedChar { ch: other, span });
            }
        };

        // After a numeric body, optionally consume a type suffix
        // (`1_i32`, `1.5f32`, ...). Suffix on a non-numeric token would
        // be nonsensical, so only attempt for Int / Float.
        let numeric_suffix = if matches!(kind, TokenKind::Int(_) | TokenKind::Float(_)) {
            self.try_read_numeric_suffix()?
        } else {
            None
        };

        // An integer literal with a float suffix (`1_f32`) is treated as
        // the equivalent float literal, so the parser doesn't need to
        // special-case it.
        let kind = match (&kind, &numeric_suffix) {
            (TokenKind::Int(n), Some(ilang_ast::Type::F32 | ilang_ast::Type::F64)) => {
                TokenKind::Float(*n as f64)
            }
            _ => kind,
        };

        // Widen the span to cover the last character actually consumed
        // (inclusive end). All bumps update `last_line` / `last_col`.
        let span = Span::range(line, col, self.last_line, self.last_col);

        Ok(Token {
            kind,
            span,
            leading_newline,
            numeric_suffix,
        })
    }

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
    fn try_read_numeric_suffix(&mut self) -> Result<Option<ilang_ast::Type>, LexError> {
        if !matches!(self.peek(), Some(c) if is_ident_start(c)) {
            return Ok(None);
        }
        let snap = LexerSnapshot {
            chars: self.chars.clone(),
            peeked: self.peeked,
            line: self.line,
            col: self.col,
            last_line: self.last_line,
            last_col: self.last_col,
            pending_newline: self.pending_newline,
        };
        let suffix_span = Span::new(self.line, self.col);
        let mut full = String::new();
        while let Some(c) = self.peek() {
            if is_ident_continue(c) {
                full.push(c);
                self.bump();
            } else {
                break;
            }
        }
        let candidate = full.strip_prefix('_').unwrap_or(&full);
        let ty = match candidate {
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
        };
        if let Some(t) = ty {
            return Ok(Some(t));
        }
        if full.starts_with('_') {
            self.chars = snap.chars;
            self.peeked = snap.peeked;
            self.line = snap.line;
            self.col = snap.col;
            self.last_line = snap.last_line;
            self.last_col = snap.last_col;
            self.pending_newline = snap.pending_newline;
            return Ok(None);
        }
        Err(LexError::InvalidNumericSuffix {
            name: full,
            span: suffix_span,
        })
    }

    /// Consume a `"..."` string literal (the leading `"` is the next char).
    /// Supports the basic C-style escapes; everything else is an error.
    /// Raw newlines inside the literal are forbidden — strings must close
    /// on the line they started on.
    fn read_string(&mut self, span: Span) -> Result<TokenKind, LexError> {
        self.bump(); // opening "
        let mut buf = String::new();
        loop {
            match self.peek() {
                None => return Err(LexError::UnterminatedString { span }),
                Some('\n') | Some('\r') => return Err(LexError::UnterminatedString { span }),
                Some('"') => {
                    self.bump();
                    return Ok(TokenKind::Str(buf));
                }
                Some('\\') => {
                    let esc_span = Span::new(self.line, self.col);
                    self.bump();
                    match self.peek() {
                        Some('n') => { self.bump(); buf.push('\n'); }
                        Some('t') => { self.bump(); buf.push('\t'); }
                        Some('r') => { self.bump(); buf.push('\r'); }
                        Some('\\') => { self.bump(); buf.push('\\'); }
                        Some('"') => { self.bump(); buf.push('"'); }
                        Some('0') => { self.bump(); buf.push('\0'); }
                        Some('a') => { self.bump(); buf.push('\x07'); }
                        Some('b') => { self.bump(); buf.push('\x08'); }
                        Some('f') => { self.bump(); buf.push('\x0c'); }
                        Some('v') => { self.bump(); buf.push('\x0b'); }
                        Some('x') => {
                            self.bump();
                            self.read_hex_byte_escape(esc_span, &mut buf)?;
                        }
                        Some('u') => {
                            self.bump();
                            self.read_unicode_escape(esc_span, &mut buf)?;
                        }
                        Some(c) => {
                            return Err(LexError::BadEscape {
                                seq: format!("\\{c}"),
                                span: esc_span,
                            });
                        }
                        None => return Err(LexError::UnterminatedString { span }),
                    }
                }
                Some(c) => {
                    self.bump();
                    buf.push(c);
                }
            }
        }
    }

    /// `\xNN` — exactly two hex digits, must encode an ASCII char
    /// (≤ 0x7F) so we don't synthesize invalid UTF-8.
    fn read_hex_byte_escape(&mut self, esc_span: Span, buf: &mut String) -> Result<(), LexError> {
        let mut hex = String::new();
        for _ in 0..2 {
            match self.peek() {
                Some(c) if c.is_ascii_hexdigit() => {
                    hex.push(c);
                    self.bump();
                }
                _ => {
                    return Err(LexError::BadEscape {
                        seq: format!("\\x{hex}"),
                        span: esc_span,
                    });
                }
            }
        }
        let val = u32::from_str_radix(&hex, 16).unwrap();
        if val > 0x7F {
            return Err(LexError::BadEscape {
                seq: format!("\\x{hex}"),
                span: esc_span,
            });
        }
        buf.push(val as u8 as char);
        Ok(())
    }

    /// `\u{NNNN}` — 1 to 6 hex digits inside braces, must be a valid
    /// Unicode scalar value (excluding surrogates).
    fn read_unicode_escape(&mut self, esc_span: Span, buf: &mut String) -> Result<(), LexError> {
        if self.peek() != Some('{') {
            return Err(LexError::BadEscape {
                seq: "\\u".into(),
                span: esc_span,
            });
        }
        self.bump();
        let mut hex = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_hexdigit() && hex.len() < 6 {
                hex.push(c);
                self.bump();
            } else {
                break;
            }
        }
        if self.peek() != Some('}') {
            return Err(LexError::BadEscape {
                seq: format!("\\u{{{hex}"),
                span: esc_span,
            });
        }
        self.bump();
        if hex.is_empty() {
            return Err(LexError::BadEscape {
                seq: "\\u{}".into(),
                span: esc_span,
            });
        }
        let val = u32::from_str_radix(&hex, 16).unwrap();
        match char::from_u32(val) {
            Some(c) => {
                buf.push(c);
                Ok(())
            }
            None => Err(LexError::BadEscape {
                seq: format!("\\u{{{hex}}}"),
                span: esc_span,
            }),
        }
    }

    fn read_ident_or_keyword(&mut self) -> TokenKind {
        let mut buf = String::new();
        while let Some(c) = self.peek() {
            if is_ident_continue(c) {
                buf.push(c);
                self.bump();
            } else {
                break;
            }
        }
        match buf.as_str() {
            "let" => TokenKind::Let,
            "fn" => TokenKind::Fn,
            "if" => TokenKind::If,
            "elif" => TokenKind::Elif,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "loop" => TokenKind::Loop,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "return" => TokenKind::Return,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "class" => TokenKind::Class,
            "new" => TokenKind::New,
            "this" => TokenKind::This,
            "as" => TokenKind::As,
            "is" => TokenKind::Is,
            "none" => TokenKind::None_,
            "some" => TokenKind::Some_,
            "enum" => TokenKind::Enum,
            "match" => TokenKind::Match,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "use" => TokenKind::Use,
            "const" => TokenKind::Const,
            "extends" => TokenKind::Extends,
            "override" => TokenKind::Override,
            "super" => TokenKind::Super,
            _ => TokenKind::Ident(buf),
        }
    }

    fn read_number(&mut self, span: Span) -> Result<TokenKind, LexError> {
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
    fn read_leading_dot_float(&mut self, span: Span) -> Result<TokenKind, LexError> {
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
