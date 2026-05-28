//! String and template-literal scanning plus the shared escape-
//! sequence readers (`\xNN` byte escapes and `\u{...}` unicode
//! escapes) used by both.

use crate::error::LexError;
use crate::token::{Span, TokenKind};

use super::Lexer;

impl<'a> Lexer<'a> {
    /// Consume a `"..."` string literal (the leading `"` is the next char).
    /// Supports the basic C-style escapes; everything else is an error.
    /// Raw newlines inside the literal are forbidden — strings must close
    /// on the line they started on.
    pub(super) fn read_string(&mut self, span: Span) -> Result<TokenKind, LexError> {
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
    pub(super) fn read_template_lit(&mut self, span: Span) -> Result<TokenKind, LexError> {
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
}
