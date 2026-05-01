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
    pending_newline: bool,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            chars: src.chars(),
            peeked: None,
            line: 1,
            col: 1,
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
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn skip_whitespace(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    if c == '\n' {
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
                        if c == '\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                _ => break,
            }
        }
    }

    fn peek_second(&mut self) -> Option<char> {
        // Need to materialize the first peeked char first so the underlying
        // iterator advances to the second one.
        let _ = self.peek();
        self.chars.clone().next()
    }

    fn next_token(&mut self) -> Result<Token, LexError> {
        self.skip_whitespace();
        let leading_newline = std::mem::take(&mut self.pending_newline);
        let line = self.line;
        let col = self.col;
        let span = Span { line, col };

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
            '.' => {
                self.bump();
                TokenKind::Dot
            }
            '#' => {
                self.bump();
                TokenKind::Hash
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
            self.try_read_numeric_suffix()
        } else {
            None
        };

        Ok(Token {
            kind,
            span,
            leading_newline,
            numeric_suffix,
        })
    }

    /// Attempt to consume a numeric type suffix at the current position.
    /// Accepts an optional leading `_` followed by one of the known type
    /// names (`i8`/`i16`/`i32`/`i64`/`u8`/`u16`/`u32`/`u64`/`f32`/`f64`).
    /// Restores lexer state if no valid suffix is present, so a literal
    /// like `1` followed by an identifier `foo` is unaffected.
    fn try_read_numeric_suffix(&mut self) -> Option<ilang_ast::Type> {
        let snap = LexerSnapshot {
            chars: self.chars.clone(),
            peeked: self.peeked,
            line: self.line,
            col: self.col,
            pending_newline: self.pending_newline,
        };
        // Optional separating underscore.
        if matches!(self.peek(), Some('_')) {
            self.bump();
        }
        let mut buf = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() {
                buf.push(c);
                self.bump();
            } else {
                break;
            }
        }
        let ty = match buf.as_str() {
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
        if ty.is_none() {
            // Roll back: pretend we never looked at the suffix candidate.
            self.chars = snap.chars;
            self.peeked = snap.peeked;
            self.line = snap.line;
            self.col = snap.col;
            self.pending_newline = snap.pending_newline;
        }
        ty
    }

    /// Consume a `"..."` string literal (the leading `"` is the next char).
    /// Supports the basic C-style escapes; everything else is an error.
    fn read_string(&mut self, span: Span) -> Result<TokenKind, LexError> {
        self.bump(); // opening "
        let mut buf = String::new();
        loop {
            match self.peek() {
                None => return Err(LexError::UnterminatedString { span }),
                Some('"') => {
                    self.bump();
                    return Ok(TokenKind::Str(buf));
                }
                Some('\\') => {
                    self.bump();
                    match self.peek() {
                        Some('n') => {
                            self.bump();
                            buf.push('\n');
                        }
                        Some('t') => {
                            self.bump();
                            buf.push('\t');
                        }
                        Some('r') => {
                            self.bump();
                            buf.push('\r');
                        }
                        Some('\\') => {
                            self.bump();
                            buf.push('\\');
                        }
                        Some('"') => {
                            self.bump();
                            buf.push('"');
                        }
                        Some('0') => {
                            self.bump();
                            buf.push('\0');
                        }
                        Some(c) => {
                            return Err(LexError::BadEscape {
                                seq: format!("\\{c}"),
                                span,
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
            "none" => TokenKind::None_,
            "some" => TokenKind::Some_,
            "enum" => TokenKind::Enum,
            "match" => TokenKind::Match,
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
            }
        }

        let mut buf = String::new();
        let mut is_float = false;

        // Integer part. `_` is allowed between digits (not at the very
        // start, since the entry condition required an ASCII digit).
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                buf.push(c);
                self.bump();
            } else if c == '_' {
                self.bump();
            } else {
                break;
            }
        }

        if let Some('.') = self.peek() {
            is_float = true;
            buf.push('.');
            self.bump();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    buf.push(c);
                    self.bump();
                } else if c == '_' {
                    self.bump();
                } else {
                    break;
                }
            }
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
                let mut saw_digit = false;
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() {
                        buf.push(c);
                        self.bump();
                        saw_digit = true;
                    } else if c == '_' {
                        self.bump();
                    } else {
                        break;
                    }
                }
                if !saw_digit {
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
            buf.parse::<i64>()
                .map(TokenKind::Int)
                .map_err(|e| LexError::InvalidNumber {
                    text: buf.clone(),
                    span,
                    reason: e.to_string(),
                })
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
        let mut digits = String::new();
        while let Some(c) = self.peek() {
            if c.is_digit(radix) {
                digits.push(c);
                self.bump();
            } else if c == '_' {
                self.bump();
            } else {
                break;
            }
        }
        if digits.is_empty() {
            return Err(LexError::InvalidNumber {
                text: format!("0{}", if radix == 16 { "x" } else { "b" }),
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
                text: digits,
                span,
                reason: e.to_string(),
            })
    }
}
