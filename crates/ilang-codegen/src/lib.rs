//! AST → Cranelift JIT compiler.
//!
//! Compiles a `Program` to native code via cranelift-jit and runs it.
//! Supports the full numeric / bool / control-flow / fn / class /
//! string / array subset; `console.log` is treated as a built-in.

mod error;
mod lower;
mod runtime;
mod ty;
mod value;

pub use error::CodegenError;
pub use lower::jit_run;
pub use value::JitValue;
