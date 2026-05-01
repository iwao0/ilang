#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Int(i64),
    Float(f64),
    Ident(String),
    // keywords
    Let,
    Fn,
    If,
    Else,
    While,
    True,
    False,
    // punctuation
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Semicolon,
    Comma,
    Colon,
    ColonColon,
    Equals,
    EqEq,
    BangEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    AmpAmp,
    PipePipe,
    Bang,
    Arrow,
    Hash,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}
