//! `lower_expr` — the per-`ExprKind` dispatcher on `BodyCx`. Each
//! arm either emits the MIR for that expression shape directly or
//! delegates to one of the more specific lowerers (`lower_call` /
//! `lower_match` / `lower_new` / ...).

use ilang_ast::{Expr, ExprKind, Symbol};

use crate::inst::{FuncId, FuncRef, Inst, MirConst, ValueId};
use crate::types::MirTy;

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
                    let prom_ty = MirTy::Promise(Box::new(inner));
                    let dst = self.fb.new_value(prom_ty.clone());
                    self.fb.push_inst(Inst::Call {
                        dst: Some(dst),
                        callee: FuncRef::Builtin(Symbol::intern("promise_with_executor")),
                        args: Box::new([exec_v, kind_v]),
                    });
                    return Ok((dst, prom_ty));
                }
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
                        // Str was missing here: assigning a
                        // function-local `let s = fnReturning(); ...
                        // this.f = s` skipped the retain, so when `s`
                        // released at scope exit the field's pointer
                        // dangled. Treat string fields like every
                        // other heap-typed field.
                        | MirTy::Str
                );
                if is_heap {
                    if !value_is_fresh {
                        self.fb.push_inst(Inst::Retain { value: vv });
                    }
                    // For init-style writes (`this.f = v` from inside
                    // `init`) the slot's previous content is the
                    // freshly-allocated zeroed bytes, not a real heap
                    // pointer — skip the load+release that would
                    // otherwise free a NULL / garbage pointer.
                    if !*is_init {
                        let old = self.fb.new_value(fty.clone());
                        self.fb.push_inst(Inst::LoadField {
                            dst: old,
                            obj: ov,
                            field: fid,
                        });
                        self.fb.push_inst(Inst::Release { value: old });
                    }
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
            ExprKind::Await(_) => {
                return Err(LowerError::Other(
                    "`await` outside an `async fn` body — desugar pass should have eliminated it".into(),
                ));
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
                // Aggregate literal — for `@extern(C) struct` /
                // top-level `struct` / `union` (zero-init heap slot
                // then store each field) and for ARC classes (the
                // looser literal form that bypasses `init`). The
                // type checker has already validated field set and
                // types; here we just emit the construction +
                // per-field stores.
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
                    let value_is_fresh = self.is_fresh_object_expr(fval);
                    let (vv, vty) = self.lower_expr(fval)?;
                    let coerced = if vty == fty {
                        vv
                    } else {
                        self.coerce(vv, &vty, &fty, fval.span)?
                    };
                    // ARC retain for heap-typed fields: same rule as
                    // AssignField. The slot started at zero (fresh
                    // alloc) so there is no prior occupant to
                    // release. CRepr structs / unions can't hold
                    // these field types — the type-checker rejects
                    // them — so this branch only fires on ARC class
                    // literals.
                    let is_heap = matches!(
                        fty,
                        MirTy::Object(_)
                            | MirTy::Array { .. }
                            | MirTy::Tuple(_)
                            | MirTy::Map { .. }
                            | MirTy::Optional(_)
                            | MirTy::Fn(_)
                            | MirTy::Str
                    );
                    if is_heap && !value_is_fresh {
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
}
