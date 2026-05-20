//! Enum / class / fn / `__main` declaration + body lowering on
//! `Lower`. Pre-registration passes (`register_enum`,
//! `declare_fn`, `register_class`, `declare_class_methods`)
//! populate id tables so call sites resolve before bodies run;
//! body passes (`lower_class_methods`, `lower_method`,
//! `lower_static_method`, `lower_fn`, `lower_main`,
//! `lower_pending_closure`) follow.

use std::collections::HashMap;

use ilang_ast::{self as ast, Expr, ExprKind, FnDecl, Stmt, Symbol};

use crate::builder::FunctionBuilder;
use crate::inst::{FuncId, FuncRef, Inst, MirConst, Terminator};
use crate::program::{FuncParam, FunctionKind};
use crate::types::MirTy;

use super::super::collect::{
    collect_cellified_names_block, collect_cellified_names_expr, collect_cellified_names_stmt,
};
use super::super::env::{Binding, Env};
use super::super::utils::{mangle_suffix, placeholder_function};
use super::super::{
    BodyCx, ClassMeta, EnumMeta, EnumVariantMeta, FnSig, Lower, LowerError, PendingClosure,
    VariantPayloadMeta,
};

impl Lower {
    pub(in crate::lower) fn register_enum(&mut self, ed: &ast::EnumDecl) -> Result<(), LowerError> {
        if !ed.type_params.is_empty() {
            return Err(LowerError::Unsupported("generic enums"));
        }
        // Reuse the pre-allocated id from the name-pre-pass when
        // present (see `lower_program`'s 1b. step). Otherwise allocate
        // a fresh one.
        let (id, was_pre_allocated) = match self.enum_ids.get(&ed.name).copied() {
            Some(id) => (id, true),
            None => {
                let id = crate::types::EnumId(self.enums.len() as u32);
                self.enum_ids.insert(ed.name, id);
                (id, false)
            }
        };

        let repr_ty = match &ed.repr_ty {
            Some(t) => self.resolve_ty(t)?,
            None => MirTy::I64,
        };
        let is_str_repr = matches!(repr_ty, MirTy::Str);
        if is_str_repr && ed.flags {
            return Err(LowerError::Unsupported(
                "@flags is not allowed on `: string`-repr enums (bitwise ops are int-only)",
            ));
        }

        let mut variants = Vec::with_capacity(ed.variants.len());
        let mut meta = EnumMeta::default();
        let mut prev_disc: i64 = -1;
        for (i, v) in ed.variants.iter().enumerate() {
            let vid = crate::inst::VariantId(i as u32);
            let (disc, disc_str): (i64, Option<String>) = match (&v.discriminant, is_str_repr) {
                (Some(ast::DiscriminantLit::Int(n)), false) => (*n, None),
                (Some(ast::DiscriminantLit::Str(s)), true) => {
                    (i as i64, Some(s.clone()))
                }
                (None, false) => (prev_disc + 1, None),
                (None, true) => {
                    return Err(LowerError::Unsupported(
                        "enum with `: string` repr requires an explicit `= \"…\"` discriminant on every variant",
                    ));
                }
                (Some(ast::DiscriminantLit::Str(_)), false) => {
                    return Err(LowerError::Unsupported(
                        "string discriminant used on a non-string-repr enum",
                    ));
                }
                (Some(ast::DiscriminantLit::Int(_)), true) => {
                    return Err(LowerError::Unsupported(
                        "integer discriminant used on a `: string` repr enum",
                    ));
                }
            };
            prev_disc = disc;
            let (payload_layout, payload_meta) = match &v.payload {
                ast::VariantPayload::Unit => (
                    crate::program::VariantPayload::Unit,
                    VariantPayloadMeta::Unit,
                ),
                ast::VariantPayload::Tuple(tys) => {
                    let mut out = Vec::with_capacity(tys.len());
                    for t in tys.iter() {
                        out.push(self.resolve_ty(t)?);
                    }
                    (
                        crate::program::VariantPayload::Tuple(out.clone().into_boxed_slice()),
                        VariantPayloadMeta::Tuple(out),
                    )
                }
                ast::VariantPayload::Struct(fields) => {
                    let mut out_named: Vec<(Symbol, MirTy)> = Vec::with_capacity(fields.len());
                    for f in fields.iter() {
                        out_named.push((f.name, self.resolve_ty(&f.ty)?));
                    }
                    (
                        crate::program::VariantPayload::Struct(
                            out_named.clone().into_boxed_slice(),
                        ),
                        VariantPayloadMeta::Struct(out_named),
                    )
                }
            };
            variants.push(crate::program::VariantDecl {
                id: vid,
                name: v.name,
                discriminant: disc,
                discriminant_str: disc_str,
                payload: payload_layout,
            });
            meta.variants.insert(
                v.name,
                EnumVariantMeta {
                    id: vid,
                    payload: payload_meta,
                },
            );
        }
        let layout = crate::program::EnumLayout {
            id,
            name: ed.name,
            repr: repr_ty,
            variants,
            is_flags: ed.flags,
        };
        if was_pre_allocated {
            // Overwrite the placeholder layout the pre-pass pushed.
            self.enums[id.0 as usize] = layout;
        } else {
            self.enums.push(layout);
        }
        self.enum_meta.insert(id, meta);
        Ok(())
    }

