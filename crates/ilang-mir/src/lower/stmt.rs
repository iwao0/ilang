//! `lower_stmt` — per-`StmtKind` dispatcher on `BodyCx`. Handles
//! `let` / `let-tuple` / `let-struct` bindings (with the host-slot
//! store path for top-level REPL slots) and falls through to
//! `lower_expr` for the expression-statement case.

use ilang_ast::{ExprKind, Stmt, StmtKind, Symbol};

use crate::inst::{FuncRef, Inst};
use crate::types::MirTy;

use super::{BodyCx, LowerError};

impl<'a> BodyCx<'a> {
    pub(super) fn lower_stmt(&mut self, stmt: &Stmt) -> Result<(), LowerError> {
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
                    if value_is_fresh && self.is_arc_heap(&vty) {
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
                } else if let (
                    ExprKind::Array(items),
                    Some(simd_ty @ MirTy::Simd { elem, lanes }),
                ) = (&value.kind, &bind_hint)
                {
                    // Array literal → SIMD vector: each element
                    // coerces to the lane scalar type, then
                    // `NewSimd` packs them into a single cranelift
                    // vector value.
                    if items.len() != *lanes as usize {
                        return Err(LowerError::Other(format!(
                            "expected {} elements for {simd_ty}, got {}",
                            lanes,
                            items.len()
                        )));
                    }
                    let lane_scalar = elem.as_scalar_mir();
                    let mut lane_vals: Vec<crate::ValueId> = Vec::with_capacity(items.len());
                    for it in items.iter() {
                        let (vv, vty) = self.lower_expr(it)?;
                        let coerced = if vty == lane_scalar {
                            vv
                        } else {
                            self.coerce(vv, &vty, &lane_scalar, it.span)?
                        };
                        lane_vals.push(coerced);
                    }
                    let dst = self.fb.new_value(simd_ty.clone());
                    self.fb.push_inst(Inst::NewSimd {
                        dst,
                        lanes: lane_vals.into_boxed_slice(),
                    });
                    (dst, simd_ty.clone())
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
                if self.is_arc_heap(&bind_ty) && !value_is_fresh_object {
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

}
