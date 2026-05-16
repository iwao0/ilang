//! Mid-level IR for ilang.
//!
//! Lives between the type-checked AST and Cranelift IR. SSA in
//! block-args style — predecessor branches pass argument lists that
//! materialise as the destination block's parameters, in lieu of φ
//! nodes. See `docs/syntax.md` for the language surface that gets
//! lowered into this IR.
//!
//! M1 scope (this crate):
//! - Pure data structures (`Program`, `Function`, `Block`, `Inst`,
//!   `Terminator`, `MirTy`).
//! - `FunctionBuilder` for incremental assembly.
//! - Textual printer for golden tests.
//! - Validator (SSA single-assign, branch-arg arity).
//!
//! AST→MIR lowering, monomorphisation, and MIR→clif lowering land in
//! follow-up steps.

pub mod builder;
pub mod inst;
pub mod lower;
pub mod monomorphize;
pub mod passes;
pub mod printer;
pub mod program;
pub mod types;
pub mod validate;

pub use builder::FunctionBuilder;
pub use lower::{lower_program, lower_program_with_slots, ty_to_mir, LowerError};
pub use inst::{
    BinOp, BlockId, CastKind, FieldId, FnSig, FuncId, FuncRef, Inst, LocalId, MirConst,
    StaticSlotId, SwitchCase, Terminator, UnOp, ValueId, VariantId, VTableSlot,
};
pub use printer::{print_function, print_program};
pub use program::{
    BitField, Block, ClassLayout, ClassRepr, EnumLayout, EnvCapture, EnvLayout, FieldDecl,
    FuncParam, Function, FunctionKind, MethodDecl, Program, StaticSlot, VTable, VariantDecl,
    VariantPayload,
};
pub use types::{ClassId, EnumId, MirFnTy, MirTy};
pub use validate::{validate_function, validate_program, ValidateError};
