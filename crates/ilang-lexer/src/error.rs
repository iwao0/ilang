use ilang_ast::Span;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum LexError {
    #[error("{span}: unexpected character {ch:?}")]
    UnexpectedChar { ch: char, span: Span },
    #[error("{span}: invalid number {text:?}: {reason}")]
    InvalidNumber {
        text: String,
        span: Span,
        reason: String,
    },
    #[error("{span}: unterminated string literal")]
    UnterminatedString { span: Span },
    #[error("{span}: unterminated template literal (missing closing backtick)")]
    UnterminatedTemplate { span: Span },
    #[error("{span}: invalid escape {seq:?} in string literal")]
    BadEscape { seq: String, span: Span },
    #[error("{span}: unterminated block comment")]
    UnterminatedBlockComment { span: Span },
    #[error("{span}: invalid numeric suffix {name:?}")]
    InvalidNumericSuffix { name: String, span: Span },
    #[error(
        "{span}: source contains the {name} character (U+{cp:04X}); these invisible / bidi-control \
         characters are rejected to prevent trojan-source attacks. If you need the code point in a \
         string literal, use a `\\u{{{cp:04X}}}` escape."
    )]
    DisallowedInvisibleChar {
        cp: u32,
        name: &'static str,
        span: Span,
    },
}

impl LexError {
    pub fn span(&self) -> Span {
        match self {
            LexError::UnexpectedChar { span, .. }
            | LexError::InvalidNumber { span, .. }
            | LexError::UnterminatedString { span }
            | LexError::UnterminatedTemplate { span }
            | LexError::BadEscape { span, .. }
            | LexError::UnterminatedBlockComment { span }
            | LexError::InvalidNumericSuffix { span, .. }
            | LexError::DisallowedInvisibleChar { span, .. } => *span,
        }
    }
}
