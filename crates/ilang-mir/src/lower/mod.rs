//! AST → MIR lowering.
//!
//! Driven by `lower_program`. Currently covers a working subset of
//! the language; remaining node kinds are listed as `Unsupported`
//! errors so the integration tests fail loudly until we expand
//! coverage. The aim is to grow this file feature-by-feature in the
//! same order as `docs/syntax.md`.

use std::collections::HashMap;

use ilang_ast::{
    self as ast, Block as AstBlock, Expr, ExprKind, Item, Program as AstProgram, Span, Stmt,
    StmtKind, Symbol, Type,
};

mod call_fn;
mod calls;
mod coerce;
mod collect;
mod control;
mod decl;
mod env;
mod fn_expr;
mod iter_ctor;
mod literals;
mod match_;
mod ops;
mod utils;

use env::{Binding, Env, LoopFrame};
pub use utils::ty_to_mir;

use crate::builder::FunctionBuilder;
use crate::inst::{
    FuncId, FuncRef, Inst, MirConst, Terminator, ValueId,
};
use crate::program::{Function, Program};
use crate::types::MirTy;

#[derive(Debug, thiserror::Error)]
pub enum LowerError {
    #[error("{0}")]
    Other(String),
    #[error("unsupported in M1: {0}")]
    Unsupported(&'static str),
}

/// Lower a (post-typecheck) AST `Program` into MIR. The caller is
/// expected to have run the type checker first; we re-derive
/// expression types locally because the AST does not carry them.
pub fn lower_program(prog: &AstProgram) -> Result<Program, LowerError> {
    lower_program_with_slots(prog, &HashMap::new())
}

/// REPL variant: lower with a slot table that maps top-level binding
/// names to a stable host-side slot index plus the binding's AST
/// `Type`. The AST type is converted to `MirTy` lazily inside the
/// lower context so class / enum names can be resolved against the
/// per-program registries.
///
/// At lowering:
///
/// 1. A top-level `let x = expr` whose name is in `slots` gets, after
///    its normal init, a `__repl_store_slot(idx, x_as_i64)` call so
///    the value persists across REPL turns.
/// 2. Any `Var(x)` lookup that misses every local binding (i.e. the
///    name is not bound in this chunk) is resolved by emitting a
///    `__repl_load_slot(idx)` call, then bit-reinterpreting the
///    returned i64 as the slot's declared MirTy.
///
/// All heap-typed slots store an i64 pointer; reinterpretation is a
/// no-op for those. Primitive slots round-trip through coerce.
pub fn lower_program_with_slots(
    prog: &AstProgram,
    slots: &HashMap<Symbol, (u32, ilang_ast::Type)>,
) -> Result<Program, LowerError> {
    let mut lower = Lower::new();
    // Defer slot-type resolution: classes/enums/etc. need the
    // class_ids/enum_ids tables that are populated during the
    // pre-passes below. We resolve and stash them right after
    // `register_class` / `register_enum` complete.
    lower.repl_slot_ast = slots.clone();
    // Built-in `Result<T, E>` is always available — the language
    // reserves the name and the loader doesn't include a stdlib file
    // for it. Pre-register so user `Result.ok(...)` / match on Result
    // resolve.
    lower.register_builtin_result();

    // 1a. Pre-pass: register every class NAME (regular + @extern(C))
    //     before resolving anything. Lets fields reference classes
    //     declared later in the file.
    for item in &prog.items {
        if let Item::Class(cd) = item {
            if !lower.class_ids.contains_key(&cd.name) {
                let id = crate::types::ClassId(lower.classes.len() as u32);
                lower.class_ids.insert(cd.name, id);
                lower.classes.push(crate::program::ClassLayout {
                    id,
                    name: cd.name,
                    parent: None,
                    fields: Vec::new(),
                    methods: Vec::new(),
                    statics: Vec::new(),
                    drop_fn: FuncId(u32::MAX),
                    vtable: None,
                    repr: crate::program::ClassRepr::ArcObject,
                    c_field_offsets: Vec::new(),
                    c_size: 0,
                    flex_elem_size: 0,
                });
                lower.class_meta.insert(id, ClassMeta::default());
            }
        }
    }

    // 1. Register every class shell (id + field indices), enum
    //    layout, and @extern(C) struct shell before any type
    //    resolution or fn declaration so signatures referencing them
    //    by name work.
    for item in &prog.items {
        match item {
            Item::Class(cd) => lower.register_class(cd)?,
            Item::Enum(ed) => lower.register_enum(ed)?,
            Item::ExternC(blk) => lower.register_extern_c_shells(blk)?,
            _ => {}
        }
    }

    // 2. Pre-register every top-level fn (signature only) so calls can
    //    refer to them regardless of declaration order.
    for item in &prog.items {
        if let Item::Fn(fd) = item {
            lower.declare_fn(fd)?;
        }
    }

    // 2b. Pre-register every @extern(C) fn / fn def so class method
    //     bodies that call them resolve correctly.
    for item in &prog.items {
        if let Item::ExternC(blk) = item {
            lower.declare_extern_c_fns(blk)?;
        }
    }

    // 3. Pre-register every method (incl. init) on every class.
    for item in &prog.items {
        if let Item::Class(cd) = item {
            lower.declare_class_methods(cd)?;
        }
    }

    // 3b. REPL slot-type resolution. The class / enum tables are now
    //     populated, so any pending `repl_slot_ast` entry (deferred by
    //     `lower_program_with_slots`) can be promoted to the typed
    //     `repl_slots` map. Entries whose type still doesn't resolve
    //     (e.g. types depending on items not in this chunk) are
    //     silently dropped — the REPL falls back to chunk-local
    //     binding semantics for those names.
    let pending_slots = std::mem::take(&mut lower.repl_slot_ast);
    for (name, (idx, ty)) in pending_slots {
        if let Ok(mir_ty) = lower.resolve_ty(&ty) {
            lower.repl_slots.insert(name, (idx, mir_ty));
        }
    }

    // 4. Lower bodies: free fns, then class methods.
    for item in &prog.items {
        match item {
            Item::Fn(fd) => lower.lower_fn(fd)?,
            Item::Class(cd) => lower.lower_class_methods(cd)?,
            // Enums have no bodies — registration handled them.
            Item::Enum(_) => {}
            Item::Const(_) => {
                // Loader's inline pass folds constants away before
                // type checking — we shouldn't see this here.
                return Err(LowerError::Other("unexpected Item::Const after loader inlining".into()));
            }
            Item::Use(_) => {
                return Err(LowerError::Other("unexpected Item::Use post-loader".into()));
            }
            Item::ExternC(blk) => lower.lower_extern_c(blk)?,
        }
    }

    // Synthesise __main from the program's top-level statements + tail.
    lower.lower_main(&prog.stmts, prog.tail.as_ref())?;

    // Drain any pending closure bodies (closures-of-closures included).
    while let Some(pc) = lower.pending_closures.pop() {
        lower.lower_pending_closure(pc)?;
    }

    Ok(lower.finish())
}

// ---------------------------------------------------------------- //

struct Lower {
    /// All MIR functions accumulated so far. The entry point goes in
    /// last (after main is built).
    funcs: Vec<Function>,
    /// Top-level fn name → FuncId, populated by the pre-registration
    /// pass. Indexes into `funcs` once `lower_fn` has been called.
    fn_ids: HashMap<Symbol, FuncId>,
    /// Top-level fn signatures (param/ret types, MIR side). Used by
    /// callers to know the return type without re-checking.
    fn_sigs: HashMap<Symbol, FnSig>,
    /// Registered class layouts (one per class declaration).
    classes: Vec<crate::program::ClassLayout>,
    /// Class name → ClassId.
    class_ids: HashMap<Symbol, crate::types::ClassId>,
    /// Per-class metadata used during lowering: field name → FieldId,
    /// method name → FuncId for the (mangle-resolved) implementation.
    class_meta: HashMap<crate::types::ClassId, ClassMeta>,
    /// Registered enum layouts.
    enums: Vec<crate::program::EnumLayout>,
    enum_ids: HashMap<Symbol, crate::types::EnumId>,
    enum_meta: HashMap<crate::types::EnumId, EnumMeta>,
    /// Top-level static slots accumulated across classes.
    statics: Vec<crate::program::StaticSlot>,
    /// Synthesised closure functions waiting for their bodies to be
    /// lowered. Drained after each outer fn body completes so that
    /// nested closures get processed without recursive borrows.
    pending_closures: Vec<PendingClosure>,
    /// Counter for anonymous fn names.
    anon_counter: u32,
    /// Per @extern(C) @lib fn binding metadata.
    extern_meta: HashMap<Symbol, ExternMeta>,
    /// `__main`'s FuncId, set during `lower_main`. Subsequent pending
    /// closures push more functions, so `funcs.len() - 1` no longer
    /// identifies the entry.
    main_id: Option<FuncId>,
    /// User-visible name → list of mangled names (each registered in
    /// `fn_ids` / `fn_sigs`). Most entries have a single mangled name
    /// equal to the user name; overloaded fns have one entry per
    /// declared overload.
    overloads: HashMap<Symbol, Vec<Symbol>>,
    /// REPL persistent slots: name → (slot index, MirTy). Set via
    /// [`lower_program_with_slots`]. Empty for non-REPL compilations.
    /// Drives host-slot store/load emission in __main and Var lookup.
    /// Resolved lazily from `repl_slot_ast` after the class / enum
    /// pre-passes register their ids.
    repl_slots: HashMap<Symbol, (u32, MirTy)>,
    /// Pre-resolution slot table from the REPL caller. Each entry is
    /// converted to `repl_slots[name]` once the lowerer's class /
    /// enum tables are populated.
    repl_slot_ast: HashMap<Symbol, (u32, ilang_ast::Type)>,
}

// Recorded for every `@extern(C) @lib(..)` fn while lowering. The
// MIR-codegen reads these fields off `Function` directly today, so
// the fields here are bookkeeping for any future passes that might
// want richer per-extern metadata in the AST→MIR layer.
#[allow(dead_code)]
struct ExternMeta {
    libs: Vec<Symbol>,
    optional: bool,
    variadic: bool,
    c_symbol: Symbol,
}

struct PendingClosure {
    func_id: FuncId,
    name: Symbol,
    params: Vec<(Symbol, MirTy)>,
    ret: MirTy,
    captures: Vec<crate::program::EnvCapture>,
    body: ast::Block,
    span: Span,
    /// Outer-method class context — preserved so `super.method(...)`
    /// inside the closure body can resolve to the parent class.
    enclosing_this_class: Option<crate::types::ClassId>,
}

#[derive(Default)]
struct EnumMeta {
    /// Variant name → (VariantId, discriminant, payload kind).
    variants: HashMap<Symbol, EnumVariantMeta>,
}

struct EnumVariantMeta {
    id: crate::inst::VariantId,
    payload: VariantPayloadMeta,
}

#[derive(Clone)]
enum VariantPayloadMeta {
    Unit,
    /// Tuple variant — element MirTys in order.
    Tuple(Vec<MirTy>),
    /// Struct variant — field name → (idx, MirTy).
    Struct(Vec<(Symbol, MirTy)>),
}

#[derive(Default)]
struct ClassMeta {
    field_ix: HashMap<Symbol, crate::inst::FieldId>,
    field_ty: HashMap<crate::inst::FieldId, MirTy>,
    /// Includes both regular methods and `init` (under the symbol "init").
    method_ids: HashMap<Symbol, FuncId>,
    method_sigs: HashMap<Symbol, FnSig>,
    /// `static name(...): T { ... }` — called as `Class.method(...)`.
    static_method_ids: HashMap<Symbol, FuncId>,
    static_method_sigs: HashMap<Symbol, FnSig>,
    /// `static name: T = ...` / `const name: T = ...` slots.
    static_slots: HashMap<Symbol, crate::inst::StaticSlotId>,
    /// `get name(): T` — synthesised method id for the getter.
    property_getter: HashMap<Symbol, (FuncId, MirTy)>,
    /// `set name(v: T)` — synthesised method id for the setter.
    property_setter: HashMap<Symbol, (FuncId, MirTy)>,
}

#[derive(Clone)]
pub(super) struct FnSig {
    pub(super) params: Vec<MirTy>,
    pub(super) ret: MirTy,
}

impl Lower {
    fn register_builtin_result(&mut self) {
        // Built-in `Result<T, E>`. Treated as non-generic at MIR level
        // (T and E both flow as i64 cells); the codegen reads payload
        // bytes uniformly.
        let id = crate::types::EnumId(self.enums.len() as u32);
        let name = Symbol::intern("Result");
        self.enum_ids.insert(name, id);
        let mut meta = EnumMeta::default();
        let ok_id = crate::inst::VariantId(0);
        let err_id = crate::inst::VariantId(1);
        meta.variants.insert(
            Symbol::intern("ok"),
            EnumVariantMeta {
                id: ok_id,
                payload: VariantPayloadMeta::Tuple(vec![MirTy::I64]),
            },
        );
        meta.variants.insert(
            Symbol::intern("err"),
            EnumVariantMeta {
                id: err_id,
                payload: VariantPayloadMeta::Tuple(vec![MirTy::I64]),
            },
        );
        self.enums.push(crate::program::EnumLayout {
            id,
            name,
            repr: MirTy::I64,
            variants: vec![
                crate::program::VariantDecl {
                    id: ok_id,
                    name: Symbol::intern("ok"),
                    discriminant: 0,
                    discriminant_str: None,
                    payload: crate::program::VariantPayload::Tuple(
                        vec![MirTy::I64].into_boxed_slice(),
                    ),
                },
                crate::program::VariantDecl {
                    id: err_id,
                    name: Symbol::intern("err"),
                    discriminant: 1,
                    discriminant_str: None,
                    payload: crate::program::VariantPayload::Tuple(
                        vec![MirTy::I64].into_boxed_slice(),
                    ),
                },
            ],
            is_flags: false,
        });
        self.enum_meta.insert(id, meta);
    }