    pub(in crate::lower) fn declare_fn(&mut self, fd: &FnDecl) -> Result<(), LowerError> {
        if !fd.type_params.is_empty() {
            return Err(LowerError::Unsupported("generic functions"));
        }
        let params: Vec<MirTy> = fd
            .params
            .iter()
            .map(|p| self.resolve_ty(&p.ty))
            .collect::<Result<Vec<_>, _>>()?;
        let ret = match &fd.ret {
            Some(t) => self.resolve_ty(t)?,
            None => MirTy::Unit,
        };
        // Mangle when this name already has a previous declaration —
        // i.e. the second+ overload. The first declaration keeps the
        // user-visible name so non-overloaded code stays simple.
        let mangled = if self.fn_ids.contains_key(&fd.name) {
            Symbol::intern(&format!("{}{}", fd.name, mangle_suffix(&params)))
        } else {
            fd.name
        };
        let id = FuncId(self.funcs.len() as u32);
        self.funcs.push(placeholder_function(mangled));
        self.fn_ids.insert(mangled, id);
        self.fn_sigs
            .insert(mangled, FnSig { params: params.clone(), ret });
        // Track overloads under the user-visible name.
        let entries = self.overloads.entry(fd.name).or_default();
        entries.push(mangled);
        // Stash the source-name → primary-mangled mapping in fnDecl
        // bookkeeping so that `lower_fn` can find the right slot.
        Ok(())
    }

