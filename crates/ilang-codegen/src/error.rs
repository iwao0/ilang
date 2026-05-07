use ilang_ast::{Span, Type, Symbol};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CodegenError {
    #[error("{span}: feature not supported by the JIT yet: {what}")]
    Unsupported { what: String, span: Span },
    #[error("{span}: type {ty} is not supported by the JIT yet")]
    UnsupportedType { ty: Type, span: Span },
    #[error("internal cranelift error: {0}")]
    Cranelift(String),
    #[error("JIT module error: {0}")]
    Module(String),
    #[error("no top-level expression to evaluate")]
    NoTopLevelValue,
}