    fn new() -> Self {
        Self {
            funcs: Vec::new(),
            fn_ids: HashMap::new(),
            fn_sigs: HashMap::new(),
            classes: Vec::new(),
            class_ids: HashMap::new(),
            class_meta: HashMap::new(),
            enums: Vec::new(),
            enum_ids: HashMap::new(),
            enum_meta: HashMap::new(),
            statics: Vec::new(),
            pending_closures: Vec::new(),
            anon_counter: 0,
            extern_meta: HashMap::new(),
            main_id: None,
            overloads: HashMap::new(),
            repl_slots: HashMap::new(),
            repl_slot_ast: HashMap::new(),
        }
    }

    fn finish(mut self) -> Program {
        // `lower_main` records the entry id; subsequent pending
        // closures push more functions afterwards, so we can't rely
        // on `funcs.len() - 1`.
        let entry = self
            .main_id
            .unwrap_or_else(|| FuncId((self.funcs.len() - 1) as u32));
        let mut p = Program::new(entry);
        p.functions = std::mem::take(&mut self.funcs);
        p.classes = std::mem::take(&mut self.classes);
        p.enums = std::mem::take(&mut self.enums);
        p.statics = std::mem::take(&mut self.statics);
        p
    }

    fn resolve_ty(&self, t: &Type) -> Result<MirTy, LowerError> {
        match t {
            // The parser doesn't know whether a bare `Foo` names a class
            // or an enum — both surface as `Type::Object`. Resolve here
            // by checking the registry that holds the name.
            Type::Object(name) => {
                if let Some(id) = self.class_ids.get(name) {
                    Ok(MirTy::Object(*id))
                } else if let Some(id) = self.enum_ids.get(name) {
                    Ok(MirTy::Enum(*id))
                } else {
                    Err(LowerError::Other(format!("unknown type: {name}")))
                }
            }
            Type::Enum(name) => match self.enum_ids.get(name) {
                Some(id) => Ok(MirTy::Enum(*id)),
                None => Err(LowerError::Other(format!("unknown enum type: {name}"))),
            },
            Type::Array { elem, fixed } => Ok(MirTy::Array {
                elem: Box::new(self.resolve_ty(elem)?),
                len: *fixed,
            }),
            Type::Tuple(elems) => {
                let mut out = Vec::with_capacity(elems.len());
                for e in elems.iter() {
                    out.push(self.resolve_ty(e)?);
                }
                Ok(MirTy::Tuple(out.into_boxed_slice()))
            }
            Type::Optional(inner) => Ok(MirTy::Optional(Box::new(self.resolve_ty(inner)?))),
            Type::Weak(inner) => match &**inner {
                Type::Object(cname) => match self.class_ids.get(cname) {
                    Some(id) => Ok(MirTy::Weak(*id)),
                    None => Err(LowerError::Other(format!("unknown class for weak ref: {cname}"))),
                },
                _ => Err(LowerError::Other("`.weak` only applies to class types".into())),
            },
            Type::Generic(g) => {
                // Built-in `Map<K, V>` is special-cased here.
                if g.base.as_str() == "Map" && g.args.len() == 2 {
                    let key = self.resolve_ty(&g.args[0])?;
                    let val = self.resolve_ty(&g.args[1])?;
                    return Ok(MirTy::Map { key: Box::new(key), val: Box::new(val) });
                }
                // Built-in `Result<T, E>` is registered as a
                // non-generic enum (i64 payload cells) — fall through
                // by name lookup.
                if let Some(id) = self.enum_ids.get(&g.base) {
                    return Ok(MirTy::Enum(*id));
                }
                Err(LowerError::Unsupported("user-defined generic types"))
            }
            Type::Fn(ft) => {
                let mut params = Vec::with_capacity(ft.params.len());
                for p in ft.params.iter() {
                    params.push(self.resolve_ty(p)?);
                }
                let ret = self.resolve_ty(&ft.ret)?;
                Ok(MirTy::Fn(Box::new(crate::types::MirFnTy {
                    params: params.into_boxed_slice(),
                    ret,
                })))
            }
            Type::RawPtr { is_const, inner } => Ok(MirTy::RawPtr {
                is_const: *is_const,
                inner: Box::new(self.resolve_ty(inner)?),
            }),
            other => ty_to_mir(other),
        }
    }


}




struct BodyCx<'a> {
    fb: &'a mut FunctionBuilder,
    env: &'a mut Env,
    ret_ty: MirTy,
    fn_ids: &'a mut HashMap<Symbol, FuncId>,
    fn_sigs: &'a mut HashMap<Symbol, FnSig>,
    loops: Vec<LoopFrame>,
    /// The receiver class when lowering a method body (`Some(cid)`).
    this_class: Option<crate::types::ClassId>,
    classes: &'a [crate::program::ClassLayout],
    class_meta: &'a HashMap<crate::types::ClassId, ClassMeta>,
    enum_ids: &'a HashMap<Symbol, crate::types::EnumId>,
    enum_meta: &'a HashMap<crate::types::EnumId, EnumMeta>,
    enums: &'a [crate::program::EnumLayout],
    statics: &'a [crate::program::StaticSlot],
    /// Slot for pushing newly-discovered anonymous closures that need
    /// their bodies lowered after the current fn finishes.
    pending: &'a mut Vec<PendingClosure>,
    funcs: &'a mut Vec<Function>,
    anon_counter: &'a mut u32,
    /// Captures available in this scope (only set when lowering a
    /// closure body — maps a captured name to its `LoadCapture(i)`
    /// index plus type).
    captures_in_scope: Option<&'a HashMap<Symbol, (u32, MirTy)>>,
    /// Names whose captures are heap cells (the cell pointer was
    /// captured, not the value snapshot). Reads / writes go through
    /// `ArrayLoad` / `ArrayStore` after a `LoadCapture` on the cell
    /// pointer.
    cell_captures: Option<&'a std::collections::HashSet<Symbol>>,
    overloads: &'a HashMap<Symbol, Vec<Symbol>>,
    /// Names that should be allocated as heap cells inside this fn
    /// body (because some inner closure captures+mutates them).
    /// Populated by a per-fn-body pre-pass.
    cellify_set: &'a std::collections::HashSet<Symbol>,
    /// REPL persistent slots: name → (slot index, MirTy). Forwarded
    /// from `Lower::repl_slots`. Drives `__repl_load_slot` emission
    /// in `Var` lookup (any fn body) and `__repl_store_slot` after
    /// top-level `let`s in `__main` when `is_main_body` is set.
    repl_slots: &'a HashMap<Symbol, (u32, MirTy)>,
    /// True iff we're lowering `__main`'s body. Restricts top-level
    /// `let` → slot-store to that scope so a same-named local in a
    /// fn body doesn't accidentally clobber the REPL slot.
    is_main_body: bool,
    /// Locals whose value is an owned `host_mir_alloc` buffer for a
    /// CRepr (no-rc-header) struct. Populated when a `let` binding
    /// stores a fresh `new T()` of a CRepr class. `release_top_scope
    /// _objects` consults this when emitting the scope-exit Release
    /// for CRepr Locals — without it, a `let p = r.origin` (where
    /// `r.origin` is just a borrow into `r`'s buffer) would
    /// erroneously free part of `r`'s memory.
    crepr_owned_locals: std::collections::HashSet<crate::inst::LocalId>,
    /// Name of the top-level slot binding currently being assigned
    /// (Some(X) while we're inside the value of `let X = ...`).
    /// `lower_fn_expr` checks this to avoid snapshotting the X slot
    /// when X appears as a free var inside the FnExpr body — that's
    /// the canonical self-recursive closure pattern, where the slot
    /// hasn't been written yet at construction time. The Var
    /// lookup inside the body resolves through the slot at call
    /// time instead.
    binding_self_name: Option<Symbol>,
}

