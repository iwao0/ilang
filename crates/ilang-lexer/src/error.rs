use thiserror::Error;

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
