//! Per-decl metadata the AST→MIR lowering accumulates as a side
//! table. Class / enum layouts live on `Program` (the user-visible
//! output), so this module holds the lookups the lowerer itself
//! needs at build time — field-name → id, method-name → FuncId, etc.

use std::collections::HashMap;

use ilang_ast::{self as ast, Span, Symbol};

use crate::inst::FuncId;
use crate::types::MirTy;

// Recorded for every `@extern(C) @lib(..)` fn while lowering. The
// MIR-codegen reads these fields off `Function` directly today, so
// the fields here are bookkeeping for any future passes that might
// want richer per-extern metadata in the AST→MIR layer.
#[allow(dead_code)]
pub(in crate::lower) struct ExternMeta {
    pub(in crate::lower) libs: Vec<Symbol>,
    pub(in crate::lower) optional: bool,
    pub(in crate::lower) variadic: bool,
    pub(in crate::lower) c_symbol: Symbol,
}

pub(in crate::lower) struct PendingClosure {
    pub(in crate::lower) func_id: FuncId,
    pub(in crate::lower) name: Symbol,
    pub(in crate::lower) params: Vec<(Symbol, MirTy)>,
    pub(in crate::lower) ret: MirTy,
    pub(in crate::lower) captures: Vec<crate::program::EnvCapture>,
    pub(in crate::lower) body: ast::Block,
    pub(in crate::lower) span: Span,
    /// Outer-method class context — preserved so `super.method(...)`
    /// inside the closure body can resolve to the parent class.
    pub(in crate::lower) enclosing_this_class: Option<crate::types::ClassId>,
    /// `Some((name, fn_ty))` when the closure body references its own
    /// binding name (`let f = fn(..) { ... f(..) ... }`) and that
    /// binding is NOT a top-level slot. The body's `Var(name)` then
    /// resolves to `Inst::ClosureSelf` (the hidden env param) typed
    /// as `fn_ty` — no capture, no retain cycle. Slot-backed
    /// top-level bindings keep the late-binding slot fallback.
    pub(in crate::lower) self_ref: Option<(Symbol, MirTy)>,
}

#[derive(Default)]
pub(in crate::lower) struct EnumMeta {
    /// Variant name → (VariantId, discriminant, payload kind).
    pub(in crate::lower) variants: HashMap<Symbol, EnumVariantMeta>,
}

pub(in crate::lower) struct EnumVariantMeta {
    pub(in crate::lower) id: crate::inst::VariantId,
    pub(in crate::lower) payload: VariantPayloadMeta,
}

#[derive(Clone)]
pub(in crate::lower) enum VariantPayloadMeta {
    Unit,
    /// Tuple variant — element MirTys in order.
    Tuple(Vec<MirTy>),
    /// Struct variant — field name → (idx, MirTy).
    Struct(Vec<(Symbol, MirTy)>),
}

#[derive(Default)]
pub(in crate::lower) struct ClassMeta {
    pub(in crate::lower) field_ix: HashMap<Symbol, crate::inst::FieldId>,
    pub(in crate::lower) field_ty: HashMap<crate::inst::FieldId, MirTy>,
    /// Includes both regular methods and `init` (under the symbol "init").
    pub(in crate::lower) method_ids: HashMap<Symbol, FuncId>,
    pub(in crate::lower) method_sigs: HashMap<Symbol, FnSig>,
    /// `static name(...): T { ... }` — called as `Class.method(...)`.
    pub(in crate::lower) static_method_ids: HashMap<Symbol, FuncId>,
    pub(in crate::lower) static_method_sigs: HashMap<Symbol, FnSig>,
    /// `static name: T = ...` / `const name: T = ...` slots.
    pub(in crate::lower) static_slots: HashMap<Symbol, crate::inst::StaticSlotId>,
    /// `get name(): T` — synthesised method id for the getter.
    pub(in crate::lower) property_getter: HashMap<Symbol, (FuncId, MirTy)>,
    /// `set name(v: T)` — synthesised method id for the setter.
    pub(in crate::lower) property_setter: HashMap<Symbol, (FuncId, MirTy)>,
    /// `pub static get name(): T` — synthesised receiver-less getter
    /// fn id for the static property. Dispatched at `Class.name`
    /// read sites; no `this` is passed.
    pub(in crate::lower) static_property_getter: HashMap<Symbol, (FuncId, MirTy)>,
    /// `pub static set name(v: T)` — synthesised receiver-less
    /// setter fn id. Dispatched at `Class.name = v` write sites.
    pub(in crate::lower) static_property_setter: HashMap<Symbol, (FuncId, MirTy)>,
}

impl ClassMeta {
    pub(in crate::lower) fn add_method(&mut self, name: Symbol, id: FuncId, sig: FnSig) {
        self.method_ids.insert(name, id);
        self.method_sigs.insert(name, sig);
    }

    pub(in crate::lower) fn add_static_method(&mut self, name: Symbol, id: FuncId, sig: FnSig) {
        self.static_method_ids.insert(name, id);
        self.static_method_sigs.insert(name, sig);
    }
}

/// Resolve a class `Symbol` to its `ClassId` by scanning the
/// registered class table. Used at lowering sites where an identifier
/// could refer to a class (e.g. `Class.method(...)` / `new Class { .. }`).
pub(in crate::lower) fn class_id_by_name(
    classes: &[crate::program::ClassLayout],
    class_meta: &HashMap<crate::types::ClassId, ClassMeta>,
    name: Symbol,
) -> Option<crate::types::ClassId> {
    class_meta
        .keys()
        .find(|cid| classes[cid.0 as usize].name == name)
        .copied()
}

#[derive(Clone)]
pub(in crate::lower) struct FnSig {
    pub(in crate::lower) params: Vec<MirTy>,
    pub(in crate::lower) ret: MirTy,
}
