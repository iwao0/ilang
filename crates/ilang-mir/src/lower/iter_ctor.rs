//! `for ... in` and `enum.Variant(...)` constructor lowering.
//!
//! - `lower_for_in` desugars `for x in iter { body }` based on the
//!   iterator's MirTy: integer ranges turn into a counted loop,
//!   arrays into an index-based walk, strings into a per-char
//!   walk, etc. The body block sees `x` bound to the per-iteration
//!   element.
//! - `lower_enum_ctor` builds an enum heap cell: looks up the
//!   variant's payload shape, lowers each arg, retains heap-typed
//!   payloads, and emits `Inst::NewEnum`.

use ilang_ast::{self as ast, Block as AstBlock, Expr, ExprKind, Symbol};

use crate::inst::{BinOp, Inst, Terminator, ValueId};
use crate::types::MirTy;

use super::utils::{cmp_op, Cmp};
use super::{BodyCx, LoopFrame, LowerError, VariantPayloadMeta};

impl<'a> BodyCx<'a> {
    pub(super) fn lower_for_in(
        &mut self,
        var: Symbol,
        iter: &Expr,
        body: &AstBlock,
    ) -> Result<(ValueId, MirTy), LowerError> {
        // `for x in <iter> { body }` desugars to a counter loop.
        // Three iter shapes:
        //   - bounded range start..end (or start..=end)
        //   - open range start..       (no upper bound; body must break)
        //   - array
        match &iter.kind {
            ExprKind::Range { start, end, inclusive } => {
                let start = start.as_deref().ok_or_else(|| {
                    LowerError::Other("range without lower bound is not iterable".into())
                })?;
                let (sv, sty) = self.lower_expr(start)?;
                if !sty.is_int() {
                    return Err(LowerError::Other("range bounds must be integer".into()));
                }
                let header = self.fb.new_block();
                let body_blk = self.fb.new_block();
                let exit = self.fb.new_block();
                let i = self.fb.add_block_param(header, sty.clone());

                self.fb.set_terminator(Terminator::Br {
                    dst: header,
                    args: Box::new([sv]),
                });
                self.fb.switch_to(header);

                let cond = if let Some(e) = end {
                    let (ev, _) = self.lower_expr(e)?;
                    let cond_op = if *inclusive {
                        cmp_op(&sty, Cmp::Le)
                    } else {
                        cmp_op(&sty, Cmp::Lt)
                    };
                    let c = self.fb.new_value(MirTy::Bool);
                    self.fb.push_inst(Inst::BinOp {
                        dst: c,
                        op: cond_op,
                        lhs: i,
                        rhs: ev,
                    });
                    Some(c)
                } else {
                    None
                };

                if let Some(c) = cond {
                    self.fb.set_terminator(Terminator::CondBr {
                        cond: c,
                        then_block: body_blk,
                        then_args: Box::new([]),
                        else_block: exit,
                        else_args: Box::new([]),
                    });
                } else {
                    self.fb.set_terminator(Terminator::Br { dst: body_blk, args: Box::new([]) });
                }

                // Step block: increments `i` and back-edges to header.
                // `continue` targets this so the increment isn't
                // skipped.
                let step = self.fb.new_block();

                self.fb.switch_to(body_blk);
                self.env.enter_scope();
                self.env.bind(var, i, sty.clone());
                self.loops.push(LoopFrame {
                    env_depth_at_entry: self.env.scopes.len(),
                    continue_target: step,
                    break_target: exit,
                });
                let _ = self.lower_block(body)?;
                self.loops.pop();
                self.env.exit_scope();
                self.fb.set_terminator(Terminator::Br { dst: step, args: Box::new([]) });

                self.fb.switch_to(step);
                let one = self.const_int(sty.clone(), 1);
                let next = self.fb.new_value(sty.clone());
                self.fb.push_inst(Inst::BinOp {
                    dst: next,
                    op: BinOp::IAdd,
                    lhs: i,
                    rhs: one,
                });
                self.fb.set_terminator(Terminator::Br {
                    dst: header,
                    args: Box::new([next]),
                });

                self.fb.switch_to(exit);
                Ok((self.const_unit(), MirTy::Unit))
            }
            _ => {
                let iter_is_fresh = self.is_fresh_object_expr(iter);
                let (av, aty) = self.lower_expr(iter)?;
                let elem_ty = match &aty {
                    MirTy::Array { elem, .. } => (**elem).clone(),
                    other => {
                        return Err(LowerError::Other(format!(
                            "for-in over non-array / non-range: {other}"
                        )))
                    }
                };
                let header = self.fb.new_block();
                let body_blk = self.fb.new_block();
                let exit = self.fb.new_block();
                let i = self.fb.add_block_param(header, MirTy::I64);

                let zero = self.const_int(MirTy::I64, 0);
                self.fb.set_terminator(Terminator::Br {
                    dst: header,
                    args: Box::new([zero]),
                });
                self.fb.switch_to(header);
                // Re-read the length every lap (live semantics, like
                // JS `for..of`): the body may push (the loop then
                // visits the appended elements) or pop (the loop ends
                // early instead of tripping the stale-length bounds
                // panic the old loop-invariant ArrayLen caused).
                // ArrayLoad re-reads the data pointer per access, so
                // a push-triggered realloc is already safe.
                let len = self.fb.new_value(MirTy::I64);
                self.fb.push_inst(Inst::ArrayLen { dst: len, arr: av });
                let c = self.fb.new_value(MirTy::Bool);
                self.fb.push_inst(Inst::BinOp {
                    dst: c,
                    op: BinOp::ILtS,
                    lhs: i,
                    rhs: len,
                });
                self.fb.set_terminator(Terminator::CondBr {
                    cond: c,
                    then_block: body_blk,
                    then_args: Box::new([]),
                    else_block: exit,
                    else_args: Box::new([]),
                });

                let step = self.fb.new_block();

                self.fb.switch_to(body_blk);
                let elem_v = self.fb.new_value(elem_ty.clone());
                self.fb.push_inst(Inst::ArrayLoad { dst: elem_v, arr: av, idx: i });
                // Register the fresh iterable for the early-`return`
                // sweep: the Release in the exit block below only
                // runs on paths that REACH the exit block — `break`
                // jumps there, but a `return` out of the body
                // bypassed it and leaked the whole fresh array
                // (`for e in m.entries() { ... return ... }`).
                // Registered at the depth OUTSIDE the loop frame so
                // the `break` / `continue` sweeps skip it (they
                // stay on paths that still reach the exit block /
                // header — releasing here too would double-free).
                if iter_is_fresh {
                    let depth = self.env.scopes.len();
                    self.live_fresh_scrutinees.push((av, depth));
                }
                self.env.enter_scope();
                // The element binding BORROWS into the array's slot
                // (ArrayLoad takes no retain — the array keeps the
                // owning share), exactly the match-payload contract.
                // Register it as a PatternBinding so the early-exit
                // sweeps skip it: as a plain Ssa binding the
                // `return` sweep released the borrow and over-
                // released the element (SIGABRT on
                // `for e in m.entries() { ... return ... }`).
                self.env.bind_pattern(var, elem_v, elem_ty.clone(), false);
                self.loops.push(LoopFrame {
                    env_depth_at_entry: self.env.scopes.len(),
                    continue_target: step,
                    break_target: exit,
                });
                let _ = self.lower_block(body)?;
                self.loops.pop();
                self.env.exit_scope();
                if iter_is_fresh {
                    self.live_fresh_scrutinees.pop();
                }
                self.fb.set_terminator(Terminator::Br { dst: step, args: Box::new([]) });

                self.fb.switch_to(step);
                let one = self.const_int(MirTy::I64, 1);
                let next = self.fb.new_value(MirTy::I64);
                self.fb.push_inst(Inst::BinOp {
                    dst: next,
                    op: BinOp::IAdd,
                    lhs: i,
                    rhs: one,
                });
                self.fb.set_terminator(Terminator::Br {
                    dst: header,
                    args: Box::new([next]),
                });

                self.fb.switch_to(exit);
                // After the for-in finishes, a fresh-receiver array
                // has no surviving owner — release it. host_release_array
                // both cascades release_object on every Object element
                // (when the array's kind_tag == 1) and frees the
                // 48-byte header + data buffer. Without this, the
                // fresh array leaks even when its elements are
                // primitives (e.g. `for x in make_arr(): i64[]`).
                let _ = len;
                if iter_is_fresh {
                    self.fb.push_inst(Inst::Release { value: av });
                }
                Ok((self.const_unit(), MirTy::Unit))
            }
        }
    }

