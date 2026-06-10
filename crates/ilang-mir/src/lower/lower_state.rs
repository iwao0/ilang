//! `Lower` — the AST→MIR driver's persistent state. Accumulates
//! function signatures, class / enum / struct layouts, REPL slot
//! bookkeeping, and the pending-closure work queue across an
//! entire `lower_program` run. Borrowed methods (`new` / `finish`
//! / `resolve_ty`) live here; the per-decl recording passes live
//! in `decl/`.

use std::collections::HashMap;

use ilang_ast::{Symbol, Type};

use crate::inst::FuncId;
use crate::program::{Function, Program};
use crate::types::MirTy;

use super::meta::{ClassMeta, EnumMeta, ExternMeta, FnSig, PendingClosure};
use super::utils::ty_to_mir;
use super::LowerError;

pub(in crate::lower) struct Lower {
    /// All MIR functions accumulated so far. The entry point goes in
    /// last (after main is built).
    pub(in crate::lower) funcs: Vec<Function>,
    /// Top-level fn name → FuncId, populated by the pre-registration
    /// pass. Indexes into `funcs` once `lower_fn` has been called.
    pub(in crate::lower) fn_ids: HashMap<Symbol, FuncId>,
    /// Top-level fn signatures (param/ret types, MIR side). Used by
    /// callers to know the return type without re-checking.
    pub(in crate::lower) fn_sigs: HashMap<Symbol, FnSig>,
    /// Registered class layouts (one per class declaration).
    pub(in crate::lower) classes: Vec<crate::program::ClassLayout>,
    /// Class name → ClassId.
    pub(in crate::lower) class_ids: HashMap<Symbol, crate::types::ClassId>,
    /// Subset of `class_ids` that names interfaces. Lets the lowerer
    /// distinguish "real class" from "interface marker" when emitting
    /// method-call dispatch.
    pub(in crate::lower) interface_ids: HashMap<Symbol, crate::types::ClassId>,
    /// Per-(interface, method-name) global slot id, allocated in
    /// declaration order. Method calls on interface-typed receivers
    /// dispatch through `__virt_dispatch(receiver_class_id, slot)`,
    /// where `slot` is shared between the interface call site and
    /// every implementing class's vtable entry.
    pub(in crate::lower) iface_method_slots: HashMap<(Symbol, Symbol), u32>,
    /// Captures the per-interface method list so call sites can look
    /// up the slot for a method name without re-scanning the AST.
    pub(in crate::lower) iface_methods_by_name: HashMap<Symbol, Vec<Symbol>>,
    /// Method signature per (interface, method) — call sites need
    /// `params` for arg coercion and `ret` for the dst value type.
    pub(in crate::lower) iface_method_sigs: HashMap<(Symbol, Symbol), FnSig>,
    /// Set of interfaces declared with `@com`. Method dispatch on a
    /// receiver typed as one of these uses raw COM vtable
    /// indirection (`(*recv)[slot](recv, args...)`) instead of the
    /// class-registry-backed `__virt_dispatch`.
    pub(in crate::lower) com_interfaces: std::collections::HashSet<Symbol>,
    /// Per-(@com interface, method) slot — 0-based and concatenated
    /// across the parent chain so the C++ COM ABI's
    /// "parent vtable first, child appends" layout drops out
    /// naturally.
    pub(in crate::lower) com_iface_slots: HashMap<(Symbol, Symbol), u32>,
    /// Per-class metadata used during lowering: field name → FieldId,
    /// method name → FuncId for the (mangle-resolved) implementation.
    pub(in crate::lower) class_meta: HashMap<crate::types::ClassId, ClassMeta>,
    /// Registered enum layouts.
    pub(in crate::lower) enums: Vec<crate::program::EnumLayout>,
    pub(in crate::lower) enum_ids: HashMap<Symbol, crate::types::EnumId>,
    pub(in crate::lower) enum_meta: HashMap<crate::types::EnumId, EnumMeta>,
    /// Top-level static slots accumulated across classes.
    pub(in crate::lower) statics: Vec<crate::program::StaticSlot>,
    /// Synthesised closure functions waiting for their bodies to be
    /// lowered. Drained after each outer fn body completes so that
    /// nested closures get processed without recursive borrows.
    pub(in crate::lower) pending_closures: Vec<PendingClosure>,
    /// Counter for anonymous fn names.
    pub(in crate::lower) anon_counter: u32,
    /// Per @extern(C) @lib fn binding metadata.
    pub(in crate::lower) extern_meta: HashMap<Symbol, ExternMeta>,
    /// `__main`'s FuncId, set during `lower_main`. Subsequent pending
    /// closures push more functions, so `funcs.len() - 1` no longer
    /// identifies the entry.
    pub(in crate::lower) main_id: Option<FuncId>,
    /// User-visible name → list of mangled names (each registered in
    /// `fn_ids` / `fn_sigs`). Most entries have a single mangled name
    /// equal to the user name; overloaded fns have one entry per
    /// declared overload.
    pub(in crate::lower) overloads: HashMap<Symbol, Vec<Symbol>>,
    /// REPL persistent slots: name → (slot index, MirTy). Set via
    /// [`lower_program_with_slots`]. Empty for non-REPL compilations.
    /// Drives host-slot store/load emission in __main and Var lookup.
    /// Resolved lazily from `repl_slot_ast` after the class / enum
    /// pre-passes register their ids.
    pub(in crate::lower) repl_slots: HashMap<Symbol, (u32, MirTy)>,
    /// Pre-resolution slot table from the REPL caller. Each entry is
    /// converted to `repl_slots[name]` once the lowerer's class /
    /// enum tables are populated.
    pub(in crate::lower) repl_slot_ast: HashMap<Symbol, (u32, ilang_ast::Type)>,
    /// When `true` (file / AOT runs), __main's epilogue releases
    /// every heap-typed slot so class deinits fire before the
    /// process exits. The interactive REPL passes `false`: slots
    /// outlive the chunk, and releasing them per chunk left the
    /// next chunk reading freed memory.
    pub(in crate::lower) release_slots_at_exit: bool,
}

