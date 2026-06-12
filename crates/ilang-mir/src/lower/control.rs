//! Control-flow lowering on `BodyCx`: `if`, `while`, `loop`, plus
//! the value-returning jumps (`break`, `continue`, `return`). Each
//! method allocates Cranelift blocks, threads heap-typed
//! intermediates through with the right retain/release rules, and
//! emits the terminator that wires everything together.

use ilang_ast::{Block as AstBlock, Expr, Span, Symbol};

use crate::inst::{Inst, MirConst, Terminator, ValueId};
use crate::types::MirTy;

use super::{Binding, BodyCx, LoopFrame, LowerError};

impl<'a> BodyCx<'a> {
    pub(super) fn lower_if(
        &mut self,
        cond: &Expr,
        then_branch: &AstBlock,
        else_branch: Option<&Expr>,
    ) -> Result<(ValueId, MirTy), LowerError> {
        let (cv, _) = self.lower_expr(cond)?;
        let then_blk = self.fb.new_block();
        let else_blk = self.fb.new_block();

        // Lower then-branch first to discover its value type.
        self.fb.set_terminator(Terminator::CondBr {
            cond: cv,
            then_block: then_blk,
            then_args: Box::new([]),
            else_block: else_blk,
            else_args: Box::new([]),
        });

        self.fb.switch_to(then_blk);
        let then_tail = self.lower_block(then_branch)?;
        // Remember the then-block we ended up at (the body may have
        // emitted control-flow that landed us in a successor) so we
        // can come back to it and emit the join terminator after
        // we've lowered the else side too.
        let then_end_blk = self.fb.current_block();

        // Lower the else branch first so we can pick a join type
        // that subsumes both arms. Lowering happens in `else_blk`'s
        // context.
        self.fb.switch_to(else_blk);
        let else_tail: Option<(ValueId, MirTy)> = match else_branch {
            Some(e) => Some(self.lower_expr(e)?),
            None => None,
        };
        let else_end_blk = self.fb.current_block();

        // Pick the join type. With no else branch, `if` is
        // statement-shaped and yields Unit. With both branches:
        //   * pick the then-tail's type by default, but
        //   * if then is `Optional<Unit>` (i.e. `none`) and else is
        //     `Optional<T>` for some non-Unit T, use the wider
        //     `Optional<T>` — otherwise the join collapses to
        //     `Optional<Unit>` and the `Optional<T> → Optional<Unit>`
        //     coerce rule double-wraps the some-arm in another
        //     Optional, corrupting cross-function return values.
        //   * symmetrically prefer else's type when else is the
        //     none arm and then carries the value.
        let result_ty = match (else_branch, &then_tail, &else_tail) {
            (None, _, _) => MirTy::Unit,
            (Some(_), Some((_, tt)), Some((_, et))) => widen_optional(tt, et),
            (Some(_), Some((_, t)), None) => t.clone(),
            (Some(_), None, Some((_, t))) => t.clone(),
            (Some(_), None, None) => MirTy::Unit,
        };

        let cont = self.fb.new_block();
        let result_val = if matches!(result_ty, MirTy::Unit) {
            None
        } else {
            Some(self.fb.add_block_param(cont, result_ty.clone()))
        };

        // Coerce branch tail values to the join block's parameter
        // type so Cranelift sees matching block-arg types. Mixed
        // narrower / wider integer branches show up in code like
        // `if cond { some_i8 } else { some_i64 }` where unify
        // pushed the result to i64 but one branch's value stayed
        // narrower.
        self.fb.switch_to(then_end_blk);
        let then_arg: Box<[ValueId]> = match (&result_ty, then_tail) {
            (MirTy::Unit, _) => Box::new([]),
            (rt, Some((v, t))) if &t == rt => Box::new([v]),
            (rt, Some((v, t))) => {
                let coerced = self.coerce(v, &t, rt, Span::dummy()).unwrap_or(v);
                Box::new([coerced])
            }
            (_, None) => Box::new([self.const_unit()]),
        };
        self.fb.set_terminator(Terminator::Br { dst: cont, args: then_arg });

        self.fb.switch_to(else_end_blk);
        let else_arg: Box<[ValueId]> = match (else_branch, else_tail) {
            (Some(_), Some((v, vty))) => {
                if matches!(result_ty, MirTy::Unit) {
                    Box::new([])
                } else if vty == result_ty {
                    Box::new([v])
                } else {
                    let coerced = self.coerce(v, &vty, &result_ty, Span::dummy()).unwrap_or(v);
                    Box::new([coerced])
                }
            }
            (Some(_), None) => {
                if matches!(result_ty, MirTy::Unit) {
                    Box::new([])
                } else {
                    Box::new([self.const_unit()])
                }
            }
            (None, _) => {
                if matches!(result_ty, MirTy::Unit) {
                    Box::new([])
                } else {
                    // No else but result is non-unit → can't happen
                    // (type checker would have rejected).
                    return Err(LowerError::Other(
                        "if without else used in value position".into(),
                    ));
                }
            }
        };
        self.fb.set_terminator(Terminator::Br { dst: cont, args: else_arg });

        self.fb.switch_to(cont);
        Ok(match result_val {
            Some(v) => (v, result_ty),
            None => (self.const_unit(), MirTy::Unit),
        })
    }

