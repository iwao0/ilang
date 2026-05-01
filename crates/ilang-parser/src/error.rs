use ilang_ast::Span;
use ilang_lexer::TokenKind;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ParseError {
    #[error("{span}: unexpected token {found:?} (expected {expected})")]
    Unexpected {
        found: TokenKind,
        expected: String,
        span: Span,
    },
    #[error("{span}: invalid assignment target")]
    InvalidAssignTarget { span: Span },
}

impl ParseError {
    pub fn span(&self) -> Span {
        match self {
            ParseError::Unexpected { span, .. } | ParseError::InvalidAssignTarget { span } => {
                *span
            }
        }
    }
}