impl Lower {
    pub(in crate::lower) fn new() -> Self {
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
            com_interfaces: std::collections::HashSet::new(),
            com_iface_slots: HashMap::new(),
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
            release_slots_at_exit: true,
        }
    }

    /// Built-in `TypeKind` enum surfaced by `typeof(x).kind`. The
    /// type checker registers a matching enum sig under the same
    /// name; mirroring it on the MIR side means `.kind` can return
    /// `MirTy::Enum(typekind_eid)` and user code can `match` on the
    /// variant names exactly the same way it `match`es on a
    /// user-declared enum.
    ///
    /// Variants and discriminants must match the order the type
    /// checker uses in `builtins.rs` so a `match` arm with name
    /// `class` resolves to discriminant 1 here too (which is what
    /// `$type.kind` reports for a class instance).
    pub(in crate::lower) fn inject_typekind_enum(&mut self) {
        use crate::inst::VariantId;
        use crate::program::{EnumLayout, VariantDecl, VariantPayload};
        use crate::types::EnumId;
        use super::meta::{EnumVariantMeta, VariantPayloadMeta};
        const VARIANTS: &[&str] = &[
            "primitive", "class", "enum", "optional", "array", "fn", "tuple",
            "string", "unit",
        ];
        let eid = EnumId(self.enums.len() as u32);
        let name = Symbol::intern("TypeKind");
        let mut variants = Vec::with_capacity(VARIANTS.len());
        let mut meta = EnumMeta::default();
        for (i, vname) in VARIANTS.iter().enumerate() {
            let vid = VariantId(i as u32);
            let sym = Symbol::intern(vname);
            variants.push(VariantDecl {
                id: vid,
                name: sym,
                discriminant: i as i64,
                discriminant_str: None,
                payload: VariantPayload::Unit,
            });
            meta.variants.insert(
                sym,
                EnumVariantMeta { id: vid, payload: VariantPayloadMeta::Unit },
            );
        }
        self.enum_ids.insert(name, eid);
        self.enum_meta.insert(eid, meta);
        self.enums.push(EnumLayout {
            id: eid,
            name,
            repr: MirTy::I64,
            variants,
            is_flags: false,
        });
    }

    pub(in crate::lower) fn finish(mut self) -> Program {
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

    pub(in crate::lower) fn resolve_ty(&self, t: &Type) -> Result<MirTy, LowerError> {
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
                // Built-in `Set<T>` — element kind matches `Map`'s key
                // (string / integer / bool); enforced by the type
                // checker, not here.
                if g.base.as_str() == "Set" && g.args.len() == 1 {
                    let elem = self.resolve_ty(&g.args[0])?;
                    return Ok(MirTy::Set { elem: Box::new(elem) });
                }
                // Built-in `Promise<T>`.
                if g.base.as_str() == "Promise" && g.args.len() == 1 {
                    return Ok(MirTy::Promise(Box::new(self.resolve_ty(&g.args[0])?)));
                }
                // Built-in `ObjCBlock<fn(...): R>` — an ObjC block.
                // At the ABI level it's a pointer to a
                // `Block_literal` struct, which we represent as
                // `i64` for now. The inner `fn(...)` shape is
                // preserved for the type checker so
                // `new ObjCBlock(closure)` can match the callback
                // signature against the surrounding binding.
                if g.base.as_str() == "ObjCBlock"
                    && g.args.len() == 1
                    && matches!(g.args[0], Type::Fn(_))
                {
                    return Ok(MirTy::I64);
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
