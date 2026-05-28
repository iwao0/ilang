//! Tokenizer. `tokenize` drives a single-pass `Lexer` that produces a
//! flat `Vec<Token>`. The scanning logic is split by token family:
//!
//! - this module: the public entry point, the bidi-control rejection
//!   pass, the `Lexer` state + core cursor helpers (`peek` / `bump` /
//!   `peek_second` / `skip_whitespace`), the `next_token` dispatcher,
//!   and identifier / keyword scanning.
//! - [`numbers`] — integer / float literals, radix prefixes, and the
//!   numeric type-suffix probe.
//! - [`strings`] — `"..."` strings, `` `...` `` template literals, and
//!   the shared escape-sequence readers.

use crate::error::LexError;
use crate::token::{Span, Token, TokenKind};

mod numbers;
mod strings;

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
}
