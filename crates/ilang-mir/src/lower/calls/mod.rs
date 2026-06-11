//! Call-shaped expression lowering on `BodyCx`:
//!
//! - `lower_super_call` — `super.method(args)` inside a method
//!   body, resolves through the parent-class chain.
//! - `lower_new` — `new Class(args)` constructor calls. Allocates
//!   the heap object, runs the user `init`, then evaluates to the
//!   freshly-built `Object(cid)`.
//! - `lower_method_call` — `obj.method(args)` dispatcher. Walks a
//!   chain of per-builtin `try_lower_*` helpers (one per file in
//!   this directory) until one matches.

use ilang_ast::{Expr, ExprKind, Span, Symbol};

use crate::inst::{FuncId, FuncRef, Inst, ValueId};
use crate::program::ClassLayout;
use crate::types::MirTy;

use super::{BodyCx, FnSig, LowerError};

mod array;
mod builtin_static;
mod c_vtable;
mod map;
mod object;
mod objc_block;
mod promise;
mod scalar;
mod set;
mod string_method;

/// Cascade `KIND_*` tag for a MirTy. Mirrors the codegen-side
/// `print_kind::kind_tag_of`. Used by Promise / Array.map codegen
/// to tell the runtime how to release the wrapped value.
///
/// `@handle` structs are opaque, pointer-sized values with no ARC
/// header and must therefore report `KIND_NONE` so the runtime
/// cascade doesn't try to release the raw OS handle.
pub(super) fn kind_tag_of_mir(ty: &MirTy, classes: &[ClassLayout]) -> i64 {
    match ty {
        MirTy::Object(cid) => {
            if classes[cid.0 as usize].is_handle {
                0
            } else {
                1
            }
        }
        MirTy::Array { .. } => 2,
        MirTy::Optional(_) => 3,
        MirTy::Tuple(_) => 4,
        MirTy::Map { .. } => 5,
        MirTy::Fn(_) => 6,
        MirTy::Str => 7,
        MirTy::Enum(_) => 8,
        MirTy::Promise(_) => 9,
        MirTy::Set { .. } => 10,
        _ => 0,
    }
}

impl<'a> BodyCx<'a> {
    pub(super) fn lower_super_call(
        &mut self,
        method: Option<Symbol>,
        args: &[Expr],
        _span: Span,
    ) -> Result<(ValueId, MirTy), LowerError> {
        let cid = self
            .this_class
            .ok_or_else(|| LowerError::Other("super outside method".into()))?;
        let parent_id = self.classes[cid.0 as usize]
            .parent
            .ok_or_else(|| LowerError::Other("super in class without parent".into()))?;
        let this_sym = Symbol::intern("this");
        let this_v = if let Some((v, _)) = self.lookup_var(this_sym) {
            v
        } else if let Some(caps) = self.captures_in_scope {
            // Closure body — `this` flows in as a captured slot.
            let (idx, cty) = caps
                .get(&this_sym)
                .cloned()
                .ok_or_else(|| LowerError::Other("super: `this` not captured".into()))?;
            let v = self.fb.new_value(cty);
            self.fb.push_inst(Inst::LoadCapture { dst: v, idx });
            v
        } else {
            return Err(LowerError::Other("super: `this` not in scope".into()));
        };

        let parent_meta = self.class_meta.get(&parent_id).unwrap();
        let target_method = method.unwrap_or(Symbol::intern("init"));
        let mid = *parent_meta.method_ids.get(&target_method).ok_or_else(|| {
            LowerError::Other(format!("parent has no method {target_method}"))
        })?;
        let sig = parent_meta.method_sigs.get(&target_method).cloned().unwrap();

        let mut arg_vals = vec![this_v];
        for (i, a) in args.iter().enumerate() {
            let (coerced, _) = self.lower_arg_to(a, sig.params.get(i + 1))?;
            arg_vals.push(coerced);
        }
        let dst = if matches!(sig.ret, MirTy::Unit) {
            None
        } else {
            Some(self.fb.new_value(sig.ret.clone()))
        };
        self.fb.push_inst(Inst::Call {
            dst,
            callee: FuncRef::Local(mid),
            args: arg_vals.into_boxed_slice(),
        });
        Ok((dst.unwrap_or_else(|| self.const_unit()), sig.ret))
    }

