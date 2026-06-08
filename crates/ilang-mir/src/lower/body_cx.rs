//! `BodyCx` — the borrowed-field bundle every per-fn-body lowering
//! pass receives. Carries the live function builder, environment,
//! plus borrowed views into the persistent `Lower` state (class /
//! enum / static tables, interface dispatch slots, REPL slot map,
//! etc.). Methods on `BodyCx` cover scope bookkeeping (lookup /
//! assignment / scope-exit release), the REPL slot bit-cast pair,
//! the block lowering driver, and a handful of per-expression
//! freshness predicates the retain / release logic consults.

use std::collections::HashMap;

use ilang_ast::{self as ast, Block as AstBlock, Expr, ExprKind, Span, Symbol, Type};

use crate::builder::FunctionBuilder;
use crate::inst::{FuncId, Inst, MirConst, Terminator, ValueId};
use crate::program::Function;
use crate::types::MirTy;

use super::env::{Binding, Env, LoopFrame};
use super::meta::{class_id_by_name, ClassMeta, EnumMeta, FnSig, PendingClosure};
use super::utils::ty_to_mir;
use super::LowerError;

/// `true` when the arm's body returns one of its own pattern
/// bindings via a `Var` tail (`some(v) { v }` /
/// `has(inner) { inner }` / `boxed { b: x } { x }`). Used by
/// `is_fresh_object_expr`'s Match arm to treat such arms as
/// fresh when the scrutinee is fresh — the arm lowerer's
/// `PatternBinding`-with-`needs_retain_on_tail` Retain has
/// already minted the +1 the caller will release.
fn arm_returns_own_binding(arm: &ast::MatchArm) -> bool {
    let mut binds: Vec<Symbol> = Vec::new();
    if let ast::PatternKind::Variant { bindings, .. } = &arm.pattern.kind {
        match bindings {
            ast::PatternBindings::Unit => {}
            ast::PatternBindings::Tuple(names) => {
                for n in names.iter() {
                    if n.as_str() != "_" {
                        binds.push(*n);
                    }
                }
            }
            ast::PatternBindings::Struct(pairs) => {
                for (_, bn) in pairs.iter() {
                    if bn.as_str() != "_" {
                        binds.push(*bn);
                    }
                }
            }
        }
    }
    if binds.is_empty() {
        return false;
    }
    let tail = match &arm.body.kind {
        ExprKind::Block(b) => match b.tail.as_deref() {
            Some(t) => t,
            None => return false,
        },
        _ => &arm.body,
    };
    matches!(&tail.kind, ExprKind::Var(n) if binds.contains(n))
}

/// `true` when `e`'s tail expression is a static `Str` literal.
/// `__release_string` is a no-op on rc=-1 (the static-cstring
/// marker), so a branch ending in a literal string contributes a
/// caller-side-Release-safe value to a Match / IfLet result —
/// same accounting role as a fresh string but without the alloc.
fn expr_tail_is_str_literal(e: &Expr) -> bool {
    let tail = match &e.kind {
        ExprKind::Block(b) => match b.tail.as_deref() {
            Some(t) => t,
            None => return false,
        },
        _ => e,
    };
    matches!(&tail.kind, ExprKind::Str(_))
}

