use crate::error::LexError;
use crate::token::{Span, Token, TokenKind};

pub fn tokenize(src: &str) -> Result<Vec<Token>, LexError> {
    reject_invisible(src)?;
    let mut lexer = Lexer::new(src);
    // Empirically ~1 token per ~4 bytes of source for this language;
    // pre-allocating avoids the geometric reallocation traffic that a
    // bare Vec::new() incurs on every push. Over-allocating slightly is
    // cheaper than the 7+ reallocations the default growth would do for
    // a moderate file.
    let mut tokens = Vec::with_capacity(src.len() / 4 + 16);
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

/// Look-up table for the invisible / bidi-control characters that
/// `reject_invisible` bans. The set covers the bidi overrides /
/// isolates / marks responsible for "trojan source" attacks
/// (CVE-2021-42574), zero-width joiners that turn into invisible
/// identifier glyphs, the alternate line / paragraph separators,
/// and U+FEFF outside the file's leading byte (a leading BOM is
/// silently allowed). String literals can still embed the code
/// points via `\u{...}` escapes.
fn invisible_name(c: char) -> Option<&'static str> {
    Some(match c {
        '\u{202A}' => "left-to-right embedding (LRE)",
        '\u{202B}' => "right-to-left embedding (RLE)",
        '\u{202C}' => "pop directional formatting (PDF)",
        '\u{202D}' => "left-to-right override (LRO)",
        '\u{202E}' => "right-to-left override (RLO)",
        '\u{2066}' => "left-to-right isolate (LRI)",
        '\u{2067}' => "right-to-left isolate (RLI)",
        '\u{2068}' => "first strong isolate (FSI)",
        '\u{2069}' => "pop directional isolate (PDI)",
        '\u{200E}' => "left-to-right mark (LRM)",
        '\u{200F}' => "right-to-left mark (RLM)",
        '\u{061C}' => "Arabic letter mark (ALM)",
        '\u{200B}' => "zero-width space (ZWSP)",
        '\u{200C}' => "zero-width non-joiner (ZWNJ)",
        '\u{200D}' => "zero-width joiner (ZWJ)",
        '\u{2028}' => "line separator (LS)",
        '\u{2029}' => "paragraph separator (PS)",
        '\u{FEFF}' => "zero-width no-break space (BOM)",
        _ => return None,
    })
}

