//! `lower_expr` — the per-`ExprKind` dispatcher on `BodyCx`. Each
//! arm either emits the MIR for that expression shape directly or
//! delegates to one of the more specific lowerers (`lower_call` /
//! `lower_match` / `lower_new` / ...).

use ilang_ast::{Expr, ExprKind, Span, Symbol};

use crate::inst::{FieldId, FuncId, FuncRef, Inst, MirConst, ValueId};
use crate::types::MirTy;

/// MIR-side mirror of `print_kind.rs::print_kind_id` (codegen).
/// Kept here so the lowering of `new Set<T>()` can embed the PK_*
/// tag without pulling in the codegen crate. Numeric values must
/// match `crates/ilang-runtime/src/kind.rs` byte-for-byte.
fn print_kind_id_for(ty: &MirTy) -> i64 {
    match ty {
        MirTy::I64 | MirTy::Size | MirTy::SSize => 0,
        MirTy::U64 => 1,
        MirTy::I32 => 2,
        MirTy::U32 => 3,
        MirTy::I16 => 4,
        MirTy::U16 => 5,
        MirTy::I8 | MirTy::CChar => 6,
        MirTy::U8 => 7,
        MirTy::Bool => 8,
        MirTy::F64 => 9,
        MirTy::F32 => 10,
        MirTy::Str => 11,
        MirTy::Object(_) => 12,
        _ => -1,
    }
}

/// MIR-side mirror of codegen's `kind_tag_of`. Same KIND_* numeric
/// values as `crates/ilang-runtime/src/kind.rs`. Used by the
/// `new Map<MyClass, V>()` lowering to seed the map's value-side
/// release dispatch.
fn kind_id_for(ty: &MirTy) -> i64 {
    match ty {
        MirTy::Object(_) => 1,           // KIND_OBJECT
        MirTy::Array { .. } => 2,         // KIND_ARRAY
        MirTy::Optional(_) => 3,          // KIND_OPTIONAL
        MirTy::Tuple(_) => 4,             // KIND_TUPLE
        MirTy::Map { .. } => 5,           // KIND_MAP
        MirTy::Fn(_) => 6,                // KIND_CLOSURE
        MirTy::Str => 7,                  // KIND_STR
        MirTy::Enum { .. } => 8,          // KIND_ENUM
        MirTy::Promise(_) => 9,           // KIND_PROMISE
        MirTy::Set { .. } => 10,          // KIND_SET
        MirTy::Weak(_) => 11,             // KIND_WEAK
        _ => 0,                            // KIND_NONE (primitives)
    }
}

use super::{Binding, BodyCx, LowerError};