pub(in crate::lower) struct BodyCx<'a> {
    pub(in crate::lower) fb: &'a mut FunctionBuilder,
    pub(in crate::lower) env: &'a mut Env,
    pub(in crate::lower) ret_ty: MirTy,
    pub(in crate::lower) fn_ids: &'a mut HashMap<Symbol, FuncId>,
    pub(in crate::lower) fn_sigs: &'a mut HashMap<Symbol, FnSig>,
    pub(in crate::lower) loops: Vec<LoopFrame>,
    /// The receiver class when lowering a method body (`Some(cid)`).
    pub(in crate::lower) this_class: Option<crate::types::ClassId>,
    pub(in crate::lower) classes: &'a [crate::program::ClassLayout],
    pub(in crate::lower) class_meta: &'a HashMap<crate::types::ClassId, ClassMeta>,
    pub(in crate::lower) interface_ids: &'a HashMap<Symbol, crate::types::ClassId>,
    pub(in crate::lower) iface_method_slots: &'a HashMap<(Symbol, Symbol), u32>,
    pub(in crate::lower) iface_method_sigs: &'a HashMap<(Symbol, Symbol), FnSig>,
    pub(in crate::lower) com_interfaces: &'a std::collections::HashSet<Symbol>,
    pub(in crate::lower) com_iface_slots: &'a HashMap<(Symbol, Symbol), u32>,
    pub(in crate::lower) enum_ids: &'a HashMap<Symbol, crate::types::EnumId>,
    pub(in crate::lower) enum_meta: &'a HashMap<crate::types::EnumId, EnumMeta>,
    pub(in crate::lower) enums: &'a [crate::program::EnumLayout],
    pub(in crate::lower) statics: &'a [crate::program::StaticSlot],
    /// Slot for pushing newly-discovered anonymous closures that need
    /// their bodies lowered after the current fn finishes.
    pub(in crate::lower) pending: &'a mut Vec<PendingClosure>,
    pub(in crate::lower) funcs: &'a mut Vec<Function>,
    pub(in crate::lower) anon_counter: &'a mut u32,
    /// Captures available in this scope (only set when lowering a
    /// closure body — maps a captured name to its `LoadCapture(i)`
    /// index plus type).
    pub(in crate::lower) captures_in_scope: Option<&'a HashMap<Symbol, (u32, MirTy)>>,
    /// Names whose captures are heap cells (the cell pointer was
    /// captured, not the value snapshot). Reads / writes go through
    /// `ArrayLoad` / `ArrayStore` after a `LoadCapture` on the cell
    /// pointer.
    pub(in crate::lower) cell_captures: Option<&'a std::collections::HashSet<Symbol>>,
    pub(in crate::lower) overloads: &'a HashMap<Symbol, Vec<Symbol>>,
    /// Names that should be allocated as heap cells inside this fn
    /// body (because some inner closure captures+mutates them).
    /// Populated by a per-fn-body pre-pass.
    pub(in crate::lower) cellify_set: &'a std::collections::HashSet<Symbol>,
    /// REPL persistent slots: name → (slot index, MirTy). Forwarded
    /// from `Lower::repl_slots`. Drives `__repl_load_slot` emission
    /// in `Var` lookup (any fn body) and `__repl_store_slot` after
    /// top-level `let`s in `__main` when `is_main_body` is set.
    pub(in crate::lower) repl_slots: &'a HashMap<Symbol, (u32, MirTy)>,
    /// True iff we're lowering `__main`'s body. Restricts top-level
    /// `let` → slot-store to that scope so a same-named local in a
    /// fn body doesn't accidentally clobber the REPL slot.
    pub(in crate::lower) is_main_body: bool,
    /// Locals whose value is an owned `host_mir_alloc` buffer for a
    /// CRepr (no-rc-header) struct. Populated when a `let` binding
    /// stores a fresh `new T()` of a CRepr class. `release_top_scope
    /// _objects` consults this when emitting the scope-exit Release
    /// for CRepr Locals — without it, a `let p = r.origin` (where
    /// `r.origin` is just a borrow into `r`'s buffer) would
    /// erroneously free part of `r`'s memory.
    pub(in crate::lower) crepr_owned_locals: std::collections::HashSet<crate::inst::LocalId>,
    /// Name of the top-level slot binding currently being assigned
    /// (Some(X) while we're inside the value of `let X = ...`).
    /// `lower_fn_expr` checks this to avoid snapshotting the X slot
    /// when X appears as a free var inside the FnExpr body — that's
    /// the canonical self-recursive closure pattern, where the slot
    /// hasn't been written yet at construction time. The Var
    /// lookup inside the body resolves through the slot at call
    /// time instead.
    pub(in crate::lower) binding_self_name: Option<Symbol>,
}

impl<'a> BodyCx<'a> {
    pub(in crate::lower) fn statics_by_id(
        &self,
        id: crate::inst::StaticSlotId,
    ) -> crate::program::StaticSlot {
        self.statics[id.0 as usize].clone()
    }

    /// Promote `class_meta.field_ty` (which may carry
    /// `MirTy::CReprEnum` for inline enum slots inside CRepr family
    /// structs) to the SSA-value MirTy a field read produces. The
    /// codegen-side `LoadField` for a `CReprEnum` slot calls
    /// `__enum_unit_get_checked` and returns a heap-box pointer, so
    /// downstream MIR ops see a regular `Enum(_)`. Callers that
    /// instead need the inline storage kind (e.g. AssignField's
    /// retain/release predicate) should keep the raw metadata.
    pub(in crate::lower) fn loaded_field_ty(meta_ty: &MirTy) -> MirTy {
        match meta_ty {
            MirTy::CReprEnum(e) => MirTy::Enum(*e),
            other => other.clone(),
        }
    }

    pub(in crate::lower) fn overloads_lookup(&self, name: Symbol) -> Option<Vec<Symbol>> {
        self.overloads.get(&name).cloned()
    }

    /// Bit-cast a value of `from` MirTy to a raw i64 for storage in a
    /// REPL slot. Heap pointers pass through; signed ints sextend;
    /// unsigned / bool zext; floats bitcast. Used by both the let-
    /// store path and any other slot-write site.
    pub(in crate::lower) fn value_to_i64(
        &mut self,
        v: ValueId,
        from: &MirTy,
    ) -> Result<ValueId, LowerError> {
        use crate::inst::CastKind;
        match from {
            MirTy::I64 | MirTy::U64 | MirTy::TypeHandle => Ok(v),
            MirTy::Object(_)
            | MirTy::Array { .. }
            | MirTy::Tuple(_)
            | MirTy::Map { .. }
            | MirTy::Set { .. }
            | MirTy::Promise(_)
            | MirTy::Optional(_)
            | MirTy::Fn(_)
            | MirTy::RawFn(_)
            | MirTy::Str
            // `CReprEnum` should never reach a REPL slot — LoadField
            // promotes it to `Enum` before the value flows further.
            // Treat it like `Enum` defensively (heap-pointer pass-
            // through) so a stray invariant break doesn't crash.
            | MirTy::Enum(_)
            | MirTy::CReprEnum(_)
            | MirTy::Weak(_)
            | MirTy::RawPtr { .. } => Ok(v),
            MirTy::I8 | MirTy::I16 | MirTy::I32 | MirTy::SSize => {
                let dst = self.fb.new_value(MirTy::I64);
                self.fb.push_inst(Inst::Cast { dst, kind: CastKind::IntResize, src: v });
                Ok(dst)
            }
            MirTy::U8 | MirTy::U16 | MirTy::U32 | MirTy::Size | MirTy::CChar | MirTy::Bool => {
                let dst = self.fb.new_value(MirTy::I64);
                // IntSignCross widens via uextend (zero-extend).
                self.fb.push_inst(Inst::Cast { dst, kind: CastKind::IntSignCross, src: v });
                Ok(dst)
            }
            MirTy::F64 | MirTy::F32 => {
                // No bitcast inst — funnel through the raw-ptr cast
                // which is a same-width identity at the clif level.
                // For F32 we'd lose the high bits; document as an
                // M1 limitation (REPL never round-trips f32 specially).
                let dst = self.fb.new_value(MirTy::I64);
                self.fb.push_inst(Inst::Cast { dst, kind: CastKind::PtrIntCast, src: v });
                Ok(dst)
            }
            MirTy::Unit => {
                // Unit slot: store a zero sentinel.
                Ok(self.const_int(MirTy::I64, 0))
            }
            MirTy::CVoid | MirTy::TypeVar(_) | MirTy::Simd { .. } => Err(LowerError::Other(format!(
                "REPL slot store: unsupported type {from}"
            ))),
        }
    }

