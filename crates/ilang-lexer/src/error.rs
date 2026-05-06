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
    #[error("{span}: invalid escape {seq:?} in string literal")]
    BadEscape { seq: String, span: Span },
    #[error("{span}: unterminated block comment")]
    UnterminatedBlockComment { span: Span },
}

impl LexError {
    pub fn span(&self) -> Span {
        match self {
            LexError::UnexpectedChar { span, .. }
            | LexError::InvalidNumber { span, .. }
            | LexError::UnterminatedString { span }
            | LexError::BadEscape { span, .. }
            | LexError::UnterminatedBlockComment { span } => *span,
        }
    }
}
