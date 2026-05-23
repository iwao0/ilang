//! Free-function call lowering on `BodyCx`.
//!
//! `lower_call(callee, args)` handles the bare `foo(x, y)` form
//! (no receiver). Built-in pseudo-functions (`typeof`, `some`, the
//! optional unwrap), overload resolution, and the per-callee
//! coercion + retain accounting all live here.

use ilang_ast::{Expr, ExprKind, Span, Symbol};

use crate::inst::{FuncRef, Inst, ValueId};
use crate::program::FunctionKind;
use crate::types::MirTy;

use super::utils::pick_overload;
use super::{BodyCx, LowerError};

impl<'a> BodyCx<'a> {
    pub(super) fn lower_call(&mut self, callee: Symbol, args: &[Expr]) -> Result<(ValueId, MirTy), LowerError> {
        // Built-in pseudo-functions handled before generic resolution.
        if callee.as_str() == "typeof" && args.len() == 1 {
            let (v, _) = self.lower_expr(&args[0])?;
            let dst = self.fb.new_value(MirTy::I64);
            self.fb.push_inst(Inst::TypeOf { dst, value: v });
            return Ok((dst, MirTy::I64));
        }
        // arrayFromCArray<T>(p: *const T, n: size_t) — special-case
        // before the generic FFI helper table because we need to
        // peek the actual T off the first arg's MirTy (`*const T`)
        // and pass an explicit elem stride to the host helper. Type
        // monomorphisation already substituted T at the source level.
        if callee.as_str() == "arrayFromCArray" && args.len() == 2 {
            let (pv, pty) = self.lower_expr(&args[0])?;
            let (nv, nty) = self.lower_expr(&args[1])?;
            let elem_ty = match &pty {
                MirTy::RawPtr { inner, .. } => (**inner).clone(),
                _ => MirTy::U8,
            };
            // Coerce length to i64.
            let n_i64 = if matches!(nty, MirTy::I64) {
                nv
            } else {
                self.coerce(nv, &nty, &MirTy::I64, args[1].span)?
            };
            // Coerce ptr to i64 so the host helper sees a uniform
            // address.
            let p_i64 = match &pty {
                MirTy::RawPtr { .. } => {
                    let dst = self.fb.new_value(MirTy::I64);
                    self.fb.push_inst(Inst::Cast {
                        dst,
                        kind: crate::inst::CastKind::PtrIntCast,
                        src: pv,
                    });
                    dst
                }
                _ => pv,
            };
            let stride = match &elem_ty {
                MirTy::I8 | MirTy::U8 | MirTy::CChar | MirTy::Bool => 1,
                MirTy::I16 | MirTy::U16 => 2,
                MirTy::I32 | MirTy::U32 | MirTy::F32 => 4,
                _ => 8,
            };
            let kind_tag = if matches!(elem_ty, MirTy::Object(_) | MirTy::Str) { 1 } else { 0 };
            let stride_v = self.const_int(MirTy::I64, stride);
            let kind_v = self.const_int(MirTy::I64, kind_tag);
            let arr_ty = MirTy::Array { elem: Box::new(elem_ty), len: None };
            let dst = self.fb.new_value(arr_ty.clone());
            self.fb.push_inst(Inst::Call {
                dst: Some(dst),
                callee: FuncRef::Builtin(Symbol::intern("__c_array_to_array")),
                args: Box::new([p_i64, n_i64, stride_v, kind_v]),
            });
            return Ok((dst, arr_ty));
        }
        // `readT(p, off): T` / `writeT(p, off, v)` raw-memory FFI
        // marshalling helpers. Each maps the source name (e.g.
        // `readU64`) to the host symbol (`__read_u64`) and the MIR
        // return type the lowerer should use. The args go through
        // unchanged — the host helper does the offset arithmetic
        // and the right-width primitive load/store.
        let mem_io = match callee.as_str() {
            "readI8" => Some(("__read_i8", MirTy::I8)),
            "readI16" => Some(("__read_i16", MirTy::I16)),
            "readI32" => Some(("__read_i32", MirTy::I32)),
            "readI64" => Some(("__read_i64", MirTy::I64)),
            "readU8" => Some(("__read_u8", MirTy::U8)),
            "readU16" => Some(("__read_u16", MirTy::U16)),
            "readU32" => Some(("__read_u32", MirTy::U32)),
            "readU64" => Some(("__read_u64", MirTy::U64)),
            "readF32" => Some(("__read_f32", MirTy::F32)),
            "readF64" => Some(("__read_f64", MirTy::F64)),
            "writeI8" => Some(("__write_i8", MirTy::Unit)),
            "writeI16" => Some(("__write_i16", MirTy::Unit)),
            "writeI32" => Some(("__write_i32", MirTy::Unit)),
            "writeI64" => Some(("__write_i64", MirTy::Unit)),
            "writeU8" => Some(("__write_u8", MirTy::Unit)),
            "writeU16" => Some(("__write_u16", MirTy::Unit)),
            "writeU32" => Some(("__write_u32", MirTy::Unit)),
            "writeU64" => Some(("__write_u64", MirTy::Unit)),
            "writeF32" => Some(("__write_f32", MirTy::Unit)),
            "writeF64" => Some(("__write_f64", MirTy::Unit)),
            _ => None,
        };
        if let Some((host_sym, ret_ty)) = mem_io {
            let mut arg_vals = Vec::with_capacity(args.len());
            for (i, a) in args.iter().enumerate() {
                let (mut v, vty) = self.lower_expr(a)?;
                // First arg is the pointer (raw or *const T) — coerce
                // to i64 so the host helper sees a uniform address.
                if i == 0 {
                    if matches!(vty, MirTy::RawPtr { .. }) {
                        let dst = self.fb.new_value(MirTy::I64);
                        self.fb.push_inst(Inst::Cast {
                            dst,
                            kind: crate::inst::CastKind::PtrIntCast,
                            src: v,
                        });
                        v = dst;
                    }
                }
                arg_vals.push(v);
            }
            let dst = if matches!(ret_ty, MirTy::Unit) {
                None
            } else {
                Some(self.fb.new_value(ret_ty.clone()))
            };
            self.fb.push_inst(Inst::Call {
                dst,
                callee: FuncRef::Builtin(Symbol::intern(host_sym)),
                args: arg_vals.into_boxed_slice(),
            });
            return Ok((dst.unwrap_or_else(|| self.const_unit()), ret_ty));
        }
        // FFI marshalling helpers (auto-routed to host symbols).
        let ffi_helper = match callee.as_str() {
            "cstrFromString" => Some(MirTy::I64),
            "stringFromCstr" => Some(MirTy::Str),
            "cstrArrayToStrings" => Some(MirTy::Array {
                elem: Box::new(MirTy::Str),
                len: None,
            }),
            "freeCstr" => Some(MirTy::Unit),
            "errnoCheck" => Some(MirTy::Optional(Box::new(MirTy::I32))),
            "errnoCheckI64" => Some(MirTy::Optional(Box::new(MirTy::I64))),
            "bytesFromBuffer" => Some(MirTy::Array {
                elem: Box::new(MirTy::U8),
                len: None,
            }),
            _ => None,
        };
        if let Some(ret_ty) = ffi_helper {
            let mut arg_vals = Vec::with_capacity(args.len());
            for a in args {
                let (v, _vty) = self.lower_expr(a)?;
                arg_vals.push(v);
            }
            let dst = if matches!(ret_ty, MirTy::Unit) {
                None
            } else {
                Some(self.fb.new_value(ret_ty.clone()))
            };
            self.fb.push_inst(Inst::Call {
                dst,
                callee: FuncRef::Builtin(callee),
                args: arg_vals.into_boxed_slice(),
            });
            return Ok((dst.unwrap_or_else(|| self.const_unit()), ret_ty));
        }
        // Local fn-typed binding → call_indirect. Also picks up
        // closure captures (the body's `f(...)` where `f` was
        // captured from the outer scope) and REPL persistent slots
        // (a fn value bound at top level in a prior chunk).
        let local_or_capture = self.lookup_var(callee).or_else(|| {
            self.captures_in_scope.and_then(|caps| {
                caps.get(&callee).cloned().map(|(idx, cty)| {
                    let v = self.fb.new_value(cty.clone());
                    self.fb.push_inst(Inst::LoadCapture { dst: v, idx });
                    (v, cty)
                })
            })
            .or_else(|| {
                self.repl_slots.get(&callee).cloned().and_then(|(idx, slot_ty)| {
                    if !matches!(slot_ty, MirTy::Fn(_)) {
                        return None;
                    }
                    let idx_v = self.const_int(MirTy::I64, idx as i64);
                    let raw = self.fb.new_value(MirTy::I64);
                    self.fb.push_inst(Inst::Call {
                        dst: Some(raw),
                        callee: FuncRef::Builtin(Symbol::intern("__repl_load_slot")),
                        args: Box::new([idx_v]),
                    });
                    // Borrow from the slot — the slot keeps the
                    // owning ref. No retain here (the call site
                    // doesn't take persistent ownership of the fn
                    // value, it just invokes it).
                    let v = self.i64_to_slot_value(raw, &slot_ty).ok()?;
                    Some((v, slot_ty))
                })
            })
        });
        if let Some((closure_v, closure_ty)) = local_or_capture {
            // Match either a normal closure (`Fn`) or a raw C fn ptr
            // (`RawFn`). The two share the same call-site shape but use
            // different MIR instructions: closures go through
            // `CallIndirect` (env appended), raw fn ptrs go through
            // `CallRawIndirect` (no env, the value is the fn ptr itself).
            let raw = matches!(&closure_ty, MirTy::RawFn(_));
            let ft_opt = match &closure_ty {
                MirTy::Fn(ft) | MirTy::RawFn(ft) => Some(ft.clone()),
                _ => None,
            };
            if let Some(ft) = ft_opt {
                let sig_params = ft.params.clone();
                let sig_ret = ft.ret.clone();
                let mut arg_vals = Vec::with_capacity(args.len());
                for (i, a) in args.iter().enumerate() {
                    let (v, vty) = self.lower_expr(a)?;
                    let coerced = match sig_params.get(i) {
                        Some(t) if t != &vty => self.coerce(v, &vty, t, a.span)?,
                        _ => v,
                    };
                    arg_vals.push(coerced);
                }
                let dst = if matches!(sig_ret, MirTy::Unit) {
                    None
                } else {
                    Some(self.fb.new_value(sig_ret.clone()))
                };
                let sig = crate::inst::FnSig {
                    params: sig_params,
                    ret: sig_ret.clone(),
                    variadic: false,
                };
                if raw {
                    self.fb.push_inst(Inst::CallRawIndirect {
                        dst,
                        callee: closure_v,
                        sig,
                        args: arg_vals.into_boxed_slice(),
                    });
                } else {
                    self.fb.push_inst(Inst::CallIndirect {
                        dst,
                        callee: closure_v,
                        sig,
                        args: arg_vals.into_boxed_slice(),
                    });
                }
                return Ok((dst.unwrap_or_else(|| self.const_unit()), sig_ret));
            }
        }
        // Overloaded fn lookup (multiple candidates registered under
        // `callee`). Pick the one whose param types accept every arg.
        if let Some(candidates) = self.overloads_lookup(callee) {
            if candidates.len() > 1 {
                // Lower args once for type inspection.
                let arg_tys: Vec<(ValueId, MirTy, Span)> = args
                    .iter()
                    .map(|a| {
                        let (v, ty) = self.lower_expr(a)?;
                        Ok((v, ty, a.span))
                    })
                    .collect::<Result<_, LowerError>>()?;

                let pick = pick_overload(self.fn_sigs, &candidates, &arg_tys);
                let chosen = match pick {
                    Some(c) => c,
                    None => {
                        return Err(LowerError::Other(format!(
                            "no matching overload for `{callee}`"
                        )))
                    }
                };
                let sig = self.fn_sigs.get(&chosen).cloned().unwrap();
                let id = *self.fn_ids.get(&chosen).unwrap();
                let mut coerced = Vec::with_capacity(arg_tys.len());
                for (i, (v, vty, span)) in arg_tys.into_iter().enumerate() {
                    let target = sig.params.get(i);
                    let cv = match target {
                        Some(t) if t != &vty => self.coerce(v, &vty, t, span)?,
                        _ => v,
                    };
                    coerced.push(cv);
                }
                let dst = if matches!(sig.ret, MirTy::Unit) {
                    None
                } else {
                    Some(self.fb.new_value(sig.ret.clone()))
                };
                self.fb.push_inst(Inst::Call {
                    dst,
                    callee: FuncRef::Local(id),
                    args: coerced.into_boxed_slice(),
                });
                return Ok((dst.unwrap_or_else(|| self.const_unit()), sig.ret));
            }
        }
        // Free function lookup first.
        if let Some(sig) = self.fn_sigs.get(&callee).cloned() {
            let id = *self.fn_ids.get(&callee).unwrap();
            let is_extern = matches!(
                self.funcs[id.0 as usize].kind,
                FunctionKind::Extern { .. }
            );
            let mut arg_vals = Vec::with_capacity(args.len());
            let mut fresh_obj_args: Vec<ValueId> = Vec::new();
            for (i, a) in args.iter().enumerate() {
                let arg_is_fresh = self.is_fresh_object_expr(a);
                // Fast path mirroring the @extern(C) struct-field
                // case (expr.rs StructLit handler): when an extern
                // fn declares a `fn(...)` parameter and the caller
                // passes a bare top-level fn name, lower it as a
                // raw FuncAddr instead of the default MakeClosure.
                // C callbacks dereference the slot as a function
                // pointer; a closure header would crash on entry.
                let want_func_ptr = is_extern
                    && i < sig.params.len()
                    && matches!(sig.params[i], MirTy::Fn(_));
                if want_func_ptr {
                    if let ExprKind::Var(name) = &a.kind {
                        if let Some(&fid) = self.fn_ids.get(name) {
                            let target = sig.params[i].clone();
                            let dst_v = self.fb.new_value(target.clone());
                            self.fb.push_inst(Inst::FuncAddr {
                                dst: dst_v,
                                func: fid,
                            });
                            arg_vals.push(dst_v);
                            continue;
                        }
                    }
                }
                let (v, vty) = self.lower_expr(a)?;
                let coerced = if i < sig.params.len() {
                    match sig.params.get(i) {
                        Some(t) if t != &vty => self.coerce(v, &vty, t, a.span)?,
                        _ => v,
                    }
                } else {
                    v
                };
                if arg_is_fresh && matches!(vty, MirTy::Object(_) | MirTy::Str) {
                    fresh_obj_args.push(coerced);
                }
                arg_vals.push(coerced);
            }
            let callee_ref = if is_extern {
                FuncRef::Local(id)
            } else {
                FuncRef::Local(id)
            };
            let dst = if matches!(sig.ret, MirTy::Unit) {
                None
            } else {
                Some(self.fb.new_value(sig.ret.clone()))
            };
            self.fb.push_inst(Inst::Call {
                dst,
                callee: callee_ref,
                args: arg_vals.into_boxed_slice(),
            });
            for fv in fresh_obj_args {
                self.fb.push_inst(Inst::Release { value: fv });
            }
            return Ok((dst.unwrap_or_else(|| self.const_unit()), sig.ret));
        }
        // Implicit `this.<callee>(args)` inside a method body.
        if let Some(cid) = self.this_class {
            let meta = self.class_meta.get(&cid).expect("class meta");
            if let Some(&mid) = meta.method_ids.get(&callee) {
                let sig = meta.method_sigs.get(&callee).cloned().unwrap();
                let (this_v, _) = self.lookup_var(Symbol::intern("this")).unwrap();
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
                return Ok((dst.unwrap_or_else(|| self.const_unit()), sig.ret));
            }
        }
        Err(LowerError::Other(format!(
            "call to undeclared function: {callee}"
        )))
    }
}
