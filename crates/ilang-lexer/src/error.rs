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
}

impl LexError {
    pub fn span(&self) -> Span {
        match self {
            LexError::UnexpectedChar { span, .. } | LexError::InvalidNumber { span, .. } => *span,
        }
    }
}