    pub(super) fn lower_while(&mut self, cond: &Expr, body: &AstBlock) -> Result<(ValueId, MirTy), LowerError> {
        let header = self.fb.new_block();
        let body_blk = self.fb.new_block();
        let exit = self.fb.new_block();

        self.fb.set_terminator(Terminator::Br { dst: header, args: Box::new([]) });
        self.fb.switch_to(header);
        let (cv, _) = self.lower_expr(cond)?;
        self.fb.set_terminator(Terminator::CondBr {
            cond: cv,
            then_block: body_blk,
            then_args: Box::new([]),
            else_block: exit,
            else_args: Box::new([]),
        });

        self.fb.switch_to(body_blk);
        self.loops.push(LoopFrame {
            env_depth_at_entry: self.env.scopes.len(),
            continue_target: header,
            break_target: exit,
        });
        let _ = self.lower_block(body)?;
        self.loops.pop();
        self.fb.set_terminator(Terminator::Br { dst: header, args: Box::new([]) });

        self.fb.switch_to(exit);
        Ok((self.const_unit(), MirTy::Unit))
    }

    pub(super) fn lower_loop(&mut self, body: &AstBlock) -> Result<(ValueId, MirTy), LowerError> {
        let header = self.fb.new_block();
        let exit = self.fb.new_block();

        self.fb.set_terminator(Terminator::Br { dst: header, args: Box::new([]) });
        self.fb.switch_to(header);
        self.loops.push(LoopFrame {
            env_depth_at_entry: self.env.scopes.len(),
            continue_target: header,
            break_target: exit,
        });
        let _ = self.lower_block(body)?;
        self.loops.pop();
        self.fb.set_terminator(Terminator::Br { dst: header, args: Box::new([]) });

        self.fb.switch_to(exit);
        // If a `break v` appeared, the exit block has a param of the
        // joined break-value type. We don't yet detect that here; the
        // type checker sets `loop_break_types`. For now `loop` without
        // value-carrying breaks evaluates to Unit.
        let exit_blk = self.fb.block(exit);
        if let Some(&v) = exit_blk.params.first() {
            let ty = self.fb.ty_of(v).clone();
            Ok((v, ty))
        } else {
            Ok((self.const_unit(), MirTy::Unit))
        }
    }