impl<'a> BodyCx<'a> {
    fn statics_by_id(&self, id: crate::inst::StaticSlotId) -> crate::program::StaticSlot {
        self.statics[id.0 as usize].clone()
    }
    fn overloads_lookup(&self, name: Symbol) -> Option<Vec<Symbol>> {
        self.overloads.get(&name).cloned()
    }

    /// Bit-cast a value of `from` MirTy to a raw i64 for storage in a
    /// REPL slot. Heap pointers pass through; signed ints sextend;
    /// unsigned / bool zext; floats bitcast. Used by both the let-
    /// store path and any other slot-write site.
    fn value_to_i64(&mut self, v: ValueId, from: &MirTy) -> Result<ValueId, LowerError> {
        use crate::inst::CastKind;
        match from {
            MirTy::I64 | MirTy::U64 => Ok(v),
            MirTy::Object(_)
            | MirTy::Array { .. }
            | MirTy::Tuple(_)
            | MirTy::Map { .. }
            | MirTy::Optional(_)
            | MirTy::Fn(_)
            | MirTy::Str
            | MirTy::Enum(_)
            | MirTy::Weak(_)
            | MirTy::RawPtr { .. } => Ok(v),
            MirTy::I8 | MirTy::I16 | MirTy::I32 | MirTy::SSize => {
                let dst = self.fb.new_value(MirTy::I64);
                self.fb.push_inst(Inst::Cast { dst, kind: CastKind::IntResize, src: v });
                Ok(dst)
            }
            MirTy::U8 | MirTy::U16 | MirTy::U32 | MirTy::Size | MirTy::CChar | MirTy::Bool => {
                let dst = self.fb.new_value(MirTy::I64);
                // IntSignCross widens via uextend (zero-extend).
                self.fb.push_inst(Inst::Cast { dst, kind: CastKind::IntSignCross, src: v });
                Ok(dst)
            }
            MirTy::F64 | MirTy::F32 => {
                // No bitcast inst — funnel through the raw-ptr cast
                // which is a same-width identity at the clif level.
                // For F32 we'd lose the high bits; document as an
                // M1 limitation (REPL never round-trips f32 specially).
                let dst = self.fb.new_value(MirTy::I64);
                self.fb.push_inst(Inst::Cast { dst, kind: CastKind::PtrIntCast, src: v });
                Ok(dst)
            }
            MirTy::Unit => {
                // Unit slot: store a zero sentinel.
                Ok(self.const_int(MirTy::I64, 0))
            }
            MirTy::CVoid | MirTy::TypeVar(_) => Err(LowerError::Other(format!(
                "REPL slot store: unsupported type {from}"
            ))),
        }
    }

    /// Reverse of `value_to_i64` — narrow a raw i64 back to the slot's
    /// declared MirTy. Heap pointers reinterpret via PtrIntCast (a
    /// no-op at the bit level); primitives narrow via Cast.
    fn i64_to_slot_value(
        &mut self,
        raw: ValueId,
        to: &MirTy,
    ) -> Result<ValueId, LowerError> {
        use crate::inst::CastKind;
        match to {
            MirTy::I64 | MirTy::U64 => Ok(raw),
            MirTy::Object(_)
            | MirTy::Array { .. }
            | MirTy::Tuple(_)
            | MirTy::Map { .. }
            | MirTy::Optional(_)
            | MirTy::Fn(_)
            | MirTy::Str
            | MirTy::Enum(_)
            | MirTy::Weak(_)
            | MirTy::RawPtr { .. } => {
                let dst = self.fb.new_value(to.clone());
                self.fb.push_inst(Inst::Cast { dst, kind: CastKind::PtrIntCast, src: raw });
                Ok(dst)
            }
            MirTy::I8 | MirTy::I16 | MirTy::I32 | MirTy::SSize
            | MirTy::U8 | MirTy::U16 | MirTy::U32 | MirTy::Size | MirTy::CChar
            | MirTy::Bool => {
                let dst = self.fb.new_value(to.clone());
                self.fb.push_inst(Inst::Cast { dst, kind: CastKind::IntResize, src: raw });
                Ok(dst)
            }
            MirTy::F64 | MirTy::F32 => {
                let dst = self.fb.new_value(to.clone());
                self.fb.push_inst(Inst::Cast { dst, kind: CastKind::PtrIntCast, src: raw });
                Ok(dst)
            }
            MirTy::Unit => Ok(self.const_unit()),
            MirTy::CVoid | MirTy::TypeVar(_) => Err(LowerError::Other(format!(
                "REPL slot load: unsupported type {to}"
            ))),
        }
    }

    /// Resolve a name to its current value, emitting `UseLocal` for
    /// mutable bindings. Returns `None` if the name is unbound.
    fn lookup_var(&mut self, name: Symbol) -> Option<(ValueId, MirTy)> {
        match self.env.lookup_binding(name)? {
            Binding::Ssa(v, t) => Some((v, t)),
            Binding::Local(lid, t) => {
                let v = self.fb.new_value(t.clone());
                self.fb.push_inst(Inst::UseLocal { dst: v, local: lid });
                Some((v, t))
            }
            Binding::Cell(cell_v, t) => {
                let zero = self.const_int(MirTy::I64, 0);
                let v = self.fb.new_value(t.clone());
                self.fb.push_inst(Inst::ArrayLoad { dst: v, arr: cell_v, idx: zero });
                Some((v, t))
            }
        }
    }

    /// Look up the cell pointer (without dereferencing) for a Cell
    /// binding. Used at closure-capture sites so the closure shares
    /// the same heap cell with the outer scope.
    fn lookup_cell_ptr(&self, name: Symbol) -> Option<(ValueId, MirTy)> {
        match self.env.lookup_binding(name)? {
            Binding::Cell(cell_v, t) => Some((cell_v, t)),
            _ => None,
        }
    }

    /// Assign to an existing binding. Returns whether the binding
    /// existed. For Local bindings, emits a `DefLocal`. For Ssa
    /// bindings, replaces the slot's payload.
    fn assign_var(&mut self, name: Symbol, v: ValueId, ty: MirTy) -> bool {
        // The rhs's MirTy may be wider than the binding's declared
        // type after `unify_numeric` promoted a mixed-sign / mixed-
        // width arithmetic operand. `i = i + 1` (i: i32) is the
        // canonical case: `i + 1` widens to i64, but the Local was
        // declared i32, so a raw `def_var` would fail the
        // Cranelift type check. Insert a narrowing coerce when the
        // shapes don't already match.
        match self.env.lookup_binding(name) {
            Some(Binding::Local(lid, slot_ty)) => {
                let coerced = if slot_ty == ty {
                    v
                } else {
                    self.coerce(v, &ty, &slot_ty, Span::dummy()).unwrap_or(v)
                };
                self.fb
                    .push_inst(Inst::DefLocal { local: lid, value: coerced });
                true
            }
            Some(Binding::Cell(cell_v, slot_ty)) => {
                let coerced = if slot_ty == ty {
                    v
                } else {
                    self.coerce(v, &ty, &slot_ty, Span::dummy()).unwrap_or(v)
                };
                let zero = self.const_int(MirTy::I64, 0);
                // For heap-typed cells the cell owns the slot's rc:
                // release the previous occupant and retain the new
                // one. Without these, the old value's share leaks
                // (or worse, double-frees on later cell release) and
                // the new value's share goes unaccounted. Caught by
                // ASan as a UAF in `host_retain_object` while
                // re-assigning closure-captured Box instances.
                let heap_slot = matches!(
                    slot_ty,
                    MirTy::Object(_)
                        | MirTy::Fn(_)
                        | MirTy::Array { .. }
                        | MirTy::Optional(_)
                        | MirTy::Tuple(_)
                        | MirTy::Map { .. }
                        | MirTy::Str
                        | MirTy::Enum(_)
                );
                if heap_slot {
                    let old = self.fb.new_value(slot_ty.clone());
                    self.fb.push_inst(Inst::ArrayLoad {
                        dst: old,
                        arr: cell_v,
                        idx: zero,
                    });
                    self.fb.push_inst(Inst::Release { value: old });
                    self.fb.push_inst(Inst::Retain { value: coerced });
                }
                self.fb.push_inst(Inst::ArrayStore {
                    arr: cell_v,
                    idx: zero,
                    value: coerced,
                });
                true
            }
            Some(Binding::Ssa(_, _)) => {
                self.env.rebind(name, v, ty);
                true
            }
            None => false,
        }
    }
}

/// Emit `Inst::Retain` on `v` when `ty` is a heap-tracked MirTy.
/// Used by paths that store a value into a container the runtime
/// will later release on cascade (cell captures, etc.).

impl<'a> BodyCx<'a> {
    fn const_int(&mut self, ty: MirTy, n: i64) -> ValueId {
        let dst = self.fb.new_value(ty);
        self.fb.push_inst(Inst::Const { dst, value: MirConst::Int(n) });
        dst
    }
    fn const_unit(&mut self) -> ValueId {
        let dst = self.fb.new_value(MirTy::Unit);
        self.fb.push_inst(Inst::Const { dst, value: MirConst::Unit });
        dst
    }

