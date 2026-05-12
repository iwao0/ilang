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

        // Determine result type from then-branch tail (or Unit).
        // Without an `else` branch, an `if` is statement-like — the
        // tail value (if any) is discarded and the overall result is
        // Unit. Otherwise we adopt the then-branch tail's type so
        // the join block carries the value through a block param.
        let result_ty = match (else_branch, &then_tail) {
            (None, _) => MirTy::Unit,
            (Some(_), Some((_, t))) => t.clone(),
            (Some(_), None) => MirTy::Unit,
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

        self.fb.switch_to(else_blk);
        let else_arg: Box<[ValueId]> = match else_branch {
            Some(e) => {
                let (v, vty) = self.lower_expr(e)?;
                if matches!(result_ty, MirTy::Unit) {
                    Box::new([])
                } else if vty == result_ty {
                    Box::new([v])
                } else {
                    let coerced = self.coerce(v, &vty, &result_ty, Span::dummy()).unwrap_or(v);
                    Box::new([coerced])
                }
            }
            None => {
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
                let needs_retain = !value_is_fresh
                    && matches!(
                        ty,
                        MirTy::Object(_)
                            | MirTy::Array { .. }
                            | MirTy::Tuple(_)
                            | MirTy::Map { .. }
                            | MirTy::Optional(_)
                            | MirTy::Fn(_)
                            | MirTy::Str
                    );
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
        // Snapshot first to avoid the &mut self borrow conflict
        // on `self.fb` inside the release calls.
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
        let mut to_release: Vec<Binding> = Vec::new();
        for scope in self.env.scopes.iter().skip(frame_depth.saturating_sub(0)) {
            for (_n, b) in scope.iter().rev() {
                let keep = match b {
                    Binding::Local(_, ty) => needs_release(ty),
                    Binding::Ssa(_, ty) => needs_release(ty),
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
                Binding::Cell(cell_v, ty) => {
                    let zero = self.const_int(MirTy::I64, 0);
                    let v = self.fb.new_value(ty.clone());
                    self.fb.push_inst(Inst::ArrayLoad {
                        dst: v,
                        arr: cell_v,
                        idx: zero,
                    });
                    self.fb.push_inst(Inst::Release { value: v });
                }
            }
        }
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
        self.fb.set_terminator(Terminator::Br { dst: target, args: Box::new([]) });
        let dead = self.fb.new_block();
        self.fb.switch_to(dead);
        Ok((self.const_unit(), MirTy::Unit))
    }

    pub(super) fn lower_return(&mut self, value: Option<&Expr>) -> Result<(ValueId, MirTy), LowerError> {
        let v = match value {
            Some(e) => {
                let (vv, vty) = self.lower_expr(e)?;
                let ret_ty = self.ret_ty.clone();
                let coerced = if vty == ret_ty || matches!(ret_ty, MirTy::Unit) {
                    vv
                } else {
                    self.coerce(vv, &vty, &ret_ty, e.span).unwrap_or(vv)
                };
                Some(coerced)
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
        self.fb.set_terminator(Terminator::Return { value: v });
        let dead = self.fb.new_block();
        self.fb.switch_to(dead);
        Ok((self.const_unit(), MirTy::Unit))
    }
}
