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
    #[error("undefined class {0:?}")]
    UndefinedClass(String),
    #[error("class {class:?} has no method {method:?}")]
    UnknownMethod { class: String, method: String },
    #[error("class {class:?} has no field {field:?}")]
    UnknownField { class: String, field: String },
    #[error("`this` used outside of a method body")]
    ThisOutsideMethod,
    #[error("expected an object, got {0}")]
    NotAnObject(String),
}
