//! Top-level MIR containers: Program, Function, Block, ClassLayout,
//! EnumLayout, VTable, StaticSlot.

use crate::inst::{BlockId, FieldId, FuncId, Inst, StaticSlotId, Terminator, ValueId, VariantId, VTableSlot};
use crate::types::{ClassId, EnumId, MirTy};
use ilang_ast::{Span, Symbol};

/// One full ilang program, post-monomorphisation.
#[derive(Debug, Clone)]
pub struct Program {
    pub functions: Vec<Function>,
    pub classes: Vec<ClassLayout>,
    pub enums: Vec<EnumLayout>,
    pub vtables: Vec<VTable>,
    pub statics: Vec<StaticSlot>,
    /// `__main` — the synthesised entry point that runs top-level
    /// statements (initialises globals, runs side-effecting code,
    /// cleans up).
    pub entry: FuncId,
}

#[derive(Debug, Clone)]
pub struct Function {
    /// Mangled name (post-overload, post-monomorph). Used as the
    /// symbol Cranelift's JITModule receives.
    pub name: Symbol,
    /// Source name for diagnostics (pre-mangle).
    pub display_name: Symbol,
    pub params: Box<[FuncParam]>,
    pub ret: MirTy,
    /// Type of every defined `ValueId`. Indexed by `ValueId.0`.
    pub value_tys: Vec<MirTy>,
    /// Optional source span per ValueId for diagnostics. Same length
    /// as `value_tys`. None for synthetic values.
    pub value_spans: Vec<Option<Span>>,
    pub blocks: Vec<Block>,
    pub entry: BlockId,
    pub kind: FunctionKind,
    /// If this function expects a closure environment, this describes
    /// the layout (the env arrives as the trailing param at the ABI
    /// level, but is hidden from `params`).
    pub closure_env: Option<EnvLayout>,
    /// Body span for diagnostics.
    pub span: Option<Span>,
    /// Type of every defined `LocalId` (mutable slot). Indexed by
    /// `LocalId.0`. Empty for fns with no mutable locals.
    pub local_tys: Vec<MirTy>,
    /// `@extern(C)` C-side symbol name. `None` for non-extern fns
    /// (or extern fns whose `@symbol` matches the ilang name).
    /// `Some(s)` causes the JIT to declare-import `s` as the C
    /// symbol while still letting the ilang code call by `name`.
    pub c_symbol: Option<Symbol>,
    /// `true` for `@extern(C) @optional` fns — the JIT installs an
    /// always-trapping stub when the C symbol can't be resolved, so
    /// the surrounding `os.libLoaded(...)` gating logic still runs.
    pub is_optional: bool,
    /// `@lib("name1", "name2", ...)` libs declared on this fn. The
    /// JIT runtime treats them as a fallback group: an
    /// `os.libLoaded(any-of-them)` query returns true as long as at
    /// least one opens.
    pub libs: Vec<Symbol>,
    /// `@extern(C)` C-variadic declaration (`fn snprintf(..., ...): i32`).
    /// At call sites, the codegen builds a per-call signature with
    /// the actual extra-arg types and dispatches via `call_indirect`.
    pub is_variadic: bool,
}

