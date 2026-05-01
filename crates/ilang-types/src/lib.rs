//! Phase 2 minimal type checker.
//!
//! Supports `i64`, `f64`, `bool`, and `()` (unit). Mixed `i64`/`f64`
//! arithmetic is allowed and promoted to `f64` (matching the runtime).
//! Function signatures and `let` annotations are checked.
//! `#[requires(...)]` attributes are not enforced — that arrives in a later
//! phase along with the capability system.

pub mod checker;
pub mod error;
mod ops;

use ilang_ast::{Program, Type};

pub use checker::TypeChecker;
pub use error::TypeError;

/// One-shot type check for callers that don't need to keep state.
pub fn check(prog: &Program) -> Result<Type, TypeError> {
    TypeChecker::new().check(prog)
}
