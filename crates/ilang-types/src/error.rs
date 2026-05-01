use ilang_ast::{Span, Type};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum TypeError {
    #[error("{span}: type mismatch: expected {expected}, got {got}")]
    Mismatch {
        expected: Type,
        got: Type,
        span: Span,
    },
    #[error("{span}: undefined variable {name:?}")]
    UndefinedVariable { name: String, span: Span },
    #[error("{span}: undefined function {name:?}")]
    UndefinedFunction { name: String, span: Span },
    #[error("{span}: function {name:?} expects {expected} arguments but got {got}")]
    ArityMismatch {
        name: String,
        expected: usize,
        got: usize,
        span: Span,
    },
    #[error("{span}: cannot apply unary op to {ty}")]
    BadUnary { ty: Type, span: Span },
    #[error("{span}: cannot apply binary op between {lhs} and {rhs}")]
    BadBinary { lhs: Type, rhs: Type, span: Span },
    #[error("{span}: function {name:?} declared to return {expected} but body produces {got}")]
    BadReturn {
        name: String,
        expected: Type,
        got: Type,
        span: Span,
    },
    #[error("{span}: undefined class {name:?}")]
    UndefinedClass { name: String, span: Span },
    #[error("{span}: class {class:?} has no field {field:?}")]
    UnknownField {
        class: String,
        field: String,
        span: Span,
    },
    #[error("{span}: class {class:?} has no method {method:?}")]
    UnknownMethod {
        class: String,
        method: String,
        span: Span,
    },
    #[error("{span}: `this` used outside of a method body")]
    ThisOutsideMethod { span: Span },
    #[error("{span}: `break` used outside of a loop")]
    BreakOutsideLoop { span: Span },
    #[error("{span}: `continue` used outside of a loop")]
    ContinueOutsideLoop { span: Span },
    #[error("{span}: `deinit` cannot be called explicitly (it runs automatically when the binding goes out of scope)")]
    CannotCallDeinit { span: Span },
    #[error("{span}: `deinit` must take no parameters and return ()")]
    BadDeinitSignature { span: Span },
}

impl TypeError {
    pub fn span(&self) -> Span {
        match self {
            TypeError::Mismatch { span, .. }
            | TypeError::UndefinedVariable { span, .. }
            | TypeError::UndefinedFunction { span, .. }
            | TypeError::ArityMismatch { span, .. }
            | TypeError::BadUnary { span, .. }
            | TypeError::BadBinary { span, .. }
            | TypeError::BadReturn { span, .. }
            | TypeError::UndefinedClass { span, .. }
            | TypeError::UnknownField { span, .. }
            | TypeError::UnknownMethod { span, .. }
            | TypeError::ThisOutsideMethod { span }
            | TypeError::BreakOutsideLoop { span }
            | TypeError::ContinueOutsideLoop { span }
            | TypeError::CannotCallDeinit { span }
            | TypeError::BadDeinitSignature { span } => *span,
        }
    }
}
