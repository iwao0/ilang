use ilang_ast::Type;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum TypeError {
    #[error("type mismatch: expected {expected}, got {got}")]
    Mismatch { expected: Type, got: Type },
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
    #[error("cannot apply unary op to {0}")]
    BadUnary(Type),
    #[error("cannot apply binary op between {0} and {1}")]
    BadBinary(Type, Type),
    #[error("function {name:?} declared to return {expected} but body produces {got}")]
    BadReturn {
        name: String,
        expected: Type,
        got: Type,
    },
}
