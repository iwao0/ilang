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
        // `$ffi.cstrFromString(s)` — parser-synthesised by the @objc
        // desugar (see ilang-parser `extern_objc::build_cstr`). The
        // `$` prefix is unreachable from user code (lex rejects it),
        // so this branch only fires for compiler-generated calls.
        // The runtime helper is a pass-through that returns the ilang
        // string's inline NUL-terminated buffer as an i64 pointer.
        if callee.as_str() == "$ffi.cstrFromString" && args.len() == 1 {
            let (v, vty) = self.lower_expr(&args[0])?;
            let s_i64 = if matches!(vty, MirTy::I64) {
                v
            } else {
                self.coerce(v, &vty, &MirTy::I64, args[0].span)?
            };
            let dst = self.fb.new_value(MirTy::RawPtr {
                is_const: true,
                inner: Box::new(MirTy::CChar),
            });
            self.fb.push_inst(Inst::Call {
                dst: Some(dst),
                callee: FuncRef::Builtin(Symbol::intern("$ffi.cstrFromString")),
                args: Box::new([s_i64]),
            });
            return Ok((dst, MirTy::RawPtr {
                is_const: true,
                inner: Box::new(MirTy::CChar),
            }));
        }
        // Built-in pseudo-functions handled before generic resolution.
        if callee.as_str() == "typeof" && args.len() == 1 {
            let (v, _) = self.lower_expr(&args[0])?;
            let dst = self.fb.new_value(MirTy::TypeHandle);
            self.fb.push_inst(Inst::TypeOf { dst, value: v });
            return Ok((dst, MirTy::TypeHandle));
        }
        // `ffi.arrayFromCArray<T>(p: *const T, n: size_t)` —
        // declared as `@intrinsic("ffi.arrayFromCArray")` inside
        // `libs/std/ffi.il`. The loader-level rename rewrites the
        // bare `arrayFromCArray` callee using the importer's
        // qualified prefix — `use std.ffi { arrayFromCArray }`
        // produces `std.ffi.arrayFromCArray`; older `use ffi { … }`
        // paths produce `ffi.arrayFromCArray`. Accept any tail-match
        // on `ffi.arrayFromCArray` so the dotted variants both
        // route through here. The MIR-side special case stays
        // because we need to peek the actual `T` off the first
        // arg's MirTy (`*const T`) and synthesise the stride /
        // kind_tag arguments the runtime helper takes; full
        // monomorphisation isn't required since `T` is known at
        // the call site from the pointer's element type.
        //
        // Critical: without this dispatch the generic-resolution
        // path lowers the call against the 2-arg declared signature
        // — `$ffi.arrayFromCArray` is declared with 4 i64 params,
        // so the missing stride / kind_tag end up as garbage
        // register contents at runtime. A multi-GB memcpy from a
        // bogus stride is the immediate `STATUS_ACCESS_VIOLATION`
        // signature this guards against.
        let is_afca = matches!(
            callee.as_str(),
            "ffi.arrayFromCArray" | "std.ffi.arrayFromCArray"
        );
        if is_afca && args.len() == 2 {
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
                callee: FuncRef::Builtin(Symbol::intern("$ffi.arrayFromCArray")),
                args: Box::new([p_i64, n_i64, stride_v, kind_v]),
            });
            return Ok((dst, arr_ty));
        }
        // The raw-memory `readT` / `writeT` helpers and the
        // cstr / errnoCheck / bytesFromBuffer family used to live
        // here as compiler-magic bare-name dispatch. They're now
        // ordinary `@intrinsic` declarations in `libs/std/ffi.il`;
        // user code reaches them via `use std.ffi { readU64, ... }`
        // and the regular call-resolution path takes over from here.
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
                        callee: FuncRef::Builtin(Symbol::intern("$repl.loadSlot")),
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
                    let (coerced, _) = self.lower_arg_to(a, sig_params.get(i))?;
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
                // The slow-path closure→fn-ptr extract (below) is
                // only correct for genuine C ABI callees, which
                // dereference the argument as a C function pointer.
                // `@intrinsic` callees go through ilang-runtime
                // helpers that expect a closure box so they can
                // read captures + invoke the ilang trampoline; for
                // those, leave the closure box intact. c_symbol
                // convention: intrinsics begin with `$`, real C
                // symbols don't. The bare-fn-name fast path above
                // intentionally keeps firing regardless — passing
                // a top-level fn (no captures) by raw addr to an
                // intrinsic is the historical contract the
                // host-side helpers rely on (e.g.
                // `$test.applyI32Cb` transmutes the ptr to a raw
                // 3-arg `extern "C" fn`).
                let callee_is_c_abi = is_extern && {
                    let csym = self.funcs[id.0 as usize].c_symbol;
                    match csym {
                        Some(s) => !s.as_str().starts_with('$'),
                        None => true,
                    }
                };
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
                let (coerced, vty) = if i < sig.params.len() {
                    self.lower_arg_to(a, sig.params.get(i))?
                } else {
                    self.lower_expr(a)?
                };
                // Same fresh-transfer rule as `lower_new` /
                // `lower_object_method`: the caller's +1 on a fresh
                // heap arg needs releasing after the call. Heap
                // types that participate match the field-assign
                // retain set (Object / Fn / Array / Tuple / Map /
                // Optional / Str).
                let needs_post_release = matches!(
                    vty,
                    MirTy::Object(_)
                        | MirTy::Fn(_)
                        | MirTy::Array { .. }
                        | MirTy::Tuple(_)
                        | MirTy::Map { .. }
                        | MirTy::Optional(_)
                        | MirTy::Str
                );
                if arg_is_fresh && needs_post_release {
                    fresh_obj_args.push(coerced);
                }
                // Re-forwarding an ilang `fn(...)` value to an
                // `@extern(C)` callee that expects a C function
                // pointer: the value is a closure box pointer
                // `[fn_addr | rc | captures…]`, but C will treat
                // the supplied argument as the function pointer
                // itself and call it. Extract the fn_addr (offset
                // 0) so the C side receives the raw code address.
                // The bare-fn-name fast path above hands in a
                // FuncAddr already; this slow path covers locals
                // / captures / parameters whose MIR type is still
                // `MirTy::Fn`.
                if want_func_ptr && callee_is_c_abi && matches!(vty, MirTy::Fn(_)) {
                    let box_addr = self.fb.new_value(MirTy::I64);
                    self.fb.push_inst(Inst::Cast {
                        dst: box_addr,
                        kind: crate::inst::CastKind::PtrIntCast,
                        src: coerced,
                    });
                    let zero = self.const_int(MirTy::I64, 0);
                    let raw = self.fb.new_value(MirTy::I64);
                    self.fb.push_inst(Inst::Call {
                        dst: Some(raw),
                        callee: FuncRef::Builtin(Symbol::intern("$ffi.readU64")),
                        args: Box::new([box_addr, zero]),
                    });
                    arg_vals.push(raw);
                    continue;
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
                return Ok((dst.unwrap_or_else(|| self.const_unit()), sig.ret));
            }
        }
        Err(LowerError::Other(format!(
            "call to undeclared function: {callee}"
        )))
    }
}