    pub(super) fn lower_new(
        &mut self,
        class: Symbol,
        args: &[Expr],
        init_method: Option<Symbol>,
    ) -> Result<(ValueId, MirTy), LowerError> {
        let class_id = super::class_id_by_name(self.classes, self.class_meta, class)
            .ok_or_else(|| LowerError::Other(format!("unknown class {class}")))?;

        // The mangle pass writes the chosen init's mangled name into
        // `init_method` when init is overloaded. Otherwise look up
        // `init` (which exists for non-overloaded inits, and also
        // for the no-init "synthetic" case below).
        let init_lookup = init_method.unwrap_or_else(|| Symbol::intern("init"));
        // Walk the parent chain so an inherited `__bind_handle` (or
        // any inherited init helper) resolves even when this class's
        // own `declare_class_methods` hasn't run yet.
        let (init_id, init_sig) = {
            let mut cur = Some(class_id);
            let mut found: (Option<FuncId>, Option<FnSig>) = (None, None);
            while let Some(c) = cur {
                let m = self.class_meta.get(&c).expect("class meta");
                if let Some(&fid) = m.method_ids.get(&init_lookup) {
                    let sig = m.method_sigs.get(&init_lookup).cloned();
                    found = (Some(fid), sig);
                    break;
                }
                cur = self.classes[c.0 as usize].parent;
            }
            found
        };

        let mut arg_vals = Vec::with_capacity(args.len());
        let mut fresh_obj_args: Vec<ValueId> = Vec::new();
        for (i, a) in args.iter().enumerate() {
            let arg_is_fresh = self.is_fresh_object_expr(a);
            let target = init_sig.as_ref().and_then(|sig| sig.params.get(i + 1));
            let (final_v, vty) = self.lower_arg_to(a, target)?;
            // Fresh heap args transfer ownership of their +1 into the
            // init; init's field assignment takes its own retain
            // (`is_arc_slot` covers every heap kind, Promise
            // included), so the caller-side temp needs releasing
            // after the call. One shared release set for all call
            // shapes — an earlier per-site copy here excluded
            // Promise on the stale belief that promise fields don't
            // retain, which leaked every fresh promise passed to an
            // init together with its settled value.
            let needs_post_release = Self::fresh_arg_needs_post_release(&vty);
            if arg_is_fresh && needs_post_release {
                fresh_obj_args.push(final_v);
            }
            arg_vals.push(final_v);
        }

        let dst = self.fb.new_value(MirTy::Object(class_id));
        let init = init_id
            // Synthesise a no-op init reference for argument-less
            // construction when the class has none. The MIR→clif
            // step interprets `FuncId(u32::MAX)` as "no user init,
            // just zero-init fields".
            .unwrap_or(FuncId(u32::MAX));
        self.fb.push_inst(Inst::NewObject {
            dst,
            class: class_id,
            init_args: arg_vals.into_boxed_slice(),
            init,
        });
        for fv in fresh_obj_args {
            self.fb.push_inst(Inst::Release { value: fv });
        }
        Ok((dst, MirTy::Object(class_id)))
    }

