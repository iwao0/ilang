pub use ilang_ast::Span;

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
    Loop,
    Break,
    Continue,
    True,
    False,
    Class,
    New,
    This,
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
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    EqEq,
    BangEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    AmpAmp,
    PipePipe,
    Bang,
    Dot,
    Hash,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    /// `true` if at least one newline appeared between the previous token
    /// (or start of input) and this token. Used by the parser for
    /// JS-style automatic semicolon insertion at statement boundaries.
    pub leading_newline: bool,
}