    /// Standard refcount calling convention: callee returns +1 to
    /// caller. Three tail flavours need different handling:
    ///
    ///  (a) Fresh allocation (NewObject / Call / Binary on Str /
    ///      array literal / closure expr / …): rc=1 is already +1
    ///      for the caller. No retain.
    ///
    ///  (b) Var that resolves to a let-bound Local INSIDE the body
    ///      block: lower_block inserts a tail-alias retain to
    ///      balance the scope-exit release; the Local's +1
    ///      transfers to the caller. No extra retain.
    ///
    ///  (c) Var that resolves to an outer-scope binding (params
    ///      like `this`, captures) OR any non-Var aliased ref
    ///      (`this.field`, `arr[i]`, etc.): no +1 exists yet for
    ///      the caller — synthesise one so `c.inc()`-style chains
    ///      and `obj.field` returns hand the caller a real
    ///      ownership share. Without this the caller-side release
    ///      eventually frees while another binding still points at
    ///      the object.
    ///
    /// Returns true iff a callee-retain WILL be emitted for this
    /// tail; the actual emission must happen AFTER lower_block (so
    /// the ValueId is known) but the lookup runs BEFORE it (so the
    /// body block's let bindings haven't shadowed the outer scope
    /// yet — otherwise a tail Var that names a let-bound Local
    /// would lookup as "not Local" and we'd over-retain transient
    /// values like `make_map()`).
    fn callee_retain_decision(&self, tail_expr: &Expr) -> bool {
        if self.is_fresh_object_expr(tail_expr) {
            return false;
        }
        match &tail_expr.kind {
            ExprKind::Var(name) => match self.env.lookup_binding(*name) {
                // Resolves in the current (outer) scope — param or
                // earlier-block tail. Needs retain.
                Some(_) => true,
                // Doesn't resolve here ⇒ Var must be bound by a
                // `let` inside the body block, which lower_block
                // already retains for the caller.
                None => false,
            },
            _ => true,
        }
    }

    fn emit_callee_retain(&mut self, tail: &Option<(ValueId, MirTy)>) {
        if let Some((v, ty)) = tail.as_ref() {
            if matches!(
                ty,
                MirTy::Object(_)
                    | MirTy::Array { .. }
                    | MirTy::Tuple(_)
                    | MirTy::Map { .. }
                    | MirTy::Optional(_)
                    | MirTy::Fn(_)
                    | MirTy::Str
            ) {
                self.fb.push_inst(Inst::Retain { value: *v });
            }
        }
    }

    fn finalise_return(&mut self, tail: Option<(ValueId, MirTy)>) -> Result<(), LowerError> {
        // Synthesise a placeholder return value when the lowerer is
        // sitting in a dead block (the user already issued `return`
        // earlier on the dominating path) but the fn signature
        // expects a non-unit return.
        let synth_placeholder = |this: &mut Self, ret_ty: &MirTy| -> ValueId {
            let v = this.fb.new_value(ret_ty.clone());
            let c = match ret_ty {
                MirTy::Bool => Inst::Const { dst: v, value: MirConst::Bool(false) },
                MirTy::F32 => Inst::Const { dst: v, value: MirConst::F32(0) },
                MirTy::F64 => Inst::Const { dst: v, value: MirConst::F64(0) },
                _ => Inst::Const { dst: v, value: MirConst::Int(0) },
            };
            this.fb.push_inst(c);
            v
        };
        let value = match (&self.ret_ty, tail) {
            (MirTy::Unit, _) => None,
            // Tail expression has unit type (e.g. `return X` desugars
            // to a unit value in a dead block) — fabricate a real
            // return value so Cranelift's verifier is happy.
            (ret_ty, Some((_, MirTy::Unit))) => Some(synth_placeholder(self, &ret_ty.clone())),
            (ret_ty, Some((v, vty))) => {
                // Auto-coerce when the tail's type is a same-shape
                // integer / float that fits the declared return.
                let ret_ty_clone = ret_ty.clone();
                if vty == ret_ty_clone {
                    Some(v)
                } else {
                    let coerced = self.coerce(v, &vty, &ret_ty_clone, Span::dummy())
                        .unwrap_or(v);
                    Some(coerced)
                }
            }
            (ret_ty, None) => Some(synth_placeholder(self, &ret_ty.clone())),
        };
        self.fb.set_terminator(Terminator::Return { value });
        Ok(())
    }

    fn lower_block(&mut self, blk: &AstBlock) -> Result<Option<(ValueId, MirTy)>, LowerError> {
        self.env.enter_scope();
        // Stop treating let bindings as top-level once we descend
        // into a nested block — block-scoped `let x = ...` should
        // bind a fresh Local instead of overwriting any same-named
        // outer slot. lower_main calls `lower_stmt` directly on
        // its top-level stmts, so this flag flip only affects
        // recursion through `lower_expr(Block)`.
        let saved_main_body = self.is_main_body;
        self.is_main_body = false;
        for stmt in &blk.stmts {
            self.lower_stmt(stmt)?;
        }
        let tail = match &blk.tail {
            Some(e) => Some(self.lower_expr(e)?),
            None => None,
        };
        self.is_main_body = saved_main_body;
        // If the tail aliases a block-local heap binding, retain it
        // so the scope-exit releases below don't drop its rc to 0.
        // Fresh tails (`new T()` / call) are already +1 owners and
        // need no retain. Only retain when the tail expression is a
        // `Var` resolving to a binding in this block's scope —
        // otherwise we'd over-retain transient values that nothing
        // releases.
        let tail_needs_retain = |ty: &MirTy| {
            matches!(
                ty,
                MirTy::Object(_)
                    | MirTy::Fn(_)
                    | MirTy::Array { .. }
                    | MirTy::Optional(_)
                    | MirTy::Tuple(_)
                    | MirTy::Map { .. }
                    | MirTy::Str
                    | MirTy::Enum(_)
            )
        };
        let tail_alias_name = blk.tail.as_ref().and_then(|e| match &e.kind {
            ExprKind::Var(name) => Some(*name),
            _ => None,
        });
        let tail_aliases_local = tail_alias_name.is_some();
        let tail = match tail {
            Some((v, ty)) if tail_needs_retain(&ty) && tail_aliases_local => {
                self.fb.push_inst(Inst::Retain { value: v });
                Some((v, ty))
            }
            other => other,
        };
        // CRepr Locals carry no rc — Retain above is a no-op for
        // them. Transfer ownership of the tail-aliased local to
        // the caller by un-marking it before scope exit, otherwise
        // `release_top_scope_objects` would free the buffer the
        // caller is about to use.
        if let Some(name) = tail_alias_name {
            if let Some(Binding::Local(lid, _)) = self.env.lookup_binding(name) {
                self.crepr_owned_locals.remove(&lid);
            }
        }
        self.release_top_scope_objects();
        self.env.exit_scope();
        Ok(tail)
    }

