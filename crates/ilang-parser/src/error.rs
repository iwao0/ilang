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
    /// `new module.Class(...)` or `let x: module.Class` referencing
    /// a module that this file didn't `use`. Allowing the reference
    /// would let an umbrella's `pub use` chain leak items into
    /// every module merged under the same prefix, even ones that
    /// never opted in.
    #[error("{span}: cannot reference {module:?}.{item:?} — this file does not `use {module:?}`")]
    UnauthorizedModuleRef {
        module: ilang_ast::Symbol,
        item: ilang_ast::Symbol,
        span: Span,
    },
    #[error("{span}: {msg}")]
    Generic { msg: String, span: Span },
}

impl ParseError {
    pub fn span(&self) -> Span {
        match self {
            ParseError::Unexpected { span, .. }
            | ParseError::InvalidAssignTarget { span }
            | ParseError::UnauthorizedModuleRef { span, .. }
            | ParseError::Generic { span, .. } => *span,
        }
    }

    /// Stamp the error's span with the file it came from. The lexer /
    /// parser work on a bare `&str` and don't know paths, so the
    /// loader sets this when wrapping the error — otherwise the span
    /// has an empty `source_file` and the CLI misattributes the error
    /// to the entry file instead of the offending module.
    pub fn set_source_file(&mut self, file: ilang_ast::Symbol) {
        let span = match self {
            ParseError::Unexpected { span, .. }
            | ParseError::InvalidAssignTarget { span }
            | ParseError::UnauthorizedModuleRef { span, .. }
            | ParseError::Generic { span, .. } => span,
        };
        span.source_file = file;
    }
}
