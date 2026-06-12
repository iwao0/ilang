//! `BodyCx` — the borrowed-field bundle every per-fn-body lowering
//! pass receives. Carries the live function builder, environment,
//! plus borrowed views into the persistent `Lower` state (class /
//! enum / static tables, interface dispatch slots, REPL slot map,
//! etc.). Methods on `BodyCx` cover scope bookkeeping (lookup /
//! assignment / scope-exit release), the REPL slot bit-cast pair,
//! the block lowering driver, and a handful of per-expression
//! freshness predicates the retain / release logic consults.

use std::collections::HashMap;

use ilang_ast::{Block as AstBlock, Expr, ExprKind, Span, Symbol, Type};

use crate::builder::FunctionBuilder;
use crate::inst::{FuncId, Inst, MirConst, Terminator, ValueId};
use crate::program::Function;
use crate::types::MirTy;

use super::env::{Binding, Env, LoopFrame};
use super::meta::{class_id_by_name, ClassMeta, EnumMeta, FnSig, PendingClosure};
use super::utils::ty_to_mir;
use super::LowerError;



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
    /// SSA values that the function-return terminator should free
    /// after the ABI consumes them (sret memcpy / chunk load / HFA
    /// spread). Populated when a fn body's tail is a fresh-allocated
    /// CRepr Object whose owned local would otherwise leak — codegen
    /// reads this through `Terminator::Return.release_value`. The
    /// local is removed from `crepr_owned_locals` at the same time so
    /// `release_top_scope_objects` doesn't double-free it before the
    /// return.
    pub(in crate::lower) crepr_return_owned: std::collections::HashSet<crate::ValueId>,
    /// Set by `lower_block_hinted` when its tail-alias/borrow
    /// Retain fired (the handed-back value owns a +1). Join sites
    /// (`lower_if`, the match arms) read it right after lowering a
    /// branch/arm body to decide whether the branch result is
    /// already OWNED — non-owned heap results get a join-side
    /// Retain so every `if`/`match` value uniformly owns its +1
    /// (mixed-freshness joins used to over-retain the fresh side:
    /// one leaked object per evaluation).
    pub(in crate::lower) last_block_tail_owned: bool,
    /// Set by `lower_arg_to` when its target coerce minted a NEW
    /// heap value (`T → T?` wrap, fixed-array copy): the lowered
    /// argument owns a fresh +1 even though the source expression
    /// reads as borrowed. Call sites OR this into their fresh-arg
    /// post-release decision — without it `takeOpt(h.b)` leaked
    /// the wrapper cell (and its retained inner share) per call.
    pub(in crate::lower) last_arg_wrapped: bool,
    /// Fresh match / if-let scrutinees whose arm body is currently
    /// being lowered, with the env depth at registration. The arm
    /// lowerer releases a fresh scrutinee at arm exit, but an early
    /// `return` / `break` / `continue` inside the body bypasses that
    /// — the exit sweeps release these instead (`return`: all of
    /// them; loop jumps: only those registered at or above the
    /// loop's entry depth).
    pub(in crate::lower) live_fresh_scrutinees: Vec<(crate::ValueId, usize)>,
    /// Env depth where the fn BODY's scopes begin — everything at
    /// `>= this` is swept by an early `return`. Set by
    /// `lower_block_for_fn_body` to `env.scopes.len()` right before
    /// it enters the body scope: a fn with params has them in scope
    /// 0 (base 1), a zero-param fn has NO param scope and its body
    /// IS scope 0 (base 0) — a fixed `skip(1)` missed every binding
    /// in zero-param fns. `__main` never sets it (usize::MAX at
    /// construction there) so a hypothetical top-level `return`
    /// can't double-release the slot-managed top-level lets.
    pub(in crate::lower) return_sweep_base: usize,
    /// Name of the top-level slot binding currently being assigned
    /// (Some(X) while we're inside the value of `let X = ...`).
    /// `lower_fn_expr` checks this to avoid snapshotting the X slot
    /// when X appears as a free var inside the FnExpr body — that's
    /// the canonical self-recursive closure pattern, where the slot
    /// hasn't been written yet at construction time. The Var
    /// lookup inside the body resolves through the slot at call
    /// time instead.
    pub(in crate::lower) binding_self_name: Option<Symbol>,
    /// `Some((name, fn_ty))` inside a closure body whose source
    /// references its own (non-slot) binding name. `lower_var_expr`
    /// resolves that name to `Inst::ClosureSelf` (the hidden env
    /// param) typed as `fn_ty`. None everywhere else.
    pub(in crate::lower) closure_self: Option<(Symbol, MirTy)>,
    /// True only during the **immediate** `lower_block_hinted` call
    /// that lowers an fn body's outermost block (the one whose tail
    /// becomes the function's return value). Sub-block calls
    /// (if-then, if-else, match arms, loop bodies, …) recurse
    /// through the same `lower_block_hinted` but must NOT apply the
    /// borrow-tail retain — those sub-block tails feed a `br
    /// block-arg` whose receiving join block re-issues its own
    /// retain at the fn-body level, so an arm-local retain would
    /// double-count. Set by `lower_block_for_fn_body` and cleared
    /// on entry to `lower_block_hinted` so any nested call is
    /// treated as a sub-block.
    pub(in crate::lower) in_fn_body_top: bool,
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

    /// `true` when a fresh heap arg's +1 must be released by the
    /// caller after a call (named fn / closure / implicit-this /
    /// object method / `new` init — every shape that takes args by
    /// borrow). One list for all five call shapes; the per-site
    /// copies diverged once (none of them had `Promise`, so a fresh
    /// promise passed as an argument leaked, visible as its settled
    /// value never dropping).
    pub(in crate::lower) fn fresh_arg_needs_post_release(ty: &MirTy) -> bool {
        matches!(
            ty,
            MirTy::Object(_)
                | MirTy::Fn(_)
                | MirTy::Array { .. }
                | MirTy::Tuple(_)
                | MirTy::Map { .. }
                | MirTy::Optional(_)
                | MirTy::Str
                | MirTy::Enum(_)
                | MirTy::Set { .. }
                | MirTy::Weak(_)
                | MirTy::Promise(_)
        )
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
    /// `true` when `ty` is `MirTy::Object(cid)` whose class layout
    /// is one of the inline-struct reprs (`CRepr` / `CPacked` /
    /// `CUnion`). These don't carry an ARC header, so they're
    /// excluded from `is_arc_slot` — but the backing buffer was
    /// `__mir_alloc`'d on `new T()`, so a discarded fresh value
    /// still needs a paired `Release` to reach `__mir_free` in
    /// `lower_inst/arc.rs::Release`. Distinct from `is_arc_heap`
    /// (which means "real refcount, real cascade").
    pub(in crate::lower) fn is_crepr_object_kind(&self, ty: &MirTy) -> bool {
        if let MirTy::Object(cid) = ty {
            let layout = &self.classes[cid.0 as usize];
            return matches!(
                layout.repr,
                crate::program::ClassRepr::CRepr
                    | crate::program::ClassRepr::CPacked
                    | crate::program::ClassRepr::CUnion
            );
        }
        false
    }

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

    /// `true` when `obj.name` is a property-getter read on a receiver
    /// whose static type resolves syntactically: a Var binding / repl
    /// slot, `this` (explicit keyword or env-bound), or a class name
    /// (static property). Getters return an owned +1 — their tails
    /// retain like any other method tail, and fresh tails own their
    /// share anyway — so the access counts as fresh and the consumer
    /// drops it. Unresolvable receiver shapes (call results, chained
    /// reads, …) fall back to the borrow default: exact for plain
    /// fields; for a property it means the getter's +1 is never
    /// dropped (a leak, never a use-after-free).
    fn field_is_property_access(&self, obj: &Expr, name: Symbol) -> bool {
        // Reflection members on a `Type` handle all hand the consumer
        // an owned value: `.name` retains a registry string,
        // `.fields` / `.methods` / `.typeArgs` mint fresh arrays,
        // `.parent` mints a fresh Optional cell, and `.kind` returns
        // an interned unit-enum box whose release is a no-op
        // (rc = -1). Classified borrowed they all leaked per read.
        if matches!(
            name.as_str(),
            "name" | "fields" | "methods" | "parent" | "typeArgs" | "kind"
        ) {
            let obj_is_typehandle = match &obj.kind {
                ExprKind::Var(n) => matches!(
                    self.peek_var_ty(*n)
                        .or_else(|| self.repl_slots.get(n).map(|(_, t)| t.clone())),
                    Some(MirTy::TypeHandle)
                ),
                ExprKind::Call { callee, .. } => callee.as_str() == "typeof",
                _ => false,
            };
            if obj_is_typehandle {
                return true;
            }
        }
        let obj_cid = match &obj.kind {
            ExprKind::This => self.this_class,
            ExprKind::Var(n) => {
                // Class-name receiver (no shadowing binding / slot)
                // — static property read.
                if self.env.lookup_binding(*n).is_none()
                    && !self.repl_slots.contains_key(n)
                {
                    if let Some(cid) =
                        super::class_id_by_name(self.classes, self.class_meta, *n)
                    {
                        return self
                            .class_meta
                            .get(&cid)
                            .is_some_and(|m| m.static_property_getter.contains_key(&name));
                    }
                }
                let ty = self
                    .peek_var_ty(*n)
                    .or_else(|| self.repl_slots.get(n).map(|(_, t)| t.clone()));
                match ty {
                    Some(MirTy::Object(cid)) => Some(cid),
                    _ => None,
                }
            }
            _ => None,
        };
        obj_cid
            .and_then(|cid| self.class_meta.get(&cid))
            .is_some_and(|m| m.property_getter.contains_key(&name))
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
        tail_owned: bool,
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
                    // Owned source wrapped into T? / T.weak — drop
                    // its share (see release_owned_wrap_source).
                    if coerced != v {
                        self.release_owned_wrap_source(v, &vty, &ret_ty_clone, tail_owned);
                    }
                    Some(coerced)
                }
            }
            (ret_ty, None) => Some(synth_placeholder(self, &ret_ty.clone())),
        };
        let release_value = value
            .map(|v| self.crepr_return_owned.contains(&v))
            .unwrap_or(false);
        self.fb.set_terminator(Terminator::Return { value, release_value });
        Ok(())
    }

    pub(in crate::lower) fn lower_block(
        &mut self,
        blk: &AstBlock,
    ) -> Result<Option<(ValueId, MirTy)>, LowerError> {
        self.lower_block_hinted(blk, None)
    }

    /// Like `lower_block_hinted`, but marks the call as the **outer**
    /// fn-body block. Enables the borrow-tail retain for the bare-var
    /// (= implicit `this.field`) tail. Sub-blocks lowered while
    /// processing this body (if-arms, match-arms, loop bodies) reset
    /// the flag so their tails don't double-retain — the join block
    /// at the fn-body level handles that.
    pub(in crate::lower) fn lower_block_for_fn_body(
        &mut self,
        blk: &AstBlock,
        tail_hint: Option<&MirTy>,
    ) -> Result<Option<(ValueId, MirTy)>, LowerError> {
        // Record where the body's scopes begin for the early-return
        // sweep (see `return_sweep_base`): scopes below this hold
        // the entry params — borrows the sweep must not release.
        self.return_sweep_base = self.env.scopes.len();
        let saved = std::mem::replace(&mut self.in_fn_body_top, true);
        let result = self.lower_block_hinted(blk, tail_hint);
        self.in_fn_body_top = saved;
        result
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
        // Snapshot the fn-body-top flag for THIS call, then clear it
        // so any sub-blocks we recurse into (if-arms, match-arms,
        // loop bodies, …) see false and skip the borrow-tail retain.
        let is_fn_body_top = std::mem::replace(&mut self.in_fn_body_top, false);
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
        let tail_is_borrow = blk.tail.as_ref().is_some_and(|e| match &e.kind {
            ExprKind::Index { .. } | ExprKind::Field { .. } => true,
            // Bare `name` inside a class method body desugars to
            // `this.name` at the field-resolution layer (lowering
            // emits `load_field this.x` for the method `get(): T { x }`).
            // It IS a borrow of `this`'s slot, so the scope-exit
            // release would drop the rc of the field's contents
            // without a paired retain. Restrict to the case where
            // `name` resolves to neither a local/cell binding nor a
            // closure capture (= really a field) and we're inside a
            // class method (`this_class` set).
            //
            // Property getter bodies are NOT special-cased: getters
            // return an owned +1 like any method tail, and the
            // consumer side classifies `obj.prop` as fresh
            // (`field_is_property_access`) and drops that share.
            ExprKind::Var(name) => {
                // `fn(): T { self_name }` — returning the running
                // closure itself: ClosureSelf is a borrow of the
                // caller's share, so the tail needs its own +1.
                if self
                    .closure_self
                    .as_ref()
                    .is_some_and(|(n, _)| n == name)
                    && self.env.lookup_binding(*name).is_none()
                {
                    return is_fn_body_top;
                }
                is_fn_body_top
                    && self.this_class.is_some()
                    && self.env.lookup_binding(*name).is_none()
                    && self
                        .captures_in_scope
                        .and_then(|c| c.get(name))
                        .is_none()
            }
            _ => false,
        });
        // Definitively record whether THIS block's tail got the
        // alias/borrow retain — join sites read the flag right
        // after lowering a branch/arm body (a stale value from a
        // nested block would mis-classify a bare-Var body, so the
        // false is unconditional).
        self.last_block_tail_owned = false;
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
                self.last_block_tail_owned = true;
                Some((v_use, ty_use))
            }
            other => other,
        };
        // CRepr Locals carry no rc — Retain above is a no-op for
        // them. Transfer ownership of the tail-aliased local off the
        // scope-exit release path; instead, mark the tail SSA so a
        // function-return terminator can hand the buffer to the
        // return ABI (sret memcpy / chunks load) and then free it.
        // Without this two-step handover, `release_top_scope_objects`
        // would free the buffer before the ABI consumed it, and the
        // ABI's load would read freed bytes.
        if let Some(name) = tail_alias_name {
            if let Some(Binding::Local(lid, _)) = self.env.lookup_binding(name) {
                if self.crepr_owned_locals.remove(&lid) {
                    if let Some((tv, _)) = &tail {
                        self.crepr_return_owned.insert(*tv);
                    }
                }
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
            // Template literals fold through `str_concat` /
            // `fmt_value`, both of which mint fresh registry strings;
            // `lower_template` releases every intermediate and
            // guarantees the final value owns its own +1 (zero-part
            // templates get a fresh copy of "").
            ExprKind::Template { .. } => true,
            // `loop { … break v … }` — `lower_break` retains borrowed
            // break values (fresh ones keep their own +1), so the
            // exit-block value the loop evaluates to always owns a
            // share. Classifying it borrowed made `let b = loop {
            // break new Box(i) }` stack a binding retain on top and
            // leak one occupant per evaluation.
            ExprKind::Loop { .. } => true,
            // `x as? C` boxes into a fresh Optional cell that owns a
            // +1 of the value (DowncastOrNone codegen mirrors
            // WeakUpgrade) — the consumer drops the cell's share.
            ExprKind::TypeDowncast { .. } => true,
            // Property-getter reads are owned: the getter's tail
            // retains like any method tail (fresh tails own their +1
            // anyway), so `h.prop` hands the consumer a share to
            // drop. Plain field reads stay borrows (the default
            // below). Only receivers whose static type resolves
            // syntactically are recognised — see the helper.
            ExprKind::Field { obj, name } => self.field_is_property_access(obj, *name),
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
            // `if` / `match` / `if let` join values are NORMALISED to
            // owned: the join sites retain any branch result that
            // doesn't already own its +1 (fresh tail, block-tail
            // alias/borrow retain, or the arm-side pattern-binding
            // retain). Mixed-freshness joins used to read as
            // non-fresh here, making the consumer add a second
            // retain on the fresh branch — one leaked object per
            // evaluation through that branch.
            ExprKind::If { .. } => true,
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
            // Match joins normalize to owned, same as `If` above.
            ExprKind::Match { .. } => true,
            // IfLet joins normalize to owned, same as `If` above.
            ExprKind::IfLet { .. } => true,
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
        for (_name, binding) in scope.into_iter().rev() {
            self.release_binding_for_scope_exit(binding);
        }
    }

    /// Early-`return` sweep: release every live heap binding in every
    /// scope above the fn's outermost scope (index 0 holds the entry
    /// parameters — borrows whose +1 stays with the caller).
    /// `lower_block`'s scope-exit pass never runs for the blocks an
    /// early return jumps out of, so without this every heap `let`
    /// live at a `return` leaked — `fn f(..) { let s = "x" + k;
    /// if c { return s.length } .. }` leaked one string per call,
    /// and the async poll fn's suspend arms (which end in a
    /// generated `return`) leaked the awaited promise + every local
    /// carried in the state. Innermost scopes release first,
    /// mirroring normal block-exit order.
    pub(in crate::lower) fn release_scopes_for_return(&mut self) {
        let base = self.return_sweep_base;
        if base == usize::MAX {
            // `__main` (slot-managed top-level) — its epilogue owns
            // the releases; a sweep here would double-release.
            return;
        }
        self.release_scopes_since(base);
        self.release_live_scrutinees_from(0);
    }

    /// Release every live heap binding in scopes `depth..` —
    /// shared by the `return` sweep (depth 1: everything above the
    /// param scope) and the `continue` sweep (the loop frame's
    /// entry depth). Innermost scopes first.
    pub(in crate::lower) fn release_scopes_since(&mut self, depth: usize) {
        let bindings: Vec<Binding> = self
            .env
            .scopes
            .iter()
            .skip(depth)
            .rev()
            .flat_map(|scope| scope.iter().rev().map(|(_n, b)| b.clone()))
            .collect();
        for binding in bindings {
            self.release_binding_for_scope_exit(binding);
        }
    }

    /// Release the fresh match / if-let scrutinees whose arm body
    /// the exiting jump is inside of. `from_depth == 0` releases
    /// all of them (early `return`); a loop jump passes the loop
    /// frame's entry depth so a match SURROUNDING the loop keeps
    /// its scrutinee (its arm continues after the loop). The
    /// `live_fresh_scrutinees` stack itself is compile-time
    /// bookkeeping popped by the match lowerer — the sweeps only
    /// emit Releases on the exiting path.
    pub(in crate::lower) fn release_live_scrutinees_from(&mut self, from_depth: usize) {
        let svs: Vec<crate::ValueId> = self
            .live_fresh_scrutinees
            .iter()
            .filter(|(_v, d)| *d >= from_depth)
            .map(|(v, _d)| *v)
            .rev()
            .collect();
        for sv in svs {
            self.fb.push_inst(Inst::Release { value: sv });
        }
    }


    /// A `T → T?` / `T → T.weak` coerce mints the wrapper's own
    /// share of the inner. When the SOURCE was itself owned (a
    /// fresh value's transfer +1, or a block tail the alias/borrow
    /// retain already bumped), that share has no other owner once
    /// the wrapper exists — release it right after the coerce.
    /// Borrowed sources (param vars, bare field reads) keep their
    /// binding's share and must NOT be released. Round-20/21
    /// probes found one leaked value per call at every wrap
    /// position (let / argument / assignment / both return paths).
    pub(in crate::lower) fn release_owned_wrap_source(
        &mut self,
        orig: crate::ValueId,
        src_ty: &MirTy,
        target_ty: &MirTy,
        owned: bool,
    ) {
        if !owned || !self.is_arc_heap(src_ty) {
            return;
        }
        // A subclass instance wraps into `Optional<Parent>` / `T.weak`
        // the same way an exact-type source does — `coerce`'s wrap arm
        // accepts the subtype, so the owned-source release must
        // recognise it too. Restricted to plain `Object` sources (the
        // subclass case); an `Optional<_>` source is an Optional→Optional
        // widen, not a wrap. Without this, `let o: Animal? = new Dog()`
        // (and the same wrap at arg / array-literal / map index-assign /
        // field-assign) retained the Dog into the cell but never dropped
        // the fresh source's +1 → one leak/call.
        let wraps = match target_ty {
            MirTy::Optional(inner) => {
                **inner == *src_ty
                    || matches!(
                        (&**inner, src_ty),
                        (MirTy::Object(_), MirTy::Object(_))
                    )
            }
            // `MirTy::Weak` carries the class id; a strong Object source
            // (the exact class or a subclass) wraps into it.
            MirTy::Weak(_) => matches!(src_ty, MirTy::Object(_)),
            _ => false,
        };
        if wraps {
            self.fb.push_inst(Inst::Release { value: orig });
        }
    }

    /// Normalize a branch / arm result to OWNED before it flows
    /// into an `if` / `match` / `if let` join: values that already
    /// own their +1 (fresh tails, block-tail alias/borrow retains,
    /// arm-side forced retains) pass through; everything else —
    /// param vars, bare borrows — gets a Retain here so the join
    /// value uniformly owns a share and `is_fresh_object_expr` can
    /// classify every join as fresh.
    pub(in crate::lower) fn ensure_join_owned(
        &mut self,
        v: crate::ValueId,
        ty: &MirTy,
        owned: bool,
    ) {
        if !owned && self.is_arc_slot(ty) {
            self.fb.push_inst(Inst::Retain { value: v });
        }
    }

    /// Fixed-length heap-element arrays enter container cells BY
    /// VALUE: mint a copy the cell owns (`$array.copyShallow` —
    /// fresh header + buffer, one retained share per element).
    /// Returns the value to store, or `None` when `ty` isn't a
    /// fixed array with ARC elements (caller falls through to the
    /// regular retain rule).
    pub(in crate::lower) fn copy_fixed_for_cell(
        &mut self,
        v: crate::ValueId,
        ty: &MirTy,
        is_fresh: bool,
    ) -> Option<crate::ValueId> {
        let MirTy::Array { elem, len: Some(n) } = ty else {
            return None;
        };
        if !self.is_arc_slot(elem) {
            return None;
        }
        // A FRESH value (a call result — fn returns mint a +1 the
        // caller owns) transfers straight into the cell: it has no
        // other owner, so the copy would orphan the original's +1.
        // Each call site's `Some(..)` branch stores the returned
        // value as the cell's owned share, which is exactly the
        // fresh transfer.
        if is_fresh {
            return Some(v);
        }
        let _ = n;
        let copy = self.fb.new_value(ty.clone());
        self.fb.push_inst(Inst::Call {
            dst: Some(copy),
            callee: crate::inst::FuncRef::Builtin(Symbol::intern("$array.copyShallow")),
            args: Box::new([v]),
        });
        Some(copy)
    }

    /// One binding's scope-exit release, shared by the normal
    /// block-exit pass (`release_top_scope_objects`) and the early
    /// `return` sweep (`release_scopes_for_return`).
    fn release_binding_for_scope_exit(&mut self, binding: Binding) {
        let needs_release = |ty: &MirTy| ty.is_heap();
        {
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
                            return;
                        }
                        // `@com interface` values are bare COM
                        // handles with no ARC header — Release
                        // here would scribble random memory at
                        // `addr - 32`. Lifetime is managed by
                        // IUnknown::Release at the user level.
                        if self.com_interfaces.contains(&layout.name) {
                            return;
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
                            return;
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
                Binding::Cell(cell_v, _) => {
                    // A cell is a heap 1-element array shared between
                    // this scope and every closure that captured it
                    // (shared mutable capture). The scope owns the
                    // creation +1 and each capturing closure retained
                    // its own share at MakeClosure — so dropping the
                    // scope's share here is safe: a closure that
                    // outlives the scope keeps the cell alive, and the
                    // last release cascades into the inner value.
                    // (`cell_v`'s value type is the `T[]` cell array,
                    // so the Release dispatches as an array release.)
                    self.fb.push_inst(Inst::Release { value: cell_v });
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