    /// Reverse of `value_to_i64` — narrow a raw i64 back to the slot's
    /// declared MirTy. Heap pointers reinterpret via PtrIntCast (a
    /// no-op at the bit level); primitives narrow via Cast.
    pub(in crate::lower) fn i64_to_slot_value(
        &mut self,
        raw: ValueId,
        to: &MirTy,
    ) -> Result<ValueId, LowerError> {
        use crate::inst::CastKind;
        match to {
            MirTy::I64 | MirTy::U64 | MirTy::TypeHandle => Ok(raw),
            MirTy::Object(_)
            | MirTy::Array { .. }
            | MirTy::Tuple(_)
            | MirTy::Map { .. }
            | MirTy::Set { .. }
            | MirTy::Promise(_)
            | MirTy::Optional(_)
            | MirTy::Fn(_)
            | MirTy::RawFn(_)
            | MirTy::Str
            | MirTy::Enum(_)
            // Defensive — see the `value_to_i64` comment.
            | MirTy::CReprEnum(_)
            | MirTy::Weak(_)
            | MirTy::RawPtr { .. } => {
                let dst = self.fb.new_value(to.clone());
                self.fb.push_inst(Inst::Cast { dst, kind: CastKind::PtrIntCast, src: raw });
                Ok(dst)
            }
            MirTy::I8 | MirTy::I16 | MirTy::I32 | MirTy::SSize
            | MirTy::U8 | MirTy::U16 | MirTy::U32 | MirTy::Size | MirTy::CChar
            | MirTy::Bool => {
                let dst = self.fb.new_value(to.clone());
                self.fb.push_inst(Inst::Cast { dst, kind: CastKind::IntResize, src: raw });
                Ok(dst)
            }
            MirTy::F64 | MirTy::F32 => {
                let dst = self.fb.new_value(to.clone());
                self.fb.push_inst(Inst::Cast { dst, kind: CastKind::PtrIntCast, src: raw });
                Ok(dst)
            }
            MirTy::Unit => Ok(self.const_unit()),
            MirTy::CVoid | MirTy::TypeVar(_) | MirTy::Simd { .. } => Err(LowerError::Other(format!(
                "REPL slot load: unsupported type {to}"
            ))),
        }
    }

    /// `true` when a value of `ty` participates in ilang's ARC.
    /// Almost the same as `MirTy::is_heap`, except that
    /// `MirTy::Object(@com_iface)` is treated as a non-ARC handle
    /// (a bare COM pointer with no ARC header — lifetime is
    /// managed by IUnknown::Release at the user level, not by
    /// retain/release at scope boundaries).
    pub(in crate::lower) fn is_arc_heap(&self, ty: &MirTy) -> bool {
        if !ty.is_heap() {
            return false;
        }
        if let MirTy::Object(cid) = ty {
            let name = self.classes[cid.0 as usize].name;
            if self.com_interfaces.contains(&name) {
                return false;
            }
        }
        true
    }

    /// `true` when a slot of `ty` owns an rc share that the lower
    /// has to retain on borrow-in and release on overwrite. Same
    /// as `is_arc_heap`, with the additional exclusion of
    /// inline-struct `Object` reprs (CRepr / CPacked / CUnion):
    /// those have no ARC header, and `Retain` / `Release` on them
    /// would walk off the front of an inline payload. Use this
    /// at every rc-slot judgement site (`ExprKind::Assign`,
    /// `AssignField`, `AssignIndex`, `StructLit`, scope-exit
    /// `needs_release`, etc.).
    pub(in crate::lower) fn is_arc_slot(&self, ty: &MirTy) -> bool {
        if !self.is_arc_heap(ty) {
            return false;
        }
        if let MirTy::Object(cid) = ty {
            let layout = &self.classes[cid.0 as usize];
            if matches!(
                layout.repr,
                crate::program::ClassRepr::CRepr
                    | crate::program::ClassRepr::CPacked
                    | crate::program::ClassRepr::CUnion
            ) {
                return false;
            }
        }
        true
    }

    /// Type-only peek for a binding — no SSA materialisation, so it
    /// can be called from AST-walking branches without committing to
    /// the binding being used.
    pub(in crate::lower) fn peek_var_ty(&self, name: Symbol) -> Option<MirTy> {
        match self.env.lookup_binding(name)? {
            Binding::Ssa(_, t) | Binding::PatternBinding(_, t, _) => Some(t),
            Binding::Local(_, t) => Some(t),
            Binding::Cell(_, t) => Some(t),
        }
    }