fn reject_invisible(src: &str) -> Result<(), LexError> {
    // Fast path: invisible chars are all multi-byte (≥ 2 bytes in UTF-8).
    // A pure-ASCII file (the common case) needs no per-char walk at all —
    // a single bytes scan is enough to confirm we can skip the work.
    if src.is_ascii() {
        return Ok(());
    }
    // Slow path: at least one non-ASCII byte exists, so a forbidden code
    // point may appear. Walk char-by-char and report position when found.
    // `line`/`col` are kept just for error reporting; we don't pay the
    // bookkeeping for files that don't reach the slow path.
    let mut line: u32 = 1;
    let mut col: u32 = 1;
    let mut byte_off: usize = 0;
    for c in src.chars() {
        if let Some(name) = invisible_name(c) {
            // A single leading BOM is the conventional UTF-8
            // marker and is silently allowed at byte offset 0.
            if !(byte_off == 0 && c == '\u{FEFF}') {
                let span = Span::new(line, col);
                return Err(LexError::DisallowedInvisibleChar {
                    cp: c as u32,
                    name,
                    span,
                });
            }
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
        byte_off += c.len_utf8();
    }
    Ok(())
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
    /// Cached second-lookahead char. `None` means "not yet computed";
    /// `Some(None)` means "already peeked, EOF reached"; `Some(Some(c))`
    /// is a fresh value. Filled lazily by `peek_second` and invalidated
    /// in `bump`. Avoids the per-call `Chars::clone()` the previous
    /// implementation performed (which walks UTF-8 state internally).
    peeked2: Option<Option<char>>,
    line: u32,
    col: u32,
    /// Position of the most recently bumped character (1-based, inclusive).
    /// Used to populate the end of a token's `Span` after reading.
    last_line: u32,
    last_col: u32,
    /// Becomes `true` whenever whitespace contains at least one `\n`; the
    /// next token consumes this flag so it knows a newline preceded it.
    pending_newline: bool,
    /// Stack of active template literals. Each frame tracks whether we
    /// are currently inside an interpolation (`${ ... }`) and, if so,
    /// how deep the `{`/`}` nesting goes — the outer `}` that closes
    /// the interpolation has `brace_depth == 0` after the bump and
    /// returns us to literal-text mode. Nesting (template inside an
    /// interpolation inside another template) just pushes another
    /// frame on top.
    template_stack: Vec<TmplFrame>,
}

#[derive(Clone, Copy)]
struct TmplFrame {
    /// `true` while the lexer is producing regular expression tokens
    /// inside a `${...}` interpolation; `false` while it's producing
    /// `TmplLit` chunks of literal text between interpolations.
    in_expr: bool,
    /// Count of unmatched `{` opened *inside* the interpolation
    /// expression. The opening `${` itself doesn't count; only `{`
    /// tokens emitted while `in_expr` is true do.
    brace_depth: u32,
}

/// Snapshot of all mutable lexer state — used to roll back when a
/// speculative read (numeric type suffix) turns out not to match.
struct LexerSnapshot<'a> {
    chars: std::str::Chars<'a>,
    peeked: Option<char>,
    peeked2: Option<Option<char>>,
    line: u32,
    col: u32,
    last_line: u32,
    last_col: u32,
    pending_newline: bool,
    template_stack: Vec<TmplFrame>,
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
            peeked2: None,
            line: 1,
            col: 1,
            last_line: 1,
            last_col: 1,
            pending_newline: false,
            template_stack: Vec::new(),
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
        // The cached second-lookahead is for the *previous* position;
        // invalidate so the next peek_second recomputes against the new
        // position. We leave `chars` untouched (it was never advanced
        // when `peeked2` was filled — see peek_second).
        self.peeked2 = None;
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
        // iterator sits at the second char. We clone once per position (the
        // old code re-cloned on every call) and cache the result in
        // `peeked2`; `bump()` invalidates the cache. `chars` itself is not
        // advanced, so other helpers that do their own `chars.clone()` for
        // deeper lookahead (numeric prefix, float-suffix probe) keep
        // working unchanged.
        let _ = self.peek();
        if self.peeked2.is_none() {
            self.peeked2 = Some(self.chars.clone().next());
        }
        self.peeked2.unwrap()
    }

    /// Shared body for operators that come in a `<c>` / `<c>=` pair
    /// (e.g. `+` / `+=`, `!` / `!=`). The leading char is at `peek`;
    /// consume it, then look for an `=` suffix.
    fn op_or_eq(&mut self, plain: TokenKind, with_eq: TokenKind) -> TokenKind {
        self.bump();
        if matches!(self.peek(), Some('=')) {
            self.bump();
            with_eq
        } else {
            plain
        }
    }

    fn next_token(&mut self) -> Result<Token, LexError> {
        // When the topmost active template literal is between
        // interpolations (i.e. we just emitted `TmplStart` or
        // `TmplExprEnd`), the next token must come from the literal
        // text run — including any closing backtick or `${`. Skip
        // straight to the template reader; whitespace and comments
        // are NOT processed there (they're literal text).
        if let Some(frame) = self.template_stack.last() {
            if !frame.in_expr {
                let leading_newline = std::mem::take(&mut self.pending_newline);
                let line = self.line;
                let col = self.col;
                let span = Span::new(line, col);
                let kind = self.read_template_lit(span)?;
                let end_line = self.last_line;
                let end_col = self.last_col;
                let mut full_span = span;
                full_span.end_line = end_line;
                full_span.end_col = end_col;
                return Ok(Token {
                    kind,
                    span: full_span,
                    leading_newline,
                    numeric_suffix: None,
                });
            }
        }
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
            '+' => self.op_or_eq(TokenKind::Plus, TokenKind::PlusEq),
            '-' => self.op_or_eq(TokenKind::Minus, TokenKind::MinusEq),
            '*' => self.op_or_eq(TokenKind::Star, TokenKind::StarEq),
            '/' => self.op_or_eq(TokenKind::Slash, TokenKind::SlashEq),
            '%' => self.op_or_eq(TokenKind::Percent, TokenKind::PercentEq),
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
                if let Some(frame) = self.template_stack.last_mut() {
                    if frame.in_expr {
                        frame.brace_depth += 1;
                    }
                }
                TokenKind::LBrace
            }
            '}' => {
                // Snapshot the template state first, then bump, then
                // update the frame — keeps the mutable borrow of
                // `template_stack` from overlapping with the `self.bump()`
                // call (which itself touches `self`).
                let frame_action = self
                    .template_stack
                    .last()
                    .copied()
                    .filter(|f| f.in_expr);
                self.bump();
                match frame_action {
                    Some(frame) if frame.brace_depth == 0 => {
                        // Closes the `${...}` opened earlier; back to
                        // literal-text mode for the remainder of this
                        // template.
                        let top = self.template_stack.last_mut().unwrap();
                        top.in_expr = false;
                        top.brace_depth = 0;
                        TokenKind::TmplExprEnd
                    }
                    Some(_) => {
                        let top = self.template_stack.last_mut().unwrap();
                        top.brace_depth -= 1;
                        TokenKind::RBrace
                    }
                    None => TokenKind::RBrace,
                }
            }
            '`' => {
                self.bump();
                self.template_stack.push(TmplFrame { in_expr: false, brace_depth: 0 });
                TokenKind::TmplStart
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
            '!' => self.op_or_eq(TokenKind::Bang, TokenKind::BangEq),
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
            '^' => self.op_or_eq(TokenKind::Caret, TokenKind::CaretEq),
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

    /// Read the next chunk inside an active template literal. Returns
    /// one of `TmplLit(text)`, `TmplExprStart` (at `${`), or
    /// `TmplEnd` (at the closing backtick). Empty literal chunks
    /// (when a `${` follows immediately after the opening backtick or
    /// another `${...}`) are still returned as `TmplLit("")` so the
    /// parser's part-stitching loop can stay uniform.
    fn read_template_lit(&mut self, span: Span) -> Result<TokenKind, LexError> {
        // Special-case the boundary tokens first so an empty buffer
        // doesn't get emitted right before them — the parser sees
        // exactly one `TmplLit` per text run, never a stray empty one
        // adjacent to a marker.
        match self.peek() {
            None => return Err(LexError::UnterminatedTemplate { span }),
            Some('`') => {
                self.bump();
                self.template_stack
                    .pop()
                    .expect("template stack underflow on closing backtick");
                return Ok(TokenKind::TmplEnd);
            }
            Some('$') if self.peek_second() == Some('{') => {
                self.bump(); // $
                self.bump(); // {
                let frame = self
                    .template_stack
                    .last_mut()
                    .expect("template stack empty inside read_template_lit");
                frame.in_expr = true;
                frame.brace_depth = 0;
                return Ok(TokenKind::TmplExprStart);
            }
            _ => {}
        }
        let mut buf = String::new();
        loop {
            match self.peek() {
                None => return Err(LexError::UnterminatedTemplate { span }),
                Some('`') => return Ok(TokenKind::TmplLit(buf)),
                Some('$') if self.peek_second() == Some('{') => {
                    return Ok(TokenKind::TmplLit(buf));
                }
                Some('\\') => {
                    let esc_span = Span::new(self.line, self.col);
                    self.bump();
                    match self.peek() {
                        Some('n') => { self.bump(); buf.push('\n'); }
                        Some('t') => { self.bump(); buf.push('\t'); }
                        Some('r') => { self.bump(); buf.push('\r'); }
                        Some('\\') => { self.bump(); buf.push('\\'); }
                        Some('`') => { self.bump(); buf.push('`'); }
                        Some('$') => { self.bump(); buf.push('$'); }
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
                        None => return Err(LexError::UnterminatedTemplate { span }),
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
            "interface" => TokenKind::Interface,
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
            "override" => TokenKind::Override,
            "super" => TokenKind::Super,
            "pub" => TokenKind::Pub,
            "async" => TokenKind::Async,
            "await" => TokenKind::Await,
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
