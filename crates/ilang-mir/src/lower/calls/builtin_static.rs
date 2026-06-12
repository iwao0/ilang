//! Name-based static dispatch — runs before the receiver is
//! lowered. Covers `console.log`, all `Promise.*` factories, the
//! `string.fromUtf16` factory, and the generic `Class.staticMethod`
//! path. Called only when the receiver expression is a bare `Var`
//! that doesn't resolve to a local binding.

use ilang_ast::{Expr, Symbol};

use crate::inst::{FuncRef, Inst, ValueId};
use crate::types::MirTy;

use super::super::{BodyCx, LowerError};
use super::kind_tag_of_mir;

impl<'a> BodyCx<'a> {
    /// Returns `Some` when the call was a builtin static dispatch
    /// (`console.log`, `Promise.*`, `string.fromUtf16`, or a normal
    /// `Class.staticMethod(...)`). Returns `None` otherwise so the
    /// caller continues with instance dispatch.
    pub(super) fn try_lower_builtin_static(
        &mut self,
        name: Symbol,
        method: Symbol,
        args: &[Expr],
    ) -> Result<Option<(ValueId, MirTy)>, LowerError> {
        // `console.log(...)` is a special-cased variadic builtin.
        if name.as_str() == "console" && method.as_str() == "log" {
            return self.lower_console_log(args).map(Some);
        }
        // Everything else here gates on "no local shadow exists for
        // this Var name" — `let Promise = …` should mask the
        // built-in factories.
        if self.lookup_var(name).is_some() {
            return Ok(None);
        }
        if name.as_str() == "Promise" {
            if let Some(out) = self.try_lower_promise_static(method, args)? {
                return Ok(Some(out));
            }
        }
        if name.as_str() == "string"
            && method.as_str() == "fromUtf16"
            && args.len() == 1
        {
            return self.lower_string_from_utf16(args).map(Some);
        }
        // `ClassName.staticMethod(args)` — walk the parent chain so
        // an inherited static (`SKScene.alloc()` where `alloc` lives
        // on SKNode → NSObject) resolves through the subclass's name.
        self.try_lower_class_static_method(name, method, args)
    }

    fn lower_console_log(&mut self, args: &[Expr]) -> Result<(ValueId, MirTy), LowerError> {
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
        Ok((self.const_unit(), MirTy::Unit))
    }

    fn try_lower_promise_static(
        &mut self,
        method: Symbol,
        args: &[Expr],
    ) -> Result<Option<(ValueId, MirTy)>, LowerError> {
        // Built-in `Promise.all(ps)` / `Promise.race(ps)`.
        if (method.as_str() == "all" || method.as_str() == "race") && args.len() == 1 {
            return self.lower_promise_combinator(method, args).map(Some);
        }
        // Internal `Promise.$promise.pending<T>()` — allocates a
        // Pending promise. Used by the async-fn desugar.
        if method.as_str() == "$promise.pending" && args.is_empty() {
            let prom_ty = MirTy::Promise(Box::new(MirTy::Unit));
            let dst = self.fb.new_value(prom_ty.clone());
            self.fb.push_inst(Inst::Call {
                dst: Some(dst),
                callee: FuncRef::Builtin(Symbol::intern("promise_pending")),
                args: Box::new([]),
            });
            return Ok(Some((dst, prom_ty)));
        }
        // Internal `Promise.$promise.settleResolve<T>(p, v)`.
        if method.as_str() == "$promise.settleResolve" && args.len() == 2 {
            return self.lower_promise_settle_resolve(args).map(Some);
        }
        // Internal `Promise.$promise.settleReject(p, msg)`.
        if method.as_str() == "$promise.settleReject" && args.len() == 2 {
            return self.lower_promise_settle_reject(args).map(Some);
        }
        // Internal `Promise.$promise.rejectFollows(upstream, target)`.
        // Both args are borrows: the runtime's forwarder cell takes
        // its own +1 on `target`, and nothing outlives the call on
        // the `upstream` side beyond the waiter the runtime registers.
        // The desugar only ever passes named locals / field reads
        // (never fresh values), so no transfer accounting is needed.
        if method.as_str() == "$promise.rejectFollows" && args.len() == 2 {
            let (uv, _) = self.lower_expr(&args[0])?;
            let (tv, _) = self.lower_expr(&args[1])?;
            self.fb.push_inst(Inst::Call {
                dst: None,
                callee: FuncRef::Builtin(Symbol::intern("promise_reject_follows")),
                args: Box::new([uv, tv]),
            });
            return Ok(Some((self.const_unit(), MirTy::Unit)));
        }
        // `Promise.reject(msg)` static factory.
        if method.as_str() == "reject" && args.len() == 1 {
            return self.lower_promise_reject(args).map(Some);
        }
        // `Promise.resolve(v)` static factory.
        if method.as_str() == "resolve" && args.len() == 1 {
            return self.lower_promise_resolve(args).map(Some);
        }
        Ok(None)
    }

    fn lower_promise_combinator(
        &mut self,
        method: Symbol,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
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
        Ok((dst, prom_ty))
    }

