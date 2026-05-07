use ilang_ast::{Span, Symbol};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum RuntimeError {
    #[error("{span}: integer division by zero")]
    DivisionByZero { span: Span },
    #[error("{span}: integer overflow")]
    Overflow { span: Span },
    #[error("{span}: undefined variable {name:?}")]
    UndefinedVariable { name: Symbol, span: Span },
    #[error("{span}: undefined function {name:?}")]
    UndefinedFunction { name: Symbol, span: Span },
    #[error("{span}: function {name:?} expects {expected} arguments but got {got}")]
    ArityMismatch {
        name: Symbol,
        expected: usize,
        got: usize,
        span: Span,
    },
    #[error("{span}: recursion depth exceeded")]
    StackOverflow { span: Span },
    #[error("{span}: type error at runtime: {msg}")]
    TypeError { msg: String, span: Span },
    #[error("{span}: undefined class {name:?}")]
    UndefinedClass { name: Symbol, span: Span },
    #[error("{span}: class {class:?} has no method {method:?}")]
    UnknownMethod {
        class: Symbol,
        method: Symbol,
        span: Span,
    },
    #[error("{span}: class {class:?} has no field {field:?}")]
    UnknownField {
        class: Symbol,
        field: Symbol,
        span: Span,
    },
    #[error("{span}: `this` used outside of a method body")]
    ThisOutsideMethod { span: Span },
    #[error("{span}: expected an object, got {actual}")]
    NotAnObject { actual: String, span: Span },
    #[error("{span}: array index {index} out of bounds (length {len})")]
    IndexOutOfBounds { index: i64, len: i64, span: Span },
    /// Thrown when a numeric `as Enum` cast lands on a value that
    /// matches no variant. Skipped for `@flags` enums (any bit
    /// pattern is a valid combination).
    #[error("{span}: enum {enum_name:?} has no variant with value {value}")]
    EnumOutOfRange {
        enum_name: Symbol,
        value: i128,
        span: Span,
    },
    /// Internal control-flow signal carried by `Result::Err` so `?` propagates
    /// it to the enclosing loop. The type checker rejects `break` outside a
    /// loop, so this never escapes a well-typed program. The payload is the
    /// `break v` value (or `Value::Unit` for bare `break`).
    #[error("`break` outside of a loop")]
    Break(crate::Value),
    /// Carries an early `return value` out of the enclosing function.
    /// Caught by `Interpreter::invoke`; the type checker keeps it from
    /// surfacing outside a function body.
    #[error("`return` outside of a function")]
    Return(crate::Value),
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
            RuntimeError::IndexOutOfBounds { index, len, .. } => {
                RuntimeError::IndexOutOfBounds { index, len, span: real }
            }
            RuntimeError::EnumOutOfRange { enum_name, value, .. } => {
                RuntimeError::EnumOutOfRange { enum_name, value, span: real }
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
            | RuntimeError::NotAnObject { span, .. }
            | RuntimeError::IndexOutOfBounds { span, .. }
            | RuntimeError::EnumOutOfRange { span, .. } => *span,
            RuntimeError::Break(_) | RuntimeError::Continue | RuntimeError::Return(_) => {
                Span::dummy()
            }
        }
    }
}
