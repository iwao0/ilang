//! Pre-registration for classes: `register_class` records the class
//! metadata + field layout, and `declare_class_methods` reserves a
//! `FuncId` for every instance / static method so call sites resolve
//! before the bodies are lowered.

use std::collections::HashMap;

use ilang_ast::{self as ast, ExprKind, Symbol};

use crate::inst::{FuncId, MirConst};
use crate::program::FunctionKind;
use crate::types::MirTy;

use super::super::utils::placeholder_function;
use super::super::{ClassMeta, FnSig, Lower, LowerError};

impl Lower {

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
                meta.add_method(name, fid, new_sig);
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
            // Inherit the parent's static-field slots so `Derived.count`
            // resolves to the SAME shared slot as `Base.count` (the
            // checker already inherited the field's type). A redeclared
            // child slot — rejected by the checker — would keep its own
            // via `or_insert`.
            let parent_static_slots: Vec<(Symbol, crate::inst::StaticSlotId)> = self
                .class_meta
                .get(&pid)
                .map(|m| m.static_slots.iter().map(|(n, s)| (*n, *s)).collect())
                .unwrap_or_default();
            let meta = self.class_meta.get_mut(&class_id).unwrap();
            for (name, slot) in parent_static_slots {
                meta.static_slots.entry(name).or_insert(slot);
            }
        }

        for m in cd.methods.iter() {
            // Generic methods are specialized by the AST
            // `monomorphize_methods` pass before MIR lowering — by
            // the time we get here, any surviving generic method
            // template is unreachable (no concrete call site
            // refers to its original name) and would just bloat
            // the MIR. Skip the template silently.
            if !m.type_params.is_empty() {
                continue;
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
            meta.add_method(m.name, id, FnSig { params, ret });
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
        } else if let Some(pid) = parent_id {
            // Swift-style deinit chaining: a class with its own deinit
            // whose ancestry also has one gets a synthesized wrapper
            // (own body first, then the nearest ancestor's drop fn).
            // The ancestor's drop_fn is itself already chained, so the
            // walk continues to the root. The wrapper — rather than a
            // tail-appended call inside the deinit body — keeps the
            // chain intact even when the body early-`return`s.
            let parent_drop = self.classes[pid.0 as usize].drop_fn;
            if parent_drop != FuncId(u32::MAX) {
                let own_drop = self.classes[class_id.0 as usize].drop_fn;
                let cname = self.classes[class_id.0 as usize].name;
                let mangled = Symbol::intern(&format!("{cname}.deinit$chain"));
                let mut fb = crate::builder::FunctionBuilder::new(
                    mangled,
                    Symbol::intern("deinit"),
                    MirTy::Unit,
                    FunctionKind::Drop { class: class_id },
                );
                let entry = fb.new_block();
                fb.switch_to(entry);
                let this_ty = MirTy::Object(class_id);
                let this_v = fb.add_block_param(entry, this_ty.clone());
                fb.push_inst(crate::inst::Inst::Call {
                    dst: None,
                    callee: crate::inst::FuncRef::Local(own_drop),
                    args: Box::new([this_v]),
                });
                fb.push_inst(crate::inst::Inst::Call {
                    dst: None,
                    callee: crate::inst::FuncRef::Local(parent_drop),
                    args: Box::new([this_v]),
                });
                fb.set_terminator(crate::inst::Terminator::Return {
                    value: None,
                    release_value: false,
                });
                let func = fb.finish(Box::new([crate::program::FuncParam {
                    name: Symbol::intern("this"),
                    ty: this_ty,
                    value: this_v,
                }]));
                let wid = FuncId(self.funcs.len() as u32);
                self.funcs.push(func);
                self.classes[class_id.0 as usize].drop_fn = wid;
            }
        }

        // Static methods — registered as top-level fns under
        // `Class.method`.
        for sm in cd.static_methods.iter() {
            // See the instance-method branch above — generic static
            // methods are likewise specialized at the AST level.
            if !sm.type_params.is_empty() {
                continue;
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
            meta.add_static_method(sm.name, id, FnSig { params, ret });
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
                    meta.add_static_method(body_name, id, FnSig { params, ret });
                } else {
                    // Synthesise unique keys for property getter/setter so
                    // they don't collide with each other or with regular
                    // methods of the same name.
                    let key = Symbol::intern(&format!("{}::get", prop.name));
                    meta.property_getter.insert(prop.name, (id, prop_ty.clone()));
                    meta.add_method(key, id, FnSig { params, ret });
                    // Make the getter a virtual method: replace any
                    // MethodDecl inherited under this key and push this
                    // class's own, so the slot loop gives it a vtable
                    // slot and a subclass override lands in the
                    // inherited slot (mirrors regular-method override).
                    method_decls.retain(|d| d.name != key);
                    method_decls.push(crate::program::MethodDecl {
                        name: key,
                        is_override: false,
                        is_static: false,
                        func: id,
                        slot: None,
                    });
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
                    meta.add_static_method(body_name, id, FnSig { params, ret });
                } else {
                    let key = Symbol::intern(&format!("{}::set", prop.name));
                    meta.property_setter.insert(prop.name, (id, prop_ty.clone()));
                    meta.add_method(key, id, FnSig { params, ret });
                    // Virtual setter — see the getter case above.
                    method_decls.retain(|d| d.name != key);
                    method_decls.push(crate::program::MethodDecl {
                        name: key,
                        is_override: false,
                        is_static: false,
                        func: id,
                        slot: None,
                    });
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
}
