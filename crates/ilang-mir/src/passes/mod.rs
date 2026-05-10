//! MIR-level optimization passes.
//!
//! Passes mutate `Function` / `Program` in place. Run **after**
//! monomorphisation and AST→MIR lowering, **before** MIR→clif.

pub mod arc_peephole;
