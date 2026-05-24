//! Anonymous-fn expression (`fn(x: T) { ... }`) lowering on
//! `BodyCx`.
//!
//! `lower_fn_expr` runs after the hoist pass has lifted every
//! `FnExpr` to a top-level synthetic fn. The body's free-var set
//! becomes the closure's capture layout; we materialise a fresh
//! `Inst::NewClosure` that bundles the synthesized fn pointer with
//! the captures and yields a fn-typed value.

use ilang_ast::{self as ast, Span, Symbol, Type};

use crate::inst::{FuncId, FuncRef, Inst, ValueId};
use crate::types::MirTy;

use super::collect::{collect_free_vars_block, collect_mut_assigned_block};
use super::utils::{placeholder_function, retain_if_heap};
use super::{BodyCx, FnSig, LowerError, PendingClosure};

impl<'a> BodyCx<'a> {
    pub(super) fn lower_fn_expr(
        &mut self,
        params: &[ast::Param],
        ret: Option<&Type>,
        body: &ast::Block,
        span: Span,
    ) -> Result<(ValueId, MirTy), LowerError> {
        // Collect free variables in the FnExpr body.
        let mut bound: std::collections::HashSet<Symbol> =
            params.iter().map(|p| p.name).collect();
        let mut frees: Vec<Symbol> = Vec::new();
        collect_free_vars_block(body, &mut bound, &mut frees);

        // Names that this closure (transitively, through nested
        // FnExprs in its body) writes via `Assign`. These need cell
        // capture so writes persist across calls. Names not in this
        // set are captured by value snapshot — independent per
        // closure (B1 semantics: sibling closures sharing the same
        // outer name do NOT see each other's writes).
        let mut writes: std::collections::HashSet<Symbol> =
            std::collections::HashSet::new();
        collect_mut_assigned_block(body, &mut writes);
        // The closure's own params are local mutable, not captured.
        for p in params.iter() {
            writes.remove(&p.name);
        }

        // Filter out names that aren't bound in the surrounding scope
        // (top-level fns / classes / enums / statics — they're
        // resolved globally, not captured).
        let mut captures: Vec<crate::program::EnvCapture> = Vec::new();
        let mut capture_vals: Vec<ValueId> = Vec::new();
        for name in frees {
            let needs_cell = writes.contains(&name);
            // 1) Source already has a cell binding in current scope —
            // share that cell directly (whether or not we write).
            if let Some((cell_v, inner_ty)) = self.lookup_cell_ptr(name) {
                capture_vals.push(cell_v);
                captures.push(crate::program::EnvCapture {
                    name,
                    ty: inner_ty,
                    is_cell: true,
                });
                continue;
            }
            // 2) Source is a captured cell from the enclosing closure
            // body — load the cell pointer (not its inner value) and
            // forward it.
            if let Some(caps) = self.captures_in_scope {
                if let Some((idx, cty)) = caps.get(&name).cloned() {
                    let outer_is_cell = self
                        .cell_captures
                        .map(|s| s.contains(&name))
                        .unwrap_or(false);
                    if outer_is_cell {
                        let cell_v = self.fb.new_value(MirTy::I64);
                        self.fb.push_inst(Inst::LoadCapture { dst: cell_v, idx });
                        capture_vals.push(cell_v);
                        captures.push(crate::program::EnvCapture {
                            name,
                            ty: cty,
                            is_cell: true,
                        });
                        continue;
                    }
                    // Outer capture is a value snapshot — load it.
                    let v = self.fb.new_value(cty.clone());
                    self.fb.push_inst(Inst::LoadCapture { dst: v, idx });
                    if needs_cell {
                        // Allocate a fresh private cell initialised
                        // from the snapshot. The cell owns its share
                        // of `v`, so retain heap-typed inners before
                        // the store — otherwise the outer scope's
                        // eventual release frees the cell's only
                        // backing object.
                        retain_if_heap(&mut self.fb, v, &cty);
                        let cell_ty = MirTy::Array {
                            elem: Box::new(cty.clone()),
                            len: None,
                        };
                        let cell_v = self.fb.new_value(cell_ty);
                        self.fb.push_inst(Inst::NewArray {
                            dst: cell_v,
                            elem: cty.clone(),
                            items: Box::new([v]),
                        });
                        capture_vals.push(cell_v);
                        captures.push(crate::program::EnvCapture {
                            name,
                            ty: cty,
                            is_cell: true,
                        });
                    } else {
                        capture_vals.push(v);
                        captures.push(crate::program::EnvCapture {
                            name,
                            ty: cty,
                            is_cell: false,
                        });
                    }
                    continue;
                }
            }
            // 3) Source is a regular local / SSA in current scope.
            if let Some((v, ty)) = self.lookup_var(name) {
                if needs_cell {
                    // Allocate a private cell initialised from the
                    // snapshot of the current value. The outer scope
                    // does NOT see writes (sibling-closure isolation).
                    retain_if_heap(&mut self.fb, v, &ty);
                    let cell_ty = MirTy::Array {
                        elem: Box::new(ty.clone()),
                        len: None,
                    };
                    let cell_v = self.fb.new_value(cell_ty);
                    self.fb.push_inst(Inst::NewArray {
                        dst: cell_v,
                        elem: ty.clone(),
                        items: Box::new([v]),
                    });
                    capture_vals.push(cell_v);
                    captures.push(crate::program::EnvCapture {
                        name,
                        ty,
                        is_cell: true,
                    });
                } else {
                    capture_vals.push(v);
                    captures.push(crate::program::EnvCapture {
                        name,
                        ty,
                        is_cell: false,
                    });
                }
                continue;
            }
            // 4) Source is a top-level slot-backed binding. Snapshot
            //    its current value at construction time so the
            //    closure body sees the captured value, not whatever
            //    the slot happens to hold at call time. (Mirrors
            //    standard "capture by value" semantics for fn-expr
            //    free vars.)
            //
            //    Self-recursive closures (`let f = fn(...) { f(...)
            //    }`) are the exception: at construction the slot
            //    hasn't been written yet, so a snapshot would
            //    capture 0/null and a later call would crash.
            //    Detect via `binding_self_name` (set by lower_stmt
            //    while lowering the let value); skip the capture so
            //    the body's `Var` lookup hits the slot fallback at
            //    call time, which is the standard "late binding"
            //    semantics expected for self-reference.
            if Some(name) == self.binding_self_name {
                continue;
            }
            if let Some((idx, slot_ty)) = self.repl_slots.get(&name).cloned() {
                let idx_v = self.const_int(MirTy::I64, idx as i64);
                let raw = self.fb.new_value(MirTy::I64);
                self.fb.push_inst(Inst::Call {
                    dst: Some(raw),
                    callee: FuncRef::Builtin(Symbol::intern("$repl.loadSlot")),
                    args: Box::new([idx_v]),
                });
                let v = self.i64_to_slot_value(raw, &slot_ty)?;
                capture_vals.push(v);
                captures.push(crate::program::EnvCapture {
                    name,
                    ty: slot_ty,
                    is_cell: false,
                });
                continue;
            }
            // Names that aren't local and aren't captures from an
            // outer closure are assumed global (top-level fn / class /
            // enum / static); they need no env entry.
        }

        // Allocate a fresh func id and build a placeholder. Resolve
        // param/ret types now so the synthesised fn has a stable sig
        // for any subsequent callers.
        let n = *self.anon_counter;
        *self.anon_counter += 1;
        let name = Symbol::intern(&format!("__anon_fn_{n}"));
        let id = FuncId(self.funcs.len() as u32);
        self.funcs.push(placeholder_function(name));
        self.fn_ids.insert(name, id);

        let param_tys: Vec<(Symbol, MirTy)> = params
            .iter()
            .map(|p| Ok((p.name, self.resolve_ty(&p.ty)?)))
            .collect::<Result<_, LowerError>>()?;
        let ret_ty = match ret {
            Some(t) => self.resolve_ty(t)?,
            None => MirTy::Unit,
        };

        // The runtime fn signature is `(params..., env)` — the env
        // pointer is passed as a hidden last param at the ABI level.
        // For MIR sig purposes we keep the user-visible params.
        let sig_params: Vec<MirTy> = param_tys.iter().map(|(_, t)| t.clone()).collect();
        self.fn_sigs.insert(
            name,
            FnSig {
                params: sig_params,
                ret: ret_ty.clone(),
            },
        );

        // Push to the pending queue — body lowered after the outer
        // fn finishes.
        self.pending.push(PendingClosure {
            func_id: id,
            name,
            params: param_tys,
            ret: ret_ty.clone(),
            captures: captures.clone(),
            body: body.clone(),
            span,
            enclosing_this_class: self.this_class,
        });

        // Emit the MakeClosure instruction.
        let fn_ty = MirTy::Fn(Box::new(crate::types::MirFnTy {
            params: captures
                .iter()
                .map(|c| c.ty.clone())
                .chain(std::iter::empty()) // captures are env, not user-visible params
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            ret: ret_ty.clone(),
        }));
        // For simplicity the displayed Fn type is the fn signature
        // sans env. Captures' types live in the EnvLayout on the
        // synthesised fn (set when lowering its body).
        let fn_ty = match fn_ty {
            // Replace the params slot with the user-visible params.
            MirTy::Fn(ft) => {
                let _ = ft;
                let user_params: Box<[MirTy]> = params
                    .iter()
                    .map(|p| self.resolve_ty(&p.ty))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_boxed_slice();
                MirTy::Fn(Box::new(crate::types::MirFnTy {
                    params: user_params,
                    ret: ret_ty,
                }))
            }
            other => other,
        };
        // Retain every heap-typed capture — the closure shares
        // ownership with the outer scope, so its captures must
        // outlive any scope-exit release of the source binding.
        // Cell captures hold a shared cell pointer (the cell itself
        // is a heap array allocated for the FnExpr) and are
        // refcounted at the cell layer separately.
        for (cv, c) in capture_vals.iter().zip(captures.iter()) {
            if self.is_arc_heap(&c.ty) && !c.is_cell {
                self.fb.push_inst(Inst::Retain { value: *cv });
            }
        }
        let dst = self.fb.new_value(fn_ty.clone());
        self.fb.push_inst(Inst::MakeClosure {
            dst,
            func: id,
            captures: capture_vals.into_boxed_slice(),
        });
        Ok((dst, fn_ty))
    }
}
