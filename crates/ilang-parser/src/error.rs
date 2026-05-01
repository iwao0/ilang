use ilang_lexer::TokenKind;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ParseError {
    #[error("unexpected token {found:?} at line {line}, col {col} (expected {expected})")]
    Unexpected {
        found: TokenKind,
        expected: String,
        line: u32,
        col: u32,
    },
    #[error("unknown type {name:?} at line {line}, col {col}")]
    UnknownType { name: String, line: u32, col: u32 },
    #[error("invalid assignment target at line {line}, col {col}")]
    InvalidAssignTarget { line: u32, col: u32 },
}
