//! MIR-level optimization passes.
//!
//! Passes mutate `Function` / `Program` in place. Run **after**
//! monomorphisation and AST→MIR lowering, **before** MIR→clif.

pub mod arc_peephole;
pub mod branch_fold;
pub mod const_fold;
pub mod dce;
pub mod dce_fn;
pub mod escape_object;
pub mod inline;
pub mod promote_locals;
pub mod util;