    fn is_fresh_object_expr(&self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::New { .. }
            | ExprKind::StructLit { .. }
            | ExprKind::Call { .. }
            | ExprKind::MethodCall { .. }
            // SuperCall returns `this` aliased — init's calling
            // convention does NOT add a +1 (see lower_method's
            // is_init terminator special-case which sets the
            // terminator directly to `return this_v` with no
            // retain). Treating super() as fresh would emit a
            // bogus release-on-discard that drops rc below the
            // alloc's +1 and triggers free-during-init.
            // Binary / Unary on heap operands (string +) lowers to a
            // host helper (str_concat etc.) that returns a freshly
            // leak_cstring'd, registry-tracked buffer. Treating them
            // as fresh prevents the let-bind retain from leaking the
            // intermediate. For non-heap operand types, "fresh" is a
            // no-op decision so this is safe to widen unconditionally.
            | ExprKind::Binary { .. }
            | ExprKind::Unary { .. }
            // Aggregate / heap literals — each lowers to a fresh
            // alloc with rc=1 already in place.
            | ExprKind::Array(_)
            | ExprKind::Tuple(_)
            | ExprKind::Some(_)
            | ExprKind::None
            | ExprKind::EnumCtor { .. }
            | ExprKind::FnExpr { .. } => true,
            // Indexing a fresh tuple / array donates ownership of the
            // selected element to the caller — the lowerer retains
            // that element exactly once on the fresh-receiver path.
            ExprKind::Index { obj, .. } => self.is_fresh_object_expr(obj),
            // A block whose tail is itself fresh produces a fresh
            // value (the inner block scope-releases its own locals).
            ExprKind::Block(b) => b
                .tail
                .as_ref()
                .map(|t| self.is_fresh_object_expr(t))
                .unwrap_or(false),
            // `if`/`match` carry the freshness of all branches; treat
            // them conservatively — fresh only if every branch's tail
            // is fresh. Non-fresh would produce an over-retain rather
            // than a use-after-free, so this is the safe direction.
            ExprKind::If { then_branch, else_branch, .. } => {
                let then_fresh = then_branch
                    .tail
                    .as_ref()
                    .map(|t| self.is_fresh_object_expr(t))
                    .unwrap_or(false);
                let else_fresh = else_branch
                    .as_ref()
                    .map(|e| self.is_fresh_object_expr(e))
                    .unwrap_or(false);
                then_fresh && else_fresh
            }
            _ => false,
        }
    }

    fn release_top_scope_objects(&mut self) {
        let scope: Vec<(Symbol, Binding)> = self
            .env
            .scopes
            .last()
            .cloned()
            .unwrap_or_default();
        let needs_release = |ty: &MirTy| {
            matches!(
                ty,
                MirTy::Object(_)
                    | MirTy::Fn(_)
                    | MirTy::Array { .. }
                    | MirTy::Optional(_)
                    | MirTy::Tuple(_)
                    | MirTy::Map { .. }
                    | MirTy::Str
                    | MirTy::Enum(_)
            )
        };
        for (_name, binding) in scope.into_iter().rev() {
            match binding {
                Binding::Local(lid, ty) if needs_release(&ty) => {
                    // For CRepr Locals, only emit Release if this
                    // Local owns the underlying buffer. Borrowed
                    // CRepr values (e.g. nested-field reads) stay
                    // alive with their parent and must NOT be
                    // freed independently.
                    if let MirTy::Object(cid) = &ty {
                        let layout = &self.classes[cid.0 as usize];
                        let is_crepr = matches!(
                            layout.repr,
                            crate::program::ClassRepr::CRepr
                                | crate::program::ClassRepr::CPacked
                                | crate::program::ClassRepr::CUnion
                        );
                        if is_crepr && !self.crepr_owned_locals.contains(&lid) {
                            continue;
                        }
                    }
                    let v = self.fb.new_value(ty.clone());
                    self.fb.push_inst(Inst::UseLocal { dst: v, local: lid });
                    self.fb.push_inst(Inst::Release { value: v });
                }
                Binding::Ssa(v, ty) if needs_release(&ty) => {
                    self.fb.push_inst(Inst::Release { value: v });
                }
                Binding::Cell(cell_v, ty) if needs_release(&ty) => {
                    let zero = self.const_int(MirTy::I64, 0);
                    let v = self.fb.new_value(ty.clone());
                    self.fb.push_inst(Inst::ArrayLoad {
                        dst: v,
                        arr: cell_v,
                        idx: zero,
                    });
                    self.fb.push_inst(Inst::Release { value: v });
                }
                _ => {}
            }
        }
    }

    fn resolve_ty(&self, t: &Type) -> Result<MirTy, LowerError> {
        match t {
            Type::Object(name) => {
                // Find class first.
                if let Some((cid, _)) = self
                    .class_meta
                    .iter()
                    .find(|(cid, _)| self.classes[cid.0 as usize].name == *name)
                {
                    return Ok(MirTy::Object(*cid));
                }
                if let Some(eid) = self.enum_ids.get(name) {
                    return Ok(MirTy::Enum(*eid));
                }
                Err(LowerError::Other(format!("unknown type: {name}")))
            }
            Type::Enum(name) => self.enum_ids.get(name).copied().map(MirTy::Enum).ok_or_else(
                || LowerError::Other(format!("unknown enum {name}")),
            ),
            Type::Array { elem, fixed } => Ok(MirTy::Array {
                elem: Box::new(self.resolve_ty(elem)?),
                len: *fixed,
            }),
            Type::Tuple(elems) => {
                let mut out = Vec::with_capacity(elems.len());
                for e in elems.iter() {
                    out.push(self.resolve_ty(e)?);
                }
                Ok(MirTy::Tuple(out.into_boxed_slice()))
            }
            Type::Optional(inner) => Ok(MirTy::Optional(Box::new(self.resolve_ty(inner)?))),
            Type::Weak(inner) => match &**inner {
                Type::Object(cname) => self
                    .classes
                    .iter()
                    .find(|c| c.name == *cname)
                    .map(|c| MirTy::Weak(c.id))
                    .ok_or_else(|| LowerError::Other(format!("unknown class for weak: {cname}"))),
                _ => Err(LowerError::Other("`.weak` only applies to class types".into())),
            },
            Type::Generic(g) if g.base.as_str() == "Map" && g.args.len() == 2 => Ok(MirTy::Map {
                key: Box::new(self.resolve_ty(&g.args[0])?),
                val: Box::new(self.resolve_ty(&g.args[1])?),
            }),
            Type::Generic(g) => self
                .enum_ids
                .get(&g.base)
                .copied()
                .map(MirTy::Enum)
                .ok_or_else(|| LowerError::Unsupported("user-defined generic types")),
            Type::Fn(ft) => {
                let mut params = Vec::with_capacity(ft.params.len());
                for p in ft.params.iter() {
                    params.push(self.resolve_ty(p)?);
                }
                let ret = self.resolve_ty(&ft.ret)?;
                Ok(MirTy::Fn(Box::new(crate::types::MirFnTy {
                    params: params.into_boxed_slice(),
                    ret,
                })))
            }
            other => ty_to_mir(other),
        }
    }

    fn lower_stmt(&mut self, stmt: &Stmt) -> Result<(), LowerError> {
        match &stmt.kind {
            StmtKind::Let { name, ty, value, .. } => {
                // `let _ = expr` discards the result. Lower the
                // expression for its side effects, then drop a
                // fresh heap result so deinit / registry release
                // fires immediately instead of being deferred to
                // the enclosing scope's exit. A borrowed result
                // (non-fresh) needs no release — the source slot
                // still owns its +1.
                if name.as_str() == "_" {
                    let value_is_fresh = self.is_fresh_object_expr(value);
                    let (v, vty) = self.lower_expr(value)?;
                    if value_is_fresh && vty.is_heap() {
                        self.fb.push_inst(Inst::Release { value: v });
                    }
                    return Ok(());
                }
                // Empty-array literal uses the binding's annotated
                // element type so `let xs: string[] = []` typechecks
                // without a needs-coerce step that doesn't exist.
                let bind_hint = ty.as_ref().and_then(|t| self.resolve_ty(t).ok());
                let value_is_fresh_object = self.is_fresh_object_expr(value);
                // While lowering this let's value, mark `name` as the
                // currently-binding self name so a recursive FnExpr
                // body referencing `name` resolves through the slot
                // at call time instead of snapshotting the (still
                // unwritten) slot at construction.
                let saved_self = self.binding_self_name;
                if self.is_main_body && self.repl_slots.contains_key(name) {
                    self.binding_self_name = Some(*name);
                }
                let (v, mty) = if let (
                    ExprKind::Array(items),
                    Some(MirTy::Array { elem, len }),
                ) = (&value.kind, &bind_hint)
                {
                    if items.is_empty() {
                        let ty_full = MirTy::Array {
                            elem: elem.clone(),
                            len: *len,
                        };
                        let dst = self.fb.new_value(ty_full.clone());
                        self.fb.push_inst(Inst::NewArrayEmpty {
                            dst,
                            elem: (**elem).clone(),
                            fixed_len: *len,
                        });
                        (dst, ty_full)
                    } else {
                        // Hint-directed lowering: build an array whose
                        // element MirTy AND fixed-length match the
                        // binding's hint, so the inline-vs-dynamic
                        // codegen layout is consistent with how
                        // ArrayLoad / ArrayLen later type-dispatch.
                        self.lower_array_literal_with_hint(
                            items,
                            Some((**elem).clone()),
                            *len,
                        )?
                    }
                } else {
                    self.lower_expr(value)?
                };
                let bind_ty = bind_hint.unwrap_or_else(|| mty.clone());
                let bound = if bind_ty != mty {
                    self.coerce(v, &mty, &bind_ty, stmt.span)?
                } else {
                    v
                };
                // For an aliased heap value (anything that isn't a
                // freshly-constructed `new T(...)` / closure expr /
                // literal), bump refcount — the binding shares
                // ownership with the source. All heap kinds (incl.
                // Array, Tuple, Map, Optional, Enum) need this so
                // the slot's scope-exit release has its own +1 to
                // drop; without it a container that releases the
                // element on overwrite (e.g. host_map_set's
                // release_by_kind) would free the buffer the slot
                // still points at.
                if bind_ty.is_heap() && !value_is_fresh_object {
                    self.fb.push_inst(Inst::Retain { value: bound });
                }
                // Slot-backed top-level binding: skip the local
                // entirely so all reads / writes (in `__main` *and*
                // any fn body) funnel through `__repl_load_slot` /
                // `__repl_store_slot`. Without this skip, `__main`
                // would read its own private Local copy and miss
                // mutations that other fns wrote through the slot.
                // `is_main_body` is cleared by `lower_block` on
                // descent so block-scoped `let x = 100` shadows
                // bind a fresh Local instead of overwriting the
                // outer slot.
                let is_slot_global = self.is_main_body
                    && self.repl_slots.contains_key(name);
                if matches!(bind_ty, MirTy::Unit) {
                    // Unit-typed bindings have no clif representation;
                    // keep the SSA path so reads return a synthetic
                    // unit value.
                    self.env.bind(*name, bound, bind_ty.clone());
                } else if is_slot_global {
                    // No local binding — slot lookup handles reads.
                } else {
                    let _ = &self.cellify_set; // legacy field, retained for ABI
                    let lid = self.fb.new_local(bind_ty.clone());
                    self.fb.push_inst(Inst::DefLocal { local: lid, value: bound });
                    self.env.bind_local(*name, lid, bind_ty.clone());
                    // Mark CRepr Locals as "owns the buffer" only
                    // when the source was a fresh `new T()` (or
                    // similar) — that's what makes the buffer
                    // safe to free at scope exit. A `let p =
                    // r.origin` style borrow stays unmarked so
                    // the scope-exit path leaves it alone.
                    if let MirTy::Object(cid) = &bind_ty {
                        let layout = &self.classes[cid.0 as usize];
                        if matches!(
                            layout.repr,
                            crate::program::ClassRepr::CRepr
                                | crate::program::ClassRepr::CPacked
                                | crate::program::ClassRepr::CUnion
                        ) && value_is_fresh_object
                        {
                            self.crepr_owned_locals.insert(lid);
                        }
                    }
                }
                // REPL: top-level let in __main with a registered slot
                // → persist the value to a host-side cell so future
                // chunks can read it via `__repl_load_slot`.
                if self.is_main_body {
                    if let Some((idx, _slot_ty)) = self.repl_slots.get(name).cloned() {
                        let idx_v = self.const_int(MirTy::I64, idx as i64);
                        // Bit-cast the bound value to i64 for storage.
                        // Heap pointer types are already i64; signed
                        // ints widen via sextend; unsigned ints / bool
                        // via zext; floats via bitcast.
                        let v_i64 = self.value_to_i64(bound, &bind_ty)?;
                        // The slot becomes the only owner of the
                        // value (slot-promoted top-level lets get NO
                        // Local binding above, so __main's exit
                        // release sweep doesn't touch the name).
                        // Aliased heap values need a fresh +1 the
                        // slot can own; fresh values already come
                        // with rc=1, so retaining again leaves rc=2
                        // and the exit-time slot release can't drive
                        // the value to drop. See
                        // top_level_let_used_in_fn_deinit_once.il.
                        if matches!(
                            bind_ty,
                            MirTy::Object(_)
                                | MirTy::Array { .. }
                                | MirTy::Tuple(_)
                                | MirTy::Map { .. }
                                | MirTy::Optional(_)
                                | MirTy::Fn(_)
                        ) && !value_is_fresh_object
                        {
                            self.fb.push_inst(Inst::Retain { value: bound });
                        }
                        self.fb.push_inst(Inst::Call {
                            dst: None,
                            callee: FuncRef::Builtin(Symbol::intern("__repl_store_slot")),
                            args: Box::new([idx_v, v_i64]),
                        });
                    }
                }
                self.binding_self_name = saved_self;
                Ok(())
            }
            StmtKind::LetTuple { elems, value } => {
                let (v, vty) = self.lower_expr(value)?;
                let tuple_tys = match &vty {
                    MirTy::Tuple(ts) => ts.clone(),
                    other => {
                        return Err(LowerError::Other(format!(
                            "let-tuple destructure on non-tuple: {other}"
                        )))
                    }
                };
                if elems.len() != tuple_tys.len() {
                    return Err(LowerError::Other(format!(
                        "tuple destructure arity {} vs tuple {}",
                        elems.len(),
                        tuple_tys.len()
                    )));
                }
                for (i, name_opt) in elems.iter().enumerate() {
                    let Some(name) = name_opt else { continue };
                    let ty = tuple_tys[i].clone();
                    let dst = self.fb.new_value(ty.clone());
                    self.fb.push_inst(Inst::TupleExtract {
                        dst,
                        tup: v,
                        idx: i as u32,
                    });
                    self.env.bind(*name, dst, ty);
                }
                Ok(())
            }
            StmtKind::LetStruct { class, fields, value } => {
                let (v, vty) = self.lower_expr(value)?;
                let class_id = match &vty {
                    MirTy::Object(c) => *c,
                    other => {
                        return Err(LowerError::Other(format!(
                            "let-struct destructure on non-object: {other}"
                        )))
                    }
                };
                let layout = &self.classes[class_id.0 as usize];
                if layout.name != *class {
                    return Err(LowerError::Other(format!(
                        "destructure class mismatch: declared {class}, value class {}",
                        layout.name
                    )));
                }
                let meta = self.class_meta.get(&class_id).expect("class meta");
                for fname in fields.iter() {
                    let &fid = meta.field_ix.get(fname).ok_or_else(|| {
                        LowerError::Other(format!("no field {fname} on {class}"))
                    })?;
                    let fty = meta.field_ty.get(&fid).cloned().unwrap();
                    let dst = self.fb.new_value(fty.clone());
                    self.fb.push_inst(Inst::LoadField { dst, obj: v, field: fid });
                    self.env.bind(*fname, dst, fty);
                }
                Ok(())
            }
            StmtKind::Expr(e) => {
                let (v, ty) = self.lower_expr(e)?;
                // If the expression-statement produced a fresh,
                // unowned heap value, release it so its refcount
                // drops to 0 (firing class deinit / freeing the
                // backing buffer). Without this, a discarded
                // method call result like `xs.map(fn(...){...})`
                // (its fresh array, plus the fresh closure arg)
                // leaks every iteration of a long-running loop.
                let is_heap = matches!(
                    ty,
                    MirTy::Object(_)
                        | MirTy::Array { .. }
                        | MirTy::Tuple(_)
                        | MirTy::Map { .. }
                        | MirTy::Optional(_)
                        | MirTy::Fn(_)
                );
                if is_heap && self.is_fresh_object_expr(e) {
                    self.fb.push_inst(Inst::Release { value: v });
                }
                Ok(())
            }
        }
    }

    fn lower_expr(&mut self, expr: &Expr) -> Result<(ValueId, MirTy), LowerError> {
        match &expr.kind {
            ExprKind::Int(n) => {
                let v = self.fb.new_value(MirTy::I64);
                self.fb.push_inst(Inst::Const { dst: v, value: MirConst::Int(*n) });
                Ok((v, MirTy::I64))
            }
            ExprKind::Bool(b) => {
                let v = self.fb.new_value(MirTy::Bool);
                self.fb.push_inst(Inst::Const { dst: v, value: MirConst::Bool(*b) });
                Ok((v, MirTy::Bool))
            }
            ExprKind::Float(f) => {
                let v = self.fb.new_value(MirTy::F64);
                self.fb.push_inst(Inst::Const { dst: v, value: MirConst::F64(f.to_bits()) });
                Ok((v, MirTy::F64))
            }
            ExprKind::Str(s) => {
                let v = self.fb.new_value(MirTy::Str);
                self.fb.push_inst(Inst::Const {
                    dst: v,
                    value: MirConst::Str(Symbol::intern(s)),
                });
                Ok((v, MirTy::Str))
            }
            ExprKind::Var(name) => {
                if let Some(found) = self.lookup_var(*name) {
                    return Ok(found);
                }
                // Closure capture takes precedence over a same-named
                // module slot — a closure that captured `factor`
                // when it was 10 must keep seeing 10 even if the
                // outer code reassigned the slot to 999.
                // Slot reads borrow ownership from the slot itself
                // (the host store keeps a stable refcount on the
                // slot's heap value). We deliberately do NOT retain
                // here — that's the same contract `lookup_var`
                // honours for Local reads, and it's what avoids the
                // per-loop-iteration leak in long-running programs
                // (e.g. `examples/sdl_breakout`'s game loop, where
                // every frame reads slot-promoted globals dozens of
                // times). Downstream `let`-binding / fn-arg /
                // closure-capture sites bump the refcount when they
                // need persistent ownership.
                if let Some(caps) = self.captures_in_scope {
                    if caps.contains_key(name) {
                        // Fall through to the existing capture
                        // handler below.
                    } else if let Some((idx, slot_ty)) = self.repl_slots.get(name).cloned() {
                        let idx_v = self.const_int(MirTy::I64, idx as i64);
                        let raw = self.fb.new_value(MirTy::I64);
                        self.fb.push_inst(Inst::Call {
                            dst: Some(raw),
                            callee: FuncRef::Builtin(Symbol::intern("__repl_load_slot")),
                            args: Box::new([idx_v]),
                        });
                        let v = self.i64_to_slot_value(raw, &slot_ty)?;
                        return Ok((v, slot_ty));
                    }
                } else if let Some((idx, slot_ty)) = self.repl_slots.get(name).cloned() {
                    // Non-closure context (regular fn body or
                    // `__main` itself): always go through the slot.
                    let idx_v = self.const_int(MirTy::I64, idx as i64);
                    let raw = self.fb.new_value(MirTy::I64);
                    self.fb.push_inst(Inst::Call {
                        dst: Some(raw),
                        callee: FuncRef::Builtin(Symbol::intern("__repl_load_slot")),
                        args: Box::new([idx_v]),
                    });
                    let v = self.i64_to_slot_value(raw, &slot_ty)?;
                    return Ok((v, slot_ty));
                }
                // Closure capture (only when lowering a closure body).
                if let Some(caps) = self.captures_in_scope {
                    if let Some((idx, cty)) = caps.get(name).cloned() {
                        let is_cell = self
                            .cell_captures
                            .map(|s| s.contains(name))
                            .unwrap_or(false);
                        if is_cell {
                            // Capture slot holds a cell pointer (i64
                            // 1-elem array). Load the pointer, then
                            // dereference to get the inner value.
                            let cell_v = self.fb.new_value(MirTy::I64);
                            self.fb.push_inst(Inst::LoadCapture { dst: cell_v, idx });
                            let zero = self.const_int(MirTy::I64, 0);
                            let v = self.fb.new_value(cty.clone());
                            self.fb.push_inst(Inst::ArrayLoad {
                                dst: v,
                                arr: cell_v,
                                idx: zero,
                            });
                            return Ok((v, cty));
                        }
                        let v = self.fb.new_value(cty.clone());
                        self.fb.push_inst(Inst::LoadCapture { dst: v, idx });
                        return Ok((v, cty));
                    }
                }
                // Top-level fn used as a value: produce a trampoline
                // closure with no captures.
                if let Some(&fid) = self.fn_ids.get(name) {
                    let sig = self.fn_sigs.get(name).cloned().unwrap();
                    let fn_ty = MirTy::Fn(Box::new(crate::types::MirFnTy {
                        params: sig.params.clone().into_boxed_slice(),
                        ret: sig.ret,
                    }));
                    let dst = self.fb.new_value(fn_ty.clone());
                    self.fb.push_inst(Inst::MakeClosure {
                        dst,
                        func: fid,
                        captures: Box::new([]),
                    });
                    return Ok((dst, fn_ty));
                }
                // Implicit `this.field` / `this.method()` — only the
                // field case applies here (method ref without call is
                // not supported in M1).
                if let Some(cid) = self.this_class {
                    let meta = self.class_meta.get(&cid).expect("class meta");
                    if let Some(&fid) = meta.field_ix.get(name) {
                        let (this_v, _) = self.lookup_var(Symbol::intern("this")).unwrap();
                        let fty = meta.field_ty.get(&fid).cloned().unwrap();
                        let v = self.fb.new_value(fty.clone());
                        self.fb.push_inst(Inst::LoadField { dst: v, obj: this_v, field: fid });
                        return Ok((v, fty));
                    }
                }
                Err(LowerError::Other(format!("unbound variable: {name}")))
            }
            ExprKind::This => {
                let this_sym = Symbol::intern("this");
                if let Some(found) = self.lookup_var(this_sym) {
                    return Ok(found);
                }
                if let Some(caps) = self.captures_in_scope {
                    if let Some((idx, cty)) = caps.get(&this_sym).cloned() {
                        let v = self.fb.new_value(cty.clone());
                        self.fb.push_inst(Inst::LoadCapture { dst: v, idx });
                        return Ok((v, cty));
                    }
                }
                Err(LowerError::Other("`this` outside method body".into()))
            }
            ExprKind::SuperCall { method, args } => self.lower_super_call(*method, args, expr.span),
            ExprKind::New { class, type_args, args, init_method } => {
                // Built-in `Map<K, V>` — `new Map<K,V>()` constructs
                // an empty map.
                if class.as_str() == "Map" && type_args.len() == 2 && args.is_empty() {
                    let key = self.resolve_ty(&type_args[0])?;
                    let val = self.resolve_ty(&type_args[1])?;
                    let ty = MirTy::Map {
                        key: Box::new(key.clone()),
                        val: Box::new(val.clone()),
                    };
                    let dst = self.fb.new_value(ty.clone());
                    self.fb.push_inst(Inst::NewMap {
                        dst,
                        key,
                        val,
                        entries: Box::new([]),
                    });
                    return Ok((dst, ty));
                }
                if !type_args.is_empty() {
                    return Err(LowerError::Unsupported("generic class instantiation"));
                }
                self.lower_new(*class, args, *init_method)
            }
            ExprKind::AssignField { obj, field, value, is_init } => {
                // `ClassName.field = v` on a static slot.
                if let ExprKind::Var(maybe_class) = &obj.kind {
                    if self.lookup_var(*maybe_class).is_none() {
                        if let Some(cid) = self.class_meta.iter().find_map(|(cid, _)| {
                            if self.classes[cid.0 as usize].name == *maybe_class {
                                Some(*cid)
                            } else {
                                None
                            }
                        }) {
                            let meta = self.class_meta.get(&cid).unwrap();
                            if let Some(&slot) = meta.static_slots.get(field) {
                                let s = self.statics_by_id(slot);
                                if s.is_const && !*is_init {
                                    return Err(LowerError::Other(format!(
                                        "cannot assign to const {field}"
                                    )));
                                }
                                let (vv, vty) = self.lower_expr(value)?;
                                let coerced = if vty == s.ty {
                                    vv
                                } else {
                                    self.coerce(vv, &vty, &s.ty, expr.span)?
                                };
                                self.fb.push_inst(Inst::StoreStatic { slot, value: coerced });
                                return Ok((self.const_unit(), MirTy::Unit));
                            }
                        }
                    }
                }
                let (ov, oty) = self.lower_expr(obj)?;
                let class_id = match &oty {
                    MirTy::Object(c) => *c,
                    other => {
                        return Err(LowerError::Other(format!(
                            "field assignment on non-object: {other}"
                        )))
                    }
                };
                // Property setter on instance.
                let meta = self.class_meta.get(&class_id).expect("class meta");
                if let Some((fid, prop_ty)) = meta.property_setter.get(field).cloned() {
                    let (vv, vty) = self.lower_expr(value)?;
                    let coerced = if vty == prop_ty {
                        vv
                    } else {
                        self.coerce(vv, &vty, &prop_ty, value.span)?
                    };
                    self.fb.push_inst(Inst::Call {
                        dst: None,
                        callee: FuncRef::Local(fid),
                        args: Box::new([ov, coerced]),
                    });
                    return Ok((self.const_unit(), MirTy::Unit));
                }
                let fid = *meta
                    .field_ix
                    .get(field)
                    .ok_or_else(|| LowerError::Other(format!("no field {field}")))?;
                let fty = meta.field_ty.get(&fid).cloned().unwrap_or(MirTy::I64);
                let value_is_fresh = self.is_fresh_object_expr(value);
                let (vv, _) = self.lower_expr(value)?;
                // ARC for any heap-typed field: retain the incoming
                // value (unless it was a fresh allocation that
                // already owned its +1) and release the previous
                // occupant. Without this, `this.balls = newArr` etc.
                // leaks the prior array's refcount on every frame
                // of `examples/sdl_breakout`'s game loop.
                let is_heap = matches!(
                    fty,
                    MirTy::Object(_)
                        | MirTy::Array { .. }
                        | MirTy::Tuple(_)
                        | MirTy::Map { .. }
                        | MirTy::Optional(_)
                        | MirTy::Fn(_)
                );
                if is_heap {
                    if !value_is_fresh {
                        self.fb.push_inst(Inst::Retain { value: vv });
                    }
                    let old = self.fb.new_value(fty.clone());
                    self.fb.push_inst(Inst::LoadField {
                        dst: old,
                        obj: ov,
                        field: fid,
                    });
                    self.fb.push_inst(Inst::Release { value: old });
                }
                self.fb.push_inst(Inst::StoreField { obj: ov, field: fid, value: vv });
                Ok((self.const_unit(), MirTy::Unit))
            }
            ExprKind::Unary { op, expr } => self.lower_unary(*op, expr, expr.span),
            ExprKind::Binary { op, lhs, rhs } => self.lower_binary(*op, lhs, rhs, expr.span),
            ExprKind::Logical { op, lhs, rhs } => self.lower_logical(*op, lhs, rhs),
            ExprKind::Block(b) => {
                let tail = self.lower_block(b)?;
                Ok(tail.unwrap_or_else(|| (self.const_unit(), MirTy::Unit)))
            }
            ExprKind::If { cond, then_branch, else_branch } => {
                self.lower_if(cond, then_branch, else_branch.as_deref())
            }
            ExprKind::While { cond, body } => self.lower_while(cond, body),
            ExprKind::Loop { body } => self.lower_loop(body),
            ExprKind::Break(v) => self.lower_break(v.as_deref()),
            ExprKind::Continue => self.lower_continue(),
            ExprKind::Return(v) => self.lower_return(v.as_deref()),
            ExprKind::Assign { target, value } => {
                let value_is_fresh = self.is_fresh_object_expr(value);
                // Snapshot the old value if the target binds an Object
                // — we need to Release it after the new value lands.
                let old_obj = self.env.lookup_binding(*target).and_then(|b| match b {
                    Binding::Local(lid, ty) if matches!(ty, MirTy::Object(_)) => {
                        let v = self.fb.new_value(ty.clone());
                        self.fb.push_inst(Inst::UseLocal { dst: v, local: lid });
                        Some(v)
                    }
                    Binding::Cell(cell_v, ty) if matches!(ty, MirTy::Object(_)) => {
                        let zero = self.const_int(MirTy::I64, 0);
                        let v = self.fb.new_value(ty.clone());
                        self.fb.push_inst(Inst::ArrayLoad {
                            dst: v,
                            arr: cell_v,
                            idx: zero,
                        });
                        Some(v)
                    }
                    _ => None,
                });
                let (v, vty) = self.lower_expr(value)?;
                if self.assign_var(*target, v, vty.clone()) {
                    if matches!(vty, MirTy::Object(_)) {
                        if !value_is_fresh {
                            self.fb.push_inst(Inst::Retain { value: v });
                        }
                        if let Some(old) = old_obj {
                            self.fb.push_inst(Inst::Release { value: old });
                        }
                    }
                    return Ok((self.const_unit(), MirTy::Unit));
                }
                // Closure capture assign: cell capture stores via
                // the loaded cell pointer.
                if let Some(caps) = self.captures_in_scope {
                    if let Some((idx, cty)) = caps.get(target).cloned() {
                        let is_cell = self
                            .cell_captures
                            .map(|s| s.contains(target))
                            .unwrap_or(false);
                        if is_cell {
                            let cell_v = self.fb.new_value(MirTy::I64);
                            self.fb.push_inst(Inst::LoadCapture { dst: cell_v, idx });
                            let zero = self.const_int(MirTy::I64, 0);
                            // Heap-typed cell: the cell owns the slot's
                            // share, so swap rc accounts before storing
                            // — release the previous occupant, retain
                            // the incoming. Without this the prior
                            // value's rc leaks (or, if the cell is
                            // re-overwritten later, the surviving
                            // alias double-frees). ASan caught the UAF
                            // in `host_retain_object` on the
                            // closure_swap_heap_capture fixture.
                            let heap_slot = matches!(
                                cty,
                                MirTy::Object(_)
                                    | MirTy::Fn(_)
                                    | MirTy::Array { .. }
                                    | MirTy::Optional(_)
                                    | MirTy::Tuple(_)
                                    | MirTy::Map { .. }
                                    | MirTy::Str
                                    | MirTy::Enum(_)
                            );
                            if heap_slot {
                                let old = self.fb.new_value(cty.clone());
                                self.fb.push_inst(Inst::ArrayLoad {
                                    dst: old,
                                    arr: cell_v,
                                    idx: zero,
                                });
                                self.fb.push_inst(Inst::Release { value: old });
                                self.fb.push_inst(Inst::Retain { value: v });
                            }
                            self.fb.push_inst(Inst::ArrayStore {
                                arr: cell_v,
                                idx: zero,
                                value: v,
                            });
                            return Ok((self.const_unit(), MirTy::Unit));
                        }
                    }
                }
                // REPL / module-global slot assign: when a fn body
                // mutates a top-level let from a `use`d module
                // (e.g. `state = state ^ ...` inside `rng.randNext`,
                // where the loader rewrote `state` to `rng.state`
                // and that name isn't in any local scope), route the
                // write through `__repl_store_slot`.
                if let Some((idx, slot_ty)) = self.repl_slots.get(target).cloned() {
                    let coerced = if vty == slot_ty {
                        v
                    } else {
                        self.coerce(v, &vty, &slot_ty, expr.span)?
                    };
                    let is_heap = matches!(
                        slot_ty,
                        MirTy::Object(_)
                            | MirTy::Array { .. }
                            | MirTy::Tuple(_)
                            | MirTy::Map { .. }
                            | MirTy::Optional(_)
                            | MirTy::Fn(_)
                    );
                    // Snapshot the prior slot value so the old heap
                    // owner gets released after the new value lands.
                    let old_v = if is_heap {
                        let idx_v = self.const_int(MirTy::I64, idx as i64);
                        let raw = self.fb.new_value(MirTy::I64);
                        self.fb.push_inst(Inst::Call {
                            dst: Some(raw),
                            callee: FuncRef::Builtin(Symbol::intern("__repl_load_slot")),
                            args: Box::new([idx_v]),
                        });
                        Some(self.i64_to_slot_value(raw, &slot_ty)?)
                    } else {
                        None
                    };
                    if is_heap && !value_is_fresh {
                        self.fb.push_inst(Inst::Retain { value: coerced });
                    }
                    let v_i64 = self.value_to_i64(coerced, &slot_ty)?;
                    let idx_v = self.const_int(MirTy::I64, idx as i64);
                    self.fb.push_inst(Inst::Call {
                        dst: None,
                        callee: FuncRef::Builtin(Symbol::intern("__repl_store_slot")),
                        args: Box::new([idx_v, v_i64]),
                    });
                    if let Some(old) = old_v {
                        self.fb.push_inst(Inst::Release { value: old });
                    }
                    return Ok((self.const_unit(), MirTy::Unit));
                }
                // Try implicit `this.<target>` field assignment.
                if let Some(cid) = self.this_class {
                    let meta = self.class_meta.get(&cid).expect("class meta");
                    if let Some(&fid) = meta.field_ix.get(target) {
                        let (this_v, _) = self.lookup_var(Symbol::intern("this")).unwrap();
                        // Heap field write: take ownership of `value`
                        // (retain if aliased) and release whatever was
                        // there before (if any). Covers every heap
                        // type — Object / Array / Tuple / Map /
                        // Optional / Fn — so re-assigning a field
                        // doesn't leak the prior occupant. Crucial
                        // for game-loop code that swaps arrays /
                        // optionals on every frame.
                        let value_is_fresh = self.is_fresh_object_expr(value);
                        let is_heap = matches!(
                            vty,
                            MirTy::Object(_)
                                | MirTy::Array { .. }
                                | MirTy::Tuple(_)
                                | MirTy::Map { .. }
                                | MirTy::Optional(_)
                                | MirTy::Fn(_)
                        );
                        if is_heap {
                            if !value_is_fresh {
                                self.fb.push_inst(Inst::Retain { value: v });
                            }
                            // Read old value and release it. Skips on
                            // null (init path).
                            let old = self.fb.new_value(vty.clone());
                            self.fb.push_inst(Inst::LoadField {
                                dst: old,
                                obj: this_v,
                                field: fid,
                            });
                            self.fb.push_inst(Inst::Release { value: old });
                        }
                        self.fb
                            .push_inst(Inst::StoreField { obj: this_v, field: fid, value: v });
                        return Ok((self.const_unit(), MirTy::Unit));
                    }
                }
                Err(LowerError::Other(format!(
                    "assignment to undefined variable: {target}"
                )))
            }
            ExprKind::Call { callee, args } => self.lower_call(*callee, args),
            ExprKind::Cast { expr: inner, ty } => {
                let (v, src_ty) = self.lower_expr(inner)?;
                let dst_ty = self.resolve_ty(ty)?;
                let out = self.coerce(v, &src_ty, &dst_ty, expr.span)?;
                Ok((out, dst_ty))
            }
            ExprKind::TypeTest { expr: inner, ty } => {
                let (v, _) = self.lower_expr(inner)?;
                let dst_ty = self.resolve_ty(ty)?;
                let class = match &dst_ty {
                    MirTy::Object(c) => *c,
                    other => {
                        return Err(LowerError::Other(format!(
                            "`is` requires a class type, got {other}"
                        )))
                    }
                };
                let dst = self.fb.new_value(MirTy::Bool);
                self.fb.push_inst(Inst::IsInstance { dst, value: v, class });
                Ok((dst, MirTy::Bool))
            }
            ExprKind::TypeDowncast { expr: inner, ty } => {
                let (v, _) = self.lower_expr(inner)?;
                let dst_ty = self.resolve_ty(ty)?;
                let class = match &dst_ty {
                    MirTy::Object(c) => *c,
                    other => {
                        return Err(LowerError::Other(format!(
                            "`as?` requires a class type, got {other}"
                        )))
                    }
                };
                let opt_ty = MirTy::Optional(Box::new(MirTy::Object(class)));
                let dst = self.fb.new_value(opt_ty.clone());
                self.fb.push_inst(Inst::DowncastOrNone { dst, value: v, class });
                Ok((dst, opt_ty))
            }
            ExprKind::Array(items) => self.lower_array_literal(items),
            ExprKind::Tuple(items) => self.lower_tuple_literal(items),
            ExprKind::None => {
                // `none` has no concrete T?; the binding context (let
                // annotation, function return type) decides. Until we
                // thread that through, materialise as `Optional<Unit>`.
                // Callers usually overwrite via coerce or fix the type
                // from the let annotation.
                let inner = MirTy::Unit;
                let ty = MirTy::Optional(Box::new(inner));
                let v = self.fb.new_value(ty.clone());
                self.fb.push_inst(Inst::Const { dst: v, value: MirConst::None });
                Ok((v, ty))
            }
            ExprKind::Some(inner) => {
                let value_is_fresh = self.is_fresh_object_expr(inner);
                let (iv, ity) = self.lower_expr(inner)?;
                // `some(arr)` where `arr` is an aliased Var that the
                // surrounding scope is about to release — bump the
                // inner's rc so the Optional doesn't dangle once the
                // source binding's scope-exit Release fires. With
                // host_release_array now actually freeing memory at
                // rc==0, omitting this retain caused use-after-free
                // in some_aliased_inner.il.
                let needs_retain = !value_is_fresh
                    && matches!(
                        ity,
                        MirTy::Object(_)
                            | MirTy::Array { .. }
                            | MirTy::Tuple(_)
                            | MirTy::Map { .. }
                            | MirTy::Optional(_)
                            | MirTy::Fn(_)
                            | MirTy::Str
                    );
                if needs_retain {
                    self.fb.push_inst(Inst::Retain { value: iv });
                }
                let ty = MirTy::Optional(Box::new(ity.clone()));
                let v = self.fb.new_value(ty.clone());
                self.fb.push_inst(Inst::NewOptional { dst: v, value: iv });
                Ok((v, ty))
            }
            ExprKind::Index { obj, index } => self.lower_index(obj, index),
            ExprKind::AssignIndex { obj, index, value } => {
                let value_is_fresh = self.is_fresh_object_expr(value);
                let (av, aty) = self.lower_expr(obj)?;
                let (iv, _) = self.lower_expr(index)?;
                let (vv, vty) = self.lower_expr(value)?;
                match aty {
                    MirTy::Array { ref elem, .. } => {
                        let elem_ty = (**elem).clone();
                        // Same shape as `StoreField` for heap elements:
                        // retain the incoming value (unless it owns its
                        // +1) and release whatever currently sits in the
                        // slot, since `__release_array`'s cascade
                        // already accounts for every stored element on
                        // drop. Without this, `arr[i] = borrowed` would
                        // share rc with the source slot (UAF) and
                        // `arr[i] = fresh` would leak the old occupant.
                        let elem_is_heap = matches!(
                            elem_ty,
                            MirTy::Object(_)
                                | MirTy::Array { .. }
                                | MirTy::Tuple(_)
                                | MirTy::Map { .. }
                                | MirTy::Optional(_)
                                | MirTy::Fn(_)
                                | MirTy::Str
                                | MirTy::Enum(_)
                        );
                        if elem_is_heap {
                            if !value_is_fresh {
                                self.fb.push_inst(Inst::Retain { value: vv });
                            }
                            let old = self.fb.new_value(elem_ty.clone());
                            self.fb.push_inst(Inst::ArrayLoad {
                                dst: old,
                                arr: av,
                                idx: iv,
                            });
                            self.fb.push_inst(Inst::Release { value: old });
                        }
                        self.fb.push_inst(Inst::ArrayStore { arr: av, idx: iv, value: vv });
                    }
                    MirTy::Map { .. } => {
                        self.fb.push_inst(Inst::MapSet { map: av, key: iv, value: vv });
                        // Map takes its own +1 share via host_map_set's
                        // retain_by_kind. For a fresh value the caller
                        // also has a transient +1 from the source
                        // expression — release it here so the only
                        // remaining share is the map's. Borrowed values
                        // (use_local etc.) leave their slot's share to
                        // be dropped by scope-exit release as usual.
                        if value_is_fresh && vty.is_heap() {
                            self.fb.push_inst(Inst::Release { value: vv });
                        }
                    }
                    other => {
                        return Err(LowerError::Other(format!(
                            "index assignment on non-array/map: {other}"
                        )))
                    }
                }
                Ok((self.const_unit(), MirTy::Unit))
            }
            ExprKind::Field { obj, name } => self.lower_field(obj, *name, expr.span),
            ExprKind::ForIn { var, iter, body } => self.lower_for_in(*var, iter, body),
            ExprKind::IfLet { name, expr: scrut, then_branch, else_branch } => {
                self.lower_if_let(*name, scrut, then_branch, else_branch.as_deref())
            }
            ExprKind::Range { .. } => Err(LowerError::Other(
                "range only valid as a for-in iter (rejected by the type checker elsewhere)".into(),
            )),
            ExprKind::MethodCall { obj, method, args } => {
                self.lower_method_call(obj, *method, args, expr.span)
            }
            ExprKind::EnumCtor { enum_name, variant, args } => {
                self.lower_enum_ctor(*enum_name, *variant, args)
            }
            ExprKind::FnExpr { params, ret, body } => {
                self.lower_fn_expr(params, ret.as_ref(), body, expr.span)
            }
            ExprKind::Closure { .. } => Err(LowerError::Other(
                "ExprKind::Closure encountered (legacy hoist form, unused)".into(),
            )),
            ExprKind::Match { scrutinee, arms } => self.lower_match(scrutinee, arms),
            ExprKind::MapLit(entries) => self.lower_map_literal(entries),
            ExprKind::StructLit { class, fields } => {
                // Aggregate literal for an @extern(C) struct. Desugars
                // to `new C()` (zero-init) + field stores.
                let class_id = self
                    .class_meta
                    .iter()
                    .find_map(|(cid, _)| {
                        if self.classes[cid.0 as usize].name == *class {
                            Some(*cid)
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| LowerError::Other(format!("unknown class {class}")))?;
                let dst = self.fb.new_value(MirTy::Object(class_id));
                self.fb.push_inst(Inst::NewObject {
                    dst,
                    class: class_id,
                    init_args: Box::new([]),
                    init: FuncId(u32::MAX),
                });
                for (fname, fval) in fields.iter() {
                    let meta = self.class_meta.get(&class_id).unwrap();
                    let fid = *meta.field_ix.get(fname).ok_or_else(|| {
                        LowerError::Other(format!("no field {fname} on {class}"))
                    })?;
                    let fty = meta.field_ty.get(&fid).cloned().unwrap();
                    let (vv, vty) = self.lower_expr(fval)?;
                    let coerced = if vty == fty {
                        vv
                    } else {
                        self.coerce(vv, &vty, &fty, fval.span)?
                    };
                    self.fb.push_inst(Inst::StoreField { obj: dst, field: fid, value: coerced });
                }
                Ok((dst, MirTy::Object(class_id)))
            }
            // M1 is feature-complete — every variant of `ExprKind`
            // is handled above. Removed the legacy catch-all because
            // the compiler now flags it as `unreachable_pattern`.
        }
    }

}