    pub(super) fn lower_enum_ctor(
        &mut self,
        enum_name: Symbol,
        variant: Symbol,
        args: &ast::CtorArgs,
    ) -> Result<(ValueId, MirTy), LowerError> {
        let id = *self.enum_ids.get(&enum_name).ok_or_else(|| {
            LowerError::Other(format!("unknown enum {enum_name}"))
        })?;
        let meta = self.enum_meta.get(&id).expect("enum meta");
        let vmeta = meta.variants.get(&variant).ok_or_else(|| {
            LowerError::Other(format!("enum {enum_name} has no variant {variant}"))
        })?;
        let vid = vmeta.id;
        let payload_meta = vmeta.payload.clone();

        let payload_vals: Vec<ValueId> = match (&payload_meta, args) {
            (VariantPayloadMeta::Unit, ast::CtorArgs::Unit) => Vec::new(),
            (VariantPayloadMeta::Tuple(tys), ast::CtorArgs::Tuple(arg_exprs)) => {
                if tys.len() != arg_exprs.len() {
                    return Err(LowerError::Other(format!(
                        "{enum_name}.{variant} expects {} args, got {}",
                        tys.len(),
                        arg_exprs.len()
                    )));
                }
                let mut out = Vec::with_capacity(tys.len());
                for (i, ae) in arg_exprs.iter().enumerate() {
                    let arg_is_fresh = self.is_fresh_object_expr(ae);
                    // Fixed-length array payload + bare array literal:
                    // lower with the payload's len hint so the literal
                    // picks the inline (header-less) layout that
                    // ArrayLoad/ArrayLen expect for `Array { len:
                    // Some(n) }`. Without the hint the literal lowers
                    // to a dynamic-header array and the no-op
                    // dynamic↔fixed identity-coerce later silently
                    // hands a header pointer where inline data is
                    // expected.
                    let (coerced, _) = self.lower_arg_to(ae, Some(&tys[i]))?;
                    // Heap payload from an aliased Var: retain so the
                    // enum value owns its own +1. Required now that
                    // host_release_array actually frees memory at
                    // rc==0 (match_fresh_scrutinee.il regression).
                    let needs_retain = !arg_is_fresh && self.is_arc_slot(&tys[i]);
                    if needs_retain {
                        self.fb.push_inst(Inst::Retain { value: coerced });
                    }
                    out.push(coerced);
                }
                out
            }
            (VariantPayloadMeta::Struct(fields), ast::CtorArgs::Struct(arg_named)) => {
                // Reorder by declaration order.
                let mut out = vec![None; fields.len()];
                for (name, ae) in arg_named.iter() {
                    let (idx, fty) = fields
                        .iter()
                        .enumerate()
                        .find_map(|(i, (fname, fty))| {
                            if fname == name {
                                Some((i, fty.clone()))
                            } else {
                                None
                            }
                        })
                        .ok_or_else(|| {
                            LowerError::Other(format!(
                                "{enum_name}.{variant} has no field {name}"
                            ))
                        })?;
                    let arg_is_fresh = self.is_fresh_object_expr(ae);
                    // See tuple-variant branch above — same composite
                    // hint propagation reason.
                    let (coerced, _) = self.lower_arg_to(ae, Some(&fty))?;
                    let needs_retain = !arg_is_fresh && self.is_arc_slot(&fty);
                    if needs_retain {
                        self.fb.push_inst(Inst::Retain { value: coerced });
                    }
                    out[idx] = Some(coerced);
                }
                out.into_iter()
                    .enumerate()
                    .map(|(i, v)| {
                        v.ok_or_else(|| {
                            LowerError::Other(format!(
                                "missing field for {enum_name}.{variant} at idx {i}"
                            ))
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?
            }
            _ => {
                return Err(LowerError::Other(format!(
                    "{enum_name}.{variant} payload-shape mismatch"
                )))
            }
        };

        let ty = MirTy::Enum(id);
        let dst = self.fb.new_value(ty.clone());
        self.fb.push_inst(Inst::NewEnum {
            dst,
            enum_id: id,
            variant: vid,
            payload: payload_vals.into_boxed_slice(),
        });
        Ok((dst, ty))
    }

}
