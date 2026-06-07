//! AST → MIR lowering.
//!
//! Driven by `lower_program`. Currently covers a working subset of
//! the language; remaining node kinds are listed as `Unsupported`
//! errors so the integration tests fail loudly until we expand
//! coverage. The aim is to grow this file feature-by-feature in the
//! same order as `docs/syntax.md`.
//!
//! The driver here orchestrates the pre-passes + body lowering;
//! mechanical work lives in siblings — `lower_state` (the
//! persistent `Lower` struct + `resolve_ty`), `body_cx` (the
//! per-fn-body `BodyCx` with scope / retain-release helpers),
//! `meta` (small bookkeeping types: `ClassMeta` / `EnumMeta` /
//! `FnSig` / `ExternMeta` / `PendingClosure`), and the per-shape
//! lowering passes already split into `decl/`, `calls/`, `stmt`,
//! `expr`, `ops`, `match_`, `iter_ctor`, `literals`, etc.

use std::collections::HashMap;

use ilang_ast::{Item, Program as AstProgram, Symbol};

mod body_cx;
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
mod lower_state;
mod match_;
mod meta;
mod ops;
mod stmt;
mod utils;

pub use utils::ty_to_mir;

use crate::inst::FuncId;
use crate::program::Program;
use crate::types::MirTy;

