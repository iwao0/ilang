//! AST → Cranelift JIT compiler.
//!
//! Compiles a `Program` to native code via cranelift-jit and runs it.
//! Currently restricted to a numeric subset (i64 + bool, control flow,
//! function definitions). Strings, arrays, classes, and the `console`
//! built-in fall back to the tree-walking interpreter.

mod error;
mod lower;

pub use error::CodegenError;
pub use lower::{jit_run, JitValue};