    pub(in crate::lower) fn lookup_var(&mut self, name: Symbol) -> Option<(ValueId, MirTy)> {
        match self.env.lookup_binding(name)? {
            Binding::Ssa(v, t) | Binding::PatternBinding(v, t, _) => Some((v, t)),
            Binding::Local(lid, t) => {
                let v = self.fb.new_value(t.clone());
                self.fb.push_inst(Inst::UseLocal { dst: v, local: lid });
                Some((v, t))
            }
            Binding::Cell(cell_v, t) => {
                let zero = self.const_int(MirTy::I64, 0);
                let v = self.fb.new_value(t.clone());
                self.fb.push_inst(Inst::ArrayLoad { dst: v, arr: cell_v, idx: zero });
                Some((v, t))
            }
        }
    }

    /// Look up the cell pointer (without dereferencing) for a Cell
    /// binding. Used at closure-capture sites so the closure shares
    /// the same heap cell with the outer scope.
    pub(in crate::lower) fn lookup_cell_ptr(&self, name: Symbol) -> Option<(ValueId, MirTy)> {
        match self.env.lookup_binding(name)? {
            Binding::Cell(cell_v, t) => Some((cell_v, t)),
            _ => None,
        }
    }

    /// Assign to an existing binding. Returns whether the binding
    /// existed. For Local bindings, emits a `DefLocal`. For Ssa
    /// bindings, replaces the slot's payload.
    pub(in crate::lower) fn assign_var(&mut self, name: Symbol, v: ValueId, ty: MirTy) -> bool {
        // The rhs's MirTy may be wider than the binding's declared
        // type after `unify_numeric` promoted a mixed-sign / mixed-
        // width arithmetic operand. `i = i + 1` (i: i32) is the
        // canonical case: `i + 1` widens to i64, but the Local was
        // declared i32, so a raw `def_var` would fail the
        // Cranelift type check. Insert a narrowing coerce when the
        // shapes don't already match.
        match self.env.lookup_binding(name) {
            Some(Binding::Local(lid, slot_ty)) => {
                let coerced = if slot_ty == ty {
                    v
                } else {
                    self.coerce(v, &ty, &slot_ty, Span::dummy()).unwrap_or(v)
                };
                self.fb
                    .push_inst(Inst::DefLocal { local: lid, value: coerced });
                true
            }
            Some(Binding::Cell(cell_v, slot_ty)) => {
                let coerced = if slot_ty == ty {
                    v
                } else {
                    self.coerce(v, &ty, &slot_ty, Span::dummy()).unwrap_or(v)
                };
                let zero = self.const_int(MirTy::I64, 0);
                // The rc swap (release old, retain new) is the
                // caller's job — `ExprKind::Assign` already
                // snapshots the prior slot and applies the
                // fresh-aware retain. Doing the swap here as well
                // would double-account (a borrowed rhs would be
                // retained twice, a fresh rhs would lose its +1).
                self.fb.push_inst(Inst::ArrayStore {
                    arr: cell_v,
                    idx: zero,
                    value: coerced,
                });
                true
            }
            Some(Binding::Ssa(_, _)) | Some(Binding::PatternBinding(_, _, _)) => {
                self.env.rebind(name, v, ty);
                true
            }
            None => false,
        }
    }

    pub(in crate::lower) fn const_int(&mut self, ty: MirTy, n: i64) -> ValueId {
        let dst = self.fb.new_value(ty);
        self.fb.push_inst(Inst::Const { dst, value: MirConst::Int(n) });
        dst
    }

    pub(in crate::lower) fn const_unit(&mut self) -> ValueId {
        let dst = self.fb.new_value(MirTy::Unit);
        self.fb.push_inst(Inst::Const { dst, value: MirConst::Unit });
        dst
    }

    /// Standard refcount calling convention: callee returns +1 to
    /// caller. Three tail flavours need different handling:
    ///
    ///  (a) Fresh allocation (NewObject / Call / Binary on Str /
    ///      array literal / closure expr / …): rc=1 is already +1
    ///      for the caller. No retain.
    ///
    ///  (b) Var that resolves to a let-bound Local INSIDE the body
    ///      block: lower_block inserts a tail-alias retain to
    ///      balance the scope-exit release; the Local's +1
    ///      transfers to the caller. No extra retain.
    ///
    ///  (c) Var that resolves to an outer-scope binding (params
    ///      like `this`, captures) OR any non-Var aliased ref
    ///      (`this.field`, `arr[i]`, etc.): no +1 exists yet for
    ///      the caller — synthesise one so `c.inc()`-style chains
    ///      and `obj.field` returns hand the caller a real
    ///      ownership share. Without this the caller-side release
    ///      eventually frees while another binding still points at
    ///      the object.
    ///
    /// Returns true iff a callee-retain WILL be emitted for this
    /// tail; the actual emission must happen AFTER lower_block (so
    /// the ValueId is known) but the lookup runs BEFORE it (so the
    /// body block's let bindings haven't shadowed the outer scope
    /// yet — otherwise a tail Var that names a let-bound Local
    /// would lookup as "not Local" and we'd over-retain transient
    /// values like `make_map()`).
    pub(in crate::lower) fn callee_retain_decision(&self, tail_expr: &Expr) -> bool {
        if self.is_fresh_object_expr(tail_expr) {
            return false;
        }
        match &tail_expr.kind {
            ExprKind::Var(name) => match self.env.lookup_binding(*name) {
                // Resolves in the current (outer) scope — param or
                // earlier-block tail. Needs retain.
                Some(_) => true,
                // Doesn't resolve here ⇒ Var must be bound by a
                // `let` inside the body block, which lower_block
                // already retains for the caller.
                None => false,
            },
            // `Index` / `Field` tails are borrow expressions that
            // `lower_block` now retains BEFORE its scope-exit
            // releases — emitting another retain here would
            // double-count.
            ExprKind::Index { .. } | ExprKind::Field { .. } => false,
            _ => true,
        }
    }