    pub(super) fn lower_break(&mut self, value: Option<&Expr>) -> Result<(ValueId, MirTy), LowerError> {
        let frame = self
            .loops
            .last()
            .ok_or_else(|| LowerError::Other("break outside loop".into()))?;
        let target = frame.break_target;
        let frame_depth = frame.env_depth_at_entry;

        let args: Box<[ValueId]> = match value {
            Some(e) => {
                let value_is_fresh = self.is_fresh_object_expr(e);
                let (v, ty) = self.lower_expr(e)?;
                // `break arr` where `arr` is an aliased Var owned by
                // a scope we're about to release: bump rc so the
                // value survives past the imminent scope-exit
                // Release. Otherwise the loop's exit-block receives
                // a pointer the scope is about to free (only matters
                // for Array/Tuple/etc. which actually free memory).
                let needs_retain = !value_is_fresh && self.is_arc_slot(&ty);
                if needs_retain {
                    self.fb.push_inst(Inst::Retain { value: v });
                }
                // Lazily attach a block param to the loop's exit block
                // the first time we see a `break v`.
                if self.fb.block(target).params.is_empty() {
                    self.fb.add_block_param(target, ty);
                }
                Box::new([v])
            }
            None => {
                if self.fb.block(target).params.is_empty() {
                    Box::new([])
                } else {
                    let unit = self.const_unit();
                    Box::new([unit])
                }
            }
        };
        // Release every heap-typed binding introduced in scopes
        // pushed since the loop frame's entry — `lower_block`'s
        // scope-exit release pass is bypassed by the early jump.
        // Stays on `MirTy::is_heap` (not `is_arc_slot`) because the
        // matching scope-exit sweep also uses the broader predicate
        // and handles CRepr / COM exclusion downstream of the
        // `needs_release` filter; switching the predicate here
        // without porting the downstream filter would change the
        // observable behaviour (owned CRepr Locals would silently
        // leak on `break`).
        let needs_release = |ty: &MirTy| ty.is_heap();
        let mut to_release: Vec<Binding> = Vec::new();
        for scope in self.env.scopes.iter().skip(frame_depth.saturating_sub(0)) {
            for (_n, b) in scope.iter().rev() {
                let keep = match b {
                    Binding::Local(_, ty) => needs_release(ty),
                    Binding::Ssa(_, ty) => needs_release(ty),
                    // PatternBinding borrows into the scrutinee —
                    // releasing here would double-account the
                    // inner the arm lowerer is already pairing
                    // against `Release(scrutinee)`.
                    Binding::PatternBinding(..) => false,
                    Binding::Cell(_, ty) => needs_release(ty),
                };
                if keep {
                    to_release.push(b.clone());
                }
            }
        }
        for b in to_release {
            match b {
                Binding::Local(lid, ty) => {
                    let v = self.fb.new_value(ty.clone());
                    self.fb.push_inst(Inst::UseLocal { dst: v, local: lid });
                    self.fb.push_inst(Inst::Release { value: v });
                }
                Binding::Ssa(v, _) => {
                    self.fb.push_inst(Inst::Release { value: v });
                }
                Binding::PatternBinding(..) => unreachable!("filtered out above"),
                Binding::Cell(cell_v, _) => {
                    // Drop the scope's share of the cell itself (its
                    // value type is the `T[]` cell array, so this
                    // dispatches as an array release whose cascade
                    // covers the inner value). Releasing the *inner*
                    // here — the old behaviour — left a dangling slot
                    // for any closure still holding the cell.
                    self.fb.push_inst(Inst::Release { value: cell_v });
                }
            }
        }
        // Fresh scrutinees of matches the break exits out of — their
        // arm-end Release is bypassed by the jump.
        self.release_live_scrutinees_from(frame_depth);
        self.fb.set_terminator(Terminator::Br { dst: target, args });
        // After break, code is unreachable in the current block. Open
        // a fresh dead block for any stray post-break statements.
        let dead = self.fb.new_block();
        self.fb.switch_to(dead);
        Ok((self.const_unit(), MirTy::Unit))
    }

    pub(super) fn lower_continue(&mut self) -> Result<(ValueId, MirTy), LowerError> {
        let frame = self
            .loops
            .last()
            .ok_or_else(|| LowerError::Other("continue outside loop".into()))?;
        let target = frame.continue_target;
        let frame_depth = frame.env_depth_at_entry;
        // Same early-exit sweep as `break` / `return`: the jump to
        // the loop header bypasses the body's scope-exit pass, so
        // every live heap binding (and fresh match scrutinee) since
        // the loop frame leaked once per `continue`.
        self.release_scopes_since(frame_depth);
        self.release_live_scrutinees_from(frame_depth);
        self.fb.set_terminator(Terminator::Br { dst: target, args: Box::new([]) });
        let dead = self.fb.new_block();
        self.fb.switch_to(dead);
        Ok((self.const_unit(), MirTy::Unit))
    }

