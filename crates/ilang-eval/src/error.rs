use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum RuntimeError {
    #[error("integer division by zero")]
    DivisionByZero,
    #[error("integer overflow")]
    Overflow,
    #[error("undefined variable {0:?}")]
    UndefinedVariable(String),
    #[error("undefined function {0:?}")]
    UndefinedFunction(String),
    #[error("function {name:?} expects {expected} arguments but got {got}")]
    ArityMismatch {
        name: String,
        expected: usize,
        got: usize,
    },
    #[error("recursion depth exceeded")]
    StackOverflow,
    #[error("type error at runtime: {0}")]
    TypeError(String),
}