    pub(super) fn lower_method_call(
        &mut self,
        obj: &Expr,
        method: Symbol,
        args: &[Expr],
        _span: Span,
    ) -> Result<(ValueId, MirTy), LowerError> {
        // `vtbl.Method(args)` where `vtbl: *T` and T is an
        // @extern(C) struct with a fn-typed `Method` field — COM
        // vtable dispatch. Peeks the type without lowering `obj`,
        // so the fall-through case doesn't double-emit instructions.
        if let Some(out) = self.try_lower_c_struct_vtable_call(obj, method, args)? {
            return Ok(out);
        }
        // Name-based static dispatch — receiver is a bare Var that
        // doesn't resolve to a local. Covers `console.log`, all
        // `Promise.*` factories, `string.fromUtf16`, and ordinary
        // `Class.staticMethod(...)`.
        if let ExprKind::Var(name) = &obj.kind {
            if let Some(out) = self.try_lower_builtin_static(*name, method, args)? {
                return Ok(out);
            }
        }

        let obj_is_fresh = self.is_fresh_object_expr(obj);
        let (ov, oty) = self.lower_expr(obj)?;

        // Type handle methods: `.fieldType(name)` / `.methodReturn(name)`
        // / `.methodParams(name)` route to the reflection builtins.
        if matches!(oty, MirTy::TypeHandle) {
            if let Some(out) = self.try_lower_type_handle_method(ov, method, args)? {
                return Ok(out);
            }
        }
        if let Some(out) = self.try_lower_objc_block_invoke(ov, &oty, method, args)? {
            return Ok(out);
        }
        if let Some(out) = self.try_lower_scalar_method(ov, &oty, method, args)? {
            return Ok(out);
        }
        // Optional / array / string builtins never hand back a value
        // that borrows the receiver's storage without its own share
        // (`unwrap` retains the inner, `pop` / `shift` transfer the
        // slot's share into the result, everything else returns a
        // primitive or a fresh container) — so a fresh receiver's
        // transient +1 drops right after the dispatch. Without this,
        // `("v" + s).length` leaked a registry string and
        // `[new Box(1)].length` leaked the whole array per call.
        // Map / Set / class-object methods handle their own freshness
        // (they take `obj_is_fresh`).
        if let Some(out) = self.try_lower_optional_method(ov, &oty, method, args)? {
            if obj_is_fresh && self.is_arc_heap(&oty) {
                self.fb.push_inst(Inst::Release { value: ov });
            }
            return Ok(out);
        }
        if let Some(out) = self.try_lower_array_method(ov, &oty, method, args)? {
            if obj_is_fresh && self.is_arc_heap(&oty) {
                self.fb.push_inst(Inst::Release { value: ov });
            }
            return Ok(out);
        }
        if let Some(out) = self.try_lower_string_method(ov, &oty, method, args)? {
            if obj_is_fresh && self.is_arc_heap(&oty) {
                self.fb.push_inst(Inst::Release { value: ov });
            }
            return Ok(out);
        }
        // Promise `.then` / `.catch` borrow the receiver: the waiter
        // holds +1 on the DOWNSTREAM (not the upstream), and an
        // already-settled upstream's queued firing takes its own
        // retain on the value — so a fresh receiver's transient +1
        // drops right after the dispatch, same as the
        // optional/array/string rule above. Without this, every
        // chained `p.then(f).catch(g)` leaked the intermediate
        // promise together with its settled value (the ManagedPromise
        // box is invisible to liveAllocBytes, but the held value
        // showed up as 1 string per iteration).
        if let Some(out) = self.try_lower_promise_method(ov, &oty, method, args)? {
            if obj_is_fresh && self.is_arc_heap(&oty) {
                self.fb.push_inst(Inst::Release { value: ov });
            }
            return Ok(out);
        }
        if let Some(out) = self.try_lower_map_method(ov, &oty, obj_is_fresh, method, args)? {
            return Ok(out);
        }
        if let Some(out) = self.try_lower_set_method(ov, &oty, obj_is_fresh, method, args)? {
            return Ok(out);
        }
        if let Some(out) = self.try_lower_weak_method(ov, &oty, method, args)? {
            return Ok(out);
        }
        if let Some(out) =
            self.try_lower_object_method(ov, &oty, obj_is_fresh, method, args)?
        {
            return Ok(out);
        }
        Err(LowerError::Unsupported(
            "method call on this type / unhandled builtin",
        ))
    }

    /// Reflection methods on a `Type` handle: `fieldType(name)`,
    /// `methodReturn(name)`, `methodParams(name)`. All take one string
    /// argument and return an Optional (heap cell or 0).
    fn try_lower_type_handle_method(
        &mut self,
        cid: ValueId,
        method: Symbol,
        args: &[Expr],
    ) -> Result<Option<(ValueId, MirTy)>, LowerError> {
        let (builtin, ret_ty): (&str, MirTy) = match method.as_str() {
            "fieldType" => (
                "type_field_type",
                MirTy::Optional(Box::new(MirTy::TypeHandle)),
            ),
            "methodReturn" => (
                "type_method_return",
                MirTy::Optional(Box::new(MirTy::TypeHandle)),
            ),
            "methodParams" => (
                "type_method_params",
                MirTy::Optional(Box::new(MirTy::Array {
                    elem: Box::new(MirTy::TypeHandle),
                    len: None,
                })),
            ),
            _ => return Ok(None),
        };
        if args.len() != 1 {
            return Err(LowerError::Other(format!(
                "Type.{}: expected 1 string argument",
                method.as_str()
            )));
        }
        let (name_v, name_ty) = self.lower_expr(&args[0])?;
        if !matches!(name_ty, MirTy::Str) {
            return Err(LowerError::Other(format!(
                "Type.{}: argument must be a string",
                method.as_str()
            )));
        }
        let v = self.fb.new_value(ret_ty.clone());
        self.fb.push_inst(Inst::Call {
            dst: Some(v),
            callee: FuncRef::Builtin(Symbol::intern(builtin)),
            args: Box::new([cid, name_v]),
        });
        Ok(Some((v, ret_ty)))
    }
}