    pub(super) fn lower_return(&mut self, value: Option<&Expr>) -> Result<(ValueId, MirTy), LowerError> {
        // A borrowed heap return value (e.g. `return s` for a local /
        // pattern binding) needs +1 before the early-return sweep
        // below releases the scope that owns it — the caller receives
        // an owned reference either way (same contract as the
        // tail-position borrow retain). Skipped when `coerce` minted
        // a NEW value (a fresh wrapper owns its +1 and already
        // retained the inner where needed, e.g. the `T → T?` wrap).
        let mut borrowed_to_retain: Option<ValueId> = None;
        let v = match value {
            Some(e) => {
                let ret_ty = self.ret_ty.clone();
                // Composite literals (`return [..]` / `return (..)`)
                // build with the declared return element types pushed
                // in, so packed arrays / narrowed tuples get correct
                // cell widths instead of defaulting to i64/f64.
                if let Some(res) = self.lower_composite_with_hint(e, &ret_ty) {
                    Some(res?.0)
                } else {
                    let value_is_fresh = self.is_fresh_object_expr(e);
                    let (vv, vty) = self.lower_expr(e)?;
                    let coerced = if vty == ret_ty || matches!(ret_ty, MirTy::Unit) {
                        vv
                    } else {
                        self.coerce(vv, &vty, &ret_ty, e.span).unwrap_or(vv)
                    };
                    if !value_is_fresh && self.is_arc_slot(&vty) && coerced == vv {
                        borrowed_to_retain = Some(coerced);
                    }
                    Some(coerced)
                }
            }
            None => None,
        };
        // The fn's signature might require a non-Unit return value
        // even when the user wrote a bare `return`. The canonical
        // case is `init()` for a class — its source signature is
        // void but the synthesised MIR returns the receiver. If we
        // emit `return_(&[])` here, Cranelift fails because the fn
        // declares one i64 return slot. Synthesise `this` for the
        // init case, a typed-zero for everything else.
        let v = if v.is_some() || matches!(self.ret_ty, MirTy::Unit) {
            v
        } else {
            let want = self.ret_ty.clone();
            if let Some((this_v, this_ty)) = self.lookup_var(Symbol::intern("this")) {
                if this_ty == want {
                    Some(this_v)
                } else {
                    Some(
                        self.coerce(this_v, &this_ty, &want, Span::dummy())
                            .unwrap_or(this_v),
                    )
                }
            } else {
                let synth = self.fb.new_value(want.clone());
                let c = match &want {
                    MirTy::Bool => Inst::Const { dst: synth, value: MirConst::Bool(false) },
                    MirTy::F32 => Inst::Const { dst: synth, value: MirConst::F32(0) },
                    MirTy::F64 => Inst::Const { dst: synth, value: MirConst::F64(0) },
                    _ => Inst::Const { dst: synth, value: MirConst::Int(0) },
                };
                self.fb.push_inst(c);
                Some(synth)
            }
        };
        // Early-return scope sweep: `lower_block`'s scope-exit pass
        // never runs for the blocks this return jumps out of, so
        // every live heap binding above the param scope is released
        // here (mirrors `lower_break`'s sweep for loops). The
        // borrowed-value retain above must precede this.
        if let Some(rv) = borrowed_to_retain {
            self.fb.push_inst(Inst::Retain { value: rv });
        }
        self.release_scopes_for_return();
        let release_value = v
            .map(|vid| self.crepr_return_owned.contains(&vid))
            .unwrap_or(false);
        self.fb.set_terminator(Terminator::Return { value: v, release_value });
        let dead = self.fb.new_block();
        self.fb.switch_to(dead);
        Ok((self.const_unit(), MirTy::Unit))
    }
}

/// Pick the wider join type when two `if`-branch tails carry
/// related Optional shapes. The only asymmetry that matters today
/// is `Optional<Unit>` (the type of the bare `none` literal) vs
/// `Optional<T>` for some non-Unit `T` — the `none` side fits any
/// `Optional<T>` at the bit level, but going the other way runs
/// through `coerce`'s `T → Optional<T>` rule and double-wraps the
/// some-arm in another Optional, which then ripples into a broken
/// `OptionalUnwrap` at call sites (the unwrap reads the inner
/// box's `kind_tag` byte instead of the stored value). For all
/// other shapes, prefer the then-branch type to preserve the
/// previous behaviour.
fn widen_optional(then_ty: &MirTy, else_ty: &MirTy) -> MirTy {
    match (then_ty, else_ty) {
        (MirTy::Optional(ti), MirTy::Optional(ei))
            if matches!(**ti, MirTy::Unit) && !matches!(**ei, MirTy::Unit) =>
        {
            else_ty.clone()
        }
        (MirTy::Optional(ti), MirTy::Optional(ei))
            if !matches!(**ti, MirTy::Unit) && matches!(**ei, MirTy::Unit) =>
        {
            then_ty.clone()
        }
        _ => then_ty.clone(),
    }
}
