//! MIR → Cranelift IR lowering.
//!
//! M1 scope: integer / float / bool primitives, `BinOp` / `UnOp`,
//! straight-line and basic block control flow, direct calls. Heap
//! types (string / array / map / object) and ARC operations are
//! follow-up work — the existing `ilang-codegen` crate handles those
//! today; this new crate carves out the new pipeline incrementally.
//!
//! Public entry point: [`compile_program`].

pub mod compile;
pub mod ty;

pub use compile::{
    compile_program, compile_with_builtins, reset_repl_slots, run_main, BuiltinDecl, CompileError,
};