#[derive(Debug, Clone)]
pub struct FuncParam {
    pub name: Symbol,
    pub ty: MirTy,
    /// The ValueId reachable inside the function body for this param.
    pub value: ValueId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionKind {
    /// Ordinary user-defined or generated function (incl. `__drop_C`,
    /// `__main`, monomorph specialisations).
    Local,
    /// Class init. Carries the class for diagnostics.
    Init { class: ClassId },
    /// Class deinit (drop). Auto-generated, may call user `deinit` body.
    Drop { class: ClassId },
    /// `@extern(C) @lib(...)` declaration — no body, MIR→clif binds
    /// via dlsym.
    Extern { sig_only: bool },
    /// `@extern(C) fn body { ... }` exposed under C ABI.
    ExternBody,
    /// Trampoline for `let f = top_level_fn` — loads no captures and
    /// forwards to the target.
    Trampoline { target: FuncId },
}

#[derive(Debug, Clone)]
pub struct Block {
    /// Block parameters — the SSA stand-in for φ. Predecessor branches
    /// pass arg lists matching this in length.
    pub params: Vec<ValueId>,
    pub insts: Vec<Inst>,
    pub term: Terminator,
}

/// Field/method/property tables for a (post-monomorph) concrete class.
#[derive(Debug, Clone)]
pub struct ClassLayout {
    pub id: ClassId,
    pub name: Symbol,
    /// Parent class for `extends` (`None` for root classes).
    pub parent: Option<ClassId>,
    pub fields: Vec<FieldDecl>,
    pub methods: Vec<MethodDecl>,
    /// Static / const slots declared on this class.
    pub statics: Vec<StaticSlotId>,
    /// Auto-generated drop function (handles deinit + heap-field
    /// release). Always present, even when no user `deinit`.
    pub drop_fn: FuncId,
    /// Vtable assigned to instances of this class. `None` only when
    /// the class is the root and has no inheritance — in practice
    /// every class gets a vtable for uniform header layout.
    pub vtable: Option<u32>,
    /// `@extern(C)` C-compat layout marker. When set, the class has C
    /// struct semantics: natural alignment, no ARC header, no methods.
    pub repr: ClassRepr,
    /// For `CRepr` / `CPacked` / `CUnion` classes: byte offset of each
    /// field within the struct. `Vec::new()` for ARC-managed classes.
    pub c_field_offsets: Vec<i64>,
    /// For `CRepr` / `CPacked` / `CUnion` classes: total byte size of
    /// the struct. Zero for ARC-managed classes.
    pub c_size: i64,
    /// For C99 flexible array members: when the last field is a
    /// dynamic array `T[]`, holds the byte size of `T`. `0` means
    /// no FAM. `new StructName(n)` then allocates `c_size + n*elem`.
    pub flex_elem_size: i64,
    /// `@com interface` marker. ilang's ARC `__retain_object` /
    /// `__release_object` would dereference the rc slot at
    /// `obj_ptr + 8`, but a COM handle's value is the bare interface
    /// pointer — `+8` lands inside the foreign object's private
    /// data. Codegen sees this flag and emits Retain/Release as
    /// no-ops; the user manages lifetime via `IUnknown::AddRef` /
    /// `Release()` explicitly.
    pub is_com_interface: bool,
    /// `@handle pub struct Name {}` marker — Win32-style nominal
    /// pointer-sized opaque handle (HWND, HINSTANCE, HMODULE, ...).
    /// Treated like `@com interface` for retain/release purposes
    /// (no rc header, no ARC plumbing) and accepts the same
    /// `↔ *void` flow at the type-checker level.
    pub is_handle: bool,
    /// Interface class-ids this class conforms to, transitively: its own
    /// declared interfaces plus every interface those inherit from
    /// (`interface B: A`). Empty for interfaces / classes with none. The
    /// `parent` field only records the FIRST base, so `is` / `as?` use
    /// this to recognise additional and inherited interfaces (a class
    /// ancestor's interfaces are reached through the `parent` chain).
    pub implements: Vec<ClassId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassRepr {
    /// Default ARC heap object: `[strong | weak | drop_fn | vtable | fields...]`.
    ArcObject,
    /// `@extern(C) struct` — natural alignment, no header.
    CRepr,
    /// `@extern(C) @packed struct` — align 1, no padding.
    CPacked,
    /// `@extern(C) union` — every field at offset 0.
    CUnion,
}

#[derive(Debug, Clone)]
pub struct FieldDecl {
    pub id: FieldId,
    pub name: Symbol,
    pub ty: MirTy,
    /// Bit-field info for `@extern(C)` `@bits(N)` fields. None for
    /// regular fields.
    pub bit_field: Option<BitField>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BitField {
    /// Offset in bits within the underlying storage unit.
    pub offset: u32,
    /// Width in bits.
    pub width: u32,
}

#[derive(Debug, Clone)]
pub struct MethodDecl {
    pub name: Symbol,
    /// `True` for methods marked `override`.
    pub is_override: bool,
    pub is_static: bool,
    /// The MIR function implementing this method (post-mangle).
    pub func: FuncId,
    /// Vtable slot, if dispatchable virtually. None for static methods,
    /// final methods, init/deinit, or properties without vtable.
    pub slot: Option<VTableSlot>,
}

#[derive(Debug, Clone)]
pub struct EnumLayout {
    pub id: EnumId,
    pub name: Symbol,
    /// Underlying integer type for the discriminant (`u64` by default,
    /// `: u32` etc. when annotated).
    pub repr: MirTy,
    pub variants: Vec<VariantDecl>,
    /// True when declared with `@flags`.
    pub is_flags: bool,
}

#[derive(Debug, Clone)]
pub struct VariantDecl {
    pub id: VariantId,
    pub name: Symbol,
    /// Integer-repr enums use `discriminant` (the raw signed int).
    /// String-repr enums use `discriminant_str` and leave the
    /// integer slot at the variant's declaration index. Exactly
    /// one slot is meaningful per enum, decided by the enum's
    /// `repr` (`MirTy::Str` ⇒ string, otherwise integer).
    pub discriminant: i64,
    pub discriminant_str: Option<String>,
    pub payload: VariantPayload,
}

#[derive(Debug, Clone)]
pub enum VariantPayload {
    Unit,
    Tuple(Box<[MirTy]>),
    Struct(Box<[(Symbol, MirTy)]>),
}

#[derive(Debug, Clone)]
pub struct VTable {
    pub class: ClassId,
    /// Each slot resolves to a function pointer (concrete `FuncId`)
    /// for the most-derived implementation visible at this class.
    pub slots: Vec<FuncId>,
}

#[derive(Debug, Clone)]
pub struct StaticSlot {
    pub id: StaticSlotId,
    pub owner: ClassId,
    pub name: Symbol,
    pub ty: MirTy,
    pub is_const: bool,
    /// Compile-time-folded initial value.
    pub init: crate::inst::MirConst,
}

#[derive(Debug, Clone)]
pub struct EnvLayout {
    /// Captures, in order. The MIR `LoadCapture` indices match this.
    pub captures: Vec<EnvCapture>,
}

#[derive(Debug, Clone)]
pub struct EnvCapture {
    pub name: Symbol,
    pub ty: MirTy,
    /// True if the capture is a mutable cell (stored as a heap cell
    /// pointer rather than a value snapshot).
    pub is_cell: bool,
}

impl Program {
    pub fn new(entry: FuncId) -> Self {
        Self {
            functions: Vec::new(),
            classes: Vec::new(),
            enums: Vec::new(),
            vtables: Vec::new(),
            statics: Vec::new(),
            entry,
        }
    }

    pub fn function(&self, id: FuncId) -> &Function {
        &self.functions[id.0 as usize]
    }
    pub fn function_mut(&mut self, id: FuncId) -> &mut Function {
        &mut self.functions[id.0 as usize]
    }
    pub fn class(&self, id: ClassId) -> &ClassLayout {
        &self.classes[id.0 as usize]
    }
    pub fn enum_(&self, id: EnumId) -> &EnumLayout {
        &self.enums[id.0 as usize]
    }
    pub fn static_slot(&self, id: StaticSlotId) -> &StaticSlot {
        &self.statics[id.0 as usize]
    }
}

impl Function {
    pub fn block(&self, id: BlockId) -> &Block {
        &self.blocks[id.0 as usize]
    }
    pub fn block_mut(&mut self, id: BlockId) -> &mut Block {
        &mut self.blocks[id.0 as usize]
    }
    pub fn ty_of(&self, v: ValueId) -> &MirTy {
        &self.value_tys[v.0 as usize]
    }
}
