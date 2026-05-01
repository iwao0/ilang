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
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            chars: src.chars(),
            peeked: None,
            line: 1,
            col: 1,
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
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.bump();
            } else {
                break;
            }
        }
    }

    fn next_token(&mut self) -> Result<Token, LexError> {
        self.skip_whitespace();
        let line = self.line;
        let col = self.col;
        let span = Span { line, col };

        let Some(c) = self.peek() else {
            return Ok(Token {
                kind: TokenKind::Eof,
                span,
            });
        };

        let kind = match c {
            '+' => {
                self.bump();
                TokenKind::Plus
            }
            '-' => {
                self.bump();
                TokenKind::Minus
            }
            '*' => {
                self.bump();
                TokenKind::Star
            }
            '/' => {
                self.bump();
                TokenKind::Slash
            }
            '%' => {
                self.bump();
                TokenKind::Percent
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
                if matches!(self.peek(), Some('=')) {
                    self.bump();
                    TokenKind::EqEq
                } else {
                    TokenKind::Equals
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
                if matches!(self.peek(), Some('=')) {
                    self.bump();
                    TokenKind::LtEq
                } else {
                    TokenKind::Lt
                }
            }
            '>' => {
                self.bump();
                if matches!(self.peek(), Some('=')) {
                    self.bump();
                    TokenKind::GtEq
                } else {
                    TokenKind::Gt
                }
            }
            '&' => {
                self.bump();
                if matches!(self.peek(), Some('&')) {
                    self.bump();
                    TokenKind::AmpAmp
                } else {
                    return Err(LexError::UnexpectedChar { ch: '&', line, col });
                }
            }
            '|' => {
                self.bump();
                if matches!(self.peek(), Some('|')) {
                    self.bump();
                    TokenKind::PipePipe
                } else {
                    return Err(LexError::UnexpectedChar { ch: '|', line, col });
                }
            }
            '#' => {
                self.bump();
                TokenKind::Hash
            }
            c if c.is_ascii_digit() => self.read_number(line, col)?,
            c if is_ident_start(c) => self.read_ident_or_keyword(),
            other => {
                return Err(LexError::UnexpectedChar {
                    ch: other,
                    line,
                    col,
                });
            }
        };

        Ok(Token { kind, span })
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
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            _ => TokenKind::Ident(buf),
        }
    }

    fn read_number(&mut self, line: u32, col: u32) -> Result<TokenKind, LexError> {
        let mut buf = String::new();
        let mut is_float = false;

        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                buf.push(c);
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
                    } else {
                        break;
                    }
                }
                if !saw_digit {
                    return Err(LexError::InvalidNumber {
                        text: buf,
                        line,
                        col,
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
                    line,
                    col,
                    reason: e.to_string(),
                })
        } else {
            buf.parse::<i64>()
                .map(TokenKind::Int)
                .map_err(|e| LexError::InvalidNumber {
                    text: buf.clone(),
                    line,
                    col,
                    reason: e.to_string(),
                })
        }
    }
}
