use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Int(i64),
    Float(f64),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LParen,
    RParen,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Error, PartialEq)]
pub enum LexError {
    #[error("unexpected character {ch:?} at line {line}, col {col}")]
    UnexpectedChar { ch: char, line: u32, col: u32 },
    #[error("invalid number {text:?} at line {line}, col {col}: {reason}")]
    InvalidNumber {
        text: String,
        line: u32,
        col: u32,
        reason: String,
    },
}

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
            c if c.is_ascii_digit() => self.read_number(line, col)?,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        tokenize(src).unwrap().into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn simple_arithmetic() {
        assert_eq!(
            kinds("1 + 2.5"),
            vec![
                TokenKind::Int(1),
                TokenKind::Plus,
                TokenKind::Float(2.5),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn all_operators() {
        assert_eq!(
            kinds("+-*/%()"),
            vec![
                TokenKind::Plus,
                TokenKind::Minus,
                TokenKind::Star,
                TokenKind::Slash,
                TokenKind::Percent,
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn float_with_exponent() {
        assert_eq!(
            kinds("2.5e-3"),
            vec![TokenKind::Float(2.5e-3), TokenKind::Eof]
        );
        assert_eq!(
            kinds("1E2"),
            vec![TokenKind::Float(100.0), TokenKind::Eof]
        );
    }

    #[test]
    fn unexpected_char() {
        assert!(matches!(
            tokenize("1 $ 2"),
            Err(LexError::UnexpectedChar { ch: '$', .. })
        ));
    }

    #[test]
    fn bad_exponent() {
        assert!(matches!(
            tokenize("1e"),
            Err(LexError::InvalidNumber { .. })
        ));
    }
}
