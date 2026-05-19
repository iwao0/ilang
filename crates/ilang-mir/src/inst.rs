//! MIR instructions and terminators.
//!
//! Each `Inst` is in SSA form: at most one defined `ValueId` (the
//! "result"), zero or more operand `ValueId`s. Operands defined in the
//! current block must precede uses textually; operands from
//! predecessors arrive via `Block::params` (block-args style, no φ).

use crate::types::{ClassId, EnumId, MirTy};
use ilang_ast::Symbol;

/// SSA value ID — index into `Function::value_tys`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ValueId(pub u32);

/// Block ID — index into `Function::blocks`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(pub u32);

/// Function reference — index into `Program::functions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FuncId(pub u32);

/// Field index inside a class's layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FieldId(pub u32);

/// Variant index inside an enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VariantId(pub u32);

/// Mutable local "slot" — represented to the codegen as a Cranelift
/// Variable so SSA construction (incl. loops) is delegated to the
/// frontend builder. Immutable lets don't need a `LocalId` and stay
/// as plain SSA values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LocalId(pub u32);

/// Vtable slot — index resolved per class hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VTableSlot(pub u32);

/// Constants representable directly in MIR.
#[derive(Debug, Clone, PartialEq)]
pub enum MirConst {
    Bool(bool),
    /// Signed/unsigned/sized integers — width is encoded in the
    /// instruction's result type.
    Int(i64),
    F32(u32), // bits, for Hash/Eq friendliness
    F64(u64),
    Str(Symbol),
    Unit,
    /// `none` of a given Optional<T>. The result type carries `T?`.
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    // Integer
    IAdd,
    ISub,
    IMul,
    IDivS,
    IDivU,
    IRemS,
    IRemU,
    IShl,
    IShrS,
    IShrU,
    IAnd,
    IOr,
    IXor,
    // Float
    FAdd,
    FSub,
    FMul,
    FDiv,
    // Comparison (result: bool)
    IEq,
    INe,
    ILtS,
    ILeS,
    IGtS,
    IGeS,
    ILtU,
    ILeU,
    IGtU,
    IGeU,
    FEq,
    FNe,
    FLt,
    FLe,
    FGt,
    FGe,
    /// Structural string equality.
    StrEq,
    StrNe,
    /// String concatenation.
    StrConcat,
    /// Like `StrConcat` but the MIR lowerer proved that the LHS is
    /// the only holder of its buffer and is about to be reassigned
    /// (the canonical `s = s + expr` shape). Lets the runtime grow
    /// the LHS buffer in place via doubling realloc instead of
    /// allocating a fresh buffer every iteration.
    StrConcatInplace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnOp {
    /// Integer negation.
    INeg,
    /// Float negation.
    FNeg,
    /// Bitwise NOT (integer).
    Not,
    /// Logical NOT (bool).
    BoolNot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CastKind {
    /// Integer widening / narrowing within the same signedness.
    IntResize,
    /// Sign-crossing reinterpret (`i32 → u32` etc.).
    IntSignCross,
    /// Integer → float.
    IntToFloat,
    /// Float → integer (explicit `as` only).
    FloatToInt,
    /// `f32 ↔ f64`.
    FloatResize,
    /// `T → T?` (Optional auto-wrap).
    OptionalWrap,
    /// `Foo → Foo.weak`.
    StrongToWeak,
    /// Raw-pointer reinterprets inside @extern(C).
    PtrCast,
    /// `*T → i64` or `i64 → *T`.
    PtrIntCast,
}

/// A reference to either a top-level (or monomorphised) function, or
/// a built-in runtime function (e.g. `array_push`, `map_get`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FuncRef {
    Local(FuncId),
    /// Runtime/built-in. Resolved at MIR→clif lowering.
    Builtin(Symbol),
    /// `@extern(C) @lib(...)` external function. Carries the symbol
    /// and library list; MIR→clif lowering wires up the dlsym binding.
    Extern { sym: Symbol, libs: Box<[Symbol]>, optional: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnSig {
    pub params: Box<[MirTy]>,
    pub ret: MirTy,
    /// Variadic suffix (printf-style).
    pub variadic: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Inst {
    /// Materialise a constant.
    Const { dst: ValueId, value: MirConst },

    BinOp { dst: ValueId, op: BinOp, lhs: ValueId, rhs: ValueId },
    UnOp { dst: ValueId, op: UnOp, src: ValueId },
    Cast { dst: ValueId, kind: CastKind, src: ValueId },

    /// Direct call. `dst = None` for unit returns.
    Call { dst: Option<ValueId>, callee: FuncRef, args: Box<[ValueId]> },
    /// Indirect call via closure value.
    CallIndirect { dst: Option<ValueId>, callee: ValueId, sig: FnSig, args: Box<[ValueId]> },
    /// Raw indirect call — calls `callee` as a bare C function pointer
    /// with no closure dispatch. Used for the result of
    /// `*void as fn(...)` casts (typically `GetProcAddress` / `dlsym`
    /// return values). No fn_ptr load from offset 0, no trailing env
    /// arg; the value flows straight into Cranelift's `call_indirect`.
    CallRawIndirect { dst: Option<ValueId>, callee: ValueId, sig: FnSig, args: Box<[ValueId]> },
    /// Virtual dispatch — looks up the slot in the receiver's vtable.
    VirtCall { dst: Option<ValueId>, recv: ValueId, slot: VTableSlot, args: Box<[ValueId]> },

    NewObject { dst: ValueId, class: ClassId, init_args: Box<[ValueId]>, init: FuncId },
    LoadField { dst: ValueId, obj: ValueId, field: FieldId },
    StoreField { obj: ValueId, field: FieldId, value: ValueId },

    NewArray { dst: ValueId, elem: MirTy, items: Box<[ValueId]> },
    NewArrayEmpty { dst: ValueId, elem: MirTy, fixed_len: Option<usize> },

    /// Build a SIMD vector value from `lanes` scalar values of
    /// the lane element type. `dst`'s `MirTy` is `Simd { elem,
    /// lanes }`. Codegen lowers via `scalar_to_vector` +
    /// `insertlane` calls into the matching cranelift vector
    /// type (`F32X4`, `I32X4`, …).
    NewSimd { dst: ValueId, lanes: Box<[ValueId]> },
    ArrayLen { dst: ValueId, arr: ValueId },
    ArrayLoad { dst: ValueId, arr: ValueId, idx: ValueId },
    ArrayStore { arr: ValueId, idx: ValueId, value: ValueId },

    NewMap { dst: ValueId, key: MirTy, val: MirTy, entries: Box<[(ValueId, ValueId)]> },
    MapGet { dst: ValueId, map: ValueId, key: ValueId },
    MapSet { map: ValueId, key: ValueId, value: ValueId },

    NewTuple { dst: ValueId, items: Box<[ValueId]> },
    TupleExtract { dst: ValueId, tup: ValueId, idx: u32 },

    NewOptional { dst: ValueId, value: ValueId },
    OptionalIsSome { dst: ValueId, opt: ValueId },
    OptionalUnwrap { dst: ValueId, opt: ValueId },

    NewEnum { dst: ValueId, enum_id: EnumId, variant: VariantId, payload: Box<[ValueId]> },
    EnumTag { dst: ValueId, value: ValueId },
    EnumPayload { dst: ValueId, value: ValueId, variant: VariantId, idx: u32 },
    /// `enum-value as string` for `: string`-repr enums. Emits a
    /// runtime lookup of the discriminant string via the enum's
    /// global id. `value`'s MirTy must be `Enum(enum_id)`; `dst`
    /// is `Str`.
    EnumDiscStr { dst: ValueId, enum_id: EnumId, value: ValueId },

    /// Build a closure with the given function pointer + captures.
    /// Captures' MirTy comes from the function's `closure_env` layout.
    MakeClosure { dst: ValueId, func: FuncId, captures: Box<[ValueId]> },
    /// Bare C function pointer — the 8-byte code address of `func`,
    /// no closure box. Used when assigning a top-level fn to an
    /// `@extern(C)` struct field of `fn(...)` type so that C code
    /// can dereference the slot as a real function pointer.
    /// `dst` has MirTy::I64.
    FuncAddr { dst: ValueId, func: FuncId },
    /// Read capture #idx from the closure currently being executed.
    /// (env pointer is implicit — the function has a hidden env param)
    LoadCapture { dst: ValueId, idx: u32 },

    /// ARC operations. `Retain` / `Release` work on any heap-typed
    /// value; the lowering looks up the runtime helper based on the
    /// operand's MirTy. WeakRetain/WeakRelease are weak-rc-only.
    Retain { value: ValueId },
    Release { value: ValueId },
    WeakRetain { value: ValueId },
    WeakRelease { value: ValueId },
    /// Weak → Optional<Object>. None if the target was freed.
    WeakUpgrade { dst: ValueId, weak: ValueId },

    /// RTTI: `typeof(x)` returns a `Type` handle.
    TypeOf { dst: ValueId, value: ValueId },
    /// `value is ClassName` — walks parent chain at runtime.
    IsInstance { dst: ValueId, value: ValueId, class: ClassId },
    /// `value as? ClassName` — `Optional<Object(class)>`.
    DowncastOrNone { dst: ValueId, value: ValueId, class: ClassId },

    /// Static-field load / store (class-level constant or mutable slot).
    LoadStatic { dst: ValueId, slot: StaticSlotId },
    StoreStatic { slot: StaticSlotId, value: ValueId },

    /// Compile-time intrinsic for runtime panic (out-of-bounds,
    /// divide-by-zero, unwrap on none). Always followed by an
    /// `Unreachable` terminator in the same block.
    Panic { msg: Symbol },

    /// Write to a mutable local "slot". Lowered to a Cranelift
    /// Variable's `def_var` — SSA construction across blocks is
    /// handled by the frontend builder.
    DefLocal { local: LocalId, value: ValueId },
    /// Read the current value of a mutable local. Lowered via
    /// Cranelift's `use_var`.
    UseLocal { dst: ValueId, local: LocalId },
    /// Take the address of a mutable local (`&x` inside @extern(C)).
    /// Forces the local to live in a Cranelift `StackSlot` so the
    /// pointer is stable across the function. `dst` has raw-pointer
    /// type at the type-check level (`*T` of the local's MirTy).
    AddrOfLocal { dst: ValueId, local: LocalId },
    /// Compute the address of a field within a class instance or an
    /// inline struct. `obj` is the heap pointer (for ARC / CRepr
    /// classes) or the inline address (for embedded CRepr structs).
    /// `class` selects the layout — the codegen looks up either
    /// `c_field_offsets[field]` (CRepr) or `OBJECT_HEADER_BYTES +
    /// field * 8` (ARC) and emits an `iadd_imm`. Used by `&x.f`,
    /// `&x.f.g`, etc.; the AST→MIR lowerer composes
    /// `UseLocal + (LoadField)* + AddrOfField` for chains.
    AddrOfField {
        dst: ValueId,
        obj: ValueId,
        class: crate::types::ClassId,
        field: FieldId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StaticSlotId(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub enum Terminator {
    /// Unconditional branch. The args are passed as `Block::params` to
    /// the destination block.
    Br { dst: BlockId, args: Box<[ValueId]> },
    /// Conditional branch on a `bool` value.
    CondBr {
        cond: ValueId,
        then_block: BlockId,
        then_args: Box<[ValueId]>,
        else_block: BlockId,
        else_args: Box<[ValueId]>,
    },
    /// Multi-way branch on an integer scrutinee. `cases` is sorted/
    /// unsorted at the lowering's discretion; clif lowering may emit
    /// a jump table or compare chain.
    Switch {
        scrutinee: ValueId,
        cases: Box<[SwitchCase]>,
        default: BlockId,
        default_args: Box<[ValueId]>,
    },
    Return { value: Option<ValueId> },
    /// Reachable only through `Panic` or other halting intrinsics.
    Unreachable,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SwitchCase {
    pub value: i64,
    pub dst: BlockId,
    pub args: Box<[ValueId]>,
}
