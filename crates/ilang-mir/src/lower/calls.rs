//! Call-shaped expression lowering on `BodyCx`:
//!
//! - `lower_super_call` — `super.method(args)` inside a method
//!   body, resolves through the parent-class chain.
//! - `lower_new` — `new Class(args)` constructor calls. Allocates
//!   the heap object, runs the user `init`, then evaluates to the
//!   freshly-built `Object(cid)`.
//! - `lower_method_call` — `obj.method(args)` on any of the heap
//!   types (Object, Optional, Array, Map, Str, Enum, Tuple). The
//!   per-type lookup tables pick the right ABI: virtual dispatch
//!   via `__virt_dispatch` for class methods, direct builtins
//!   for the rest.

use ilang_ast::{Expr, ExprKind, Span, Symbol};

use crate::inst::{FuncId, FuncRef, Inst, ValueId};
use crate::program::ClassLayout;
use crate::types::MirTy;

use super::utils::retain_if_heap;
use super::{BodyCx, FnSig, LowerError};

/// Cascade `KIND_*` tag for a MirTy. Mirrors the codegen-side
/// `print_kind::kind_tag_of`. Used by Promise codegen to tell
/// the runtime how to release the wrapped value.
///
/// `@handle` structs are opaque, pointer-sized values with no ARC
/// header and must therefore report `KIND_NONE` so the runtime
/// cascade doesn't try to release the raw OS handle.
fn kind_tag_of_mir(ty: &MirTy, classes: &[ClassLayout]) -> i64 {
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
            let (v, vty) = self.lower_expr(a)?;
            let coerced = match sig.params.get(i + 1) {
                Some(t) if t != &vty => self.coerce(v, &vty, t, a.span)?,
                _ => v,
            };
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
        let class_id = *self
            .class_meta
            .iter()
            .find_map(|(cid, _)| {
                let cl = &self.classes[cid.0 as usize];
                if cl.name == class {
                    Some(cid)
                } else {
                    None
                }
            })
            .ok_or_else(|| LowerError::Other(format!("unknown class {class}")))?;

        // The mangle pass writes the chosen init's mangled name into
        // `init_method` when init is overloaded. Otherwise look up
        // `init` (which exists for non-overloaded inits, and also for
        // the no-init "synthetic" case below).
        let init_lookup = init_method.unwrap_or_else(|| Symbol::intern("init"));
        // Walk the parent chain so an inherited `__bind_handle` (or
        // any inherited init helper) resolves even when this class's
        // own `declare_class_methods` hasn't run yet — declaration
        // order between siblings inside a folder-binding can put a
        // `new <sibling>(...) with __bind_handle` body lowering ahead
        // of the sibling's method-inheritance pass.
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

        // Lower constructor args.
        let mut arg_vals = Vec::with_capacity(args.len());
        let mut fresh_obj_args: Vec<ValueId> = Vec::new();
        for (i, a) in args.iter().enumerate() {
            let arg_is_fresh = self.is_fresh_object_expr(a);
            let (v, vty) = self.lower_expr(a)?;
            let final_v = if let Some(sig) = &init_sig {
                if let Some(target) = sig.params.get(i + 1) {
                    if vty == *target {
                        v
                    } else {
                        self.coerce(v, &vty, target, a.span)?
                    }
                } else {
                    v
                }
            } else {
                v
            };
            if arg_is_fresh && matches!(vty, MirTy::Object(_)) {
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
        // Release fresh Object args — the constructor took a borrow
        // and any field-store-side retain has already kept what it
        // needs. The fresh +1 from `new T(...)` would otherwise leak.
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
        let obj_is_fresh = self.is_fresh_object_expr(obj);
        let _ = obj_is_fresh;
        // `vtbl.Method(args)` where `vtbl: *T` and T is an @extern(C)
        // struct with a fn-typed `Method` field — COM vtable
        // dispatch. Load the fn pointer at the field's byte offset,
        // then `CallRawIndirect` with the arg list. No ARC
        // bookkeeping: the receiver is a raw C pointer and the fn
        // pointer is a plain address.
        if let Some(out) = self.try_lower_c_struct_vtable_call(obj, method, args)? {
            return Ok(out);
        }
        // `console.log(...)` is a special-cased variadic builtin.
        if let ExprKind::Var(name) = &obj.kind {
            if name.as_str() == "console" && method.as_str() == "log" {
                let mut arg_vals = Vec::with_capacity(args.len());
                let mut fresh_str_args: Vec<ValueId> = Vec::new();
                for a in args {
                    let arg_is_fresh = self.is_fresh_object_expr(a);
                    let (v, vty) = self.lower_expr(a)?;
                    if arg_is_fresh && matches!(vty, MirTy::Str) {
                        fresh_str_args.push(v);
                    }
                    arg_vals.push(v);
                }
                self.fb.push_inst(Inst::Call {
                    dst: None,
                    callee: FuncRef::Builtin(Symbol::intern("console_log")),
                    args: arg_vals.into_boxed_slice(),
                });
                for fv in fresh_str_args {
                    self.fb.push_inst(Inst::Release { value: fv });
                }
                return Ok((self.const_unit(), MirTy::Unit));
            }
            // Built-in `Promise.all(ps)` / `Promise.race(ps)`
            // aggregate combinators. The argument is a `Promise<T>[]`;
            // we read T from the array element's MirTy::Promise(inner)
            // so the runtime knows how to release each result value.
            if self.lookup_var(*name).is_none()
                && name.as_str() == "Promise"
                && (method.as_str() == "all" || method.as_str() == "race")
                && args.len() == 1
            {
                let arg_is_fresh = self.is_fresh_object_expr(&args[0]);
                let (av, aty) = self.lower_expr(&args[0])?;
                let inner_t = match &aty {
                    MirTy::Array { elem, .. } => match elem.as_ref() {
                        MirTy::Promise(t) => (**t).clone(),
                        _ => MirTy::Unit,
                    },
                    _ => MirTy::Unit,
                };
                if !arg_is_fresh {
                    self.fb.push_inst(Inst::Retain { value: av });
                }
                let value_kind = kind_tag_of_mir(&inner_t, self.classes);
                let kind_v = self.const_int(MirTy::I64, value_kind);
                let ret_inner = if method.as_str() == "all" {
                    MirTy::Array { elem: Box::new(inner_t.clone()), len: None }
                } else {
                    inner_t.clone()
                };
                let prom_ty = MirTy::Promise(Box::new(ret_inner));
                let dst = self.fb.new_value(prom_ty.clone());
                let builtin = if method.as_str() == "all" {
                    "promise_all"
                } else {
                    "promise_race"
                };
                self.fb.push_inst(Inst::Call {
                    dst: Some(dst),
                    callee: FuncRef::Builtin(Symbol::intern(builtin)),
                    args: Box::new([av, kind_v]),
                });
                return Ok((dst, prom_ty));
            }
            // Internal `Promise.__pending<T>()` — allocates a Pending
            // promise. The desugar pass that synthesises async fn
            // poll bodies emits this; users see it through the type
            // checker registration but shouldn't call it directly.
            // T is unrecoverable at MIR (no args constrain it) so we
            // return `Promise<()>`; the desugar's surrounding code
            // assigns through a typed binding which the MIR coerce
            // pass treats as a no-op (every Promise is an i64 ptr).
            if self.lookup_var(*name).is_none()
                && name.as_str() == "Promise"
                && method.as_str() == "__pending"
                && args.is_empty()
            {
                let prom_ty = MirTy::Promise(Box::new(MirTy::Unit));
                let dst = self.fb.new_value(prom_ty.clone());
                self.fb.push_inst(Inst::Call {
                    dst: Some(dst),
                    callee: FuncRef::Builtin(Symbol::intern("promise_pending")),
                    args: Box::new([]),
                });
                return Ok((dst, prom_ty));
            }
            // Internal `Promise.__settleResolve<T>(p, v)` — used by
            // the generated async-fn poll fn at the end of an async
            // body. Takes ownership of v (kind read from v's MirTy).
            if self.lookup_var(*name).is_none()
                && name.as_str() == "Promise"
                && method.as_str() == "__settleResolve"
                && args.len() == 2
            {
                let p_is_fresh = self.is_fresh_object_expr(&args[0]);
                let (pv, _) = self.lower_expr(&args[0])?;
                if !p_is_fresh {
                    self.fb.push_inst(Inst::Retain { value: pv });
                }
                let v_is_fresh = self.is_fresh_object_expr(&args[1]);
                let (vv, vty) = self.lower_expr(&args[1])?;
                if !v_is_fresh && self.is_arc_heap(&vty) {
                    self.fb.push_inst(Inst::Retain { value: vv });
                }
                let kind = kind_tag_of_mir(&vty, self.classes);
                let kind_v = self.const_int(MirTy::I64, kind);
                self.fb.push_inst(Inst::Call {
                    dst: None,
                    callee: FuncRef::Builtin(Symbol::intern("promise_settle_resolve")),
                    args: Box::new([pv, vv, kind_v]),
                });
                // The runtime release_promise of `pv` happens when
                // the surrounding scope's release fires.
                return Ok((self.const_unit(), MirTy::Unit));
            }
            // Internal `Promise.__settleReject(p, msg)`.
            if self.lookup_var(*name).is_none()
                && name.as_str() == "Promise"
                && method.as_str() == "__settleReject"
                && args.len() == 2
            {
                let p_is_fresh = self.is_fresh_object_expr(&args[0]);
                let (pv, _) = self.lower_expr(&args[0])?;
                if !p_is_fresh {
                    self.fb.push_inst(Inst::Retain { value: pv });
                }
                let msg_is_fresh = self.is_fresh_object_expr(&args[1]);
                let (mv, _) = self.lower_expr(&args[1])?;
                if !msg_is_fresh {
                    self.fb.push_inst(Inst::Retain { value: mv });
                }
                self.fb.push_inst(Inst::Call {
                    dst: None,
                    callee: FuncRef::Builtin(Symbol::intern("promise_settle_reject")),
                    args: Box::new([pv, mv]),
                });
                return Ok((self.const_unit(), MirTy::Unit));
            }
            // Built-in `Promise.reject(msg)` static factory. The
            // returned promise's T is `Unit` (nothing carries the
            // rejection back to the consumer).
            if self.lookup_var(*name).is_none()
                && name.as_str() == "Promise"
                && method.as_str() == "reject"
                && args.len() == 1
            {
                let msg_is_fresh = self.is_fresh_object_expr(&args[0]);
                let (mv, _) = self.lower_expr(&args[0])?;
                if !msg_is_fresh {
                    self.fb.push_inst(Inst::Retain { value: mv });
                }
                let prom_ty = MirTy::Promise(Box::new(MirTy::Unit));
                let dst = self.fb.new_value(prom_ty.clone());
                self.fb.push_inst(Inst::Call {
                    dst: Some(dst),
                    callee: FuncRef::Builtin(Symbol::intern("promise_reject")),
                    args: Box::new([mv]),
                });
                return Ok((dst, prom_ty));
            }
            // Built-in `Promise.resolve(v)` static factory. T is
            // inferred from the argument's lowered MirTy; the kind
            // tag goes through to `__promise_resolve` so the cell's
            // cascade-on-drop knows how to release the inner value.
            if self.lookup_var(*name).is_none()
                && name.as_str() == "Promise"
                && method.as_str() == "resolve"
                && args.len() == 1
            {
                let arg_is_fresh = self.is_fresh_object_expr(&args[0]);
                let (vv, vty) = self.lower_expr(&args[0])?;
                // The runtime takes ownership of the value (its rc
                // moves into the promise's Resolved state). For a
                // borrowed scrutinee we retain so the caller's +1
                // stays intact.
                if !arg_is_fresh && self.is_arc_heap(&vty) {
                    self.fb.push_inst(Inst::Retain { value: vv });
                }
                let kind = kind_tag_of_mir(&vty, self.classes);
                let kind_v = self.const_int(MirTy::I64, kind);
                let prom_ty = MirTy::Promise(Box::new(vty.clone()));
                let dst = self.fb.new_value(prom_ty.clone());
                self.fb.push_inst(Inst::Call {
                    dst: Some(dst),
                    callee: FuncRef::Builtin(Symbol::intern("promise_resolve")),
                    args: Box::new([vv, kind_v]),
                });
                return Ok((dst, prom_ty));
            }
            // `ClassName.staticMethod(args)` when the ident names a
            // class with no local shadow.
            if self.lookup_var(*name).is_none() {
                let class_id = self
                    .class_meta
                    .iter()
                    .find_map(|(cid, _)| {
                        if self.classes[cid.0 as usize].name == *name {
                            Some(*cid)
                        } else {
                            None
                        }
                    });
                // Walk the parent chain so an inherited static
                // (`SKScene.alloc()` where `alloc` lives on
                // SKNode → NSObject) resolves through the
                // subclass's name.
                let mut owning_cid: Option<crate::types::ClassId> = None;
                let mut cur = class_id;
                while let Some(c) = cur {
                    if self.class_meta.get(&c)
                        .and_then(|m| m.static_method_ids.get(&method))
                        .is_some()
                    {
                        owning_cid = Some(c);
                        break;
                    }
                    cur = self.classes[c.0 as usize].parent;
                }
                if let Some(cid) = owning_cid {
                    let meta = self.class_meta.get(&cid).unwrap();
                    if let Some(&fid) = meta.static_method_ids.get(&method) {
                        let sig = meta.static_method_sigs.get(&method).cloned().unwrap();
                        let mut arg_vals = Vec::with_capacity(args.len());
                        let mut fresh_args: Vec<ValueId> = Vec::new();
                        for (i, a) in args.iter().enumerate() {
                            let arg_is_fresh = self.is_fresh_object_expr(a);
                            let (v, vty) = self.lower_expr(a)?;
                            let coerced = match sig.params.get(i) {
                                Some(t) if t != &vty => self.coerce(v, &vty, t, a.span)?,
                                _ => v,
                            };
                            if arg_is_fresh && matches!(vty, MirTy::Object(_) | MirTy::Str) {
                                fresh_args.push(coerced);
                            }
                            arg_vals.push(coerced);
                        }
                        let dst = if matches!(sig.ret, MirTy::Unit) {
                            None
                        } else {
                            Some(self.fb.new_value(sig.ret.clone()))
                        };
                        self.fb.push_inst(Inst::Call {
                            dst,
                            callee: FuncRef::Local(fid),
                            args: arg_vals.into_boxed_slice(),
                        });
                        for fv in fresh_args {
                            self.fb.push_inst(Inst::Release { value: fv });
                        }
                        return Ok((dst.unwrap_or_else(|| self.const_unit()), sig.ret));
                    }
                }
            }
        }
        let (ov, oty) = self.lower_expr(obj)?;
        // `ObjCBlock<fn(...)>.invoke(args)` — call the block.
        // Receiver lowers to MirTy::I64 (the block pointer), so we
        // can't disambiguate on receiver type alone; the type
        // checker already restricted us to void-returning
        // signatures so the dispatch is purely a function of the
        // lowered arg shapes. New shapes append, matching the
        // `BlockKind` table in `ilang_runtime::objc_blocks`.
        if (method.as_str() == "invoke" || method.as_str() == "__invokeIdToId")
            && matches!(oty, MirTy::I64)
        {
            // Suppress this fast-path for non-ObjCBlock i64 obj —
            // the type checker only routes ObjCBlock.invoke here
            // (other i64 receivers don't expose `invoke` as a
            // method), so this match is safe in practice.
            let mut arg_vs: Vec<ValueId> = Vec::with_capacity(args.len());
            let mut arg_tys: Vec<MirTy> = Vec::with_capacity(args.len());
            for a in args {
                let (v, t) = self.lower_expr(a)?;
                arg_vs.push(v);
                arg_tys.push(t);
            }
            // The mangler tagged i64-returning invokes with a
            // distinct method name so MIR can pick the obj-to-obj
            // runtime invoker (`__ilang_invoke_obj_to_obj_block`,
            // returns i64). The void-returning fast-paths keep the
            // original `invoke` name.
            let returns_id = method.as_str() == "__invokeIdToId";
            let builtin = if returns_id {
                match arg_tys.as_slice() {
                    [MirTy::I64] => Some("invoke_obj_to_obj_block"),
                    _ => None,
                }
            } else {
                match arg_tys.as_slice() {
                    [] => Some("invoke_void_block"),
                    [MirTy::I64] => Some("invoke_obj_block"),
                    [MirTy::I64, MirTy::I64] => Some("invoke_void_bytes_block"),
                    [MirTy::I64, MirTy::I64, MirTy::I64] => {
                        Some("invoke_void_three_obj_block")
                    }
                    [MirTy::Bool] => Some("invoke_void_bool_block"),
                    _ => None,
                }
            };
            if let Some(name) = builtin {
                let mut call_args: Vec<ValueId> = Vec::with_capacity(arg_vs.len() + 1);
                call_args.push(ov);
                call_args.extend(arg_vs);
                if returns_id {
                    let dst = self.fb.new_value(MirTy::I64);
                    self.fb.push_inst(Inst::Call {
                        dst: Some(dst),
                        callee: FuncRef::Builtin(Symbol::intern(name)),
                        args: call_args.into_boxed_slice(),
                    });
                    return Ok((dst, MirTy::I64));
                } else {
                    self.fb.push_inst(Inst::Call {
                        dst: None,
                        callee: FuncRef::Builtin(Symbol::intern(name)),
                        args: call_args.into_boxed_slice(),
                    });
                    return Ok((self.const_unit(), MirTy::Unit));
                }
            }
            return Err(LowerError::Other(format!(
                "ObjCBlock.invoke(...) signature not yet supported: \
                 returns_id={returns_id}, args={:?}",
                arg_tys
            )));
        }
        // `.toString()` is available on every numeric / bool / string.
        if method.as_str() == "toString" && args.is_empty() {
            if oty.is_int() || oty.is_float() || matches!(oty, MirTy::Bool | MirTy::Str) {
                let v = self.fb.new_value(MirTy::Str);
                let builtin = match &oty {
                    MirTy::Bool => "bool_to_string",
                    MirTy::Str => "str_to_string",
                    t if t.is_float() => "float_to_string",
                    _ => "int_to_string",
                };
                self.fb.push_inst(Inst::Call {
                    dst: Some(v),
                    callee: FuncRef::Builtin(Symbol::intern(builtin)),
                    args: Box::new([ov]),
                });
                return Ok((v, MirTy::Str));
            }
        }
        // Limited builtin dispatch for arrays / Optional / strings.
        // User-class method dispatch arrives with classes (later step).
        match (&oty, method.as_str()) {
            (MirTy::Optional(_), "unwrap") => {
                if !args.is_empty() {
                    return Err(LowerError::Other("Optional.unwrap takes no args".into()));
                }
                let inner = match &oty {
                    MirTy::Optional(t) => (**t).clone(),
                    _ => unreachable!(),
                };
                let v = self.fb.new_value(inner.clone());
                self.fb.push_inst(Inst::OptionalUnwrap { dst: v, opt: ov });
                // The unwrapped value aliases the Optional cell's
                // `value` slot — same heap pointer. Without a retain,
                // the receiver and the Optional cell's eventual
                // cascade-release would both decrement the same rc,
                // double-freeing the inner. Bump rc on heap-typed
                // inners so the two release sites balance. (Caught by
                // ASan as a UAF in `host_release_optional` while
                // tearing down `Optional<Optional<Str>>`.)
                if matches!(
                    inner,
                    MirTy::Object(_)
                        | MirTy::Array { .. }
                        | MirTy::Tuple(_)
                        | MirTy::Map { .. }
                        | MirTy::Optional(_)
                        | MirTy::Fn(_)
                        | MirTy::Str
                ) {
                    self.fb.push_inst(Inst::Retain { value: v });
                }
                Ok((v, inner))
            }
            (MirTy::Array { elem, .. }, "push") => {
                if args.len() != 1 {
                    return Err(LowerError::Other("Array.push takes 1 arg".into()));
                }
                let elem_ty = (**elem).clone();
                let value_is_fresh = self.is_fresh_object_expr(&args[0]);
                let (av, aty) = self.lower_expr(&args[0])?;
                let coerced = if aty == elem_ty {
                    av
                } else {
                    self.coerce(av, &aty, &elem_ty, args[0].span)?
                };
                // Bump rc on borrowed heap values — `array_push` stores
                // the cell verbatim, but `__release_array`'s cascade
                // will eventually release every stored element. Without
                // this retain, `surviving.push(b)` where `b = arr[i]`
                // would share rc with the source slot, dropping the
                // element to 0 when the source local exits and freeing
                // it out from under the receiving array.
                if !value_is_fresh {
                    retain_if_heap(&mut self.fb, coerced, &elem_ty);
                }
                self.fb.push_inst(Inst::Call {
                    dst: None,
                    callee: FuncRef::Builtin(Symbol::intern("array_push")),
                    args: Box::new([ov, coerced]),
                });
                Ok((self.const_unit(), MirTy::Unit))
            }
            (MirTy::Array { elem, .. }, "pop") => {
                let elem_ty = (**elem).clone();
                let opt_ty = MirTy::Optional(Box::new(elem_ty.clone()));
                let v = self.fb.new_value(opt_ty.clone());
                self.fb.push_inst(Inst::Call {
                    dst: Some(v),
                    callee: FuncRef::Builtin(Symbol::intern("array_pop")),
                    args: Box::new([ov]),
                });
                Ok((v, opt_ty))
            }
            (MirTy::Array { .. }, "indexOf") => {
                if args.len() != 1 {
                    return Err(LowerError::Other("Array.indexOf takes 1 arg".into()));
                }
                let (av, _) = self.lower_expr(&args[0])?;
                let v = self.fb.new_value(MirTy::I64);
                self.fb.push_inst(Inst::Call {
                    dst: Some(v),
                    callee: FuncRef::Builtin(Symbol::intern("array_index_of")),
                    args: Box::new([ov, av]),
                });
                Ok((v, MirTy::I64))
            }
            (MirTy::Array { elem, .. }, "map") => {
                if args.len() != 1 {
                    return Err(LowerError::Other("Array.map takes 1 arg".into()));
                }
                let elem_ty = (**elem).clone();
                let (fv, fty) = self.lower_expr(&args[0])?;
                // Result element type is the closure's return type.
                let ret_ty = if let MirTy::Fn(ft) = &fty {
                    ft.ret.clone()
                } else {
                    elem_ty.clone()
                };
                let arr_ty = MirTy::Array { elem: Box::new(ret_ty.clone()), len: None };
                // Pass the result element's KIND_* tag to host_array_map
                // so the result array's drop cascades correctly. Tags
                // mirror compile.rs's `kind_tag_of`.
                let kind = match &ret_ty {
                    MirTy::Object(_) => 1,
                    MirTy::Array { .. } => 2,
                    MirTy::Optional(_) => 3,
                    MirTy::Tuple(_) => 4,
                    MirTy::Map { .. } => 5,
                    MirTy::Fn(_) => 6,
                    MirTy::Str => 7,
                    _ => 0,
                };
                let kind_v = self.const_int(MirTy::I64, kind);
                let v = self.fb.new_value(arr_ty.clone());
                self.fb.push_inst(Inst::Call {
                    dst: Some(v),
                    callee: FuncRef::Builtin(Symbol::intern("array_map")),
                    args: Box::new([ov, fv, kind_v]),
                });
                Ok((v, arr_ty))
            }
            (MirTy::Array { elem, .. }, "filter") => {
                if args.len() != 1 {
                    return Err(LowerError::Other("Array.filter takes 1 arg".into()));
                }
                let arr_ty = MirTy::Array { elem: elem.clone(), len: None };
                let (fv, _) = self.lower_expr(&args[0])?;
                let v = self.fb.new_value(arr_ty.clone());
                self.fb.push_inst(Inst::Call {
                    dst: Some(v),
                    callee: FuncRef::Builtin(Symbol::intern("array_filter")),
                    args: Box::new([ov, fv]),
                });
                Ok((v, arr_ty))
            }
            (MirTy::Array { .. }, "forEach") => {
                if args.len() != 1 {
                    return Err(LowerError::Other("Array.forEach takes 1 arg".into()));
                }
                let (fv, _) = self.lower_expr(&args[0])?;
                self.fb.push_inst(Inst::Call {
                    dst: None,
                    callee: FuncRef::Builtin(Symbol::intern("array_for_each")),
                    args: Box::new([ov, fv]),
                });
                Ok((self.const_unit(), MirTy::Unit))
            }
            (MirTy::Array { elem, .. }, "slice") => {
                let arr_ty = MirTy::Array { elem: elem.clone(), len: None };
                let mut arg_vals = vec![ov];
                for a in args {
                    let (v, _) = self.lower_expr(a)?;
                    arg_vals.push(v);
                }
                let v = self.fb.new_value(arr_ty.clone());
                self.fb.push_inst(Inst::Call {
                    dst: Some(v),
                    callee: FuncRef::Builtin(Symbol::intern("array_slice")),
                    args: arg_vals.into_boxed_slice(),
                });
                Ok((v, arr_ty))
            }
            (MirTy::Array { .. }, "includes") => {
                if args.len() != 1 {
                    return Err(LowerError::Other("Array.includes takes 1 arg".into()));
                }
                let (av, _) = self.lower_expr(&args[0])?;
                let v = self.fb.new_value(MirTy::Bool);
                self.fb.push_inst(Inst::Call {
                    dst: Some(v),
                    callee: FuncRef::Builtin(Symbol::intern("array_includes")),
                    args: Box::new([ov, av]),
                });
                Ok((v, MirTy::Bool))
            }
            (MirTy::Str, m) => {
                let (builtin_name, ret_ty) = match m {
                    "charAt" => ("str_char_at", MirTy::Str),
                    "includes" => ("str_includes", MirTy::Bool),
                    "startsWith" => ("str_starts_with", MirTy::Bool),
                    "endsWith" => ("str_ends_with", MirTy::Bool),
                    "toUpper" => ("str_to_upper", MirTy::Str),
                    "toLower" => ("str_to_lower", MirTy::Str),
                    "trim" => ("str_trim", MirTy::Str),
                    "split" => (
                        "str_split",
                        MirTy::Array { elem: Box::new(MirTy::Str), len: None },
                    ),
                    "replace" => ("str_replace", MirTy::Str),
                    "slice" => ("str_slice", MirTy::Str),
                    other => {
                        return Err(LowerError::Other(format!(
                            "unknown string method `{other}`"
                        )))
                    }
                };
                let mut arg_vals = vec![ov];
                for a in args {
                    let (v, _) = self.lower_expr(a)?;
                    arg_vals.push(v);
                }
                let dst = if matches!(ret_ty, MirTy::Unit) {
                    None
                } else {
                    Some(self.fb.new_value(ret_ty.clone()))
                };
                self.fb.push_inst(Inst::Call {
                    dst,
                    callee: FuncRef::Builtin(Symbol::intern(builtin_name)),
                    args: arg_vals.into_boxed_slice(),
                });
                Ok((dst.unwrap_or_else(|| self.const_unit()), ret_ty))
            }
            (MirTy::Promise(inner), m @ ("then" | "catch")) => {
                if args.len() != 1 {
                    return Err(LowerError::Other(format!(
                        "Promise.{m} takes 1 callback arg"
                    )));
                }
                // Lower the callback closure; from its fn-ty we
                // figure out the downstream Promise's element type
                // (then's `cb: fn(T): U` ⇒ Promise<U>; catch's
                // `cb: fn(string): T` ⇒ Promise<T>).
                let cb_is_fresh = self.is_fresh_object_expr(&args[0]);
                let (cb_v, cb_ty) = self.lower_expr(&args[0])?;
                let out_inner = match (&cb_ty, m) {
                    (MirTy::Fn(ft), _) => ft.ret.clone(),
                    (_, "catch") => (**inner).clone(),
                    _ => MirTy::Unit,
                };
                let out_kind = kind_tag_of_mir(&out_inner, self.classes);
                let out_kind_v = self.const_int(MirTy::I64, out_kind);
                // Runtime takes ownership of the callback's +1.
                if !cb_is_fresh {
                    self.fb.push_inst(Inst::Retain { value: cb_v });
                }
                let result_ty = MirTy::Promise(Box::new(out_inner));
                let dst = self.fb.new_value(result_ty.clone());
                let builtin = if m == "then" { "promise_then" } else { "promise_catch" };
                self.fb.push_inst(Inst::Call {
                    dst: Some(dst),
                    callee: FuncRef::Builtin(Symbol::intern(builtin)),
                    args: Box::new([ov, cb_v, out_kind_v]),
                });
                Ok((dst, result_ty))
            }
            (MirTy::Map { key, val }, m) => {
                let (builtin_name, ret_ty) = match m {
                    "get" => (
                        "map_get_optional",
                        MirTy::Optional(Box::new((**val).clone())),
                    ),
                    "has" => ("map_has", MirTy::Bool),
                    "delete" => ("map_delete", MirTy::Bool),
                    "set" => ("map_set", MirTy::Unit),
                    "size" => ("map_size", MirTy::I64),
                    "keys" => (
                        "map_keys",
                        MirTy::Array { elem: Box::new((**key).clone()), len: None },
                    ),
                    "values" => (
                        "map_values",
                        MirTy::Array { elem: Box::new((**val).clone()), len: None },
                    ),
                    other => {
                        return Err(LowerError::Other(format!("unknown map method `{other}`")))
                    }
                };
                let mut arg_vals = vec![ov];
                let mut arg_meta: Vec<(bool, crate::inst::ValueId, MirTy)> = Vec::new();
                for a in args {
                    let arg_is_fresh = self.is_fresh_object_expr(a);
                    let (v, vty) = self.lower_expr(a)?;
                    // Map host fns are uniformly (i64, i64, i64). Cast
                    // smaller / float / bool args to i64 cells.
                    let v_ext = if matches!(vty, MirTy::I64 | MirTy::U64)
                        || vty.is_heap()
                        || vty.is_float()
                    {
                        // i64-shaped or f64-shaped values pass through;
                        // floats reinterpret bits via host
                        // `extend_to_i64` at the codegen layer.
                        v
                    } else if vty.is_int() || matches!(vty, MirTy::Bool) {
                        let dst_v = self.fb.new_value(MirTy::I64);
                        self.fb.push_inst(Inst::Cast {
                            dst: dst_v,
                            kind: crate::inst::CastKind::IntResize,
                            src: v,
                        });
                        dst_v
                    } else {
                        v
                    };
                    arg_vals.push(v_ext);
                    arg_meta.push((arg_is_fresh, v_ext, vty));
                }
                let dst = if matches!(ret_ty, MirTy::Unit) {
                    None
                } else {
                    Some(self.fb.new_value(ret_ty.clone()))
                };
                self.fb.push_inst(Inst::Call {
                    dst,
                    callee: FuncRef::Builtin(Symbol::intern(builtin_name)),
                    args: arg_vals.into_boxed_slice(),
                });
                // m.set takes its own +1 share via host_map_set's
                // retain_by_kind. Mirror the AssignIndex path — for a
                // fresh value the caller's transient +1 is released
                // here so the only remaining share is the map's.
                if m == "set" {
                    if let Some((is_fresh, vv, vty)) = arg_meta.get(1) {
                        if *is_fresh && self.is_arc_heap(vty) {
                            self.fb.push_inst(Inst::Release { value: *vv });
                        }
                    }
                }
                // Fresh map receiver, non-Object result: release the
                // map after the dispatch so its cascade fires.
                if obj_is_fresh
                    && !matches!(ret_ty, MirTy::Object(_))
                    && m != "get"
                    && m != "set"
                {
                    self.fb.push_inst(Inst::Release { value: ov });
                }
                Ok((dst.unwrap_or_else(|| self.const_unit()), ret_ty))
            }
            (MirTy::Weak(class_id), "get") => {
                let opt_ty = MirTy::Optional(Box::new(MirTy::Object(*class_id)));
                let dst = self.fb.new_value(opt_ty.clone());
                self.fb.push_inst(Inst::WeakUpgrade { dst, weak: ov });
                Ok((dst, opt_ty))
            }
            (MirTy::Object(class_id), _) => {
                // Interface dispatch: when the static receiver type
                // is an interface, look the method's slot up in the
                // global iface table and emit a `VirtCall` against
                // the receiver. The runtime reads the receiver's
                // actual class id from the heap header and routes to
                // the implementing class's fn registered at this
                // slot during class lowering.
                let iface_name = self
                    .interface_ids
                    .iter()
                    .find_map(|(n, cid)| if cid == class_id { Some(*n) } else { None });
                if let Some(ifn) = iface_name {
                    // @com interface — receiver is a `*void` whose
                    // first 8 bytes point to a fn-pointer vtable.
                    // Dispatch goes through `ComCall` (raw vtable
                    // indirection); slot is per-interface and
                    // 0-based, concatenated from any parent chain.
                    if self.com_interfaces.contains(&ifn) {
                        let slot = self
                            .com_iface_slots
                            .get(&(ifn, method))
                            .copied()
                            .ok_or_else(|| {
                                LowerError::Other(format!(
                                    "@com interface `{ifn}` has no method `{method}`"
                                ))
                            })?;
                        let sig = self
                            .iface_method_sigs
                            .get(&(ifn, method))
                            .cloned()
                            .ok_or_else(|| {
                                LowerError::Other(format!(
                                    "@com interface `{ifn}` method `{method}` has no recorded signature"
                                ))
                            })?;
                        // Receiver becomes the first C ABI param
                        // (the COM `this` pointer). The codegen
                        // prepends `recv` itself, so the call's
                        // signature here lists `this` as the first
                        // param. Force the recv MIR type onto i64
                        // so the call_indirect sees a plain pointer.
                        let recv_i64 = self.fb.new_value(MirTy::I64);
                        self.fb.push_inst(Inst::Cast {
                            dst: recv_i64,
                            kind: crate::inst::CastKind::PtrIntCast,
                            src: ov,
                        });
                        let mut com_sig_params: Vec<MirTy> =
                            Vec::with_capacity(sig.params.len() + 1);
                        com_sig_params.push(MirTy::I64);
                        com_sig_params.extend(sig.params.iter().cloned());
                        let mut user_args: Vec<ValueId> = Vec::with_capacity(args.len());
                        for (i, a) in args.iter().enumerate() {
                            let (v, vty) = self.lower_expr(a)?;
                            let target = sig.params.get(i);
                            let coerced = match target {
                                Some(t) if t != &vty => self.coerce(v, &vty, t, a.span)?,
                                _ => v,
                            };
                            user_args.push(coerced);
                        }
                        let dst = if matches!(sig.ret, MirTy::Unit) {
                            None
                        } else {
                            Some(self.fb.new_value(sig.ret.clone()))
                        };
                        let com_sig = crate::inst::FnSig {
                            params: com_sig_params.into_boxed_slice(),
                            ret: sig.ret.clone(),
                            variadic: false,
                        };
                        self.fb.push_inst(Inst::ComCall {
                            dst,
                            recv: recv_i64,
                            slot,
                            sig: com_sig,
                            args: user_args.into_boxed_slice(),
                        });
                        return Ok((dst.unwrap_or_else(|| self.const_unit()), sig.ret));
                    }
                    let slot = self
                        .iface_method_slots
                        .get(&(ifn, method))
                        .copied()
                        .ok_or_else(|| {
                            LowerError::Other(format!(
                                "interface `{ifn}` has no method `{method}`"
                            ))
                        })?;
                    let sig = self
                        .iface_method_sigs
                        .get(&(ifn, method))
                        .cloned()
                        .ok_or_else(|| {
                            LowerError::Other(format!(
                                "interface `{ifn}` method `{method}` has no recorded signature"
                            ))
                        })?;
                    let mut user_args: Vec<ValueId> = Vec::with_capacity(args.len());
                    for (i, a) in args.iter().enumerate() {
                        let (v, vty) = self.lower_expr(a)?;
                        let target = sig.params.get(i);
                        let coerced = match target {
                            Some(t) if t != &vty => self.coerce(v, &vty, t, a.span)?,
                            _ => v,
                        };
                        user_args.push(coerced);
                    }
                    let dst = if matches!(sig.ret, MirTy::Unit) {
                        None
                    } else {
                        Some(self.fb.new_value(sig.ret.clone()))
                    };
                    self.fb.push_inst(Inst::VirtCall {
                        dst,
                        recv: ov,
                        slot: crate::inst::VTableSlot(slot),
                        args: user_args.into_boxed_slice(),
                    });
                    if obj_is_fresh && !matches!(sig.ret, MirTy::Object(_)) {
                        self.fb.push_inst(Inst::Release { value: ov });
                    }
                    return Ok((dst.unwrap_or_else(|| self.const_unit()), sig.ret));
                }
                let meta = self.class_meta.get(class_id).expect("class meta");
                let mid = *meta.method_ids.get(&method).ok_or_else(|| {
                    LowerError::Other(format!("no method `{method}` on class"))
                })?;
                let sig = meta.method_sigs.get(&method).cloned().unwrap();
                let slot = self.classes[class_id.0 as usize]
                    .methods
                    .iter()
                    .find(|m| m.name == method)
                    .and_then(|m| m.slot);

                let mut arg_vals_all = Vec::with_capacity(args.len() + 1);
                arg_vals_all.push(ov);
                let mut fresh_obj_args: Vec<ValueId> = Vec::new();
                for (i, a) in args.iter().enumerate() {
                    let arg_is_fresh = self.is_fresh_object_expr(a);
                    let (v, vty) = self.lower_expr(a)?;
                    let target = sig.params.get(i + 1);
                    let coerced = match target {
                        Some(t) if t != &vty => self.coerce(v, &vty, t, a.span)?,
                        _ => v,
                    };
                    if arg_is_fresh && matches!(vty, MirTy::Object(_)) {
                        fresh_obj_args.push(coerced);
                    }
                    arg_vals_all.push(coerced);
                }
                let dst = if matches!(sig.ret, MirTy::Unit) {
                    None
                } else {
                    Some(self.fb.new_value(sig.ret.clone()))
                };
                if let Some(slot) = slot {
                    let user_args: Box<[ValueId]> =
                        arg_vals_all[1..].to_vec().into_boxed_slice();
                    self.fb.push_inst(Inst::VirtCall {
                        dst,
                        recv: ov,
                        slot,
                        args: user_args,
                    });
                } else {
                    self.fb.push_inst(Inst::Call {
                        dst,
                        callee: FuncRef::Local(mid),
                        args: arg_vals_all.into_boxed_slice(),
                    });
                }
                for fv in fresh_obj_args {
                    self.fb.push_inst(Inst::Release { value: fv });
                }
                // Release a fresh receiver that nothing else owns, but
                // only when the result isn't itself an Object that may
                // alias the receiver's fields.
                if obj_is_fresh && !matches!(sig.ret, MirTy::Object(_)) {
                    self.fb.push_inst(Inst::Release { value: ov });
                }
                Ok((dst.unwrap_or_else(|| self.const_unit()), sig.ret))
            }
            _ => Err(LowerError::Unsupported(
                "method call on this type / unhandled builtin",
            )),
        }
    }

    /// COM vtable dispatch: `vtbl.Method(args)` where `vtbl` is
    /// `*T` and `T` is an @extern(C) struct with a fn-typed
    /// `Method` field. Returns `Ok(None)` when the receiver
    /// isn't a raw pointer to a CRepr/CPacked/CUnion struct, or
    /// when no fn-typed field by that name exists — the caller
    /// then falls through to the normal method-dispatch paths.
    ///
    /// The receiver's type must be discoverable *without* lowering
    /// (so the fall-through case doesn't double-emit instructions
    /// for `obj`). We peek through Var / cast / Paren forms; other
    /// shapes fall through unchanged.
    fn try_lower_c_struct_vtable_call(
        &mut self,
        obj: &Expr,
        method: Symbol,
        args: &[Expr],
    ) -> Result<Option<(ValueId, MirTy)>, LowerError> {
        // Peek at the receiver's type without lowering. Two
        // shapes cover the practical cases:
        //   - `vtbl` — a local of declared type `*T`
        //   - `(expr as *T)` — explicit cast targeting `*T`
        let inner_cid = self.peek_c_struct_ptr_class(obj)?;
        let Some(cid) = inner_cid else {
            return Ok(None);
        };
        let meta = self.class_meta.get(&cid).expect("class meta");
        let Some(&fid) = meta.field_ix.get(&method) else {
            return Err(LowerError::Other(format!(
                "no field `{method}` on c-struct class id #{}",
                cid.0
            )));
        };
        let fty = meta.field_ty.get(&fid).cloned().unwrap();
        let MirTy::Fn(ft) = fty else {
            return Err(LowerError::Other(format!(
                "field `{method}` on c-struct is not fn-typed (got {fty})"
            )));
        };
        let off = self.classes[cid.0 as usize]
            .c_field_offsets
            .get(fid.0 as usize)
            .copied()
            .ok_or_else(|| LowerError::Other(format!("missing c_field_offset for `{method}`")))?;
        // Type peek matched — lower the receiver now (this is the
        // first and only side-effecting lower_expr on `obj`).
        let (ov, _oty) = self.lower_expr(obj)?;
        // Load `*(u64*)(ptr + off)` via the __read_u64 builtin.
        let addr = self.fb.new_value(MirTy::I64);
        self.fb.push_inst(Inst::Cast {
            dst: addr,
            kind: crate::inst::CastKind::PtrIntCast,
            src: ov,
        });
        let off_v = self.const_int(MirTy::I64, off);
        let raw_u64 = self.fb.new_value(MirTy::U64);
        self.fb.push_inst(Inst::Call {
            dst: Some(raw_u64),
            callee: FuncRef::Builtin(Symbol::intern("__read_u64")),
            args: Box::new([addr, off_v]),
        });
        let raw_fn_ty = MirTy::RawFn(ft.clone());
        let callee_v = self.fb.new_value(raw_fn_ty.clone());
        self.fb.push_inst(Inst::Cast {
            dst: callee_v,
            kind: crate::inst::CastKind::PtrIntCast,
            src: raw_u64,
        });
        // Lower args, coercing each to the declared param type.
        let mut arg_vals = Vec::with_capacity(args.len());
        for (i, a) in args.iter().enumerate() {
            let (v, vty) = self.lower_expr(a)?;
            let coerced = match ft.params.get(i) {
                Some(t) if t != &vty => self.coerce(v, &vty, t, a.span)?,
                _ => v,
            };
            arg_vals.push(coerced);
        }
        let sig = crate::inst::FnSig {
            params: ft.params.clone(),
            ret: ft.ret.clone(),
            variadic: false,
        };
        let dst = if matches!(ft.ret, MirTy::Unit) {
            None
        } else {
            Some(self.fb.new_value(ft.ret.clone()))
        };
        self.fb.push_inst(Inst::CallRawIndirect {
            dst,
            callee: callee_v,
            sig,
            args: arg_vals.into_boxed_slice(),
        });
        Ok(Some((dst.unwrap_or_else(|| self.const_unit()), ft.ret.clone())))
    }

    /// Peek at an expression's static type to see whether it's
    /// `*T` where T is a CRepr/CPacked/CUnion struct. Returns
    /// `Some(class_id)` when it is. Used by the COM-vtable
    /// dispatch fast path to gate without committing side effects.
    fn peek_c_struct_ptr_class(
        &mut self,
        e: &Expr,
    ) -> Result<Option<crate::types::ClassId>, LowerError> {
        use crate::program::ClassRepr;
        let ty = match &e.kind {
            ExprKind::Var(name) => self.lookup_var(*name).map(|(_, t)| t),
            ExprKind::Cast { ty, .. } => Some(self.resolve_ty(ty)?),
            _ => None,
        };
        let Some(MirTy::RawPtr { inner, .. }) = ty else {
            return Ok(None);
        };
        let MirTy::Object(cid) = *inner else {
            return Ok(None);
        };
        let is_c_struct = matches!(
            self.classes[cid.0 as usize].repr,
            ClassRepr::CRepr | ClassRepr::CPacked | ClassRepr::CUnion
        );
        Ok(if is_c_struct { Some(cid) } else { None })
    }
}