    /// Allocate a `ClassId` and field-index table for a class
    /// declaration. Method registration is deferred until
    /// `declare_class_methods` so that a class's own fields can be
    /// resolved by its method signatures.
    pub(in crate::lower) fn register_class(&mut self, cd: &ast::ClassDecl) -> Result<(), LowerError> {
        if !cd.type_params.is_empty() {
            return Err(LowerError::Unsupported("generic classes"));
        }
        if cd.is_repr_c || cd.is_packed || cd.is_union || cd.extern_lib.is_some() {
            return Err(LowerError::Unsupported("@extern(C) classes"));
        }
        // Static methods, fields, const, and properties are wired
        // below in declare_class_methods / register_class.

        // The parser puts the first `:` base into `cd.parent`. If the
        // pre-pass didn't register it as a class, it's actually an
        // interface — interfaces have no MIR layout, so treat the
        // class as having no real parent here. The interface dispatch
        // table is wired up separately below.
        let parent_id = if let Some(parent_name) = cd.parent {
            self.class_ids.get(&parent_name).copied()
        } else {
            None
        };

        // The pre-pass in `lower_program` may have already reserved
        // an id + placeholder layout for this class to enable forward
        // references. Reuse it when present.
        let id = match self.class_ids.get(&cd.name) {
            Some(existing) => *existing,
            None => {
                let id = crate::types::ClassId(self.classes.len() as u32);
                self.class_ids.insert(cd.name, id);
                self.classes.push(crate::program::ClassLayout {
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
                id
            }
        };
        if !self.class_meta.contains_key(&id) {
            self.class_meta.insert(id, ClassMeta::default());
        }

        let mut meta = ClassMeta::default();
        let mut fields = Vec::new();
        let mut next_fid: u32 = 0;
        // Inherit parent fields first (preserve their FieldIds as
        // contiguous indexes into the child's field vec).
        if let Some(pid) = parent_id {
            let parent = &self.classes[pid.0 as usize].clone();
            for f in &parent.fields {
                let fid = crate::inst::FieldId(next_fid);
                next_fid += 1;
                meta.field_ix.insert(f.name, fid);
                meta.field_ty.insert(fid, f.ty.clone());
                fields.push(crate::program::FieldDecl {
                    id: fid,
                    name: f.name,
                    ty: f.ty.clone(),
                    bit_field: None,
                });
            }
        }
        for fd in cd.fields.iter() {
            let fid = crate::inst::FieldId(next_fid);
            next_fid += 1;
            let fty = self.resolve_ty(&fd.ty)?;
            meta.field_ix.insert(fd.name, fid);
            meta.field_ty.insert(fid, fty.clone());
            fields.push(crate::program::FieldDecl {
                id: fid,
                name: fd.name,
                ty: fty,
                bit_field: None,
            });
        }
        // Static / const fields → StaticSlot table.
        let mut static_slot_ids = Vec::new();
        for sf in cd.static_fields.iter() {
            let slot_id = crate::inst::StaticSlotId(self.statics.len() as u32);
            let ty = self.resolve_ty(&sf.ty)?;
            let init_const = match &sf.value.kind {
                ExprKind::Int(n) => MirConst::Int(*n),
                ExprKind::Float(f) => MirConst::F64(f.to_bits()),
                ExprKind::Bool(b) => MirConst::Bool(*b),
                // String slots can't carry a literal in the
                // static-data section — the loader has emitted a
                // synthetic startup `AssignField` that fills in
                // the real heap pointer. Initial bytes = null.
                ExprKind::Str(_) if matches!(ty, MirTy::Str) => MirConst::Int(0),
                ExprKind::Str(s) => MirConst::Str(Symbol::intern(s)),
                // Non-literal initializer — the loader has
                // emitted a synthetic top-level
                // `ClassName.field = expr` (with `is_init:
                // true`) so the slot gets the real value at
                // program startup. Use a typed zero default
                // here just for the slot's static layout.
                _ => match &ty {
                    MirTy::F32 | MirTy::F64 => MirConst::F64(0u64),
                    MirTy::Bool => MirConst::Bool(false),
                    // Strings live as heap pointers — the slot's
                    // initial 8 bytes are a null pointer; the
                    // runtime-init AssignField fills in the real
                    // value at startup.
                    MirTy::Str => MirConst::Int(0),
                    _ => MirConst::Int(0),
                },
            };
            self.statics.push(crate::program::StaticSlot {
                id: slot_id,
                owner: id,
                name: sf.name,
                ty,
                is_const: sf.is_const,
                init: init_const,
            });
            static_slot_ids.push(slot_id);
            meta.static_slots.insert(sf.name, slot_id);
        }

        // Update the placeholder layout in place — the pre-pass
        // already pushed it onto `self.classes`.
        let layout = &mut self.classes[id.0 as usize];
        layout.parent = parent_id;
        layout.fields = fields;
        layout.statics = static_slot_ids;
        layout.repr = crate::program::ClassRepr::ArcObject;
        self.class_meta.insert(id, meta);
        Ok(())
    }

    /// Pre-register signatures for every method on the class so that
    /// in-class calls (`this.foo()` / cross-method) resolve regardless
    /// of declaration order.
    pub(in crate::lower) fn declare_class_methods(&mut self, cd: &ast::ClassDecl) -> Result<(), LowerError> {
        let class_id = *self.class_ids.get(&cd.name).expect("class registered");
        let class_obj_ty = MirTy::Object(class_id);
        let mut method_decls = Vec::new();

        // Inherit parent's method registry (init / deinit are
        // per-class; instance methods carry over for `super` resolution
        // and direct calls). Override below replaces the FuncId.
        let parent_id = self.classes[class_id.0 as usize].parent;
        if let Some(pid) = parent_id {
            let parent_meta_clone: Vec<(Symbol, FuncId, FnSig)> = self
                .class_meta
                .get(&pid)
                .map(|m| {
                    m.method_ids
                        .iter()
                        .filter(|(n, _)| n.as_str() != "init" && n.as_str() != "deinit")
                        .map(|(name, fid)| {
                            let sig = m.method_sigs.get(name).cloned().unwrap();
                            (*name, *fid, sig)
                        })
                        .collect()
                })
                .unwrap_or_default();
            // Re-sign parent methods so the receiver type points to
            // this class instead of the parent (subtype substitution).
            let meta = self.class_meta.get_mut(&class_id).unwrap();
            for (name, fid, sig) in parent_meta_clone {
                let mut new_sig = sig.clone();
                if let Some(first) = new_sig.params.first_mut() {
                    *first = class_obj_ty.clone();
                }
                meta.method_ids.insert(name, fid);
                meta.method_sigs.insert(name, new_sig);
                method_decls.push(crate::program::MethodDecl {
                    name,
                    is_override: false,
                    is_static: false,
                    func: fid,
                    slot: None,
                });
            }
            // Inherit parent's property getter / setter slots too
            // so a subclass's `child.pos` (where `pos` is declared
            // on the parent as `pub get pos(): T`) resolves through
            // the same FuncId. Child-declared accessors of the same
            // name overwrite below in the `cd.properties` loop.
            let parent_props: Vec<(Symbol, (FuncId, MirTy), Option<(FuncId, MirTy)>)> = self
                .class_meta
                .get(&pid)
                .map(|m| {
                    m.property_getter
                        .iter()
                        .map(|(name, get)| {
                            let set = m.property_setter.get(name).cloned();
                            (*name, get.clone(), set)
                        })
                        .collect()
                })
                .unwrap_or_default();
            // Static accessors mirror the inheritance rule of static
            // methods — subclass call sites can reach the parent's
            // `pub static get` / `pub static set` via the child's
            // class name (`Child.blackColor` finds `Parent`'s slot).
            let parent_static_props: Vec<(Symbol, (FuncId, MirTy), Option<(FuncId, MirTy)>)> =
                self
                    .class_meta
                    .get(&pid)
                    .map(|m| {
                        m.static_property_getter
                            .iter()
                            .map(|(name, get)| {
                                let set = m.static_property_setter.get(name).cloned();
                                (*name, get.clone(), set)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
            let meta = self.class_meta.get_mut(&class_id).unwrap();
            for (name, get, set) in parent_props {
                meta.property_getter.insert(name, get);
                if let Some(s) = set {
                    meta.property_setter.insert(name, s);
                }
            }
            for (name, get, set) in parent_static_props {
                meta.static_property_getter.insert(name, get);
                if let Some(s) = set {
                    meta.static_property_setter.insert(name, s);
                }
            }
        }

        for m in cd.methods.iter() {
            if !m.type_params.is_empty() {
                return Err(LowerError::Unsupported("generic methods"));
            }
            // Mangled name: `Class.method` (init included). This is the
            // post-overload-resolution name; until overloading is wired,
            // we use a single function per (class, method) pair.
            let mangled = Symbol::intern(&format!("{}.{}", cd.name, m.name));
            let id = FuncId(self.funcs.len() as u32);
            self.funcs.push(placeholder_function(mangled));
            self.fn_ids.insert(mangled, id);

            // Method signature: `this` is an implicit first parameter.
            let mut params = vec![class_obj_ty.clone()];
            for p in m.params.iter() {
                params.push(self.resolve_ty(&p.ty)?);
            }
            // `init` synthesises a return of `this` so callers of
            // `new C(args)` get the constructed instance back.
            let user_ret = match &m.ret {
                Some(t) => self.resolve_ty(t)?,
                None => MirTy::Unit,
            };
            let ret = if m.name == "init" {
                MirTy::Object(class_id)
            } else {
                user_ret
            };
            self.fn_sigs.insert(mangled, FnSig { params: params.clone(), ret: ret.clone() });

            let kind = if m.name == "init" {
                FunctionKind::Init { class: class_id }
            } else if m.name == "deinit" {
                // Record the deinit fn as the class's drop fn so
                // codegen can call it on Release.
                self.classes[class_id.0 as usize].drop_fn = id;
                FunctionKind::Drop { class: class_id }
            } else {
                FunctionKind::Local
            };
            // Patch the placeholder with the right kind so
            // post-lowering consumers can recognise it.
            self.funcs[id.0 as usize].kind = kind.clone();
            self.funcs[id.0 as usize].name = mangled;

            let meta = self.class_meta.get_mut(&class_id).unwrap();
            meta.method_ids.insert(m.name, id);
            meta.method_sigs.insert(m.name, FnSig { params, ret });
            // Replace any inherited entry of the same name (override).
            method_decls.retain(|d: &crate::program::MethodDecl| d.name != m.name);
            method_decls.push(crate::program::MethodDecl {
                name: m.name,
                is_override: m.is_override,
                is_static: false,
                func: id,
                slot: None,
            });
        }

        // If this class doesn't define its own deinit but inherits
        // from a parent that has one, point our drop_fn at the
        // parent's so dropping a subclass instance still triggers
        // the parent's destruction chain. Parent classes are
        // processed before children (source-order requirement), so
        // the parent's drop_fn is already set by the time we get
        // here.
        if self.classes[class_id.0 as usize].drop_fn == FuncId(u32::MAX) {
            if let Some(pid) = parent_id {
                let parent_drop = self.classes[pid.0 as usize].drop_fn;
                if parent_drop != FuncId(u32::MAX) {
                    self.classes[class_id.0 as usize].drop_fn = parent_drop;
                }
            }
        }

        // Static methods — registered as top-level fns under
        // `Class.method`.
        for sm in cd.static_methods.iter() {
            if !sm.type_params.is_empty() {
                return Err(LowerError::Unsupported("generic static methods"));
            }
            let mangled = Symbol::intern(&format!("{}.{}", cd.name, sm.name));
            let id = FuncId(self.funcs.len() as u32);
            self.funcs.push(placeholder_function(mangled));
            self.fn_ids.insert(mangled, id);

            let params: Vec<MirTy> = sm
                .params
                .iter()
                .map(|p| self.resolve_ty(&p.ty))
                .collect::<Result<Vec<_>, _>>()?;
            let ret = match &sm.ret {
                Some(t) => self.resolve_ty(t)?,
                None => MirTy::Unit,
            };
            self.fn_sigs.insert(
                mangled,
                FnSig { params: params.clone(), ret: ret.clone() },
            );
            self.funcs[id.0 as usize].name = mangled;
            self.funcs[id.0 as usize].kind = FunctionKind::Local;

            let meta = self.class_meta.get_mut(&class_id).unwrap();
            meta.static_method_ids.insert(sm.name, id);
            meta.static_method_sigs.insert(sm.name, FnSig { params, ret });
        }

        // Properties — synthesise getter/setter as methods. Static
        // accessors (`pub static get/set`) drop the implicit `this`
        // param and register into the static-property maps instead;
        // dispatch at `Class.name` read / write sites reads them
        // directly without passing a receiver.
        for prop in cd.properties.iter() {
            let prop_ty = self.resolve_ty(&prop.ty)?;
            let class_obj_ty = MirTy::Object(class_id);
            let mangle_prefix = if prop.is_static { "get_static" } else { "get" };
            if let Some(_getter_decl) = &prop.getter {
                let mangled =
                    Symbol::intern(&format!("{}.{}_{}", cd.name, mangle_prefix, prop.name));
                let id = FuncId(self.funcs.len() as u32);
                self.funcs.push(placeholder_function(mangled));
                self.fn_ids.insert(mangled, id);
                let params = if prop.is_static {
                    Vec::new()
                } else {
                    vec![class_obj_ty.clone()]
                };
                let ret = prop_ty.clone();
                self.fn_sigs.insert(
                    mangled,
                    FnSig { params: params.clone(), ret: ret.clone() },
                );
                self.funcs[id.0 as usize].name = mangled;
                self.funcs[id.0 as usize].kind = FunctionKind::Local;
                let meta = self.class_meta.get_mut(&class_id).unwrap();
                if prop.is_static {
                    meta.static_property_getter.insert(prop.name, (id, prop_ty.clone()));
                    // Mirror into `static_method_ids` so the shared
                    // `lower_static_method` body-lowerer can find it
                    // by the mangled name we passed for the FnDecl.
                    let body_name = Symbol::intern(&format!("get_static_{}", prop.name));
                    meta.static_method_ids.insert(body_name, id);
                    meta.static_method_sigs.insert(body_name, FnSig { params, ret });
                } else {
                    // Synthesise unique keys for property getter/setter so
                    // they don't collide with each other or with regular
                    // methods of the same name.
                    let key = Symbol::intern(&format!("{}::get", prop.name));
                    meta.property_getter.insert(prop.name, (id, prop_ty.clone()));
                    meta.method_sigs.insert(key, FnSig { params, ret });
                    meta.method_ids.insert(key, id);
                }
            }
            let mangle_setter = if prop.is_static { "set_static" } else { "set" };
            if let Some(_setter_decl) = &prop.setter {
                let mangled =
                    Symbol::intern(&format!("{}.{}_{}", cd.name, mangle_setter, prop.name));
                let id = FuncId(self.funcs.len() as u32);
                self.funcs.push(placeholder_function(mangled));
                self.fn_ids.insert(mangled, id);
                let params = if prop.is_static {
                    vec![prop_ty.clone()]
                } else {
                    vec![class_obj_ty.clone(), prop_ty.clone()]
                };
                let ret = MirTy::Unit;
                self.fn_sigs.insert(
                    mangled,
                    FnSig { params: params.clone(), ret: ret.clone() },
                );
                self.funcs[id.0 as usize].name = mangled;
                self.funcs[id.0 as usize].kind = FunctionKind::Local;
                let meta = self.class_meta.get_mut(&class_id).unwrap();
                if prop.is_static {
                    meta.static_property_setter.insert(prop.name, (id, prop_ty.clone()));
                    let body_name = Symbol::intern(&format!("set_static_{}", prop.name));
                    meta.static_method_ids.insert(body_name, id);
                    meta.static_method_sigs.insert(body_name, FnSig { params, ret });
                } else {
                    let key = Symbol::intern(&format!("{}::set", prop.name));
                    meta.property_setter.insert(prop.name, (id, prop_ty.clone()));
                    meta.method_sigs.insert(key, FnSig { params, ret });
                    meta.method_ids.insert(key, id);
                }
            }
        }

        // Assign vtable slots: inherit parent slots for same-named
        // methods, append new slots otherwise. Init / deinit aren't
        // dispatched virtually.
        let parent_slots: HashMap<Symbol, crate::inst::VTableSlot> = match parent_id {
            Some(pid) => self.classes[pid.0 as usize]
                .methods
                .iter()
                .filter_map(|m| m.slot.map(|s| (m.name, s)))
                .collect(),
            None => HashMap::new(),
        };
        let mut next_slot: u32 = parent_slots
            .values()
            .map(|s| s.0 + 1)
            .max()
            .unwrap_or(0);
        for d in method_decls.iter_mut() {
            if matches!(d.name.as_str(), "init" | "deinit") {
                continue;
            }
            let slot = match parent_slots.get(&d.name) {
                Some(s) => *s,
                None => {
                    let s = crate::inst::VTableSlot(next_slot);
                    next_slot += 1;
                    s
                }
            };
            d.slot = Some(slot);
        }
        // Interface dispatch: for every interface this class declares
        // (including the parser's first base if it's actually an
        // interface, plus the explicit `interfaces` list), append a
        // synthetic MethodDecl per interface method pointing at the
        // class's implementation. The slot comes from the global
        // `iface_method_slots` table — by construction it sits above
        // the class-method slot range, so `__virt_dispatch` can be
        // shared.
        let mut declared_ifaces: Vec<Symbol> = Vec::new();
        if let Some(p) = cd.parent {
            if self.interface_ids.contains_key(&p) {
                declared_ifaces.push(p);
            }
        }
        for ifn in cd.interfaces.iter() {
            declared_ifaces.push(*ifn);
        }
        for ifn in declared_ifaces.iter() {
            let methods = self
                .iface_methods_by_name
                .get(ifn)
                .cloned()
                .unwrap_or_default();
            for m_name in methods.iter() {
                let Some(slot) = self.iface_method_slots.get(&(*ifn, *m_name)).copied() else {
                    continue;
                };
                // Find this class's MethodDecl with that name. The
                // overload mangler renames methods to per-overload
                // names, so look at the source-name list we just
                // built and the parsed `methods` for the function id.
                let func_id = method_decls
                    .iter()
                    .find(|d| d.name == *m_name)
                    .map(|d| d.func);
                let Some(func) = func_id else { continue };
                method_decls.push(crate::program::MethodDecl {
                    name: *m_name,
                    is_override: false,
                    is_static: false,
                    func,
                    slot: Some(crate::inst::VTableSlot(slot)),
                });
            }
        }
        let layout = &mut self.classes[class_id.0 as usize];
        layout.methods = method_decls;
        Ok(())
    }

    pub(in crate::lower) fn lower_class_methods(&mut self, cd: &ast::ClassDecl) -> Result<(), LowerError> {
        let class_id = *self.class_ids.get(&cd.name).expect("class registered");
        for m in cd.methods.iter() {
            self.lower_method(class_id, cd.name, m)?;
        }
        // Static methods → lower like a free fn (no `this` injection).
        for sm in cd.static_methods.iter() {
            self.lower_static_method(class_id, cd.name, sm)?;
        }
        // Property getters/setters — lowered like instance methods,
        // but the m.name passed in for the lookup is a synthetic
        // `prop::get` / `prop::set` key (built in declare_class_methods).
        // Static accessors take a different path: their body has no
        // `this`, and the synthesised fn is registered as a top-level
        // static method under a distinct mangled name.
        for prop in cd.properties.iter() {
            if let Some(g) = &prop.getter {
                let mut g2 = g.clone();
                if prop.is_static {
                    g2.name = Symbol::intern(&format!("get_static_{}", prop.name));
                    self.lower_static_method(class_id, cd.name, &g2)?;
                } else {
                    g2.name = Symbol::intern(&format!("{}::get", prop.name));
                    self.lower_method(class_id, cd.name, &g2)?;
                }
            }
            if let Some(s) = &prop.setter {
                let mut s2 = s.clone();
                if prop.is_static {
                    s2.name = Symbol::intern(&format!("set_static_{}", prop.name));
                    self.lower_static_method(class_id, cd.name, &s2)?;
                } else {
                    s2.name = Symbol::intern(&format!("{}::set", prop.name));
                    self.lower_method(class_id, cd.name, &s2)?;
                }
            }
        }
        Ok(())
    }

    pub(in crate::lower) fn lower_pending_closure(&mut self, pc: PendingClosure) -> Result<(), LowerError> {
        let mut fb = FunctionBuilder::new(pc.name, pc.name, pc.ret.clone(), FunctionKind::Local);
        fb.set_span(pc.span);
        let entry = fb.new_block();
        fb.switch_to(entry);

        let mut env = Env::default();
        let mut params_box = Vec::with_capacity(pc.params.len());
        for (pname, pty) in pc.params.iter() {
            let v = fb.add_block_param(entry, pty.clone());
            env.bind(*pname, v, pty.clone());
            params_box.push(FuncParam {
                name: *pname,
                ty: pty.clone(),
                value: v,
            });
        }

        // Build the captures-in-scope map.
        let mut caps_map: HashMap<Symbol, (u32, MirTy)> = HashMap::new();
        let mut cell_caps_set: std::collections::HashSet<Symbol> =
            std::collections::HashSet::new();
        for (i, c) in pc.captures.iter().enumerate() {
            caps_map.insert(c.name, (i as u32, c.ty.clone()));
            if c.is_cell {
                cell_caps_set.insert(c.name);
            }
        }

        // Names cellified inside this closure body too (for nested
        // FnExprs that mutate further captures).
        let mut cellify_inner: std::collections::HashSet<Symbol> =
            std::collections::HashSet::new();
        collect_cellified_names_block(&pc.body, &mut cellify_inner);

        let __cells_empty: std::collections::HashSet<Symbol> = std::collections::HashSet::new();
        let mut bcx = BodyCx {
            fb: &mut fb,
            env: &mut env,
            ret_ty: pc.ret.clone(),
            fn_ids: &mut self.fn_ids,
            fn_sigs: &mut self.fn_sigs,
            loops: Vec::new(),
            this_class: pc.enclosing_this_class,
            classes: &self.classes,
            class_meta: &self.class_meta,
            interface_ids: &self.interface_ids,
            iface_method_slots: &self.iface_method_slots,
            iface_method_sigs: &self.iface_method_sigs,
            com_interfaces: &self.com_interfaces,
            com_iface_slots: &self.com_iface_slots,
            enum_ids: &self.enum_ids,
            enum_meta: &self.enum_meta,
            enums: &self.enums,
            statics: &self.statics,
            pending: &mut self.pending_closures,
            funcs: &mut self.funcs,
            anon_counter: &mut self.anon_counter,
            captures_in_scope: Some(&caps_map),
            cell_captures: Some(&cell_caps_set),
            overloads: &self.overloads,
            cellify_set: &cellify_inner,
            repl_slots: &self.repl_slots,
            is_main_body: false,
            binding_self_name: None,
            crepr_owned_locals: std::collections::HashSet::new(),
        };
        let needs_retain = pc
            .body
            .tail
            .as_ref()
            .map(|e| bcx.callee_retain_decision(e))
            .unwrap_or(false);
        let tail = bcx.lower_block(&pc.body)?;
        if needs_retain {
            bcx.emit_callee_retain(&tail);
        }
        bcx.finalise_return(tail)?;

        let env_layout = if pc.captures.is_empty() {
            None
        } else {
            Some(crate::program::EnvLayout {
                captures: pc.captures.clone(),
            })
        };
        let mut func = fb.finish(params_box.into_boxed_slice());
        func.closure_env = env_layout;
        self.funcs[pc.func_id.0 as usize] = func;
        Ok(())
    }

    pub(in crate::lower) fn lower_static_method(
        &mut self,
        class_id: crate::types::ClassId,
        class_name: Symbol,
        m: &FnDecl,
    ) -> Result<(), LowerError> {
        let id = *self
            .class_meta
            .get(&class_id)
            .unwrap()
            .static_method_ids
            .get(&m.name)
            .expect("static method pre-registered");
        let sig = self
            .class_meta
            .get(&class_id)
            .unwrap()
            .static_method_sigs
            .get(&m.name)
            .cloned()
            .expect("static sig pre-registered");

        let mangled = Symbol::intern(&format!("{}.{}", class_name, m.name));
        let mut fb = FunctionBuilder::new(mangled, m.name, sig.ret.clone(), FunctionKind::Local);
        fb.set_span(m.span);
        let entry = fb.new_block();
        fb.switch_to(entry);

        let mut env = Env::default();
        let mut params_box = Vec::with_capacity(sig.params.len());
        for (param, pty) in m.params.iter().zip(sig.params.iter()) {
            let v = fb.add_block_param(entry, pty.clone());
            env.bind(param.name, v, pty.clone());
            params_box.push(FuncParam {
                name: param.name,
                ty: pty.clone(),
                value: v,
            });
        }

        let mut __cellify: std::collections::HashSet<Symbol> = std::collections::HashSet::new();
        collect_cellified_names_block(&m.body, &mut __cellify);
        let mut bcx = BodyCx {
            fb: &mut fb,
            env: &mut env,
            ret_ty: sig.ret.clone(),
            fn_ids: &mut self.fn_ids,
            fn_sigs: &mut self.fn_sigs,
            loops: Vec::new(),
            this_class: None, // static — no `this`
            classes: &self.classes,
            class_meta: &self.class_meta,
            interface_ids: &self.interface_ids,
            iface_method_slots: &self.iface_method_slots,
            iface_method_sigs: &self.iface_method_sigs,
            com_interfaces: &self.com_interfaces,
            com_iface_slots: &self.com_iface_slots,
            enum_ids: &self.enum_ids,
            enum_meta: &self.enum_meta,
            enums: &self.enums,
            statics: &self.statics,
            pending: &mut self.pending_closures,
            funcs: &mut self.funcs,
            anon_counter: &mut self.anon_counter,
            captures_in_scope: None,
            cell_captures: None,
            cellify_set: &__cellify,
            overloads: &self.overloads,
            repl_slots: &self.repl_slots,
            is_main_body: false,
            binding_self_name: None,
            crepr_owned_locals: std::collections::HashSet::new(),
        };
        let needs_retain = m
            .body
            .tail
            .as_ref()
            .map(|e| bcx.callee_retain_decision(e))
            .unwrap_or(false);
        let tail = bcx.lower_block(&m.body)?;
        if needs_retain {
            bcx.emit_callee_retain(&tail);
        }
        bcx.finalise_return(tail)?;

        let func = fb.finish(params_box.into_boxed_slice());
        self.funcs[id.0 as usize] = func;
        Ok(())
    }

    pub(in crate::lower) fn lower_method(
        &mut self,
        class_id: crate::types::ClassId,
        _class_name: Symbol,
        m: &FnDecl,
    ) -> Result<(), LowerError> {
        let id = *self
            .class_meta
            .get(&class_id)
            .unwrap()
            .method_ids
            .get(&m.name)
            .expect("method pre-registered");
        let sig = self
            .class_meta
            .get(&class_id)
            .unwrap()
            .method_sigs
            .get(&m.name)
            .cloned()
            .expect("method sig pre-registered");

        // Use the FuncId's pre-registered name so property getters /
        // setters keep their unique `Class.get_<prop>` identity (the
        // `m.name` we got is the synthetic `<prop>::get` key).
        let mangled = self.funcs[id.0 as usize].name;
        let kind = self.funcs[id.0 as usize].kind.clone();
        let mut fb = FunctionBuilder::new(mangled, m.name, sig.ret.clone(), kind);
        fb.set_span(m.span);
        let entry = fb.new_block();
        fb.switch_to(entry);

        // First param is `this`.
        let this_v = fb.add_block_param(entry, sig.params[0].clone());
        let mut params_box = vec![FuncParam {
            name: Symbol::intern("this"),
            ty: sig.params[0].clone(),
            value: this_v,
        }];

        let mut env = Env::default();
        env.bind(Symbol::intern("this"), this_v, sig.params[0].clone());

        for (param, pty) in m.params.iter().zip(sig.params.iter().skip(1)) {
            let v = fb.add_block_param(entry, pty.clone());
            env.bind(param.name, v, pty.clone());
            params_box.push(FuncParam {
                name: param.name,
                ty: pty.clone(),
                value: v,
            });
        }

        let mut __cellify: std::collections::HashSet<Symbol> = std::collections::HashSet::new();
        collect_cellified_names_block(&m.body, &mut __cellify);
        let mut bcx = BodyCx {
            fb: &mut fb,
            env: &mut env,
            ret_ty: sig.ret.clone(),
            fn_ids: &mut self.fn_ids,
            fn_sigs: &mut self.fn_sigs,
            loops: Vec::new(),
            this_class: Some(class_id),
            classes: &self.classes,
            class_meta: &self.class_meta,
            interface_ids: &self.interface_ids,
            iface_method_slots: &self.iface_method_slots,
            iface_method_sigs: &self.iface_method_sigs,
            com_interfaces: &self.com_interfaces,
            com_iface_slots: &self.com_iface_slots,
            enum_ids: &self.enum_ids,
            enum_meta: &self.enum_meta,
            enums: &self.enums,
            statics: &self.statics,
            pending: &mut self.pending_closures,
            funcs: &mut self.funcs,
            anon_counter: &mut self.anon_counter,
            captures_in_scope: None,
            cell_captures: None,
            cellify_set: &__cellify,
            overloads: &self.overloads,
            repl_slots: &self.repl_slots,
            is_main_body: false,
            binding_self_name: None,
            crepr_owned_locals: std::collections::HashSet::new(),
        };
        let needs_retain = m
            .body
            .tail
            .as_ref()
            .map(|e| bcx.callee_retain_decision(e))
            .unwrap_or(false);
        let tail = bcx.lower_block(&m.body)?;
        let is_init = matches!(m.name.as_str(), "init");
        if is_init {
            bcx.fb.set_terminator(Terminator::Return { value: Some(this_v) });
        } else {
            if needs_retain {
                bcx.emit_callee_retain(&tail);
            }
            bcx.finalise_return(tail)?;
        }

        let func = fb.finish(params_box.into_boxed_slice());
        self.funcs[id.0 as usize] = func;
        Ok(())
    }

    pub(in crate::lower) fn lower_fn(&mut self, fd: &FnDecl) -> Result<(), LowerError> {
        // Resolve the right mangled name by matching this FnDecl's
        // param types against the candidates registered for `fd.name`.
        let target_params: Vec<MirTy> = fd
            .params
            .iter()
            .map(|p| self.resolve_ty(&p.ty))
            .collect::<Result<Vec<_>, _>>()?;
        let candidates = self
            .overloads
            .get(&fd.name)
            .cloned()
            .unwrap_or_default();
        let mangled = candidates
            .iter()
            .copied()
            .find(|m| {
                self.fn_sigs
                    .get(m)
                    .map(|s| s.params == target_params)
                    .unwrap_or(false)
            })
            .unwrap_or(fd.name);
        let sig = self
            .fn_sigs
            .get(&mangled)
            .cloned()
            .ok_or_else(|| LowerError::Other(format!("fn {} not pre-registered", fd.name)))?;
        let id = *self.fn_ids.get(&mangled).expect("declared above");

        let mut fb = FunctionBuilder::new(
            mangled,
            fd.name,
            sig.ret.clone(),
            FunctionKind::Local,
        );
        fb.set_span(fd.span);
        let entry = fb.new_block();
        fb.switch_to(entry);

        let mut env = Env::default();
        let mut params_box = Vec::with_capacity(fd.params.len());
        for (param, pty) in fd.params.iter().zip(sig.params.iter()) {
            let v = fb.add_block_param(entry, pty.clone());
            env.bind(param.name, v, pty.clone());
            params_box.push(FuncParam {
                name: param.name,
                ty: pty.clone(),
                value: v,
            });
        }

        let mut __cellify: std::collections::HashSet<Symbol> = std::collections::HashSet::new();
        collect_cellified_names_block(&fd.body, &mut __cellify);
        let mut bcx = BodyCx {
            fb: &mut fb,
            env: &mut env,
            ret_ty: sig.ret.clone(),
            fn_ids: &mut self.fn_ids,
            fn_sigs: &mut self.fn_sigs,
            loops: Vec::new(),
            this_class: None,
            classes: &self.classes,
            class_meta: &self.class_meta,
            interface_ids: &self.interface_ids,
            iface_method_slots: &self.iface_method_slots,
            iface_method_sigs: &self.iface_method_sigs,
            com_interfaces: &self.com_interfaces,
            com_iface_slots: &self.com_iface_slots,
            enum_ids: &self.enum_ids,
            enum_meta: &self.enum_meta,
            enums: &self.enums,
            statics: &self.statics,
            pending: &mut self.pending_closures,
            funcs: &mut self.funcs,
            anon_counter: &mut self.anon_counter,
            captures_in_scope: None,
            cell_captures: None,
            cellify_set: &__cellify,
            overloads: &self.overloads,
            repl_slots: &self.repl_slots,
            is_main_body: false,
            binding_self_name: None,
            crepr_owned_locals: std::collections::HashSet::new(),
        };
        let needs_retain = fd
            .body
            .tail
            .as_ref()
            .map(|e| bcx.callee_retain_decision(e))
            .unwrap_or(false);
        let tail = bcx.lower_block(&fd.body)?;
        if needs_retain {
            bcx.emit_callee_retain(&tail);
        }
        bcx.finalise_return(tail)?;

        let func = fb.finish(params_box.into_boxed_slice());
        self.funcs[id.0 as usize] = func;
        Ok(())
    }

    pub(in crate::lower) fn lower_main(&mut self, stmts: &[Stmt], tail: Option<&Expr>) -> Result<(), LowerError> {
        let main_name = Symbol::intern("__main");
        let mut fb = FunctionBuilder::new(main_name, main_name, MirTy::I64, FunctionKind::Local);
        let entry = fb.new_block();
        fb.switch_to(entry);

        let mut env = Env::default();

        // Lower statements then tail.
        let mut __cellify: std::collections::HashSet<Symbol> = std::collections::HashSet::new();
        for s in stmts {
            collect_cellified_names_stmt(s, &mut __cellify);
        }
        if let Some(t) = tail {
            collect_cellified_names_expr(t, &mut __cellify);
        }
        let mut bcx = BodyCx {
            fb: &mut fb,
            env: &mut env,
            ret_ty: MirTy::I64,
            fn_ids: &mut self.fn_ids,
            fn_sigs: &mut self.fn_sigs,
            loops: Vec::new(),
            this_class: None,
            classes: &self.classes,
            class_meta: &self.class_meta,
            interface_ids: &self.interface_ids,
            iface_method_slots: &self.iface_method_slots,
            iface_method_sigs: &self.iface_method_sigs,
            com_interfaces: &self.com_interfaces,
            com_iface_slots: &self.com_iface_slots,
            enum_ids: &self.enum_ids,
            enum_meta: &self.enum_meta,
            enums: &self.enums,
            statics: &self.statics,
            pending: &mut self.pending_closures,
            funcs: &mut self.funcs,
            anon_counter: &mut self.anon_counter,
            captures_in_scope: None,
            cell_captures: None,
            cellify_set: &__cellify,
            overloads: &self.overloads,
            repl_slots: &self.repl_slots,
            is_main_body: true,
            binding_self_name: None,
            crepr_owned_locals: std::collections::HashSet::new(),
        };
        for stmt in stmts {
            bcx.lower_stmt(stmt)?;
        }
        let tail_val = match tail {
            Some(expr) => Some(bcx.lower_expr(expr)?),
            None => None,
        };

        // Release every top-level heap-typed `let` in reverse-bind
        // order so deinit fires before the process exits — matches
        // the existing `release_globals_at_exit` semantics.
        let top_scope: Vec<(Symbol, Binding)> = bcx
            .env
            .scopes
            .first()
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
        for (_name, binding) in top_scope.into_iter().rev() {
            match binding {
                Binding::Local(lid, ty) if needs_release(&ty) => {
                    // CRepr Locals: only release the ones that own
                    // their backing buffer (mirrors the check in
                    // `release_top_scope_objects`). A `let p =
                    // r.origin` borrow stays alive with its
                    // parent struct.
                    if let MirTy::Object(cid) = &ty {
                        let layout = &bcx.classes[cid.0 as usize];
                        let is_crepr = matches!(
                            layout.repr,
                            crate::program::ClassRepr::CRepr
                                | crate::program::ClassRepr::CPacked
                                | crate::program::ClassRepr::CUnion
                        );
                        if is_crepr && !bcx.crepr_owned_locals.contains(&lid) {
                            continue;
                        }
                    }
                    let v = bcx.fb.new_value(ty.clone());
                    bcx.fb.push_inst(Inst::UseLocal { dst: v, local: lid });
                    bcx.fb.push_inst(Inst::Release { value: v });
                }
                Binding::Ssa(v, ty) if needs_release(&ty) => {
                    bcx.fb.push_inst(Inst::Release { value: v });
                }
                Binding::Cell(cell_v, ty) if needs_release(&ty) => {
                    let zero = bcx.const_int(MirTy::I64, 0);
                    let v = bcx.fb.new_value(ty.clone());
                    bcx.fb.push_inst(Inst::ArrayLoad {
                        dst: v,
                        arr: cell_v,
                        idx: zero,
                    });
                    bcx.fb.push_inst(Inst::Release { value: v });
                }
                _ => {}
            }
        }
        // Slot-backed top-level heap lets: balance the retain at
        // store-time with a release at process exit so any class
        // `deinit` fires before main returns. Emitted in
        // descending-slot order to mirror reverse-bind LIFO release.
        let mut slot_releases: Vec<(u32, MirTy)> = bcx
            .repl_slots
            .iter()
            .filter(|(_, (_, ty))| needs_release(ty))
            .map(|(_, (idx, ty))| (*idx, ty.clone()))
            .collect();
        slot_releases.sort_by(|a, b| b.0.cmp(&a.0));
        for (idx, ty) in slot_releases {
            let idx_v = bcx.const_int(MirTy::I64, idx as i64);
            let raw = bcx.fb.new_value(MirTy::I64);
            bcx.fb.push_inst(Inst::Call {
                dst: Some(raw),
                callee: FuncRef::Builtin(Symbol::intern("__repl_load_slot")),
                args: Box::new([idx_v]),
            });
            let v = bcx.i64_to_slot_value(raw, &ty)?;
            bcx.fb.push_inst(Inst::Release { value: v });
        }

        // __main returns `i64` (process exit code). If the program
        // tail is an i64 expression, return that; otherwise return 0.
        let ret_val = match tail_val {
            Some((v, MirTy::I64)) => v,
            _ => bcx.const_int(MirTy::I64, 0),
        };
        bcx.fb.set_terminator(Terminator::Return { value: Some(ret_val) });

        let func = fb.finish(Box::new([]));
        let id = FuncId(self.funcs.len() as u32);
        self.funcs.push(func);
        self.main_id = Some(id);
        Ok(())
    }
}
