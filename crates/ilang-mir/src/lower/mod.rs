//! AST → MIR lowering.
//!
//! Driven by `lower_program`. Currently covers a working subset of
//! the language; remaining node kinds are listed as `Unsupported`
//! errors so the integration tests fail loudly until we expand
//! coverage. The aim is to grow this file feature-by-feature in the
//! same order as `docs/syntax.md`.

use std::collections::HashMap;

use ilang_ast::{
    self as ast, Block as AstBlock, Expr, ExprKind, Item, Program as AstProgram, Span, Symbol,
    Type,
};

mod call_fn;
mod calls;
mod coerce;
mod collect;
mod control;
mod decl;
mod env;
mod expr;
mod fn_expr;
mod iter_ctor;
mod literals;
mod match_;
mod ops;
mod stmt;
mod utils;

use env::{Binding, Env, LoopFrame};
pub use utils::ty_to_mir;

use crate::builder::FunctionBuilder;
use crate::inst::{FuncId, Inst, MirConst, Terminator, ValueId};
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
    // `Result<T, E>` is no longer pre-registered as a built-in
    // enum. It is monomorphized per call site like any other
    // generic enum (the `monomorphize_enums` pass synthesizes an
    // `Item::Enum` named e.g. `Result<i64, string>`, which the
    // ordinary `register_enum` path picks up below). The previous
    // built-in registration kept all Result payload cells as i64
    // and ended up coexisting with the synthesized per-args enum
    // under two distinct `MirTy::Enum` ids — leaking out as
    // "no coercion from enum#1 to enum#0" or, in patterns,
    // `err(e: string)` resolving to `e: i64`.

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
    // 1a'. Interfaces share the `class_ids` namespace so type-name
    //      resolution (`Type::Object("Drawable")`) finds them. They
    //      get a `ClassLayout` shell with no fields / methods and
    //      participate only as a type tag — instances are always
    //      values of an implementing class.
    // Interface method slots use a separate ID range, well above any
    // normal class vtable slot, so the existing
    // `__virt_dispatch(class_id, slot)` machinery can be reused
    // without colliding with class-method slot ids.
    const IFACE_SLOT_BASE: u32 = 1 << 20;
    let mut next_iface_slot: u32 = IFACE_SLOT_BASE;
    for item in &prog.items {
        if let Item::Interface(i) = item {
            if !lower.class_ids.contains_key(&i.name) {
                let id = crate::types::ClassId(lower.classes.len() as u32);
                lower.class_ids.insert(i.name, id);
                lower.classes.push(crate::program::ClassLayout {
                    id,
                    name: i.name,
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
                lower.interface_ids.insert(i.name, id);
            }
            // Allocate one global slot per interface-method-name —
            // class implementations register their fn at this slot.
            let mut names = Vec::with_capacity(i.methods.len());
            for m in i.methods.iter() {
                lower
                    .iface_method_slots
                    .insert((i.name, m.name), next_iface_slot);
                next_iface_slot += 1;
                names.push(m.name);
            }
            lower.iface_methods_by_name.insert(i.name, names);
        }
    }

    // 1b. Pre-pass: register every enum NAME (allocate id only) so
    //     a forward-referencing enum can resolve another enum's
    //     name in its variant payload. Mirrors the class pre-pass.
    //     Without this, e.g. v2's `__<fn>_State` (generated by the
    //     async desugar) whose payload references a monomorphized
    //     `Box<string>` (appended at the END of items by
    //     monomorphize_enums) fails registration because Box<string>
    //     isn't in `enum_ids` yet when __<fn>_State's variants are
    //     resolved.
    for item in &prog.items {
        if let Item::Enum(ed) = item {
            if ed.type_params.is_empty() && !lower.enum_ids.contains_key(&ed.name) {
                let id = crate::types::EnumId(lower.enums.len() as u32);
                lower.enum_ids.insert(ed.name, id);
                lower.enums.push(crate::program::EnumLayout {
                    id,
                    name: ed.name,
                    repr: MirTy::I64,
                    variants: Vec::new(),
                    is_flags: ed.flags,
                });
                lower.enum_meta.insert(id, EnumMeta::default());
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

    // Resolve interface method signatures now that class / enum
    // types they reference are registered.
    for item in &prog.items {
        if let Item::Interface(i) = item {
            for m in i.methods.iter() {
                let mut params: Vec<MirTy> = Vec::new();
                for p in m.params.iter() {
                    params.push(lower.resolve_ty(&p.ty)?);
                }
                let ret = match &m.ret {
                    Some(t) => lower.resolve_ty(t)?,
                    None => MirTy::Unit,
                };
                lower
                    .iface_method_sigs
                    .insert((i.name, m.name), FnSig { params, ret });
            }
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
            Item::Interface(_) => {}
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
    /// Subset of `class_ids` that names interfaces. Lets the lowerer
    /// distinguish "real class" from "interface marker" when emitting
    /// method-call dispatch.
    interface_ids: HashMap<Symbol, crate::types::ClassId>,
    /// Per-(interface, method-name) global slot id, allocated in
    /// declaration order. Method calls on interface-typed receivers
    /// dispatch through `__virt_dispatch(receiver_class_id, slot)`,
    /// where `slot` is shared between the interface call site and
    /// every implementing class's vtable entry.
    iface_method_slots: HashMap<(Symbol, Symbol), u32>,
    /// Captures the per-interface method list so call sites can look
    /// up the slot for a method name without re-scanning the AST.
    iface_methods_by_name: HashMap<Symbol, Vec<Symbol>>,
    /// Method signature per (interface, method) — call sites need
    /// `params` for arg coercion and `ret` for the dst value type.
    iface_method_sigs: HashMap<(Symbol, Symbol), FnSig>,
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

    fn new() -> Self {
        Self {
            funcs: Vec::new(),
            fn_ids: HashMap::new(),
            fn_sigs: HashMap::new(),
            classes: Vec::new(),
            class_ids: HashMap::new(),
            interface_ids: HashMap::new(),
            iface_method_slots: HashMap::new(),
            iface_methods_by_name: HashMap::new(),
            iface_method_sigs: HashMap::new(),
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
                // Built-in `Promise<T>`.
                if g.base.as_str() == "Promise" && g.args.len() == 1 {
                    return Ok(MirTy::Promise(Box::new(self.resolve_ty(&g.args[0])?)));
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
    interface_ids: &'a HashMap<Symbol, crate::types::ClassId>,
    iface_method_slots: &'a HashMap<(Symbol, Symbol), u32>,
    iface_method_sigs: &'a HashMap<(Symbol, Symbol), FnSig>,
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
            | MirTy::Promise(_)
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
            | MirTy::Promise(_)
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
            // `Index` / `Field` tails are borrow expressions that
            // `lower_block` now retains BEFORE its scope-exit
            // releases — emitting another retain here would
            // double-count.
            ExprKind::Index { .. } | ExprKind::Field { .. } => false,
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
        // Heap-typed tails that **borrow** into a still-live owner
        // (e.g. `arr[i]` reads from `arr`'s element area;
        // `obj.field` reads from `obj`'s slot) need an extra +1
        // here, BEFORE the scope-exit releases below — otherwise
        // the borrowed pointer would dangle by the time the caller
        // dereferences it. Restrict to the syntactic shapes we
        // know are borrows (`Index` / `Field`); other non-`Var`
        // shapes (calls, `super(...)`, literals) already manage
        // their own ownership and would over-retain.
        let tail_is_borrow = blk.tail.as_ref().is_some_and(|e| {
            matches!(&e.kind, ExprKind::Index { .. } | ExprKind::Field { .. })
        });
        let tail = match tail {
            Some((v, ty))
                if tail_needs_retain(&ty)
                    && (tail_aliases_local || tail_is_borrow) =>
            {
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
            | ExprKind::Await(_)
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
            Type::Generic(g) if g.base.as_str() == "Promise" && g.args.len() == 1 => {
                Ok(MirTy::Promise(Box::new(self.resolve_ty(&g.args[0])?)))
            }
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

}
