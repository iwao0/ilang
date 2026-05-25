pub use ilang_ast::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Int(i64),
    Float(f64),
    Str(String),
    Ident(String),
    // keywords
    Let,
    Fn,
    If,
    Elif,
    Else,
    While,
    Loop,
    Break,
    Continue,
    Return,
    True,
    False,
    Class,
    Interface,
    New,
    This,
    As,
    Is,
    None_,
    Some_,
    Enum,
    Match,
    For,
    In,
    Use,
    Const,
    Override,
    Super,
    Pub,
    Async,
    Await,
    FatArrow,
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
    Amp,
    AmpEq,
    Pipe,
    PipeEq,
    Caret,
    CaretEq,
    Tilde,
    LtLt,
    LtLtEq,
    GtGt,
    GtGtEq,
    Bang,
    Dot,
    DotDot,
    DotDotEq,
    DotDotDot,
    At,
    Question,
    /// `` ` `` opening a template literal. The parser sees this once,
    /// then alternates between `TmplLit(text)` and embedded
    /// `TmplExprStart ... TmplExprEnd` sequences, and finishes on
    /// `TmplEnd`.
    TmplStart,
    /// A literal text run inside a template literal. May be empty
    /// (`` `${x}${y}` `` round-trips through three empty runs).
    /// String escapes (`\``, `\${`, `\\`, `\n`, `\t`, `\r`, `\0`,
    /// `\u{...}`) have already been decoded by the lexer.
    TmplLit(String),
    /// `${` opening an interpolation; the tokens that follow are
    /// regular expression tokens until the matching `}` becomes
    /// `TmplExprEnd`.
    TmplExprStart,
    /// `}` closing the interpolation opened by `TmplExprStart` (only
    /// the outer one — `{`/`}` nested inside the interpolated
    /// expression are normal `LBrace`/`RBrace`).
    TmplExprEnd,
    /// `` ` `` closing a template literal.
    TmplEnd,
    Eof,
}

impl TokenKind {
    /// Source spelling for word-shaped keyword tokens (e.g. `Class`
    /// → `Some("class")`). Returns `None` for non-keyword variants
    /// and for symbol-shaped keywords like `FatArrow` (`=>`).
    ///
    /// Lets parsers that accept keywords in identifier positions
    /// (e.g. `obj.class`, `Enum.return`) share one whitelist instead
    /// of re-listing every variant.
    pub fn keyword_str(&self) -> Option<&'static str> {
        Some(match self {
            TokenKind::Let => "let",
            TokenKind::Fn => "fn",
            TokenKind::If => "if",
            TokenKind::Elif => "elif",
            TokenKind::Else => "else",
            TokenKind::While => "while",
            TokenKind::Loop => "loop",
            TokenKind::Break => "break",
            TokenKind::Continue => "continue",
            TokenKind::Return => "return",
            TokenKind::True => "true",
            TokenKind::False => "false",
            TokenKind::Class => "class",
            TokenKind::Interface => "interface",
            TokenKind::New => "new",
            TokenKind::This => "this",
            TokenKind::As => "as",
            TokenKind::Is => "is",
            TokenKind::None_ => "none",
            TokenKind::Some_ => "some",
            TokenKind::Enum => "enum",
            TokenKind::Match => "match",
            TokenKind::For => "for",
            TokenKind::In => "in",
            TokenKind::Use => "use",
            TokenKind::Const => "const",
            TokenKind::Override => "override",
            TokenKind::Super => "super",
            TokenKind::Pub => "pub",
            TokenKind::Async => "async",
            TokenKind::Await => "await",
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    /// `true` if at least one newline appeared between the previous token
    /// (or start of input) and this token. Used by the parser for
    /// JS-style automatic semicolon insertion at statement boundaries.
    pub leading_newline: bool,
    /// Set on numeric literals that ended with a type suffix (`1_i32`,
    /// `1.5f32`, ...). The parser wraps such literals in an explicit
    /// `as`-cast so the rest of the pipeline sees the declared type.
    pub numeric_suffix: Option<ilang_ast::Type>,
}