impl<'a> BodyCx<'a> {
    pub(super) fn lower_expr(&mut self, expr: &Expr) -> Result<(ValueId, MirTy), LowerError> {
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
            ExprKind::Template { parts } => self.lower_template(parts),
            ExprKind::Var(name) => self.lower_var_expr(*name),
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
                // Built-in `new Promise<T>(executor)` — schedules
                // the executor on the work-stealing pool with two
                // synthetic resolve/reject callbacks bound to the
                // freshly-allocated pending promise.
                if class.as_str() == "Promise"
                    && type_args.len() == 1
                    && args.len() == 1
                {
                    let inner = self.resolve_ty(&type_args[0])?;
                    let exec_is_fresh = self.is_fresh_object_expr(&args[0]);
                    let (exec_v, _exec_ty) = self.lower_expr(&args[0])?;
                    if !exec_is_fresh {
                        self.fb.push_inst(Inst::Retain { value: exec_v });
                    }
                    let kind = match &inner {
                        MirTy::Object(_) => 1,
                        MirTy::Array { .. } => 2,
                        MirTy::Optional(_) => 3,
                        MirTy::Tuple(_) => 4,
                        MirTy::Map { .. } => 5,
                        MirTy::Fn(_) => 6,
                        MirTy::Str => 7,
                        MirTy::Enum(_) => 8,
                        MirTy::Promise(_) => 9,
                        _ => 0,
                    };
                    let kind_v = self.const_int(MirTy::I64, kind);
                    // Float-kind of T — the runtime picks a resolve
                    // stub whose ABI matches the executor's
                    // `resolve: fn(T)` (a float T rides a float
                    // register; the i64 stub would read the env as
                    // the value).
                    let fk = match &inner {
                        MirTy::F32 => 1,
                        MirTy::F64 => 2,
                        _ => 0,
                    };
                    let fk_v = self.const_int(MirTy::I64, fk);
                    let prom_ty = MirTy::Promise(Box::new(inner));
                    let dst = self.fb.new_value(prom_ty.clone());
                    self.fb.push_inst(Inst::Call {
                        dst: Some(dst),
                        callee: FuncRef::Builtin(Symbol::intern("promise_with_executor")),
                        args: Box::new([exec_v, kind_v, fk_v]),
                    });
                    return Ok((dst, prom_ty));
                }
                // `new ObjCBlock(closure)` — pick a kind code based on
                // the closure's lowered MirTy and call the runtime
                // dispatcher `__ilang_make_objc_block(closure, kind)`.
                // Kinds are stable; they match `BlockKind` in
                // `ilang_runtime::objc_blocks`. New shapes append.
                if class.as_str() == "ObjCBlock" && args.len() == 1 {
                    let (closure_v, closure_ty) = self.lower_expr(&args[0])?;
                    let MirTy::Fn(ft) = closure_ty else {
                        return Err(LowerError::Other(
                            "new ObjCBlock(...) requires a fn closure argument"
                                .into(),
                        ));
                    };
                    // True when a MirTy is an `id`-shaped slot
                    // (NSObject subclass instance or its Optional
                    // wrapper). Both lower to a single i64 handle
                    // at the C ABI level but the ObjC encoding
                    // string differs — `@` for id vs `^v` for
                    // void*.
                    fn is_id_shaped(t: &MirTy) -> bool {
                        matches!(t, MirTy::Object(_))
                            || matches!(t, MirTy::Optional(inner)
                                if matches!(inner.as_ref(), MirTy::Object(_)))
                    }
                    let kind: i64 = match (ft.params.as_ref(), &ft.ret) {
                        ([], MirTy::Unit) => 0,
                        ([MirTy::I64], MirTy::Unit) => 1,
                        ([MirTy::I64], MirTy::I64) => 2,
                        ([MirTy::I64, MirTy::I64], MirTy::Unit) => 3,
                        ([MirTy::I64, MirTy::I64, MirTy::I64], MirTy::Unit) => 4,
                        ([MirTy::Bool], MirTy::Unit) => 5,
                        ([a, b], MirTy::Unit)
                            if is_id_shaped(a) && is_id_shaped(b) => 6,
                        _ => {
                            return Err(LowerError::Other(format!(
                                "new ObjCBlock(...) signature not yet supported: \
                                 expected one of fn(), fn(i64), fn(i64): i64, \
                                 fn(i64, i64), fn(i64, i64, i64), fn(bool), \
                                 fn(id-shaped, id-shaped); got {:?} -> {:?}",
                                ft.params, ft.ret
                            )));
                        }
                    };
                    let kind_v = self.const_int(MirTy::I64, kind);
                    let dst = self.fb.new_value(MirTy::I64);
                    self.fb.push_inst(Inst::Call {
                        dst: Some(dst),
                        callee: FuncRef::Builtin(Symbol::intern("make_objc_block")),
                        args: Box::new([closure_v, kind_v]),
                    });
                    return Ok((dst, MirTy::I64));
                }
                if class.as_str() == "Map" && type_args.len() == 2 && args.is_empty() {
                    let key = self.resolve_ty(&type_args[0])?;
                    let val = self.resolve_ty(&type_args[1])?;
                    let ty = MirTy::Map {
                        key: Box::new(key.clone()),
                        val: Box::new(val.clone()),
                    };
                    let dst = self.fb.new_value(ty.clone());
                    // Object key: take the parallel path Set uses —
                    // materialise the class's `equals` / `hashCode`
                    // addresses and call `$map.newObject(eq, hash)`.
                    // Skips the regular `NewMap` codegen which assumes
                    // the primitive Int/Str store layout.
                    if let MirTy::Object(class_id) = &key {
                        let meta = self
                            .class_meta
                            .get(class_id)
                            .expect("Map<MyClass, V>: missing class meta");
                        let eq_id = meta
                            .method_ids
                            .get(&Symbol::intern("equals"))
                            .copied()
                            .expect("Map<MyClass, V>: missing `equals` method id");
                        let hash_id = meta
                            .method_ids
                            .get(&Symbol::intern("hashCode"))
                            .copied()
                            .expect("Map<MyClass, V>: missing `hashCode` method id");
                        let eq_addr = self.fb.new_value(MirTy::I64);
                        self.fb.push_inst(Inst::FuncAddr { dst: eq_addr, func: eq_id });
                        let hash_addr = self.fb.new_value(MirTy::I64);
                        self.fb.push_inst(Inst::FuncAddr { dst: hash_addr, func: hash_id });
                        self.fb.push_inst(Inst::Call {
                            dst: Some(dst),
                            callee: FuncRef::Builtin(Symbol::intern("map_new_object")),
                            args: Box::new([eq_addr, hash_addr]),
                        });
                        // Set the value-side print kind + val_kind so
                        // `console.log` / cascade-release can find the
                        // right shape. Key side is hard-coded to
                        // PK_OBJECT inside the runtime.
                        let val_pk = print_kind_id_for(&val);
                        // PK_OBJECT = 12. Mirrors `runtime/src/kind.rs`.
                        let key_pk_v = self.const_int(MirTy::I64, 12);
                        let val_pk_v = self.const_int(MirTy::I64, val_pk);
                        self.fb.push_inst(Inst::Call {
                            dst: None,
                            callee: FuncRef::Builtin(Symbol::intern("map_set_print_kinds")),
                            args: Box::new([dst, key_pk_v, val_pk_v]),
                        });
                        let val_kind = kind_id_for(&val);
                        let val_kind_v = self.const_int(MirTy::I64, val_kind);
                        self.fb.push_inst(Inst::Call {
                            dst: None,
                            callee: FuncRef::Builtin(Symbol::intern("map_set_value_kind")),
                            args: Box::new([dst, val_kind_v]),
                        });
                        return Ok((dst, ty));
                    }
                    self.fb.push_inst(Inst::NewMap {
                        dst,
                        key,
                        val,
                        entries: Box::new([]),
                    });
                    return Ok((dst, ty));
                }
                // `new Set<T>()` — allocate via `$set.new` and tag the
                // element's print kind so `$print.set` can format
                // entries correctly. No bespoke `Inst::NewSet`: the
                // builder leans on the regular Call lowering path so
                // adding the type doesn't ripple into every MIR pass
                // that pattern-matches `Inst`.
                if class.as_str() == "Set" && type_args.len() == 1 && args.is_empty() {
                    let elem = self.resolve_ty(&type_args[0])?;
                    let ty = MirTy::Set { elem: Box::new(elem.clone()) };
                    let set_ptr = self.fb.new_value(ty.clone());
                    // Object element: route through `$set.newObject`
                    // with the class's `equals` / `hashCode` method
                    // addresses. ARC retain / release on stored
                    // elements is handled inside the runtime via
                    // `__retain_object` / `__release_object`, so the
                    // user-side protocol stays the two-method
                    // `equals` + `hashCode` pair.
                    if let MirTy::Object(class_id) = &elem {
                        let meta = self
                            .class_meta
                            .get(class_id)
                            .expect("Set<MyClass>: missing class meta — type-check should have caught this");
                        let eq_id = meta
                            .method_ids
                            .get(&Symbol::intern("equals"))
                            .copied()
                            .expect("Set<MyClass>: missing `equals` method id");
                        let hash_id = meta
                            .method_ids
                            .get(&Symbol::intern("hashCode"))
                            .copied()
                            .expect("Set<MyClass>: missing `hashCode` method id");
                        let eq_addr = self.fb.new_value(MirTy::I64);
                        self.fb.push_inst(Inst::FuncAddr {
                            dst: eq_addr,
                            func: eq_id,
                        });
                        let hash_addr = self.fb.new_value(MirTy::I64);
                        self.fb.push_inst(Inst::FuncAddr {
                            dst: hash_addr,
                            func: hash_id,
                        });
                        self.fb.push_inst(Inst::Call {
                            dst: Some(set_ptr),
                            callee: FuncRef::Builtin(Symbol::intern("set_new_object")),
                            args: Box::new([eq_addr, hash_addr]),
                        });
                        // Object sets carry PK_OBJECT internally and
                        // ignore the `setElemPrintKind` call (see the
                        // runtime guard in `__set_set_elem_print_kind`);
                        // skip the call so we don't add a useless
                        // instruction here.
                        return Ok((set_ptr, ty));
                    }
                    self.fb.push_inst(Inst::Call {
                        dst: Some(set_ptr),
                        callee: FuncRef::Builtin(Symbol::intern("set_new")),
                        args: Box::new([]),
                    });
                    let pk = print_kind_id_for(&elem);
                    let pk_v = self.const_int(MirTy::I64, pk);
                    self.fb.push_inst(Inst::Call {
                        dst: None,
                        callee: FuncRef::Builtin(Symbol::intern("set_set_elem_print_kind")),
                        args: Box::new([set_ptr, pk_v]),
                    });
                    return Ok((set_ptr, ty));
                }
                if !type_args.is_empty() {
                    return Err(LowerError::Unsupported("generic class instantiation"));
                }
                self.lower_new(*class, args, *init_method)
            }
            ExprKind::AssignField { obj, field, value, is_init } => {
                // `ClassName.name = v` — static setter takes
                // precedence over static slot writes, mirroring the
                // read-side precedence in `lower_field`.
                if let ExprKind::Var(maybe_class) = &obj.kind {
                    if self.lookup_var(*maybe_class).is_none() {
                        if let Some(cid) =
                            super::class_id_by_name(self.classes, self.class_meta, *maybe_class)
                        {
                            let meta = self.class_meta.get(&cid).unwrap();
                            if let Some((fid, prop_ty)) =
                                meta.static_property_setter.get(field).cloned()
                            {
                                let value_is_fresh = self.is_fresh_object_expr(value);
                                let (coerced, vty) = self.lower_arg_to(value, Some(&prop_ty))?;
                                self.fb.push_inst(Inst::Call {
                                    dst: None,
                                    callee: crate::inst::FuncRef::Local(fid),
                                    args: Box::new([coerced]),
                                });
                                // The setter body stores through the
                                // usual retain-on-store paths — drop a
                                // fresh value's transient +1 here.
                                if value_is_fresh && self.is_arc_heap(&vty) {
                                    self.fb.push_inst(Inst::Release { value: coerced });
                                }
                                return Ok((self.const_unit(), MirTy::Unit));
                            }
                            if let Some(&slot) = meta.static_slots.get(field) {
                                let s = self.statics_by_id(slot);
                                if s.is_const && !*is_init {
                                    return Err(LowerError::Other(format!(
                                        "cannot assign to const {field}"
                                    )));
                                }
                                let (coerced, _) = self.lower_arg_to(value, Some(&s.ty))?;
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
                    let value_is_fresh = self.is_fresh_object_expr(value);
                    let (coerced, vty) = self.lower_arg_to(value, Some(&prop_ty))?;
                    self.fb.push_inst(Inst::Call {
                        dst: None,
                        callee: FuncRef::Local(fid),
                        args: Box::new([ov, coerced]),
                    });
                    // The setter body stores its borrowed param
                    // through AssignField, which takes its own retain
                    // — a fresh value's transient +1 drops here, or
                    // `h.prop = new Box(..)` leaks one per call.
                    if value_is_fresh && self.is_arc_heap(&vty) {
                        self.fb.push_inst(Inst::Release { value: coerced });
                    }
                    return Ok((self.const_unit(), MirTy::Unit));
                }
                let fid = *meta
                    .field_ix
                    .get(field)
                    .ok_or_else(|| LowerError::Other(format!("no field {field}")))?;
                let fty = meta.field_ty.get(&fid).cloned().unwrap_or(MirTy::I64);
                let src_is_fresh = self.is_fresh_object_expr(value);
                self.last_block_tail_owned = false;
                let (vv0, vty) = match self.lower_composite_with_hint(value, &fty) {
                    Some(res) => res?,
                    None => self.lower_expr(value)?,
                };
                let src_owned = src_is_fresh || self.last_block_tail_owned;
                // Coerce the rhs to the field's declared type — this
                // is where `T → T?` Optional auto-wrap fires for
                // `this.f = expr` / `obj.f = expr` (struct-literal
                // and `init`-time `this.f = ...` paths already do
                // this a few hundred lines down). Without it, a
                // raw `fn()` value gets stored into a `fn()?` slot
                // and later `if let some(x) = obj.f` reads garbage.
                // Only the `T → T?` Optional auto-wrap shape is
                // coerced here. Other shape mismatches (e.g. existing
                // `Optional<Object>` → `Optional<Weak<_>>` field
                // assignment, which the codegen handles by reusing
                // the raw heap pointer) pass through unchanged so
                // we don't regress those paths.
                self.store_value_to_field(
                    ov, fid, &fty, vv0, vty, src_is_fresh, src_owned, *is_init, value.span,
                )?;
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
                // Pattern: `s = s + expr` with both sides typed as
                // string. The MIR Local for `s` is provably the only
                // holder of its buffer (assignment retires the
                // previous pointer), so route the concat through the
                // inplace runtime helper that grows `s`'s backing
                // via doubling realloc instead of allocating a fresh
                // buffer every iteration. Bypassed for closure
                // captures (cell-backed bindings) where alias
                // reasoning is harder.
                if let ExprKind::Binary { op: ilang_ast::BinOp::Add, lhs, rhs } = &value.kind {
                    if let ExprKind::Var(lname) = &lhs.kind {
                        if lname == target
                            && matches!(
                                self.env.lookup_binding(*target),
                                Some(Binding::Local(_, MirTy::Str))
                            )
                        {
                            let (lv, lty) = self.lower_expr(lhs)?;
                            let (rv, rty) = self.lower_expr(rhs)?;
                            if matches!(lty, MirTy::Str) && matches!(rty, MirTy::Str) {
                                let tmp = self.fb.new_value(MirTy::Str);
                                self.fb.push_inst(Inst::BinOp {
                                    dst: tmp,
                                    op: crate::inst::BinOp::StrConcatInplace,
                                    lhs: lv,
                                    rhs: rv,
                                });
                                if self.assign_var(*target, tmp, MirTy::Str) {
                                    return Ok((self.const_unit(), MirTy::Unit));
                                }
                            }
                        }
                    }
                }
                let value_is_fresh = self.is_fresh_object_expr(value);
                // Snapshot the old value when the target slot owns rc.
                // The authoritative slot-rc kind list is `MirTy::is_heap`
                // (mirrored by `release_field_by_kind` in
                // runtime/cascade.rs and the scope-exit `needs_release`
                // lists in this crate). Object-only was a longstanding
                // gap — every other heap kind leaked the previous
                // slot's value on `xs = ...` reassignment.
                let old_obj_ty: Option<(ValueId, MirTy)> = self
                    .env
                    .lookup_binding(*target)
                    .and_then(|b| match b {
                        Binding::Local(lid, ty) if self.is_arc_slot(&ty) => {
                            let v = self.fb.new_value(ty.clone());
                            self.fb.push_inst(Inst::UseLocal { dst: v, local: lid });
                            Some((v, ty))
                        }
                        Binding::Cell(cell_v, ty) if self.is_arc_slot(&ty) => {
                            let zero = self.const_int(MirTy::I64, 0);
                            let v = self.fb.new_value(ty.clone());
                            self.fb.push_inst(Inst::ArrayLoad {
                                dst: v,
                                arr: cell_v,
                                idx: zero,
                            });
                            Some((v, ty))
                        }
                        _ => None,
                    });
                let target_ty = match self.env.lookup_binding(*target) {
                    Some(Binding::Ssa(_, ty))
                    | Some(Binding::PatternBinding(_, ty, _))
                    | Some(Binding::Local(_, ty))
                    | Some(Binding::Cell(_, ty)) => Some(ty),
                    None => self.repl_slots.get(target).map(|(_, ty)| ty.clone()),
                };
                self.last_block_tail_owned = false;
                let (v, vty) = match target_ty
                    .as_ref()
                    .and_then(|t| self.lower_composite_with_hint(value, t))
                {
                    Some(res) => res?,
                    None => self.lower_expr(value)?,
                };
                // Coerce up-front so retain dispatches on the slot's
                // kind, not the source's. `let w: T.weak = c` where
                // `c: T` needs `__retain_weak` (a weak-table bump) —
                // a raw `Retain(v)` on the source `Object` would
                // bump strong rc instead, leaking the strong owner.
                let (v_slot, slot_ty) = match target_ty.as_ref() {
                    Some(t) if t != &vty => {
                        let coerced = self
                            .coerce(v, &vty, t, value.span)
                            .unwrap_or(v);
                        // Owned source wrapped into T? / T.weak —
                        // drop its share (see
                        // release_owned_wrap_source).
                        if coerced != v {
                            let owned = value_is_fresh || self.last_block_tail_owned;
                            self.release_owned_wrap_source(v, &vty, t, owned);
                        }
                        (coerced, t.clone())
                    }
                    Some(t) => (v, t.clone()),
                    None => (v, vty.clone()),
                };
                // Mirror stmt-let: a wrap coerce minted a fresh
                // cell — don't retain it a second time.
                let value_is_fresh =
                    value_is_fresh || (v_slot != v && self.is_arc_slot(&slot_ty));
                if self.assign_var(*target, v_slot, slot_ty.clone()) {
                    if self.is_arc_slot(&slot_ty) {
                        if !value_is_fresh {
                            self.fb.push_inst(Inst::Retain { value: v_slot });
                        }
                        if let Some((old, _)) = old_obj_ty {
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
                            let heap_slot = self.is_arc_slot(&cty);
                            if heap_slot {
                                let old = self.fb.new_value(cty.clone());
                                self.fb.push_inst(Inst::ArrayLoad {
                                    dst: old,
                                    arr: cell_v,
                                    idx: zero,
                                });
                                self.fb.push_inst(Inst::Release { value: old });
                                // Fresh rhs already owns its +1 — letting
                                // it ride into the cell as the cell's
                                // share keeps the accounting tight. The
                                // tail-Var Retain in `lower_block_hinted`
                                // covers the symmetric case where the
                                // closure body returns the captured cell
                                // via a `Var` tail (it now treats Cell
                                // captures the same as `Binding::Cell`,
                                // adding the caller's +1).
                                if !value_is_fresh {
                                    self.fb.push_inst(Inst::Retain { value: v });
                                }
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
                    let is_heap = self.is_arc_slot(&slot_ty);
                    // Snapshot the prior slot value so the old heap
                    // owner gets released after the new value lands.
                    let old_v = if is_heap {
                        let idx_v = self.const_int(MirTy::I64, idx as i64);
                        let raw = self.fb.new_value(MirTy::I64);
                        self.fb.push_inst(Inst::Call {
                            dst: Some(raw),
                            callee: FuncRef::Builtin(Symbol::intern("$repl.loadSlot")),
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
                        callee: FuncRef::Builtin(Symbol::intern("$repl.storeSlot")),
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
                        // Resolve `this` the same way the bare-field
                        // *read* path does: a method-internal closure
                        // sees `this` only through its captures, not as
                        // a local. Without the capture fallback this
                        // panicked on `Option::unwrap()` whenever a
                        // bare field was assigned inside a closure
                        // (the read path already handled it).
                        let fty = meta.field_ty.get(&fid).cloned().unwrap_or(MirTy::I64);
                        // Resolve `this` the same way the bare-field
                        // *read* path does: a method-internal closure
                        // sees `this` only through its captures, not as
                        // a local. Without the capture fallback this
                        // panicked on `Option::unwrap()` whenever a
                        // bare field was assigned inside a closure
                        // (the read path already handled it).
                        let this_sym = Symbol::intern("this");
                        let (this_v, _) = if let Some(pair) = self.lookup_var(this_sym) {
                            pair
                        } else if let Some(caps) = self.captures_in_scope {
                            let &(cap_idx, ref this_ty) = caps.get(&this_sym).expect(
                                "class method closure must capture `this` for implicit field assignment",
                            );
                            let dst = self.fb.new_value(this_ty.clone());
                            self.fb.push_inst(Inst::LoadCapture { dst, idx: cap_idx });
                            (dst, this_ty.clone())
                        } else {
                            return Err(LowerError::Other(format!(
                                "cannot resolve `this` for implicit field assignment `{target}`"
                            )));
                        };
                        // Route the already-lowered rhs (`v` : `vty`,
                        // produced above without a field hint since the
                        // target binding lookup returned no type) through
                        // the shared field-store helper. This gives the
                        // bare write the same `T → T?` / weak /
                        // fixed-array wrap semantics as `this.f = v` —
                        // without it, `slot = box` against a `Box?` field
                        // stored a raw object into the Optional slot and
                        // crashed on release. `last_block_tail_owned`
                        // still reflects that lowering.
                        let src_owned = value_is_fresh || self.last_block_tail_owned;
                        self.store_value_to_field(
                            this_v, fid, &fty, v, vty.clone(), value_is_fresh, src_owned,
                            false, value.span,
                        )?;
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
                // Use the actual MIR type the coerce produced — for
                // `RawPtr → Type::Fn` we tagged the value as RawFn
                // (8-byte raw fn ptr) instead of the AST-derived Fn
                // (16-byte closure box). Returning `dst_ty` would lose
                // that tag and break the next call-site dispatch.
                let actual_ty = self.fb.ty_of(out).clone();
                Ok((out, actual_ty))
            }
            ExprKind::TypeTest { expr: inner, ty } => {
                let value_is_fresh = self.is_fresh_object_expr(inner);
                let (v, vty) = self.lower_expr(inner)?;
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
                // `is` only reads the class id — a fresh operand's
                // transient +1 drops here (`makeB() is B` leaked the
                // whole object per call).
                if value_is_fresh && self.is_arc_heap(&vty) {
                    self.fb.push_inst(Inst::Release { value: v });
                }
                Ok((dst, MirTy::Bool))
            }
            ExprKind::TypeDowncast { expr: inner, ty } => {
                let value_is_fresh = self.is_fresh_object_expr(inner);
                let (v, vty) = self.lower_expr(inner)?;
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
                // The Optional cell takes its own +1 on the hit path
                // (and on a miss the operand is untouched) — a fresh
                // operand's transient +1 drops here either way.
                if value_is_fresh && self.is_arc_heap(&vty) {
                    self.fb.push_inst(Inst::Release { value: v });
                }
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
            ExprKind::Await(_) => {
                return Err(LowerError::Other(
                    "`await` outside an `async fn` body — desugar pass should have eliminated it".into(),
                ));
            }
            ExprKind::Some(inner) => {
                let value_is_fresh = self.is_fresh_object_expr(inner);
                let (iv0, ity) = self.lower_expr(inner)?;
                // Fixed-length heap-element array inner — the cell
                // takes a VALUE COPY (no rc to share; storing the
                // pointer would double-own the source's buffer).
                let iv = if let Some(copy) = self.copy_fixed_for_cell(iv0, &ity, value_is_fresh) {
                    copy
                } else {
                    // `some(arr)` where `arr` is an aliased Var that the
                    // surrounding scope is about to release — bump the
                    // inner's rc so the Optional doesn't dangle once the
                    // source binding's scope-exit Release fires. With
                    // host_release_array now actually freeing memory at
                    // rc==0, omitting this retain caused use-after-free
                    // in some_aliased_inner.il.
                    let needs_retain = !value_is_fresh && self.is_arc_slot(&ity);
                    if needs_retain {
                        self.fb.push_inst(Inst::Retain { value: iv0 });
                    }
                    iv0
                };
                let ty = MirTy::Optional(Box::new(ity.clone()));
                let v = self.fb.new_value(ty.clone());
                self.fb.push_inst(Inst::NewOptional { dst: v, value: iv });
                Ok((v, ty))
            }
            ExprKind::Index { obj, index } => self.lower_index(obj, index),
            ExprKind::AssignIndex { obj, index, value } => {
                let value_is_fresh = self.is_fresh_object_expr(value);
                let index_is_fresh = self.is_fresh_object_expr(index);
                let (av, aty) = self.lower_expr(obj)?;
                let (iv, ity) = self.lower_expr(index)?;
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
                        //
                        // Coerce before retain so a `T → T.weak` store
                        // (`arr: T.weak[]; arr[i] = strong_t`) emits
                        // `__retain_weak`, not the strong-rc bump that
                        // would orphan the strong owner.
                        let vv_slot = if vty != elem_ty {
                            self.coerce(vv, &vty, &elem_ty, value.span).unwrap_or(vv)
                        } else {
                            vv
                        };
                        // A `T → T?` / `T → T.weak` coerce mints a fresh
                        // wrapper cell that already owns its +1 (coerce
                        // retained the inner). Treat it like a fresh
                        // value: drop the owned source's share, skip the
                        // aliased retain below — otherwise `arr[i] = box`
                        // against `T?[]` leaked the source (fresh) or
                        // double-counted the cell (borrowed).
                        let wrapped = vv_slot != vv;
                        let elem_is_heap = self.is_arc_slot(&elem_ty);
                        // Fixed-length-array elements have value
                        // semantics: the cell takes a copy on store.
                        let vv_slot = match self.copy_fixed_for_cell(vv_slot, &elem_ty, value_is_fresh) {
                            Some(copy) => copy,
                            None => {
                                if wrapped {
                                    self.release_owned_wrap_source(
                                        vv, &vty, &elem_ty, value_is_fresh,
                                    );
                                } else if elem_is_heap && !value_is_fresh {
                                    self.fb.push_inst(Inst::Retain { value: vv_slot });
                                }
                                vv_slot
                            }
                        };
                        if elem_is_heap {
                            let old = self.fb.new_value(elem_ty.clone());
                            self.fb.push_inst(Inst::ArrayLoad {
                                dst: old,
                                arr: av,
                                idx: iv,
                            });
                            self.fb.push_inst(Inst::Release { value: old });
                        }
                        self.fb.push_inst(Inst::ArrayStore { arr: av, idx: iv, value: vv_slot });
                    }
                    MirTy::Map { ref val, .. } => {
                        // Coerce the value to the map's declared V before
                        // the store — the index-assign sugar must match
                        // `m.set(k, v)` (which routes through
                        // `lower_arg_to`). Without this, `m[k] = new Box`
                        // against `Map<_, Box?>` stored a raw Box where a
                        // `Box?` cell was expected, so `$map.release`
                        // later cascade-released it under the Optional
                        // kind and dereferenced a garbage inner pointer.
                        let val_ty = (**val).clone();
                        let vv_slot = if vty != val_ty {
                            self.coerce(vv, &vty, &val_ty, value.span).unwrap_or(vv)
                        } else {
                            vv
                        };
                        // A `T → T?` / `T → T.weak` coerce mints a fresh
                        // wrapper cell (`coerce` already retained the
                        // inner). `host_map_set` then retains the cell
                        // too, so we hold a transient +1 on it.
                        let wrapped = vv_slot != vv;
                        self.fb.push_inst(Inst::MapSet { map: av, key: iv, value: vv_slot });
                        // Map takes its own +1 share on both key and
                        // value via host_map_set's retains. For a fresh
                        // key / value the caller also holds a transient
                        // +1 from the source expression — release it
                        // here so the only remaining share is the map's.
                        // Borrowed values (use_local etc.) leave their
                        // slot's share to be dropped by scope-exit
                        // release as usual.
                        if index_is_fresh && self.is_arc_heap(&ity) {
                            self.fb.push_inst(Inst::Release { value: iv });
                        }
                        if wrapped {
                            // Drop the owned source's +1 (fresh only —
                            // a borrowed source keeps its share, and
                            // `coerce`'s inner-retain is the cell's).
                            self.release_owned_wrap_source(
                                vv, &vty, &val_ty, value_is_fresh,
                            );
                            // Release the transient share on the fresh
                            // wrapper cell left after host_map_set's retain.
                            if self.is_arc_heap(&val_ty) {
                                self.fb.push_inst(Inst::Release { value: vv_slot });
                            }
                        } else if value_is_fresh && self.is_arc_heap(&vty) {
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
            ExprKind::StructLit { class, fields, .. } => {
                // Aggregate literal — for `@extern(C) struct` /
                // top-level `struct` / `union` (zero-init heap slot
                // then store each field) and for ARC classes (the
                // looser literal form that bypasses `init`). The
                // type checker has already validated field set and
                // types; here we just emit the construction +
                // per-field stores.
                let class_id = super::class_id_by_name(self.classes, self.class_meta, *class)
                    .ok_or_else(|| LowerError::Other(format!("unknown class {class}")))?;
                let dst = self.fb.new_value(MirTy::Object(class_id));
                self.fb.push_inst(Inst::NewObject {
                    dst,
                    class: class_id,
                    init_args: Box::new([]),
                    init: FuncId(u32::MAX),
                });
                let class_is_crepr = matches!(
                    self.classes[class_id.0 as usize].repr,
                    crate::program::ClassRepr::CRepr
                        | crate::program::ClassRepr::CPacked
                        | crate::program::ClassRepr::CUnion
                );
                for (fname, fval) in fields.iter() {
                    let meta = self.class_meta.get(&class_id).unwrap();
                    let fid = *meta.field_ix.get(fname).ok_or_else(|| {
                        LowerError::Other(format!("no field {fname} on {class}"))
                    })?;
                    let fty = meta.field_ty.get(&fid).cloned().unwrap();
                    // Fast path: a bare top-level fn name assigned to a
                    // `fn(...)` field of an `@extern(C)` struct must
                    // produce the raw 8-byte code address, not a
                    // closure box. C code dereferences the slot as a
                    // function pointer; a closure header would crash.
                    if class_is_crepr {
                        if let MirTy::Fn(_) = &fty {
                            if let ExprKind::Var(name) = &fval.kind {
                                if let Some(&top_fid) = self.fn_ids.get(name) {
                                    let dst_v = self.fb.new_value(fty.clone());
                                    self.fb.push_inst(Inst::FuncAddr {
                                        dst: dst_v,
                                        func: top_fid,
                                    });
                                    self.fb.push_inst(Inst::StoreField {
                                        obj: dst,
                                        field: fid,
                                        value: dst_v,
                                    });
                                    continue;
                                }
                            }
                        }
                    }
                    let value_is_fresh = self.is_fresh_object_expr(fval);
                    // Push the field's declared type into composite
                    // literals on the RHS. This covers fixed-length
                    // array fields (`pos: f32[3]`) — which need the
                    // inline, header-less layout `StoreField` memcpys
                    // from — as well as dynamic packed arrays and
                    // narrowed tuples, all of which would otherwise
                    // default to i64/f64 cells.
                    let (coerced, _) = self.lower_arg_to(fval, Some(&fty))?;
                    // ARC retain for heap-typed fields: same rule as
                    // AssignField. The slot started at zero (fresh
                    // alloc) so there is no prior occupant to
                    // release. CRepr structs / unions can't hold
                    // these field types — the type-checker rejects
                    // them — so this branch only fires on ARC class
                    // literals.
                    if self.is_arc_slot(&fty) && !value_is_fresh {
                        self.fb.push_inst(Inst::Retain { value: coerced });
                    }
                    self.fb.push_inst(Inst::StoreField { obj: dst, field: fid, value: coerced });
                }
                Ok((dst, MirTy::Object(class_id)))
            }
            // M1 is feature-complete — every variant of `ExprKind`
            // is handled above. Removed the legacy catch-all because
            // the compiler now flags it as `unreachable_pattern`.
        }
    }

    pub(super) fn lower_template(
        &mut self,
        parts: &[ilang_ast::TemplatePart],
    ) -> Result<(ValueId, MirTy), LowerError> {
        // Emit each part as a string-typed value, then fold them all
        // together via `str_concat`. For interpolated expressions we
        // route through the `fmt_value` builtin; the codegen layer
        // looks at the MIR type of the value to pick the right host
        // conversion (mirroring `console.log`'s emit_print_value).
        //
        // rc bookkeeping: `fmt_value` always mints a fresh registry
        // string (even for Str input — `$fmt.str` copies), and so
        // does every `str_concat`. Those transients drop right after
        // the concat that consumed them; only interned `Const` string
        // literals (the part literals and the leading "") must not be
        // released. The final `acc` keeps its +1 — `Template` is
        // classified fresh in `is_fresh_object_expr`, so the consumer
        // releases it like any other fresh string.
        let empty = self.fb.new_value(MirTy::Str);
        self.fb.push_inst(Inst::Const {
            dst: empty,
            value: MirConst::Str(Symbol::intern("")),
        });
        let mut acc = empty;
        let mut acc_is_fresh = false;
        for part in parts {
            let (piece, piece_is_fresh) = match part {
                ilang_ast::TemplatePart::Str(s) => {
                    let v = self.fb.new_value(MirTy::Str);
                    self.fb.push_inst(Inst::Const {
                        dst: v,
                        value: MirConst::Str(Symbol::intern(s)),
                    });
                    (v, false)
                }
                ilang_ast::TemplatePart::Expr(e) => {
                    let part_is_fresh = self.is_fresh_object_expr(e);
                    let (val, val_ty) = self.lower_expr(e)?;
                    let s = self.fb.new_value(MirTy::Str);
                    self.fb.push_inst(Inst::Call {
                        dst: Some(s),
                        callee: FuncRef::Builtin(Symbol::intern("fmt_value")),
                        args: Box::new([val]),
                    });
                    // `fmt_value` only reads the value (Str input is
                    // copied) — a fresh part's transient +1 drops
                    // here, or `${`inner${x}`}` / `${"a" + b}`
                    // leaked the inner string per evaluation.
                    if part_is_fresh && self.is_arc_heap(&val_ty) {
                        self.fb.push_inst(Inst::Release { value: val });
                    }
                    (s, true)
                }
            };
            let next = self.fb.new_value(MirTy::Str);
            self.fb.push_inst(Inst::Call {
                dst: Some(next),
                callee: FuncRef::Builtin(Symbol::intern("str_concat")),
                args: Box::new([acc, piece]),
            });
            if acc_is_fresh {
                self.fb.push_inst(Inst::Release { value: acc });
            }
            if piece_is_fresh {
                self.fb.push_inst(Inst::Release { value: piece });
            }
            acc = next;
            acc_is_fresh = true;
        }
        // Zero-part template: `acc` is still the interned "" literal,
        // which the consumer must not release. Mint a fresh copy so
        // the fresh-Template contract holds unconditionally.
        if !acc_is_fresh {
            let s = self.fb.new_value(MirTy::Str);
            self.fb.push_inst(Inst::Call {
                dst: Some(s),
                callee: FuncRef::Builtin(Symbol::intern("fmt_value")),
                args: Box::new([acc]),
            });
            acc = s;
        }
        Ok((acc, MirTy::Str))
    }

    fn lower_var_expr(&mut self, name: Symbol) -> Result<(ValueId, MirTy), LowerError> {
        if let Some(found) = self.lookup_var(name) {
            return Ok(found);
        }
        // Implicit `this.field` resolves before module-level repl_slots
        // so a same-named module `let foo = ...` doesn't hijack a class
        // field also named `foo` (the usual "class members shadow
        // globals" rule). `this` itself lives in env for a regular
        // method body and in `captures_in_scope` for a method-internal
        // closure body — try both. The duplicate Var-as-field branch
        // further down stays for the no-captures top-level fn case
        // where `this_class` is unset; it's a no-op when this branch
        // already returned.
        if let Some(cid) = self.this_class {
            let meta = self.class_meta.get(&cid).expect("class meta");
            if let Some(&fid) = meta.field_ix.get(&name) {
                let this_sym = Symbol::intern("this");
                let (this_v, _) = if let Some(pair) = self.lookup_var(this_sym) {
                    pair
                } else if let Some(caps) = self.captures_in_scope {
                    let &(cap_idx, ref this_ty) = caps
                        .get(&this_sym)
                        .expect("class method closure must capture `this` for implicit field ref");
                    let dst = self.fb.new_value(this_ty.clone());
                    self.fb.push_inst(Inst::LoadCapture { dst, idx: cap_idx });
                    (dst, this_ty.clone())
                } else {
                    return Err(LowerError::Other(format!(
                        "cannot resolve `this` for implicit field reference `{name}`"
                    )));
                };
                let meta_fty = meta.field_ty.get(&fid).cloned().unwrap();
                let fty = super::BodyCx::loaded_field_ty(&meta_fty);
                let v = self.fb.new_value(fty.clone());
                self.fb.push_inst(Inst::LoadField { dst: v, obj: this_v, field: fid });
                return Ok((v, fty));
            }
        }
        // Closure capture takes precedence over a same-named module
        // slot — a closure that captured `factor` when it was 10 must
        // keep seeing 10 even if the outer code reassigned the slot
        // to 999. Slot reads borrow ownership from the slot itself
        // (the host store keeps a stable refcount on the slot's heap
        // value). We deliberately do NOT retain here — that mirrors
        // `lookup_var`'s contract for Local reads and avoids the
        // per-loop-iteration leak in long-running programs (e.g.
        // `examples/sdl_breakout`'s game loop). Downstream
        // `let`-binding / fn-arg / closure-capture sites bump the
        // refcount when they need persistent ownership.
        if let Some(caps) = self.captures_in_scope {
            if caps.contains_key(&name) {
                // Fall through to the existing capture handler below.
            } else if let Some((idx, slot_ty)) = self.repl_slots.get(&name).cloned() {
                let idx_v = self.const_int(MirTy::I64, idx as i64);
                let raw = self.fb.new_value(MirTy::I64);
                self.fb.push_inst(Inst::Call {
                    dst: Some(raw),
                    callee: FuncRef::Builtin(Symbol::intern("$repl.loadSlot")),
                    args: Box::new([idx_v]),
                });
                let v = self.i64_to_slot_value(raw, &slot_ty)?;
                return Ok((v, slot_ty));
            }
        } else if let Some((idx, slot_ty)) = self.repl_slots.get(&name).cloned() {
            // Non-closure context (regular fn body or `__main`
            // itself): always go through the slot.
            let idx_v = self.const_int(MirTy::I64, idx as i64);
            let raw = self.fb.new_value(MirTy::I64);
            self.fb.push_inst(Inst::Call {
                dst: Some(raw),
                callee: FuncRef::Builtin(Symbol::intern("$repl.loadSlot")),
                args: Box::new([idx_v]),
            });
            let v = self.i64_to_slot_value(raw, &slot_ty)?;
            return Ok((v, slot_ty));
        }
        // Self-recursive closure: the body's reference to its own
        // (non-slot) binding name is the running closure itself —
        // materialise the hidden env param instead of a capture
        // (a capture would snapshot an unbuilt value or retain-cycle
        // through a cell). Borrowed: the caller's share keeps the
        // closure alive for the duration of the call.
        if let Some((sname, sty)) = self.closure_self.clone() {
            if name == sname {
                let v = self.fb.new_value(sty.clone());
                self.fb.push_inst(Inst::ClosureSelf { dst: v });
                return Ok((v, sty));
            }
        }
        // Closure capture (only when lowering a closure body).
        if let Some(caps) = self.captures_in_scope {
            if let Some((idx, cty)) = caps.get(&name).cloned() {
                let is_cell = self
                    .cell_captures
                    .map(|s| s.contains(&name))
                    .unwrap_or(false);
                if is_cell {
                    // Capture slot holds a cell pointer (i64 1-elem
                    // array). Load the pointer, then dereference to
                    // get the inner value.
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
        // Top-level fn used as a value: produce a trampoline closure
        // with no captures.
        if let Some(&fid) = self.fn_ids.get(&name) {
            let sig = self.fn_sigs.get(&name).cloned().unwrap();
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
        // Implicit `this.field` — method-ref-without-call is not
        // supported in M1.
        if let Some(cid) = self.this_class {
            let meta = self.class_meta.get(&cid).expect("class meta");
            if let Some(&fid) = meta.field_ix.get(&name) {
                // `this` lives in `env` for a regular method body,
                // but in a method-internal closure it's a capture
                // (the `E::This` arm above walks captures_in_scope
                // for the same reason). Try env first; fall back to
                // a `LoadCapture` if the surrounding closure captured
                // `this`.
                let this_sym = Symbol::intern("this");
                let (this_v, _) = if let Some(pair) = self.lookup_var(this_sym) {
                    pair
                } else if let Some(caps) = self.captures_in_scope {
                    let &(cap_idx, ref this_ty) = caps
                        .get(&this_sym)
                        .expect("class method closure must capture `this` for implicit field ref");
                    let dst = self.fb.new_value(this_ty.clone());
                    self.fb.push_inst(Inst::LoadCapture { dst, idx: cap_idx });
                    (dst, this_ty.clone())
                } else {
                    return Err(LowerError::Other(format!(
                        "cannot resolve `this` for implicit field reference `{name}`"
                    )));
                };
                let meta_fty = meta.field_ty.get(&fid).cloned().unwrap();
                let fty = super::BodyCx::loaded_field_ty(&meta_fty);
                let v = self.fb.new_value(fty.clone());
                self.fb.push_inst(Inst::LoadField { dst: v, obj: this_v, field: fid });
                return Ok((v, fty));
            }
        }
        Err(LowerError::Other(format!("unbound variable: {name}")))
    }

    /// Store an already-lowered rhs (`vv0` : `vty`) into field `fid` of
    /// object `obj_v` (declared type `fty`), applying the full
    /// field-write semantics: `T → T?` / subtype Optional auto-wrap,
    /// `Object → Weak` re-typing, fixed-array value-copy, and the
    /// retain-new / release-old rc dance. `src_is_fresh` is the rhs's
    /// freshness; `src_owned` is `fresh || tail-owned`. `is_init` skips
    /// the release of the previous (zeroed) occupant.
    ///
    /// Shared by `this.f = v` / `obj.f = v` (`AssignField`) and the
    /// implicit bare-name field write `f = v` inside a method. The bare
    /// path used to open-code a degenerate store with no wrap, so a
    /// `slot = box` against a `Box?` field stored a raw object into the
    /// Optional slot and crashed on release (SIGSEGV).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn store_value_to_field(
        &mut self,
        obj_v: ValueId,
        fid: FieldId,
        fty: &MirTy,
        vv0: ValueId,
        vty: MirTy,
        src_is_fresh: bool,
        src_owned: bool,
        is_init: bool,
        span: Span,
    ) -> Result<(), LowerError> {
        // `T → T?` Optional auto-wrap, incl. an object-shaped subtype
        // source (`h.o = new Dog()` against `o: Animal?`, or
        // `h.o = [new Dog()]` against `o: Animal[]?`) — matching
        // `coerce`'s wrap arm. An `Optional<_>` source is an
        // Optional→Optional widen handled by the codegen, not a wrap,
        // so it is excluded (the `**inner == vty` exact arm still
        // covers same-type). Without the subtype case the raw value
        // was stored into the `?` slot and crashed on release.
        let obj_shape = |t: &MirTy| {
            matches!(
                t,
                MirTy::Object(_)
                    | MirTy::Array { .. }
                    | MirTy::Tuple(_)
                    | MirTy::Map { .. }
                    | MirTy::Optional(_)
            )
        };
        let needs_optional_wrap = matches!(
            fty,
            MirTy::Optional(inner)
                if **inner == vty
                    || (obj_shape(inner)
                        && obj_shape(&vty)
                        && !matches!(vty, MirTy::Optional(_)))
        );
        // Object → Weak: clif-level identity but the value's MIR type
        // must switch BEFORE the retain below, otherwise `Retain`
        // lowers to `__retain_object` and we leak a strong +1 onto a
        // weak-counted slot. Pre-coerce, then let the generic retain
        // path dispatch via `__retain_weak`.
        let needs_strong_to_weak =
            matches!((&vty, fty), (MirTy::Object(_), MirTy::Weak(_)));
        let (vv, value_is_fresh) = if needs_optional_wrap {
            let coerced = self.coerce(vv0, &vty, fty, span)?;
            // `coerce` already inserted the heap retain for the inner
            // of `T → T?`, and the resulting cell is a fresh
            // `NewOptional` allocation — treat it as fresh so the store
            // below doesn't add a second retain on the outer Optional.
            // An OWNED source's transfer +1 dies here (see
            // release_owned_wrap_source).
            self.release_owned_wrap_source(vv0, &vty, fty, src_owned);
            (coerced, true)
        } else if needs_strong_to_weak {
            let coerced = self.coerce(vv0, &vty, fty, span)?;
            self.release_owned_wrap_source(vv0, &vty, fty, src_owned);
            (coerced, src_is_fresh)
        } else {
            (vv0, src_is_fresh)
        };
        // Fixed-length array field with ARC elements (value semantics):
        //   * FRESH literal source — transfer the array (its rc=1
        //     becomes the field's share).
        //   * anything else — `$array.copyShallow` value copy; the
        //     copy's +1 is the field's.
        // The old occupant (skipped for init writes — zeroed slot)
        // drops its share via a plain array Release.
        if let MirTy::Array { elem, len: Some(_) } = fty {
            if self.is_arc_slot(elem) {
                let stored = if value_is_fresh {
                    vv
                } else {
                    let copy = self.fb.new_value(fty.clone());
                    self.fb.push_inst(Inst::Call {
                        dst: Some(copy),
                        callee: FuncRef::Builtin(Symbol::intern("$array.copyShallow")),
                        args: Box::new([vv]),
                    });
                    copy
                };
                if !is_init {
                    let old = self.fb.new_value(fty.clone());
                    self.fb.push_inst(Inst::LoadField { dst: old, obj: obj_v, field: fid });
                    self.fb.push_inst(Inst::Release { value: old });
                }
                self.fb.push_inst(Inst::StoreField { obj: obj_v, field: fid, value: stored });
                return Ok(());
            }
        }
        // ARC for any heap-typed field: retain the incoming value
        // (unless it was a fresh allocation that already owned its +1)
        // and release the previous occupant. `BodyCx::is_arc_slot` is
        // the authoritative rc-slot predicate (heap kind ∧ not COM
        // iface ∧ not CRepr / CPacked / CUnion `Object`).
        let is_heap = self.is_arc_slot(fty);
        if is_heap {
            if !value_is_fresh {
                self.fb.push_inst(Inst::Retain { value: vv });
            }
            // For init-style writes the slot's previous content is the
            // freshly-allocated zeroed bytes, not a real heap pointer —
            // skip the load+release that would free a NULL / garbage
            // pointer.
            if !is_init {
                let old = self.fb.new_value(fty.clone());
                self.fb.push_inst(Inst::LoadField { dst: old, obj: obj_v, field: fid });
                self.fb.push_inst(Inst::Release { value: old });
            }
        }
        self.fb.push_inst(Inst::StoreField { obj: obj_v, field: fid, value: vv });
        // CRepr inline enum field: `StoreField` codegen extracts the
        // discriminant out of the SSA Enum heap-box and narrows it into
        // the struct slot. The heap-box's rc=1 from `NewEnum` never
        // reaches a paired Release — drop it now when the rhs was fresh
        // (a literal `Mode.foo` ctor). Borrowed rhs keeps the source
        // owner's +1, so no release here.
        if matches!(fty, MirTy::CReprEnum(_))
            && value_is_fresh
            && matches!(&vty, MirTy::Enum(_))
        {
            self.fb.push_inst(Inst::Release { value: vv });
        }
        Ok(())
    }
}
