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
    #[error("undefined class {0:?}")]
    UndefinedClass(String),
    #[error("class {class:?} has no field {field:?}")]
    UnknownField { class: String, field: String },
    #[error("class {class:?} has no method {method:?}")]
    UnknownMethod { class: String, method: String },
    #[error("`this` used outside of a method body")]
    ThisOutsideMethod,
    #[error("`break` used outside of a loop")]
    BreakOutsideLoop,
    #[error("`continue` used outside of a loop")]
    ContinueOutsideLoop,
}