pub(in crate::lower) use body_cx::BodyCx;
pub(in crate::lower) use env::{Binding, LoopFrame};
pub(in crate::lower) use lower_state::Lower;
pub(in crate::lower) use meta::{
    class_id_by_name, ClassMeta, EnumMeta, EnumVariantMeta, ExternMeta, FnSig,
    PendingClosure, VariantPayloadMeta,
};

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
                    is_com_interface: false,
                    is_handle: false,
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
        // Top-level `Item::Interface` and `@objc interface`
        // declarations nested in `@extern(ObjC)` blocks share the
        // same MIR registration — both produce a class-id shell
        // that resolves as a type, with one virtual-dispatch slot
        // per declared method.
        let iface_list: Vec<&ilang_ast::InterfaceDecl> = match item {
            Item::Interface(i) => vec![i],
            Item::ExternC(b) => b.interfaces.iter().collect(),
            _ => continue,
        };
        for i in iface_list {
            if !lower.class_ids.contains_key(&i.name) {
                let id = crate::types::ClassId(lower.classes.len() as u32);
                lower.class_ids.insert(i.name, id);
                // `@objc interface` shells need a synthetic
                // `handle: i64` field at index 0 — every @objc
                // class that implements one carries the slot at
                // HEADER+0, so dispatch wrappers that load
                // `arg.handle as *id` from an interface-typed
                // param can use the normal field-load path. Plain
                // ilang interfaces leave the fields empty since
                // their values are arbitrary ilang class
                // instances with their own layouts.
                let fields: Vec<crate::program::FieldDecl> = if i.is_objc {
                    vec![crate::program::FieldDecl {
                        id: crate::inst::FieldId(0),
                        name: Symbol::intern("handle"),
                        ty: MirTy::I64,
                        bit_field: None,
                    }]
                } else {
                    Vec::new()
                };
                lower.classes.push(crate::program::ClassLayout {
                    id,
                    name: i.name,
                    parent: None,
                    fields,
                    methods: Vec::new(),
                    statics: Vec::new(),
                    drop_fn: FuncId(u32::MAX),
                    vtable: None,
                    repr: crate::program::ClassRepr::ArcObject,
                    c_field_offsets: Vec::new(),
                    c_size: 0,
                    flex_elem_size: 0,
                    is_com_interface: false,
                    is_handle: false,
                });
                let mut meta = ClassMeta::default();
                if i.is_objc {
                    meta.field_ix
                        .insert(Symbol::intern("handle"), crate::inst::FieldId(0));
                    meta.field_ty.insert(crate::inst::FieldId(0), MirTy::I64);
                }
                lower.class_meta.insert(id, meta);
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
            if i.is_com {
                lower.com_interfaces.insert(i.name);
                // Also stamp the matching ClassLayout so codegen
                // can recognise @com interfaces without re-deriving
                // them from a side-table. Used by Retain / Release
                // codegen to skip the ARC rc-bump on COM handles.
                if let Some(cid) = lower.class_ids.get(&i.name).copied() {
                    lower.classes[cid.0 as usize].is_com_interface = true;
                }
            }
        }
    }
    // Build a name → declaration index once for both the slot pass
    // below and the signature-resolution pass further down. The
    // previous shape re-scanned `prog.items` (and allocated a fresh
    // `Vec` each step) on every parent lookup, which on DirectX /
    // Windows COM bindings turned into thousands of linear scans per
    // lowering. References borrow `prog` directly; lowering only
    // mutates `lower`, so they stay valid throughout the fn.
    let mut iface_by_name: std::collections::HashMap<
        Symbol,
        &ilang_ast::InterfaceDecl,
    > = std::collections::HashMap::new();
    for item in &prog.items {
        match item {
            Item::Interface(i) => {
                iface_by_name.insert(i.name, i);
            }
            Item::ExternC(b) => {
                for i in b.interfaces.iter() {
                    iface_by_name.insert(i.name, i);
                }
            }
            _ => {}
        }
    }

    // 1a''. Once every `@com interface` is registered, walk each one
    //       to build its per-interface vtable-slot table. Slots are
    //       0-based; an `extends`-parent's slots come first, matching
    //       the C++ COM ABI (`IUnknown::QueryInterface` at slot 0 of
    //       every derived interface). Done in a separate pass so the
    //       parent name resolves regardless of declaration order.
    for item in &prog.items {
        let iface_list: Vec<&ilang_ast::InterfaceDecl> = match item {
            Item::Interface(i) => vec![i],
            Item::ExternC(b) => b.interfaces.iter().collect(),
            _ => continue,
        };
        for i in iface_list {
            if !i.is_com {
                continue;
            }
            // Inheritance chain — collect own + ancestors in order
            // from root to leaf. The leaf's own methods are
            // appended last so its slot range is `[parent_total ..
            // parent_total + own.len())`.
            //
            // Type-check rejects cycles before lowering, but MIR
            // walks the raw AST `parent` field (not the validated
            // `InterfaceSig`), so a misbehaving caller that hands us
            // an unchecked AST could still feed in a cycle. Guard
            // with a `visited` set so the walk terminates regardless.
            let mut chain: Vec<Symbol> = Vec::new();
            let mut visited: std::collections::HashSet<Symbol> =
                std::collections::HashSet::new();
            let mut cur: Option<Symbol> = Some(i.name);
            while let Some(name) = cur {
                if !visited.insert(name) {
                    break;
                }
                chain.push(name);
                cur = iface_by_name.get(&name).and_then(|d| d.parent);
            }
            chain.reverse();
            let mut slot: u32 = 0;
            for ancestor in chain {
                if let Some(ad) = iface_by_name.get(&ancestor) {
                    for m in ad.methods.iter() {
                        lower
                            .com_iface_slots
                            .insert((i.name, m.name), slot);
                        slot += 1;
                    }
                }
            }
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
    // Inject the built-in `TypeKind` enum AFTER user enums have been
    // registered so user-declared enums keep stable ids starting at 0
    // (the `.kind` lowering looks up TypeKind by name, not by id).
    lower.inject_typekind_enum();

    // Resolve interface method signatures now that class / enum
    // types they reference are registered. Walk both top-level
    // `Item::Interface` and `@objc interface` declarations nested
    // inside `@extern(ObjC)` blocks — the registration pass above
    // covers both, so the signature recording must too.
    //
    // `@com interface` additionally records every inherited
    // method against the child name so `device.Release()` (whose
    // `Release` lives on the IUnknown parent) finds its signature
    // and slot through the same `iface_method_sigs` /
    // `com_iface_slots` lookup as own methods.
    for item in &prog.items {
        let iface_list: Vec<&ilang_ast::InterfaceDecl> = match item {
            Item::Interface(i) => vec![i],
            Item::ExternC(b) => b.interfaces.iter().collect(),
            _ => continue,
        };
        for i in iface_list {
            // Walk the parent chain (root → leaf) so inherited
            // signatures land under this interface's name first.
            // Defensive `visited` guard so an unchecked-AST input
            // with a cycle can't hang the lower pass — see the
            // matching note on the slot-assignment walk above.
            let mut chain: Vec<Symbol> = Vec::new();
            if i.is_com {
                let mut visited: std::collections::HashSet<Symbol> =
                    std::collections::HashSet::new();
                visited.insert(i.name);
                let mut cur: Option<Symbol> = i.parent;
                while let Some(name) = cur {
                    if !visited.insert(name) {
                        break;
                    }
                    chain.push(name);
                    cur = iface_by_name.get(&name).and_then(|d| d.parent);
                }
                chain.reverse();
            }
            chain.push(i.name);
            for ancestor in chain {
                let Some(decl) = iface_by_name.get(&ancestor).copied() else {
                    continue;
                };
                for m in decl.methods.iter() {
                    let mut params: Vec<MirTy> = Vec::with_capacity(m.params.len());
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
    // 3a. Same for `@objc class`es nested inside ExternC blocks. The
    //     per-block `lower_extern_c` also calls `declare_class_methods`,
    //     but that runs later in step 4 — interleaved with body
    //     lowering for sibling items. A top-level `pub fn` that
    //     references a sibling-block class would otherwise hit an
    //     empty `method_ids` at lower time, because the class's
    //     block hasn't reached its method-population step yet.
    for item in &prog.items {
        if let Item::ExternC(blk) = item {
            for inner in blk.items.iter() {
                if let ilang_ast::ExternCItem::Class(cd) = inner {
                    lower.declare_class_methods(cd)?;
                }
            }
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