    fn lower_promise_settle_resolve(
        &mut self,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
        let p_is_fresh = self.is_fresh_object_expr(&args[0]);
        let (pv, _) = self.lower_expr(&args[0])?;
        if !p_is_fresh {
            self.fb.push_inst(Inst::Retain { value: pv });
        }
        let v_is_fresh = self.is_fresh_object_expr(&args[1]);
        let (vv, vty) = self.lower_expr(&args[1])?;
        let vv = match self.copy_fixed_for_cell(vv, &vty) {
            Some(copy) => copy,
            None => {
                if !v_is_fresh && self.is_arc_heap(&vty) {
                    self.fb.push_inst(Inst::Retain { value: vv });
                }
                vv
            }
        };
        let kind = kind_tag_of_mir(&vty, self.classes);
        let kind_v = self.const_int(MirTy::I64, kind);
        self.fb.push_inst(Inst::Call {
            dst: None,
            callee: FuncRef::Builtin(Symbol::intern("promise_settle_resolve")),
            args: Box::new([pv, vv, kind_v]),
        });
        Ok((self.const_unit(), MirTy::Unit))
    }

    fn lower_promise_settle_reject(
        &mut self,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
        let p_is_fresh = self.is_fresh_object_expr(&args[0]);
        let (pv, _) = self.lower_expr(&args[0])?;
        if !p_is_fresh {
            self.fb.push_inst(Inst::Retain { value: pv });
        }
        let msg_is_fresh = self.is_fresh_object_expr(&args[1]);
        let (mv, _) = self.lower_expr(&args[1])?;
        // The type checker pins msg to `string` for this builtin
        // (see ilang-types/.../builtins.rs `$promise.settleReject`),
        // so the unconditional Retain is sound — Str is always
        // ARC-heap. The settleResolve path runs an explicit
        // `is_arc_heap` check because its value is generic.
        if !msg_is_fresh {
            self.fb.push_inst(Inst::Retain { value: mv });
        }
        self.fb.push_inst(Inst::Call {
            dst: None,
            callee: FuncRef::Builtin(Symbol::intern("promise_settle_reject")),
            args: Box::new([pv, mv]),
        });
        Ok((self.const_unit(), MirTy::Unit))
    }

    fn lower_promise_reject(&mut self, args: &[Expr]) -> Result<(ValueId, MirTy), LowerError> {
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
        Ok((dst, prom_ty))
    }

    fn lower_promise_resolve(&mut self, args: &[Expr]) -> Result<(ValueId, MirTy), LowerError> {
        let arg_is_fresh = self.is_fresh_object_expr(&args[0]);
        let (vv, vty) = self.lower_expr(&args[0])?;
        // Fixed-length-array values: the promise cell takes a value
        // copy (its own +1 — consumed like a fresh transfer, so no
        // borrow retain).
        let vv = match self.copy_fixed_for_cell(vv, &vty) {
            Some(copy) => copy,
            None => {
                if !arg_is_fresh && self.is_arc_heap(&vty) {
                    self.fb.push_inst(Inst::Retain { value: vv });
                }
                vv
            }
        };
        let kind = kind_tag_of_mir(&vty, self.classes);
        let kind_v = self.const_int(MirTy::I64, kind);
        let prom_ty = MirTy::Promise(Box::new(vty.clone()));
        let dst = self.fb.new_value(prom_ty.clone());
        self.fb.push_inst(Inst::Call {
            dst: Some(dst),
            callee: FuncRef::Builtin(Symbol::intern("promise_resolve")),
            args: Box::new([vv, kind_v]),
        });
        Ok((dst, prom_ty))
    }

    fn lower_string_from_utf16(
        &mut self,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
        let (av, _) = self.lower_expr(&args[0])?;
        let dst = self.fb.new_value(MirTy::Str);
        self.fb.push_inst(Inst::Call {
            dst: Some(dst),
            callee: FuncRef::Builtin(Symbol::intern("str_from_utf16")),
            args: Box::new([av]),
        });
        Ok((dst, MirTy::Str))
    }

    fn try_lower_class_static_method(
        &mut self,
        name: Symbol,
        method: Symbol,
        args: &[Expr],
    ) -> Result<Option<(ValueId, MirTy)>, LowerError> {
        let class_id = super::super::class_id_by_name(self.classes, self.class_meta, name);
        let mut owning_cid: Option<crate::types::ClassId> = None;
        let mut cur = class_id;
        while let Some(c) = cur {
            if self
                .class_meta
                .get(&c)
                .and_then(|m| m.static_method_ids.get(&method))
                .is_some()
            {
                owning_cid = Some(c);
                break;
            }
            cur = self.classes[c.0 as usize].parent;
        }
        let Some(cid) = owning_cid else {
            return Ok(None);
        };
        let meta = self.class_meta.get(&cid).unwrap();
        let Some(&fid) = meta.static_method_ids.get(&method) else {
            return Ok(None);
        };
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
        Ok(Some((dst.unwrap_or_else(|| self.const_unit()), sig.ret)))
    }
}