    pub(in crate::lower) fn emit_callee_retain(&mut self, tail: &Option<(ValueId, MirTy)>) {
        if let Some((v, ty)) = tail.as_ref() {
            if self.is_arc_slot(ty) {
                self.fb.push_inst(Inst::Retain { value: *v });
            }
        }
    }

    pub(in crate::lower) fn finalise_return(
        &mut self,
        tail: Option<(ValueId, MirTy)>,
    ) -> Result<(), LowerError> {
        // Synthesise a placeholder return value when the lowerer is
        // sitting in a dead block (the user already issued `return`
        // earlier on the dominating path) but the fn signature
        // expects a non-unit return.
        let synth_placeholder = |this: &mut Self, ret_ty: &MirTy| -> ValueId {
            let v = this.fb.new_value(ret_ty.clone());
            let c = match ret_ty {
                MirTy::Bool => Inst::Const { dst: v, value: MirConst::Bool(false) },
                MirTy::F32 => Inst::Const { dst: v, value: MirConst::F32(0) },
                MirTy::F64 => Inst::Const { dst: v, value: MirConst::F64(0) },
                _ => Inst::Const { dst: v, value: MirConst::Int(0) },
            };
            this.fb.push_inst(c);
            v
        };
        // A heap-typed tail in a Unit-returning fn is a discarded
        // expression-statement — its value would otherwise leak
        // (lower_stmt's `Expr` arm releases discarded fresh heap
        // results, but the tail position doesn't go through that
        // path). Release it here, matching the stmt-discard rule.
        // Done up-front so the main match below can stay shaped
        // around `&self.ret_ty` without needing self-borrow for
        // the predicate.
        let tail = if matches!(self.ret_ty, MirTy::Unit) {
            match tail {
                Some((v, vty)) if self.is_arc_slot(&vty) => {
                    self.fb.push_inst(Inst::Release { value: v });
                    None
                }
                _ => None,
            }
        } else {
            tail
        };
        let value = match (&self.ret_ty, tail) {
            (MirTy::Unit, _) => None,
            // Tail expression has unit type (e.g. `return X` desugars
            // to a unit value in a dead block) — fabricate a real
            // return value so Cranelift's verifier is happy.
            (ret_ty, Some((_, MirTy::Unit))) => Some(synth_placeholder(self, &ret_ty.clone())),
            (ret_ty, Some((v, vty))) => {
                // Auto-coerce when the tail's type is a same-shape
                // integer / float that fits the declared return.
                let ret_ty_clone = ret_ty.clone();
                if vty == ret_ty_clone {
                    Some(v)
                } else {
                    let coerced = self.coerce(v, &vty, &ret_ty_clone, Span::dummy())
                        .unwrap_or(v);
                    Some(coerced)
                }
            }
            (ret_ty, None) => Some(synth_placeholder(self, &ret_ty.clone())),
        };
        self.fb.set_terminator(Terminator::Return { value });
        Ok(())
    }

    pub(in crate::lower) fn lower_block(
        &mut self,
        blk: &AstBlock,
    ) -> Result<Option<(ValueId, MirTy)>, LowerError> {
        self.lower_block_hinted(blk, None)
    }

