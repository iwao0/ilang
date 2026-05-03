//! AST → Cranelift JIT compiler.
//!
//! Compiles a `Program` to native code via cranelift-jit and runs it.
//! Supports the full numeric / bool / control-flow / fn / class /
//! string / array subset; `console.log` is treated as a built-in.

mod arc;
mod compiler;
mod drops;
mod env;
mod error;
mod lower_ctrl;
mod lower_expr;
mod lower_op;
mod lower_stmt;
mod math_externs;
mod native_extern;
mod test_externs;
mod monomorphize;
mod runtime;
mod ty;
mod value;

pub use compiler::{jit_run, jit_run_with};
pub use error::CodegenError;
pub use value::{JitEnumPayload, JitValue};
