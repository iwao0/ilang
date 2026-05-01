//! AST → Cranelift JIT compiler.
//!
//! Compiles a `Program` to native code via cranelift-jit and runs it.
//! Supports the full numeric / bool / control-flow / fn / class /
//! string / array subset; `console.log` is treated as a built-in.

mod arc;
mod env;
mod error;
mod lower;
mod lower_ctrl;
mod lower_op;
mod runtime;
mod ty;
mod value;

pub use error::CodegenError;
pub use lower::jit_run;
pub use value::JitValue;