    /// Like `lower_block`, but `tail_hint` (when set) is the type the
    /// block's tail expression flows into — used so a function body
    /// whose tail is a bare composite literal (`fn f(): i32[] { [..] }`)
    /// builds it with the right element widths instead of defaulting
    /// to i64/f64 cells. Only the tail is affected; nested blocks
    /// lowered while processing the statements get no hint.
    pub(in crate::lower) fn lower_block_hinted(
        &mut self,
        blk: &AstBlock,
        tail_hint: Option<&MirTy>,
    ) -> Result<Option<(ValueId, MirTy)>, LowerError> {
        self.env.enter_scope();
        // Stop treating let bindings as top-level once we descend
        // into a nested block — block-scoped `let x = ...` should
        // bind a fresh Local instead of overwriting any same-named
        // outer slot. lower_main calls `lower_stmt` directly on
        // its top-level stmts, so this flag flip only affects
        // recursion through `lower_expr(Block)`.
        let saved_main_body = self.is_main_body;
        self.is_main_body = false;
        for stmt in &blk.stmts {
            self.lower_stmt(stmt)?;
        }
        let tail = match &blk.tail {
            Some(e) => Some(match tail_hint.and_then(|h| self.lower_composite_with_hint(e, h)) {
                Some(res) => res?,
                None => self.lower_expr(e)?,
            }),
            None => None,
        };
        self.is_main_body = saved_main_body;
        // If the tail aliases a block-local heap binding, retain it
        // so the scope-exit releases below don't drop its rc to 0.
        // Fresh tails (`new T()` / call) are already +1 owners and
        // need no retain. Only retain when the tail expression is a
        // `Var` resolving to a binding in this block's scope —
        // otherwise we'd over-retain transient values that nothing
        // releases.
        // Closure captures self for use_local conflicts; precompute
        // the predicate against `tail` once so the match below can
        // stay shaped as guards without borrowing self twice.
        let tail_needs_retain_flag = tail
            .as_ref()
            .map(|(_, ty)| self.is_arc_slot(ty))
            .unwrap_or(false);
        let tail_alias_name = blk.tail.as_ref().and_then(|e| match &e.kind {
            ExprKind::Var(name) => Some(*name),
            _ => None,
        });
        // Retain when the tail Var resolves to one of:
        //   - `Local` / `Cell`: a let-bound storage slot whose
        //     scope-exit release drops the +1 we hand back here.
        //   - `PatternBinding(_, _, true)`: a match arm / if let
        //     binding whose surrounding arm will issue
        //     `Release(scrutinee)`. The cascade would free the
        //     inner the binding aliases — the retain pairs with
        //     that release so the returned value survives.
        //     `false` means the scrutinee was borrowed; cascade
        //     never runs and the outer `let`'s retain
        //     (Match-is-not-fresh path) already keeps the rc
        //     balanced — retaining here would double-account.
        //
        // `Binding::Ssa` (fn-entry params) is excluded — they
        // aren't owned by any release sweep, so retaining for them
        // would leave a permanent +1 on the returned cell
        // (per-call leak in `fn f(x: Box): Box { x }`).
        //
        // Captured cells from the enclosing scope are missing from
        // this body's `env` (closures use `captures_in_scope` for
        // those instead), but their slot share is still owned by
        // the outer scope, so a tail `Var` reading one needs the
        // same +1 — without it the caller would see the captured
        // cell's rc=1 share, release it, and the cell would
        // dangle on the next read.
        let tail_aliases_local = tail_alias_name
            .and_then(|name| {
                if let Some(b) = self.env.lookup_binding(name) {
                    return Some(match b {
                        Binding::Local(..) | Binding::Cell(..) => true,
                        Binding::PatternBinding(_, _, needs_retain) => needs_retain,
                        Binding::Ssa(..) => false,
                    });
                }
                if let Some(caps) = self.captures_in_scope {
                    if caps.get(&name).is_some() {
                        let is_cell = self
                            .cell_captures
                            .map(|s| s.contains(&name))
                            .unwrap_or(false);
                        return Some(is_cell);
                    }
                }
                None
            })
            .unwrap_or(false);
        // Heap-typed tails that **borrow** into a still-live owner
        // (e.g. `arr[i]` reads from `arr`'s element area;
        // `obj.field` reads from `obj`'s slot) need an extra +1
        // here, BEFORE the scope-exit releases below — otherwise
        // the borrowed pointer would dangle by the time the caller
        // dereferences it. Restrict to the syntactic shapes we
        // know are borrows (`Index` / `Field`); other non-`Var`
        // shapes (calls, `super(...)`, literals) already manage
        // their own ownership and would over-retain.
        let tail_is_borrow = blk.tail.as_ref().is_some_and(|e| {
            matches!(&e.kind, ExprKind::Index { .. } | ExprKind::Field { .. })
        });
        let tail = match tail {
            Some((v, ty))
                if tail_needs_retain_flag
                    && (tail_aliases_local || tail_is_borrow) =>
            {
                // When the block has a hint that doesn't match the
                // tail's runtime kind, apply the coerce BEFORE the
                // retain so the Retain dispatches to the hint's
                // runtime convention. The only mismatch that needs
                // this today is Object → Weak: `retain v_obj` would
                // bump strong rc on the value the scope-exit release
                // is about to drop to zero, leaving the caller with
                // an orphaned strong +1 typed as Weak (caller's
                // `__release_weak` doesn't decrement strong rc, so
                // the object body leaks). Coercing first lets Retain
                // emit `__retain_weak`, matching the caller side.
                let span = blk.tail.as_ref().map(|e| e.span).unwrap_or(Span::dummy());
                let (v_use, ty_use) = match (tail_hint, &ty) {
                    (Some(h @ MirTy::Weak(_)), MirTy::Object(_)) => {
                        match self.coerce(v, &ty, h, span) {
                            Ok(coerced) => (coerced, h.clone()),
                            Err(_) => (v, ty.clone()),
                        }
                    }
                    _ => (v, ty.clone()),
                };
                self.fb.push_inst(Inst::Retain { value: v_use });
                Some((v_use, ty_use))
            }
            other => other,
        };
        // CRepr Locals carry no rc — Retain above is a no-op for
        // them. Transfer ownership of the tail-aliased local to
        // the caller by un-marking it before scope exit, otherwise
        // `release_top_scope_objects` would free the buffer the
        // caller is about to use.
        if let Some(name) = tail_alias_name {
            if let Some(Binding::Local(lid, _)) = self.env.lookup_binding(name) {
                self.crepr_owned_locals.remove(&lid);
            }
        }
        self.release_top_scope_objects();
        self.env.exit_scope();
        Ok(tail)
    }

