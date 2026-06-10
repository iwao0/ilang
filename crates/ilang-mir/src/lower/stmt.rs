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
                    // Fire `Release` for both ARC-heap kinds (the
                    // usual `__release_object` / cascade dispatch)
                    // and CRepr-family `Object` values (which take
                    // the `__mir_free(buffer, c_size)` arm in
                    // `lower_inst/arc.rs::Release`). Without the
                    // second case, `let _ = make_crepr_box()` leaks
                    // the `__mir_alloc`'d buffer — caller-side
                    // ownership transferred away from the callee's
                    // tail-alias `crepr_owned_locals` remove, and
                    // no other path reclaims it.
                    let needs_release =
                        self.is_arc_heap(&vty) || self.is_crepr_object_kind(&vty);
                    if value_is_fresh && needs_release {
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
                } else if matches!(value.kind, ExprKind::FnExpr { .. }) {
                    // Non-slot `let f = fn(..) { ... f(..) ... }` —
                    // the FnExpr lowering skips the self capture and
                    // records a `self_ref` on the pending closure;
                    // the body's `f` then resolves to ClosureSelf
                    // (the closure's own env pointer).
                    self.binding_self_name = Some(*name);
                }
                let (v, mty) = if let (
                    ExprKind::Array(items),
                    Some(simd_ty @ MirTy::Simd { elem, lanes }),
                ) = (&value.kind, &bind_hint)
                {
                    // Array literal → SIMD vector: each element
                    // coerces to the lane scalar type, then
                    // `NewSimd` packs them into a single cranelift
                    // vector value. (No `lower_composite_with_hint`
                    // arm covers array→simd, so handle it up front.)
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
                } else if let Some(h) = &bind_hint {
                    // Push the binding's declared type into a composite
                    // literal RHS (array / tuple / some / none) so it is
                    // built with the right element widths and the empty
                    // array gets the annotated element type. Non-literal
                    // values fall through to a plain lowering + the
                    // coerce below.
                    match self.lower_composite_with_hint(value, h) {
                        Some(res) => res?,
                        None => self.lower_expr(value)?,
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
                } else if self.cellify_set.contains(name) {
                    // This name is captured AND mutated by some inner
                    // closure — bind it as a shared 1-element heap cell
                    // so reads/writes here and in every closure that
                    // captures it go through the same storage (shared
                    // mutable capture: a write through one closure is
                    // visible to the outer scope and to sibling
                    // closures). Reads/writes use ArrayLoad/Store[0];
                    // closures capture the cell pointer.
                    let cell_ty = MirTy::Array {
                        elem: Box::new(bind_ty.clone()),
                        len: None,
                    };
                    let cell_v = self.fb.new_value(cell_ty);
                    self.fb.push_inst(Inst::NewArray {
                        dst: cell_v,
                        elem: bind_ty.clone(),
                        items: Box::new([bound]),
                    });
                    self.env.bind_cell(*name, cell_v, bind_ty.clone());
                } else {
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
                        if self.is_arc_slot(&bind_ty) && !value_is_fresh_object {
                            self.fb.push_inst(Inst::Retain { value: bound });
                        }
                        self.fb.push_inst(Inst::Call {
                            dst: None,
                            callee: FuncRef::Builtin(Symbol::intern("$repl.storeSlot")),
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
                    let meta_fty = meta.field_ty.get(&fid).cloned().unwrap();
                    // Promote `CReprEnum` → `Enum` for the SSA
                    // binding so let-struct destructure flows the
                    // heap-box form downstream (mirrors
                    // `lower_field`).
                    let fty = super::BodyCx::loaded_field_ty(&meta_fty);
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
                if self.is_arc_slot(&ty) && self.is_fresh_object_expr(e) {
                    self.fb.push_inst(Inst::Release { value: v });
                }
                Ok(())
            }
        }
    }

}
