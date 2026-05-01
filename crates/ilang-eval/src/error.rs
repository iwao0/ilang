use ilang_ast::Span;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum RuntimeError {
    #[error("{span}: integer division by zero")]
    DivisionByZero { span: Span },
    #[error("{span}: integer overflow")]
    Overflow { span: Span },
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
    #[error("{span}: recursion depth exceeded")]
    StackOverflow { span: Span },
    #[error("{span}: type error at runtime: {msg}")]
    TypeError { msg: String, span: Span },
    #[error("{span}: undefined class {name:?}")]
    UndefinedClass { name: String, span: Span },
    #[error("{span}: class {class:?} has no method {method:?}")]
    UnknownMethod {
        class: String,
        method: String,
        span: Span,
    },
    #[error("{span}: class {class:?} has no field {field:?}")]
    UnknownField {
        class: String,
        field: String,
        span: Span,
    },
    #[error("{span}: `this` used outside of a method body")]
    ThisOutsideMethod { span: Span },
    #[error("{span}: expected an object, got {actual}")]
    NotAnObject { actual: String, span: Span },
    /// Internal control-flow signal carried by `Result::Err` so `?` propagates
    /// it to the enclosing loop. The type checker rejects `break` outside a
    /// loop, so this never escapes a well-typed program.
    #[error("`break` outside of a loop")]
    Break,
    /// Sibling of `Break` for `continue`.
    #[error("`continue` outside of a loop")]
    Continue,
}

impl RuntimeError {
    /// Replace a placeholder `Span::dummy()` (left by helper modules like
    /// `ops`) with the surrounding expression's real span.
    pub fn with_span(self, real: Span) -> Self {
        if self.span() != Span::dummy() {
            return self;
        }
        match self {
            RuntimeError::DivisionByZero { .. } => RuntimeError::DivisionByZero { span: real },
            RuntimeError::Overflow { .. } => RuntimeError::Overflow { span: real },
            RuntimeError::TypeError { msg, .. } => RuntimeError::TypeError { msg, span: real },
            RuntimeError::ArityMismatch {
                name,
                expected,
                got,
                ..
            } => RuntimeError::ArityMismatch {
                name,
                expected,
                got,
                span: real,
            },
            RuntimeError::StackOverflow { .. } => RuntimeError::StackOverflow { span: real },
            RuntimeError::UndefinedVariable { name, .. } => {
                RuntimeError::UndefinedVariable { name, span: real }
            }
            RuntimeError::UndefinedFunction { name, .. } => {
                RuntimeError::UndefinedFunction { name, span: real }
            }
            RuntimeError::UndefinedClass { name, .. } => {
                RuntimeError::UndefinedClass { name, span: real }
            }
            RuntimeError::UnknownMethod { class, method, .. } => RuntimeError::UnknownMethod {
                class,
                method,
                span: real,
            },
            RuntimeError::UnknownField { class, field, .. } => RuntimeError::UnknownField {
                class,
                field,
                span: real,
            },
            RuntimeError::ThisOutsideMethod { .. } => {
                RuntimeError::ThisOutsideMethod { span: real }
            }
            RuntimeError::NotAnObject { actual, .. } => {
                RuntimeError::NotAnObject { actual, span: real }
            }
            other => other,
        }
    }


    /// `Span::dummy()` for the internal `Break`/`Continue` signals which
    /// never surface to the user (the type checker has already rejected
    /// stray break/continue at compile time).
    pub fn span(&self) -> Span {
        match self {
            RuntimeError::DivisionByZero { span }
            | RuntimeError::Overflow { span }
            | RuntimeError::UndefinedVariable { span, .. }
            | RuntimeError::UndefinedFunction { span, .. }
            | RuntimeError::ArityMismatch { span, .. }
            | RuntimeError::StackOverflow { span }
            | RuntimeError::TypeError { span, .. }
            | RuntimeError::UndefinedClass { span, .. }
            | RuntimeError::UnknownMethod { span, .. }
            | RuntimeError::UnknownField { span, .. }
            | RuntimeError::ThisOutsideMethod { span }
            | RuntimeError::NotAnObject { span, .. } => *span,
            RuntimeError::Break | RuntimeError::Continue => Span::dummy(),
        }
    }
}