    pub(in crate::lower) fn is_fresh_object_expr(&self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::New { .. }
            | ExprKind::StructLit { .. }
            | ExprKind::Call { .. }
            | ExprKind::MethodCall { .. }
            // SuperCall returns `this` aliased — init's calling
            // convention does NOT add a +1 (see lower_method's
            // is_init terminator special-case which sets the
            // terminator directly to `return this_v` with no
            // retain). Treating super() as fresh would emit a
            // bogus release-on-discard that drops rc below the
            // alloc's +1 and triggers free-during-init.
            // Binary / Unary on heap operands (string +) lowers to a
            // host helper (str_concat etc.) that returns a freshly
            // leak_cstring'd, registry-tracked buffer. Treating them
            // as fresh prevents the let-bind retain from leaking the
            // intermediate. For non-heap operand types, "fresh" is a
            // no-op decision so this is safe to widen unconditionally.
            | ExprKind::Binary { .. }
            | ExprKind::Unary { .. }
            // Aggregate / heap literals — each lowers to a fresh
            // alloc with rc=1 already in place.
            | ExprKind::Array(_)
            | ExprKind::Tuple(_)
            | ExprKind::MapLit(_)
            | ExprKind::Some(_)
            | ExprKind::Await(_)
            | ExprKind::None
            | ExprKind::EnumCtor { .. }
            | ExprKind::FnExpr { .. } => true,
            // Indexing a fresh tuple / array donates ownership of the
            // selected element to the caller — the lowerer retains
            // that element exactly once on the fresh-receiver path.
            ExprKind::Index { obj, .. } => self.is_fresh_object_expr(obj),
            // A block whose tail is itself fresh produces a fresh
            // value (the inner block scope-releases its own locals).
            ExprKind::Block(b) => b
                .tail
                .as_ref()
                .map(|t| self.is_fresh_object_expr(t))
                .unwrap_or(false),
            // `if`/`match` carry the freshness of all branches; treat
            // them conservatively — fresh only if every branch's tail
            // is fresh. Non-fresh would produce an over-retain rather
            // than a use-after-free, so this is the safe direction.
            ExprKind::If { then_branch, else_branch, .. } => {
                let then_fresh = then_branch
                    .tail
                    .as_ref()
                    .map(|t| self.is_fresh_object_expr(t))
                    .unwrap_or(false);
                let else_fresh = else_branch
                    .as_ref()
                    .map(|e| self.is_fresh_object_expr(e))
                    .unwrap_or(false);
                then_fresh && else_fresh
            }
            // Same all-arms-fresh rule as `If` above. Divergent arms
            // (`return` / `panic`) aren't tracked here and read as
            // non-fresh; that's strictly conservative — a match whose
            // only non-diverging arm is fresh will miss the
            // optimisation, but never over-frees.
            //
            // Additionally, when the scrutinee is fresh and an arm
            // hands its own pattern binding straight back as the tail
            // (`some(v) { v }`, `has(inner) { inner }`), the arm's
            // `Binding::PatternBinding(_, _, needs_retain_on_tail=true)`
            // path has already emitted a `Retain(bv)` (via the
            // `lower_block_hinted` tail-Var pair). The arm's effective
            // result therefore owns its own +1 — fresh from the
            // caller's standpoint — even though the AST tail is a
            // plain `Var`. Treat that case as fresh so that
            // `caller-releases-fresh` (call_arg release / `let _ = ...`
            // release / etc.) closes the loop instead of double-
            // retaining via the stmt.rs::Let path.
            ExprKind::Match { scrutinee, arms } => {
                let scrut_fresh = self.is_fresh_object_expr(scrutinee);
                !arms.is_empty()
                    && arms.iter().all(|arm| {
                        self.is_fresh_object_expr(&arm.body)
                            || (scrut_fresh && arm_returns_own_binding(arm))
                            || expr_tail_is_str_literal(&arm.body)
                    })
            }
            // Mirror Match: `if let some(name) = scrut { ... } else { ... }`
            // is fresh iff both branches are fresh. The then branch can
            // also count as fresh when the scrutinee is fresh and its
            // tail is a `Var(name)` (the pattern binding) — the
            // PatternBinding tail-Var Retain has already minted a +1
            // the caller will release. Str-literal tails (rc=-1)
            // count too — `__release_string` is a no-op on static
            // literals, so handing one back as a branch result keeps
            // the caller's `Release` safe.
            ExprKind::IfLet { name, expr: scrut, then_branch, else_branch } => {
                let scrut_fresh = self.is_fresh_object_expr(scrut);
                let then_fresh = then_branch
                    .tail
                    .as_ref()
                    .map(|t| {
                        self.is_fresh_object_expr(t)
                            || (scrut_fresh
                                && matches!(&t.kind, ExprKind::Var(n) if *n == *name))
                            || matches!(&t.kind, ExprKind::Str(_))
                    })
                    .unwrap_or(false);
                let else_fresh = else_branch
                    .as_ref()
                    .map(|e| {
                        self.is_fresh_object_expr(e)
                            || expr_tail_is_str_literal(e)
                    })
                    .unwrap_or(false);
                then_fresh && else_fresh
            }
            // A bare reference to a top-level `fn` lowers to a
            // `MakeClosure` (trampoline) — fresh allocation with
            // rc=1 each time. A local / param / captured Var of
            // the same name shadows the top-level fn and is NOT
            // fresh (the lookup_binding path lowers to a borrow).
            ExprKind::Var(name) => {
                self.env.lookup_binding(*name).is_none()
                    && self.fn_ids.contains_key(name)
            }
            _ => false,
        }
    }

    pub(in crate::lower) fn release_top_scope_objects(&mut self) {
        let scope: Vec<(Symbol, Binding)> = self
            .env
            .scopes
            .last()
            .cloned()
            .unwrap_or_default();
        let needs_release = |ty: &MirTy| ty.is_heap();
        for (_name, binding) in scope.into_iter().rev() {
            match binding {
                Binding::Local(lid, ty) if needs_release(&ty) => {
                    // For CRepr Locals, only emit Release if this
                    // Local owns the underlying buffer. Borrowed
                    // CRepr values (e.g. nested-field reads) stay
                    // alive with their parent and must NOT be
                    // freed independently.
                    if let MirTy::Object(cid) = &ty {
                        let layout = &self.classes[cid.0 as usize];
                        let is_crepr = matches!(
                            layout.repr,
                            crate::program::ClassRepr::CRepr
                                | crate::program::ClassRepr::CPacked
                                | crate::program::ClassRepr::CUnion
                        );
                        if is_crepr && !self.crepr_owned_locals.contains(&lid) {
                            continue;
                        }
                        // `@com interface` values are bare COM
                        // handles with no ARC header — Release
                        // here would scribble random memory at
                        // `addr - 32`. Lifetime is managed by
                        // IUnknown::Release at the user level.
                        if self.com_interfaces.contains(&layout.name) {
                            continue;
                        }
                    }
                    let v = self.fb.new_value(ty.clone());
                    self.fb.push_inst(Inst::UseLocal { dst: v, local: lid });
                    self.fb.push_inst(Inst::Release { value: v });
                }
                Binding::Ssa(v, ty) if needs_release(&ty) => {
                    if let MirTy::Object(cid) = &ty {
                        let layout = &self.classes[cid.0 as usize];
                        if self.com_interfaces.contains(&layout.name) {
                            continue;
                        }
                    }
                    self.fb.push_inst(Inst::Release { value: v });
                }
                Binding::PatternBinding(..) => {
                    // Match-arm / if-let bindings borrow into the
                    // scrutinee cell — release is the scrutinee's
                    // job (the arm lowerer pairs
                    // `Release(scrutinee)` with the matching
                    // `Retain` from `tail_aliases_local` when the
                    // body returns the binding directly).
                }
                Binding::Cell(..) => {
                    // A cell is a heap 1-element array shared between
                    // this scope and every closure that captured it
                    // (shared mutable capture). We must NOT release its
                    // contents here: a captured closure may outlive the
                    // scope and still read/write the cell, so a
                    // scope-exit release would free a value the closure
                    // still points at (use-after-free). Closures leak
                    // their captured cells today (see jit_setup's
                    // `__register_closure_capture` "leak for now"), so
                    // the cell + its contents leak rather than dangle.
                }
                _ => {}
            }
        }
    }

    pub(in crate::lower) fn resolve_ty(&self, t: &Type) -> Result<MirTy, LowerError> {
        match t {
            Type::Object(name) => {
                // Find class first.
                if let Some(cid) = class_id_by_name(self.classes, self.class_meta, *name) {
                    return Ok(MirTy::Object(cid));
                }
                if let Some(eid) = self.enum_ids.get(name) {
                    return Ok(MirTy::Enum(*eid));
                }
                Err(LowerError::Other(format!("unknown type: {name}")))
            }
            // `*T` / `*const T`. Recurse so user-defined `@extern(C)`
            // structs survive in the pointer's inner type. The
            // `ty_to_mir` fallback would silently degrade `*FooStruct`
            // to `*void` because that helper doesn't know about the
            // class registry, which breaks field access on raw
            // pointers to CRepr structs.
            Type::RawPtr { is_const, inner } => Ok(MirTy::RawPtr {
                is_const: *is_const,
                inner: Box::new(self.resolve_ty(inner)?),
            }),
            Type::Enum(name) => self.enum_ids.get(name).copied().map(MirTy::Enum).ok_or_else(
                || LowerError::Other(format!("unknown enum {name}")),
            ),
            Type::Array { elem, fixed } => Ok(MirTy::Array {
                elem: Box::new(self.resolve_ty(elem)?),
                len: *fixed,
            }),
            Type::Tuple(elems) => {
                let mut out = Vec::with_capacity(elems.len());
                for e in elems.iter() {
                    out.push(self.resolve_ty(e)?);
                }
                Ok(MirTy::Tuple(out.into_boxed_slice()))
            }
            Type::Optional(inner) => Ok(MirTy::Optional(Box::new(self.resolve_ty(inner)?))),
            Type::Weak(inner) => match &**inner {
                Type::Object(cname) => self
                    .classes
                    .iter()
                    .find(|c| c.name == *cname)
                    .map(|c| MirTy::Weak(c.id))
                    .ok_or_else(|| LowerError::Other(format!("unknown class for weak: {cname}"))),
                _ => Err(LowerError::Other("`.weak` only applies to class types".into())),
            },
            Type::Generic(g) if g.base.as_str() == "Map" && g.args.len() == 2 => Ok(MirTy::Map {
                key: Box::new(self.resolve_ty(&g.args[0])?),
                val: Box::new(self.resolve_ty(&g.args[1])?),
            }),
            Type::Generic(g) if g.base.as_str() == "Set" && g.args.len() == 1 => {
                Ok(MirTy::Set { elem: Box::new(self.resolve_ty(&g.args[0])?) })
            }
            Type::Generic(g) if g.base.as_str() == "Promise" && g.args.len() == 1 => {
                Ok(MirTy::Promise(Box::new(self.resolve_ty(&g.args[0])?)))
            }
            Type::Generic(g)
                if g.base.as_str() == "ObjCBlock"
                    && g.args.len() == 1
                    && matches!(g.args[0], Type::Fn(_)) =>
            {
                Ok(MirTy::I64)
            }
            Type::Generic(g) => self
                .enum_ids
                .get(&g.base)
                .copied()
                .map(MirTy::Enum)
                .ok_or_else(|| LowerError::Unsupported("user-defined generic types")),
            Type::Fn(ft) => {
                let mut params = Vec::with_capacity(ft.params.len());
                for p in ft.params.iter() {
                    params.push(self.resolve_ty(p)?);
                }
                let ret = self.resolve_ty(&ft.ret)?;
                Ok(MirTy::Fn(Box::new(crate::types::MirFnTy {
                    params: params.into_boxed_slice(),
                    ret,
                })))
            }
            other => ty_to_mir(other),
        }
    }
}
