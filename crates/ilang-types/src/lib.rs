//! Phase 2 minimal type checker.
//!
//! Supports `i64`, `f64`, `bool`, and `()` (unit). Mixed `i64`/`f64`
//! arithmetic is allowed and promoted to `f64` (matching the runtime).
//! Function signatures and `let` annotations are checked.
//! `#[requires(...)]` attributes are not enforced — that arrives in a later
//! phase along with the capability system.

pub mod checker;
pub mod error;
pub mod mangle;
mod ops;

use ilang_ast::{Program, Type};

pub use checker::{TypeChecker, TypeWarning};
pub use error::TypeError;

/// One-shot type check for callers that don't need to keep state.
/// Returns the program's final type alongside every error collected
/// during the pass — an empty `Vec` means the program type-checks
/// cleanly.
pub fn check(prog: &Program) -> (Type, Vec<TypeError>) {
    TypeChecker::new().check(prog)
}
