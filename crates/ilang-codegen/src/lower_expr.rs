//! Expression lowering — the big `match e.kind` plus its handful of
//! companion helpers (`lower_array_literal`, `build_array`,
//! `lower_console_log`, `call_method`).

use cranelift::prelude::*;
use cranelift_codegen::ir::types::{F32, F64, I16, I32, I64, I8};
use cranelift_module::Module;
use ilang_ast::{Expr, ExprKind, Symbol};

use crate::arc::{
    emit_bind_retain, emit_release_heap, emit_release_object, emit_release_string,
    emit_retain_heap, emit_retain_object, is_aliased_heap_source,
};
use crate::env::{class_ids_from, enum_ids_from, LowerCtx};
use crate::error::CodegenError;
use crate::lower_ctrl::{lower_for_in, lower_if, lower_loop, lower_while};
use crate::lower_op::{coerce, lower_binary, lower_logical, lower_unary};
use crate::lower_stmt::lower_block_value;
use crate::runtime::{
    ARRAY_DATA_OFFSET, ARRAY_LEN_OFFSET, MAP_KEY_KIND_BOOL, MAP_KEY_KIND_INT,
    MAP_KEY_KIND_STR, MAP_KEY_KIND_UINT,
};
use crate::ty::{
    intern_array_kind, intern_map_kind, intern_optional_inner, ArrayKind,
    EnumVariantLayout, JitTy, MapKind, TV, ENUM_PAYLOAD_OFFSET, ENUM_TAG_OFFSET,
};

pub(crate) fn lower_expr(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    e: &Expr,
) -> Result<Option<TV>, CodegenError> {
    match &e.kind {
        ExprKind::StructLit { .. } => unreachable!(
            "ExprKind::StructLit is desugared by the parser's normalize pass"
        ),
        ExprKind::Int(n) => Ok(Some((b.ins().iconst(I64, *n), JitTy::I64))),
        ExprKind::Float(f) => Ok(Some((b.ins().f64const(*f), JitTy::F64))),
        ExprKind::Bool(v) => Ok(Some((b.ins().iconst(I8, if *v { 1 } else { 0 }), JitTy::Bool))),
        ExprKind::Str(s) => {
            let ptr = lc.intern_string(s);
            Ok(Some((b.ins().iconst(I64, ptr), JitTy::Str)))
        }
        ExprKind::This => match lc.this {
            Some((var, class_id)) => Ok(Some((b.use_var(var), JitTy::Object(class_id)))),
            None => Err(CodegenError::Unsupported {
                what: "`this` outside a method body".into(),
                span: e.span,
            }),
        },
        ExprKind::Var(name) => {
            // Closure capture: while lowering a wrapper body, a Var
            // for a captured name resolves to a load from the env
            // pointer at the recorded offset.
            if let Some(env) = lc.closure_capture_env.as_ref() {
                if let Some(entry) = env.captures.iter().find(|(n, _, _)| n == name)
                {
                    let offset = entry.1;
                    let jty = entry.2;
                    let env_ptr = b.use_var(env.env_var);
                    let raw = b.ins().load(
                        I64,
                        MemFlags::trusted(),
                        env_ptr,
                        offset as i32,
                    );
                    let v = match jty {
                        JitTy::I64 | JitTy::U64 => raw,
                        JitTy::F64 => b.ins().bitcast(F64, MemFlags::new(), raw),
                        JitTy::Bool => b.ins().ireduce(I8, raw),
                        // Heap pointer captures: load i64 ptr; the
                        // type itself just labels it for downstream
                        // ARC handling.
                        t if t.is_heap() => raw,
                        _ => unreachable!(
                            "unhandled closure capture type {jty:?}"
                        ),
                    };
                    return Ok(Some((v, jty)));
                }
            }
            if let Some(&(var, vt)) = lc.env.bindings.get(name) {
                return Ok(Some((b.use_var(var), vt)));
            }
            // `@extern static`: load the value through the resolved
            // C address. The width follows the declared type.
            if let Some(&addr) = lc.extern_static_addrs.get(name) {
                let ast_ty = lc
                    .extern_static_types
                    .get(name)
                    .expect("static type recorded alongside addr");
                let jty = jit_ty_from_primitive(ast_ty);
                let cl_ty = jty.cl().expect("primitive static");
                let addr_v = b.ins().iconst(I64, addr);
                let v = b.ins().load(cl_ty, MemFlags::trusted(), addr_v, 0);
                return Ok(Some((v, jty)));
            }
            // Unit-typed bindings (`let x = loop {...}`) carry no
            // Cranelift value — return Ok(None), letting void-tolerant
            // contexts (statement positions, further `let` RHS) handle
            // it the same way they handle bare `loop {}`.
            if lc.env.unit_bindings.contains(name) {
                return Ok(None);
            }
            // Implicit-`this` field access inside a method body.
            if let Some((this_var, class_id)) = lc.this {
                let layout = &lc.class_layouts[class_id as usize];
                if let Some(&(offset, fty)) = layout.fields.get(name) {
                    let this = b.use_var(this_var);
                    let v = b.ins().load(
                        fty.cl().expect("non-unit field"),
                        MemFlags::trusted(),
                        this,
                        offset as i32,
                    );
                    return Ok(Some((v, fty)));
                }
            }
            // First-class function reference to a top-level fn:
            // construct a no-capture closure whose wrapper is a
            // trampoline that ignores env_ptr and forwards to the
            // real fn. The trampoline is generated lazily and
            // cached per fn name (see `ensure_trampoline`).
            if let Some(entry) = lc.funcs.get(name).cloned() {
                let (id, params, ret) = entry;
                let trampoline_id = ensure_trampoline(b, lc, name.as_str(), id, &params, ret)?;
                // Allocate a 0-capture closure pointing at the trampoline.
                // No captures → no drop fn needed (drop_fn_ptr=0).
                let alloc_ref = lc
                    .module
                    .declare_func_in_func(lc.alloc_closure_id, b.func);
                let zero = b.ins().iconst(I64, 0);
                let call = b.ins().call(alloc_ref, &[zero, zero]);
                let closure_ptr = b.inst_results(call)[0];
                let func_ref = lc.module.declare_func_in_func(trampoline_id, b.func);
                let fn_addr = b.ins().func_addr(I64, func_ref);
                b.ins().store(MemFlags::trusted(), fn_addr, closure_ptr, 0);
                let sig_id = crate::ty::intern_fn_sig(
                    lc.fn_signatures,
                    crate::ty::FnSignature { params, ret },
                );
                return Ok(Some((closure_ptr, JitTy::Fn(sig_id))));
            }
            Err(CodegenError::Unsupported {
                what: format!("unknown variable {name:?}"),
                span: e.span,
            })
        }
        ExprKind::FnExpr { .. } => {
            // Hoisting pass should have replaced this with Closure.
            Err(CodegenError::Unsupported {
                what: "anonymous function reached lowering — hoist pass failed".into(),
                span: e.span,
            })
        }
        ExprKind::Closure { fn_name, captures } => {
            lower_closure_construct(b, lc, fn_name.as_str(), captures, e.span)
        }
        ExprKind::MapLit(entries) => lower_map_lit(b, lc, entries, e.span),
        ExprKind::TypeTest { expr, ty } => {
            Ok(Some(lower_type_test_or_downcast(b, lc, expr, ty, false, e.span)?))
        }
        ExprKind::TypeDowncast { expr, ty } => {
            Ok(Some(lower_type_test_or_downcast(b, lc, expr, ty, true, e.span)?))
        }
        ExprKind::Cast { expr, ty } => {
            let inner = lower_expr(b, lc, expr)?.ok_or_else(|| CodegenError::Unsupported {
                what: "cast on unit".into(),
                span: e.span,
            })?;
            let target = JitTy::from_ast(
                ty,
                e.span,
                &class_ids_from(lc),
                &enum_ids_from(lc),
                lc.enum_layouts,
                lc.array_kinds,
                lc.optional_inners,
                lc.fn_signatures,
                lc.map_kinds,
                lc.tuple_kinds,
            )?;
            // FFI cast: `i64 ↔ opaque-extern class (no deinit)`.
            // Both representations are a raw i64 pointer at runtime
            // — no bit-level conversion required, just retag the
            // JIT type so subsequent uses pick up the right marshalling.
            let opaque_no_deinit = |t: JitTy| match t {
                JitTy::Object(class_id) => {
                    lc.class_layouts[class_id as usize].extern_lib.is_some()
                        && !lc.class_methods[class_id as usize].contains_key(&"deinit".into())
                }
                _ => false,
            };
            if (inner.1 == JitTy::I64 && opaque_no_deinit(target))
                || (opaque_no_deinit(inner.1) && target == JitTy::I64)
            {
                return Ok(Some((inner.0, target)));
            }
            let v = coerce(b, inner, target, e.span)?;
            Ok(Some((v, target)))
        }
        ExprKind::Unary { op, expr } => lower_unary(b, lc, *op, expr, e.span),
        ExprKind::Binary { op, lhs, rhs } => lower_binary(b, lc, *op, lhs, rhs),
        ExprKind::Logical { op, lhs, rhs } => Ok(Some((
            lower_logical(b, lc, *op, lhs, rhs)?,
            JitTy::Bool,
        ))),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => lower_if(b, lc, cond, then_branch, else_branch.as_deref()),
        ExprKind::Block(block) => lower_block_value(b, lc, block),
        ExprKind::While { cond, body } => {
            lower_while(b, lc, cond, body)?;
            Ok(None)
        }
        ExprKind::Loop { body } => Ok(lower_loop(b, lc, body, e.span)?),
        ExprKind::ForIn { var, iter, body } => {
            lower_for_in(b, lc, var.as_str(), iter, body)?;
            Ok(None)
        }
        ExprKind::Range { .. } => Err(CodegenError::Unsupported {
            what: "range expression `a..b` is only valid as a `for-in` iterator".into(),
            span: e.span,
        }),
        ExprKind::Break(value) => {
            // Snapshot the innermost loop frame's after-block + slot
            // before lowering `value`, so the lowering of `value` can
            // see the right `lc.loops` state if it does anything weird.
            let frame = lc
                .loops
                .last()
                .ok_or_else(|| CodegenError::Unsupported {
                    what: "break outside loop".into(),
                    span: e.span,
                })?
                .clone();
            let target = frame.1;
            if let Some(v_expr) = value {
                let (v, _vt) = lower_expr(b, lc, v_expr)?.ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: "break value is unit".into(),
                        span: v_expr.span,
                    }
                })?;
                if let Some((slot, _jty)) = frame.2 {
                    b.def_var(slot, v);
                }
            }
            b.ins().jump(target, &[]);
            let dead = b.create_block();
            b.switch_to_block(dead);
            b.seal_block(dead);
            Ok(None)
        }
        ExprKind::SuperCall { method, args } => {
            // Direct (non-virtual) call to the parent class's
            // specific method. The lexical class is recorded in
            // `lc.current_class` while lowering a method body.
            let cur = lc.current_class.clone().ok_or_else(|| CodegenError::Unsupported {
                what: "`super` outside a class method body".into(),
                span: e.span,
            })?;
            let parent_name = lc
                .class_parents
                .get(&Symbol::intern(&cur))
                .cloned()
                .ok_or_else(|| CodegenError::Unsupported {
                    what: format!("class {cur:?} has no parent for `super`"),
                    span: e.span,
                })?;
            let parent_id = *crate::env::class_ids_from(lc)
                .get(&parent_name)
                .ok_or_else(|| CodegenError::Unsupported {
                    what: format!("unknown parent class {parent_name:?}"),
                    span: e.span,
                })?;
            let lookup: Symbol = method.unwrap_or_else(|| "init".into());
            let info = lc.class_methods[parent_id as usize]
                .get(&lookup)
                .cloned()
                .ok_or_else(|| CodegenError::Unsupported {
                    what: format!(
                        "super.{lookup}: parent class {parent_name:?} has no method"
                    ),
                    span: e.span,
                })?;
            // Receiver = `this` (must exist; the type checker
            // already enforced super only inside a class method).
            let this_v = match lc.this {
                Some((var, _)) => b.use_var(var),
                None => {
                    return Err(CodegenError::Unsupported {
                        what: "`super` requires a `this` receiver".into(),
                        span: e.span,
                    });
                }
            };
            emit_retain_object(b, lc, this_v);
            let mut arg_vals = Vec::with_capacity(args.len() + 1);
            arg_vals.push(this_v);
            for (i, a) in args.iter().enumerate() {
                let (av, at) = lower_expr(b, lc, a)?.ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: "argument is unit".into(),
                        span: a.span,
                    }
                })?;
                let coerced = coerce(b, (av, at), info.params[i], a.span)?;
                emit_bind_retain(b, lc, &a.kind, at, info.params[i], coerced);
                arg_vals.push(coerced);
            }
            let func_ref = lc.module.declare_func_in_func(info.id, b.func);
            let call = b.ins().call(func_ref, &arg_vals);
            if matches!(info.ret, JitTy::Unit) {
                Ok(None)
            } else {
                Ok(Some((b.inst_results(call)[0], info.ret)))
            }
        }
        ExprKind::Continue => {
            let target = lc.loops.last().ok_or_else(|| CodegenError::Unsupported {
                what: "continue outside loop".into(),
                span: e.span,
            })?.0;
            b.ins().jump(target, &[]);
            let dead = b.create_block();
            b.switch_to_block(dead);
            b.seal_block(dead);
            Ok(None)
        }
        ExprKind::Return(value) => lower_return(b, lc, value.as_deref(), e.span),
        ExprKind::EnumCtor {
            enum_name,
            variant,
            args,
        } => lower_enum_ctor(b, lc, enum_name.as_str(), variant.as_str(), args, e.span),
        ExprKind::Match { scrutinee, arms } => lower_match(b, lc, scrutinee, arms, e.span),
        ExprKind::Assign { target, value } => {
            // Ordinary local first; then implicit-`this` field write.
            if let Some(&(var, var_ty)) = lc.env.bindings.get(target) {
                let is_heap =
                    var_ty.is_heap();
                // Capture the old value before def_var, so we can drop
                // the previous reference once the new one is in place.
                let old_val = if is_heap { Some(b.use_var(var)) } else { None };
                let (val, vt) = lower_expr(b, lc, value)?.ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: "assigning unit".into(),
                        span: e.span,
                    }
                })?;
                let coerced = coerce(b, (val, vt), var_ty, e.span)?;
                emit_bind_retain(b, lc, &value.kind, vt, var_ty, coerced);
                b.def_var(var, coerced);
                if let Some(old) = old_val {
                    emit_release_heap(b, lc, old, var_ty);
                }
                return Ok(None);
            }
            // `@extern static` write: store the (coerced) value at
            // the resolved address.
            if let Some(&addr) = lc.extern_static_addrs.get(target) {
                let ast_ty = lc
                    .extern_static_types
                    .get(target)
                    .expect("static type recorded alongside addr")
                    .clone();
                let jty = jit_ty_from_primitive(&ast_ty);
                let (val, vt) = lower_expr(b, lc, value)?.ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: "assigning unit".into(),
                        span: e.span,
                    }
                })?;
                let coerced = coerce(b, (val, vt), jty, e.span)?;
                let addr_v = b.ins().iconst(I64, addr);
                b.ins().store(MemFlags::trusted(), coerced, addr_v, 0);
                return Ok(None);
            }
            if let Some((this_var, class_id)) = lc.this {
                let layout = &lc.class_layouts[class_id as usize];
                if let Some(&(offset, fty)) = layout.fields.get(target) {
                    let is_heap =
                        fty.is_heap();
                    let this = b.use_var(this_var);
                    let old_val = if is_heap {
                        Some(b.ins().load(
                            fty.cl().expect("non-unit field"),
                            MemFlags::trusted(),
                            this,
                            offset as i32,
                        ))
                    } else {
                        None
                    };
                    let (val, vt) = lower_expr(b, lc, value)?.ok_or_else(|| {
                        CodegenError::Unsupported {
                            what: "assigning unit".into(),
                            span: e.span,
                        }
                    })?;
                    let coerced = coerce(b, (val, vt), fty, e.span)?;
                    emit_bind_retain(b, lc, &value.kind, vt, fty, coerced);
                    b.ins()
                        .store(MemFlags::trusted(), coerced, this, offset as i32);
                    if let Some(old) = old_val {
                        emit_release_heap(b, lc, old, fty);
                    }
                    return Ok(None);
                }
            }
            Err(CodegenError::Unsupported {
                what: format!("unknown variable {target:?}"),
                span: e.span,
            })
        }
        ExprKind::AssignField { obj, field, value } => {
            // Static field write: `ClassName.field = v`.
            if let ExprKind::Var(rname) = &obj.kind {
                if !lc.env.bindings.contains_key(rname) {
                    let key = (rname.clone(), field.clone());
                    if let Some(&slot) = lc.static_field_slots.get(&key) {
                        let ty = lc.static_field_types.get(&key).cloned().unwrap();
                        let (val, vt) = lower_expr(b, lc, value)?.ok_or_else(|| {
                            CodegenError::Unsupported {
                                what: "static field value is unit".into(),
                                span: value.span,
                            }
                        })?;
                        let target_jty = match &ty {
                            ilang_ast::Type::I8 => JitTy::I8,
                            ilang_ast::Type::I16 => JitTy::I16,
                            ilang_ast::Type::I32 => JitTy::I32,
                            ilang_ast::Type::I64 => JitTy::I64,
                            ilang_ast::Type::U8 => JitTy::U8,
                            ilang_ast::Type::U16 => JitTy::U16,
                            ilang_ast::Type::U32 => JitTy::U32,
                            ilang_ast::Type::U64 => JitTy::U64,
                            ilang_ast::Type::F32 => JitTy::F32,
                            ilang_ast::Type::F64 => JitTy::F64,
                            ilang_ast::Type::Bool => JitTy::Bool,
                            ilang_ast::Type::Array { elem, fixed: None } => {
                                let elem_jty = jit_ty_from_primitive(elem);
                                let array_id = intern_array_kind(
                                    lc.array_kinds,
                                    ArrayKind { elem: elem_jty, fixed: None },
                                );
                                JitTy::Array(array_id)
                            }
                            _ => unreachable!("checker rejects other types"),
                        };
                        let coerced = coerce(b, (val, vt), target_jty, value.span)?;
                        let addr = lc.static_field_base_addr + (slot as i64) * 8;
                        let addr_v = b.ins().iconst(I64, addr);
                        // Heap-typed static field assignment: retain
                        // the new array (if it came from an aliased
                        // source — fresh allocations already start at
                        // rc=1) and release the old before swapping
                        // pointers, mirroring the instance-field write.
                        if matches!(target_jty, JitTy::Array(_)) {
                            if crate::arc::is_aliased_heap_source(&value.kind) {
                                crate::arc::emit_retain_heap(b, lc, coerced, target_jty);
                            }
                            let old = b.ins().load(I64, MemFlags::trusted(), addr_v, 0);
                            crate::arc::emit_release_heap(b, lc, old, target_jty);
                            b.ins().store(MemFlags::trusted(), coerced, addr_v, 0);
                            return Ok(None);
                        }
                        let bits = match target_jty {
                            JitTy::I64 | JitTy::U64 => coerced,
                            JitTy::I8 | JitTy::U8 | JitTy::Bool => b.ins().uextend(I64, coerced),
                            JitTy::I16 | JitTy::U16 => b.ins().uextend(I64, coerced),
                            JitTy::I32 | JitTy::U32 => b.ins().uextend(I64, coerced),
                            JitTy::F32 => {
                                let i = b.ins().bitcast(I32, MemFlags::new(), coerced);
                                b.ins().uextend(I64, i)
                            }
                            JitTy::F64 => b.ins().bitcast(I64, MemFlags::new(), coerced),
                            _ => unreachable!(),
                        };
                        b.ins().store(MemFlags::trusted(), bits, addr_v, 0);
                        return Ok(None);
                    }
                }
            }
            let (obj_v, obj_t) = lower_expr(b, lc, obj)?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "field assignment receiver is unit".into(),
                    span: obj.span,
                }
            })?;
            let class_id = match obj_t {
                JitTy::Object(id) => id,
                _ => {
                    return Err(CodegenError::Unsupported {
                        what: "field assignment on non-object".into(),
                        span: obj.span,
                    });
                }
            };
            // Property setter dispatch — symmetric with the getter
            // path in lower Field. Coerces the rhs to the setter's
            // param type, retains as if passing into a normal method.
            let prop_key = format!("__prop_set_{field}");
            if let Some(info) =
                lc.class_methods[class_id as usize].get(&Symbol::intern(&prop_key)).cloned()
            {
                emit_retain_object(b, lc, obj_v);
                let (val, vt) = lower_expr(b, lc, value)?.ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: "field value is unit".into(),
                        span: e.span,
                    }
                })?;
                let coerced = coerce(b, (val, vt), info.params[0], e.span)?;
                emit_bind_retain(b, lc, &value.kind, vt, info.params[0], coerced);
                let func_ref = lc.module.declare_func_in_func(info.id, b.func);
                b.ins().call(func_ref, &[obj_v, coerced]);
                return Ok(None);
            }
            let layout = &lc.class_layouts[class_id as usize];
            let (offset, fty) = *layout.fields.get(field).ok_or_else(|| {
                CodegenError::Unsupported {
                    what: format!("unknown field {field:?}"),
                    span: e.span,
                }
            })?;
            // Embedded `@extern(C) struct` field: writing a struct value
            // means COPYING the inner's bytes into the embedded
            // slot, not storing the pointer (which is what a heap
            // class field would do).
            let is_embedded_struct = if let JitTy::Object(inner_id) = fty {
                layout.is_repr_c
                    && lc.class_layouts[inner_id as usize].is_repr_c
            } else {
                false
            };
            // Bitfield write: read-modify-write on the storage unit.
            //   old = load unit
            //   cleared = old & ~(mask << bit_offset)
            //   newbits = (value & mask) << bit_offset
            //   store(cleared | newbits)
            // Higher bits of `value` are silently truncated to the
            // declared width — matches C's bitfield assignment.
            if let Some(bf) = layout.bitfields.get(field).copied() {
                let unit_ty = fty.cl().expect("non-unit bitfield underlying");
                let (val, vt) = lower_expr(b, lc, value)?.ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: "bitfield value is unit".into(),
                        span: e.span,
                    }
                })?;
                let coerced = coerce(b, (val, vt), fty, e.span)?;
                let width_mask: i64 =
                    if bf.width >= 64 { -1 } else { (1i64 << bf.width) - 1 };
                let shifted_mask: i64 = if bf.width >= 64 {
                    -1
                } else {
                    width_mask << bf.bit_offset
                };
                let old = b.ins().load(unit_ty, MemFlags::trusted(), obj_v, offset as i32);
                let cleared = b.ins().band_imm(old, !shifted_mask);
                let val_masked = b.ins().band_imm(coerced, width_mask);
                let val_shifted = if bf.bit_offset == 0 {
                    val_masked
                } else {
                    b.ins().ishl_imm(val_masked, bf.bit_offset as i64)
                };
                let merged = b.ins().bor(cleared, val_shifted);
                b.ins().store(MemFlags::trusted(), merged, obj_v, offset as i32);
                return Ok(None);
            }
            if is_embedded_struct {
                let inner_id = match fty {
                    JitTy::Object(id) => id,
                    _ => unreachable!(),
                };
                let copy_size =
                    lc.class_layouts[inner_id as usize].size as usize;
                let (val, _vt) = lower_expr(b, lc, value)?.ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: "field value is unit".into(),
                        span: e.span,
                    }
                })?;
                // Bytewise copy: 8-byte chunks first, then taper
                // down to 4 / 2 / 1 for the trailing remainder.
                // Source and dest are both struct user-pointers
                // aligned to at least the struct's max alignment,
                // so MemFlags::trusted is fine.
                let mut copied = 0usize;
                let dst_base = if offset == 0 {
                    obj_v
                } else {
                    b.ins().iadd_imm(obj_v, offset as i64)
                };
                let chunks: [(usize, cranelift::prelude::Type); 4] =
                    [(8, I64), (4, I32), (2, I16), (1, I8)];
                for &(width, cl_ty) in &chunks {
                    while copy_size - copied >= width {
                        let off = copied as i32;
                        let v = b.ins().load(cl_ty, MemFlags::trusted(), val, off);
                        b.ins().store(MemFlags::trusted(), v, dst_base, off);
                        copied += width;
                    }
                }
                return Ok(None);
            }
            let is_heap = fty.is_heap();
            // Read the old field value first so we can release it after
            // the new one is in place.
            let old_val = if is_heap {
                Some(b.ins().load(
                    fty.cl().expect("non-unit field"),
                    MemFlags::trusted(),
                    obj_v,
                    offset as i32,
                ))
            } else {
                None
            };
            let (val, vt) = lower_expr(b, lc, value)?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "field value is unit".into(),
                    span: e.span,
                }
            })?;
            let coerced = coerce(b, (val, vt), fty, e.span)?;
            emit_bind_retain(b, lc, &value.kind, vt, fty, coerced);
            b.ins()
                .store(MemFlags::trusted(), coerced, obj_v, offset as i32);
            if let Some(old) = old_val {
                emit_release_heap(b, lc, old, fty);
            }
            Ok(None)
        }
        ExprKind::Field { obj, name } => {
            // Static field read: `ClassName.field`. Loads the slot
            // from the JIT compiler's static-field storage and bit-
            // reinterprets to the declared type.
            if let ExprKind::Var(rname) = &obj.kind {
                if !lc.env.bindings.contains_key(rname) {
                    let key = (rname.clone(), name.clone());
                    if let Some(&slot) = lc.static_field_slots.get(&key) {
                        let ty = lc.static_field_types.get(&key).cloned().unwrap();
                        let addr = lc.static_field_base_addr + (slot as i64) * 8;
                        let addr_v = b.ins().iconst(I64, addr);
                        let raw = b.ins().load(I64, MemFlags::trusted(), addr_v, 0);
                        let (v, jty) = match ty {
                            ilang_ast::Type::I8 => (b.ins().ireduce(I8, raw), JitTy::I8),
                            ilang_ast::Type::I16 => (b.ins().ireduce(I16, raw), JitTy::I16),
                            ilang_ast::Type::I32 => (b.ins().ireduce(I32, raw), JitTy::I32),
                            ilang_ast::Type::I64 => (raw, JitTy::I64),
                            ilang_ast::Type::U8 => (b.ins().ireduce(I8, raw), JitTy::U8),
                            ilang_ast::Type::U16 => (b.ins().ireduce(I16, raw), JitTy::U16),
                            ilang_ast::Type::U32 => (b.ins().ireduce(I32, raw), JitTy::U32),
                            ilang_ast::Type::U64 => (raw, JitTy::U64),
                            ilang_ast::Type::F32 => {
                                let lo = b.ins().ireduce(I32, raw);
                                (b.ins().bitcast(F32, MemFlags::new(), lo), JitTy::F32)
                            }
                            ilang_ast::Type::F64 => {
                                (b.ins().bitcast(F64, MemFlags::new(), raw), JitTy::F64)
                            }
                            ilang_ast::Type::Bool => {
                                (b.ins().ireduce(I8, raw), JitTy::Bool)
                            }
                            ilang_ast::Type::Array { elem, fixed: None } => {
                                let elem_jty = jit_ty_from_primitive(&elem);
                                let array_id = intern_array_kind(
                                    lc.array_kinds,
                                    ArrayKind { elem: elem_jty, fixed: None },
                                );
                                (raw, JitTy::Array(array_id))
                            }
                            _ => unreachable!("checker rejects other types"),
                        };
                        return Ok(Some((v, jty)));
                    }
                }
            }
            let (obj_v, obj_t) = lower_expr(b, lc, obj)?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "field receiver is unit".into(),
                    span: obj.span,
                }
            })?;
            // Built-in `array.length` reads the len slot of the header.
            if matches!(obj_t, JitTy::Array(_)) && name == "length" {
                let len = b.ins().load(
                    I64,
                    MemFlags::trusted(),
                    obj_v,
                    ARRAY_LEN_OFFSET,
                );
                return Ok(Some((len, JitTy::I64)));
            }
            // Built-in `string.length` (Unicode code-point count).
            if matches!(obj_t, JitTy::Str) && name == "length" {
                let release = !is_aliased_heap_source(&obj.kind);
                let r = lc.module.declare_func_in_func(lc.strfns.length, b.func);
                let call = b.ins().call(r, &[obj_v]);
                let n = b.inst_results(call)[0];
                if release {
                    emit_release_string(b, lc, obj_v);
                }
                return Ok(Some((n, JitTy::I64)));
            }
            // Built-in `Type.name` / `Type.kind` / `Type.parent` (RTTI).
            if obj_t == JitTy::TypeRef {
                if name == "name" {
                    let s = b.ins().load(
                        I64,
                        MemFlags::trusted(),
                        obj_v,
                        crate::runtime::TYPE_META_NAME_OFFSET,
                    );
                    // The metadata's name string is allocated with a
                    // saturated rc, so retain/release stay no-ops —
                    // but emit retain for symmetry with other paths
                    // that produce owned strings.
                    let r = lc.module.declare_func_in_func(lc.strfns.retain, b.func);
                    b.ins().call(r, &[s]);
                    return Ok(Some((s, JitTy::Str)));
                }
                if name == "kind" {
                    let k = b.ins().load(
                        I32,
                        MemFlags::trusted(),
                        obj_v,
                        crate::runtime::TYPE_META_KIND_OFFSET,
                    );
                    return Ok(Some((k, JitTy::Enum(lc.typekind_enum_id))));
                }
                if name == "parent" {
                    // Parent slot is a nullable `TypeMeta*` (0 = none).
                    // `Optional<TypeRef>` is just the same nullable
                    // pointer in JIT (TypeRef is_heap), so we can
                    // return the loaded value directly.
                    let p = b.ins().load(
                        I64,
                        MemFlags::trusted(),
                        obj_v,
                        crate::runtime::TYPE_META_PARENT_OFFSET,
                    );
                    let opt_id = crate::ty::intern_optional_inner(
                        lc.optional_inners,
                        JitTy::TypeRef,
                    );
                    return Ok(Some((p, JitTy::Optional(opt_id))));
                }
                if name == "fields" || name == "methods" {
                    // Both slots hold a saturated-rc `string[]`
                    // (ArrayHeader). Loading and returning works just
                    // like a regular `string[]` field — retain is
                    // a no-op for saturated arrays.
                    let off = if name == "fields" {
                        crate::runtime::TYPE_META_FIELDS_OFFSET
                    } else {
                        crate::runtime::TYPE_META_METHODS_OFFSET
                    };
                    let arr = b.ins().load(I64, MemFlags::trusted(), obj_v, off);
                    let array_id = crate::ty::intern_array_kind(
                        lc.array_kinds,
                        crate::ty::ArrayKind { elem: JitTy::Str, fixed: None },
                    );
                    return Ok(Some((arr, JitTy::Array(array_id))));
                }
                if name == "typeArgs" {
                    let arr = b.ins().load(
                        I64,
                        MemFlags::trusted(),
                        obj_v,
                        crate::runtime::TYPE_META_TYPE_ARGS_OFFSET,
                    );
                    let array_id = crate::ty::intern_array_kind(
                        lc.array_kinds,
                        crate::ty::ArrayKind { elem: JitTy::TypeRef, fixed: None },
                    );
                    return Ok(Some((arr, JitTy::Array(array_id))));
                }
            }
            // Built-in Optional properties: `isSome` / `isNone`.
            // Optional values are nullable pointers (i64); compare to 0.
            if let JitTy::Optional(_) = obj_t {
                if name == "isSome" || name == "isNone" {
                    let zero = b.ins().iconst(I64, 0);
                    let cc = if name == "isSome" {
                        IntCC::NotEqual
                    } else {
                        IntCC::Equal
                    };
                    let v = b.ins().icmp(cc, obj_v, zero);
                    return Ok(Some((v, JitTy::Bool)));
                }
            }
            // Built-in Result properties: `isOk` / `isErr`. Read the
            // i32 tag at offset 0 and compare to the variant's tag.
            if let JitTy::EnumHeap(eid) = obj_t {
                if name == "isOk" || name == "isErr" {
                    let layout = &lc.enum_layouts[eid as usize];
                    let lname = layout.name.as_str();
                    if lname == "Result" || lname.starts_with("Result<") {
                        let target_variant = if name == "isOk" { "ok" } else { "err" };
                        let target_sym = Symbol::intern(target_variant);
                        let tag = layout
                            .variants
                            .iter()
                            .zip(layout.tags.iter())
                            .find_map(|(v, t)| (*v == target_sym).then_some(*t))
                            .expect("Result variant tag");
                        let tag_v = b.ins().load(I32, MemFlags::trusted(), obj_v, ENUM_TAG_OFFSET);
                        let want = b.ins().iconst(I32, tag);
                        let v = b.ins().icmp(IntCC::Equal, tag_v, want);
                        if !is_aliased_heap_source(&obj.kind) {
                            emit_release_heap(b, lc, obj_v, obj_t);
                        }
                        return Ok(Some((v, JitTy::Bool)));
                    }
                }
            }
            let class_id = match obj_t {
                JitTy::Object(id) => id,
                _ => {
                    return Err(CodegenError::Unsupported {
                        what: "field access on non-object".into(),
                        span: obj.span,
                    });
                }
            };
            // Property getter dispatch: declare_methods registered any
            // accessor as `__prop_get_<name>`. Falls through to direct
            // field load when the class has no such property.
            let prop_key = format!("__prop_get_{name}");
            if let Some(info) =
                lc.class_methods[class_id as usize].get(&Symbol::intern(&prop_key)).cloned()
            {
                emit_retain_object(b, lc, obj_v);
                let func_ref = lc.module.declare_func_in_func(info.id, b.func);
                let call = b.ins().call(func_ref, &[obj_v]);
                if matches!(info.ret, JitTy::Unit) {
                    return Ok(None);
                }
                return Ok(Some((b.inst_results(call)[0], info.ret)));
            }
            let layout = &lc.class_layouts[class_id as usize];
            let (offset, fty) = *layout.fields.get(name).ok_or_else(|| {
                CodegenError::Unsupported {
                    what: format!("unknown field {name:?}"),
                    span: e.span,
                }
            })?;
            // Nested embedded `@extern(C) struct` field: the inner struct's
            // bytes live inline in the outer's allocation. Return a
            // pointer into the embedded slot (no load) so chain
            // access `outer.inner.x` reads/writes the right slot.
            // No retain — the pointer borrows from `obj_v`.
            if let (true, JitTy::Object(inner_id)) =
                (layout.is_repr_c, fty)
            {
                if lc.class_layouts[inner_id as usize].is_repr_c {
                    let v = if offset == 0 {
                        obj_v
                    } else {
                        b.ins().iadd_imm(obj_v, offset as i64)
                    };
                    return Ok(Some((v, fty)));
                }
            }
            // Embedded fixed-length numeric array — same idea: the
            // bytes live inline, so the value carries the base
            // address of the array slot. Index lowering recognises
            // `JitTy::EmbeddedArray` and computes per-element
            // offsets directly (no heap header indirection).
            if matches!(fty, JitTy::EmbeddedArray(_) | JitTy::FlexArray(_)) {
                let v = if offset == 0 {
                    obj_v
                } else {
                    b.ins().iadd_imm(obj_v, offset as i64)
                };
                return Ok(Some((v, fty)));
            }
            // Bitfield read: load the storage unit, shift down to the
            // field's bit position, mask to its width. The underlying
            // type is unsigned, so a logical shift / mask gives the
            // correctly zero-extended value. (Signed bitfields would
            // need an arithmetic shift to sign-extend; rejected at
            // type-check.)
            if let Some(bf) = layout.bitfields.get(name).copied() {
                let unit_ty = fty.cl().expect("non-unit bitfield underlying");
                let raw = b.ins().load(unit_ty, MemFlags::trusted(), obj_v, offset as i32);
                let shifted = if bf.bit_offset == 0 {
                    raw
                } else {
                    b.ins().ushr_imm(raw, bf.bit_offset as i64)
                };
                let mask: i64 = if bf.width >= 64 { -1 } else { (1i64 << bf.width) - 1 };
                let masked = b.ins().band_imm(shifted, mask);
                return Ok(Some((masked, fty)));
            }
            let v = b.ins().load(
                fty.cl().expect("non-unit field"),
                MemFlags::trusted(),
                obj_v,
                offset as i32,
            );
            Ok(Some((v, fty)))
        }
        ExprKind::MethodCall { obj, method, args } => {
            // Intercept the built-in `console.log(...)`. The receiver
            // expression is `console`, which has type Object("Console") at
            // the type-checker level but no class layout in the JIT — we
            // never need its value.
            if let ExprKind::Var(name) = &obj.kind {
                if name == "console" && method == "log" {
                    return lower_console_log(b, lc, args).map(|_| None);
                }
                // Static method dispatch: `ClassName.method(args)` is
                // registered in `lc.funcs` under the qualified name
                // `ClassName.method` (no receiver). Variables shadow
                // class names — the env lookup wins if both exist.
                if !lc.env.bindings.contains_key(name) {
                    let qualified = format!("{name}.{method}");
                    if let Some(entry) = lc.funcs.get(&Symbol::intern(&qualified)).cloned() {
                        let (id, param_tys, ret_ty) = entry;
                        let mut arg_vals = Vec::with_capacity(args.len());
                        for (i, a) in args.iter().enumerate() {
                            let (av, at) = lower_expr(b, lc, a)?.ok_or_else(|| {
                                CodegenError::Unsupported {
                                    what: "argument is unit".into(),
                                    span: a.span,
                                }
                            })?;
                            let coerced = coerce(b, (av, at), param_tys[i], a.span)?;
                            emit_bind_retain(b, lc, &a.kind, at, param_tys[i], coerced);
                            arg_vals.push(coerced);
                        }
                        let func_ref = lc.module.declare_func_in_func(id, b.func);
                        let call = b.ins().call(func_ref, &arg_vals);
                        if matches!(ret_ty, JitTy::Unit) {
                            return Ok(None);
                        }
                        return Ok(Some((b.inst_results(call)[0], ret_ty)));
                    }
                }
            }
            let (obj_v, obj_t) = lower_expr(b, lc, obj)?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "method receiver is unit".into(),
                    span: obj.span,
                }
            })?;
            // `@flags` enum: `f.has(other)` lowers as `(f & other) == other`.
            // The type checker only allows `has` here when both sides
            // share the flags enum type, so the JitTys match by
            // construction.
            if method == "has" && obj_t.is_int() && args.len() == 1 {
                let (av, at) = lower_expr(b, lc, &args[0])?.ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: "has arg is unit".into(),
                        span: args[0].span,
                    }
                })?;
                let coerced = coerce(b, (av, at), obj_t, args[0].span)?;
                let masked = b.ins().band(obj_v, coerced);
                let eq = b.ins().icmp(IntCC::Equal, masked, coerced);
                return Ok(Some((eq, JitTy::Bool)));
            }
            // Built-in Weak method: get() returns Optional<Object>.
            if let JitTy::Weak(class_id) = obj_t {
                if method == "get" {
                    let r = lc.module.declare_func_in_func(lc.weak_get_id, b.func);
                    let call = b.ins().call(r, &[obj_v]);
                    let result = b.inst_results(call)[0];
                    let opt_id = intern_optional_inner(
                        lc.optional_inners,
                        JitTy::Object(class_id),
                    );
                    return Ok(Some((result, JitTy::Optional(opt_id))));
                }
                return Err(CodegenError::Unsupported {
                    what: format!("weak has no method {method:?}"),
                    span: e.span,
                });
            }
            // Built-in `Type` introspection methods (RTTI lookup).
            if obj_t == JitTy::TypeRef
                && (method.as_str() == "fieldType"
                    || method.as_str() == "methodReturn"
                    || method.as_str() == "methodParams")
            {
                return Ok(Some(lower_type_member_lookup(
                    b, lc, obj_v, method.as_str(), &args[0], e.span,
                )?));
            }
            // Built-in Optional methods: `unwrap`. (`isSome` / `isNone`
            // are properties — see ExprKind::Field above.)
            if let JitTy::Optional(id) = obj_t {
                match method.as_str() {
                    "unwrap" => {
                        // Null-check the box pointer. On none, panic
                        // matching the interpreter's "unwrap on `none`"
                        // error rather than dereferencing 0.
                        let zero = b.ins().iconst(I64, 0);
                        let is_none = b.ins().icmp(IntCC::Equal, obj_v, zero);
                        let panic_blk = b.create_block();
                        let ok_blk = b.create_block();
                        b.ins().brif(is_none, panic_blk, &[], ok_blk, &[]);
                        b.switch_to_block(panic_blk);
                        b.seal_block(panic_blk);
                        let r = lc.module.declare_func_in_func(lc.panic_unwrap_none_id, b.func);
                        b.ins().call(r, &[]);
                        b.ins().trap(cranelift_codegen::ir::TrapCode::user(3).expect("trap"));
                        b.switch_to_block(ok_blk);
                        b.seal_block(ok_blk);
                        let inner = lc.optional_inners[id as usize];
                        if inner.is_heap() {
                            return Ok(Some((obj_v, inner)));
                        }
                        // Primitive: load payload from box at offset 8.
                        let cl_ty = inner.cl().expect("primitive cl ty");
                        let v = b.ins().load(
                            cl_ty,
                            cranelift::prelude::MemFlags::trusted(),
                            obj_v,
                            crate::runtime::OPT_PRIM_PAYLOAD_OFFSET,
                        );
                        return Ok(Some((v, inner)));
                    }
                    _ => {
                        return Err(CodegenError::Unsupported {
                            what: format!("optional has no method {method:?}"),
                            span: e.span,
                        });
                    }
                }
            }
            // Built-in `array.push(x)` dispatches to the per-width FFI.
            if let JitTy::Array(id) = obj_t {
                if method == "push" {
                    if args.len() != 1 {
                        return Err(CodegenError::Unsupported {
                            what: format!("array.push takes 1 arg, got {}", args.len()),
                            span: e.span,
                        });
                    }
                    let elem_jty = lc.array_kinds[id as usize].elem;
                    let (av, at) = lower_expr(b, lc, &args[0])?.ok_or_else(|| {
                        CodegenError::Unsupported {
                            what: "push arg is unit".into(),
                            span: args[0].span,
                        }
                    })?;
                    let coerced = coerce(b, (av, at), elem_jty, args[0].span)?;
                    // If the element is heap-typed and came from an
                    // aliased source (Var/Field/Index/This), the caller
                    // still owns a reference. Without a retain here the
                    // array's pointer and the caller's binding would
                    // both think they own the same single rc, and the
                    // second drop would double-free. Fresh allocations
                    // (`new`, call result, `[..]`, "a"+"b") arrive at
                    // rc=1 and need no extra retain.
                    if elem_jty.is_heap() && is_aliased_heap_source(&args[0].kind) {
                        emit_retain_heap(b, lc, coerced, elem_jty);
                    }
                    let push_id = match elem_jty.size_bytes() {
                        1 => lc.arrfns.push_i8,
                        2 => lc.arrfns.push_i16,
                        4 => match elem_jty {
                            JitTy::F32 => lc.arrfns.push_f32,
                            _ => lc.arrfns.push_i32,
                        },
                        8 => match elem_jty {
                            JitTy::F64 => lc.arrfns.push_f64,
                            _ => lc.arrfns.push_i64,
                        },
                        n => {
                            return Err(CodegenError::Unsupported {
                                what: format!("array.push of {n}-byte element"),
                                span: e.span,
                            });
                        }
                    };
                    let r = lc.module.declare_func_in_func(push_id, b.func);
                    b.ins().call(r, &[obj_v, coerced]);
                    return Ok(None);
                }
                if method == "pop" {
                    if !args.is_empty() {
                        return Err(CodegenError::Unsupported {
                            what: "array.pop takes no args".into(),
                            span: e.span,
                        });
                    }
                    let elem_jty = lc.array_kinds[id as usize].elem;
                    return lower_array_pop(b, lc, obj_v, obj, id, elem_jty);
                }
                if method == "indexOf" || method == "includes" {
                    if args.len() != 1 {
                        return Err(CodegenError::Unsupported {
                            what: format!("array.{method} takes 1 arg"),
                            span: e.span,
                        });
                    }
                    let elem_jty = lc.array_kinds[id as usize].elem;
                    if matches!(elem_jty, JitTy::Unit) {
                        return Err(CodegenError::Unsupported {
                            what: format!("array.{method} on unit element"),
                            span: e.span,
                        });
                    }
                    let (av, at) = lower_expr(b, lc, &args[0])?.ok_or_else(|| {
                        CodegenError::Unsupported {
                            what: format!("{method} arg is unit"),
                            span: args[0].span,
                        }
                    })?;
                    let needle = coerce(b, (av, at), elem_jty, args[0].span)?;
                    let idx = emit_array_index_of(b, lc, obj_v, needle, elem_jty);
                    // Aliased rhs heap needle (Var/Field/Index) was
                    // borrowed; non-aliased (e.g. a literal "x" + "y"
                    // result) was a fresh allocation that needs release
                    // now that the search is done. Strings only — other
                    // heap kinds compare by pointer identity and the
                    // caller's binding still owns them.
                    if matches!(elem_jty, JitTy::Str) && !is_aliased_heap_source(&args[0].kind) {
                        emit_release_string(b, lc, needle);
                    }
                    let release_recv = !is_aliased_heap_source(&obj.kind);
                    if release_recv {
                        emit_release_heap(b, lc, obj_v, obj_t);
                    }
                    if method == "indexOf" {
                        return Ok(Some((idx, JitTy::I64)));
                    }
                    let neg_one = b.ins().iconst(I64, -1);
                    let found = b.ins().icmp(IntCC::NotEqual, idx, neg_one);
                    return Ok(Some((found, JitTy::Bool)));
                }
                if method == "slice" {
                    if args.len() != 2 {
                        return Err(CodegenError::Unsupported {
                            what: "array.slice takes 2 args".into(),
                            span: e.span,
                        });
                    }
                    let elem_jty = lc.array_kinds[id as usize].elem;
                    return lower_array_slice(b, lc, obj_v, obj, &args[0], &args[1], id, elem_jty);
                }
                if method == "map" || method == "filter" || method == "forEach" {
                    if args.len() != 1 {
                        return Err(CodegenError::Unsupported {
                            what: format!("array.{method} takes 1 arg"),
                            span: e.span,
                        });
                    }
                    let elem_jty = lc.array_kinds[id as usize].elem;
                    return lower_array_higher_order(
                        b, lc, obj_v, obj, &args[0], id, elem_jty, method.as_str(),
                    );
                }
            }
            // Built-in string methods (JS-style camelCase).
            if matches!(obj_t, JitTy::Str) {
                let release_recv = !is_aliased_heap_source(&obj.kind);
                let nullary = |fid: cranelift_module::FuncId, ret: JitTy, b: &mut FunctionBuilder, lc: &mut LowerCtx| {
                    let r = lc.module.declare_func_in_func(fid, b.func);
                    let call = b.ins().call(r, &[obj_v]);
                    let v = b.inst_results(call)[0];
                    if release_recv {
                        emit_release_string(b, lc, obj_v);
                    }
                    Ok(Some((v, ret)))
                };
                match method.as_str() {
                    "charAt" => {
                        if args.len() != 1 {
                            return Err(CodegenError::Unsupported {
                                what: format!("charAt takes 1 arg, got {}", args.len()),
                                span: e.span,
                            });
                        }
                        let (av, at) = lower_expr(b, lc, &args[0])?.ok_or_else(|| {
                            CodegenError::Unsupported {
                                what: "charAt arg is unit".into(),
                                span: args[0].span,
                            }
                        })?;
                        let idx = coerce(b, (av, at), JitTy::I64, args[0].span)?;
                        let r = lc.module.declare_func_in_func(lc.strfns.char_at, b.func);
                        let call = b.ins().call(r, &[obj_v, idx]);
                        let v = b.inst_results(call)[0];
                        if release_recv {
                            emit_release_string(b, lc, obj_v);
                        }
                        return Ok(Some((v, JitTy::Str)));
                    }
                    "includes" | "startsWith" | "endsWith" => {
                        if args.len() != 1 {
                            return Err(CodegenError::Unsupported {
                                what: format!("{method} takes 1 arg, got {}", args.len()),
                                span: e.span,
                            });
                        }
                        let (av, at) = lower_expr(b, lc, &args[0])?.ok_or_else(|| {
                            CodegenError::Unsupported {
                                what: format!("{method} arg is unit"),
                                span: args[0].span,
                            }
                        })?;
                        if !matches!(at, JitTy::Str) {
                            return Err(CodegenError::Unsupported {
                                what: format!("{method} expects string arg"),
                                span: args[0].span,
                            });
                        }
                        let release_arg = !is_aliased_heap_source(&args[0].kind);
                        let fid = match method.as_str() {
                            "includes" => lc.strfns.includes,
                            "startsWith" => lc.strfns.starts_with,
                            "endsWith" => lc.strfns.ends_with,
                            _ => unreachable!(),
                        };
                        let r = lc.module.declare_func_in_func(fid, b.func);
                        let call = b.ins().call(r, &[obj_v, av]);
                        let v = b.inst_results(call)[0];
                        if release_recv {
                            emit_release_string(b, lc, obj_v);
                        }
                        if release_arg {
                            emit_release_string(b, lc, av);
                        }
                        return Ok(Some((v, JitTy::Bool)));
                    }
                    "toUpper" => return nullary(lc.strfns.to_upper, JitTy::Str, b, lc),
                    "toLower" => return nullary(lc.strfns.to_lower, JitTy::Str, b, lc),
                    "trim" => return nullary(lc.strfns.trim, JitTy::Str, b, lc),
                    "replace" => {
                        if args.len() != 2 {
                            return Err(CodegenError::Unsupported {
                                what: format!("replace takes 2 args, got {}", args.len()),
                                span: e.span,
                            });
                        }
                        let (nv, nt) = lower_expr(b, lc, &args[0])?.ok_or_else(|| CodegenError::Unsupported {
                            what: "replace needle is unit".into(), span: args[0].span,
                        })?;
                        let (rv, rt) = lower_expr(b, lc, &args[1])?.ok_or_else(|| CodegenError::Unsupported {
                            what: "replace replacement is unit".into(), span: args[1].span,
                        })?;
                        if !matches!(nt, JitTy::Str) || !matches!(rt, JitTy::Str) {
                            return Err(CodegenError::Unsupported {
                                what: "replace expects string args".into(), span: e.span,
                            });
                        }
                        let release_n = !is_aliased_heap_source(&args[0].kind);
                        let release_r = !is_aliased_heap_source(&args[1].kind);
                        let r = lc.module.declare_func_in_func(lc.strfns.replace, b.func);
                        let call = b.ins().call(r, &[obj_v, nv, rv]);
                        let v = b.inst_results(call)[0];
                        if release_recv { emit_release_string(b, lc, obj_v); }
                        if release_n { emit_release_string(b, lc, nv); }
                        if release_r { emit_release_string(b, lc, rv); }
                        return Ok(Some((v, JitTy::Str)));
                    }
                    "slice" => {
                        if args.len() != 2 {
                            return Err(CodegenError::Unsupported {
                                what: format!("slice takes 2 args, got {}", args.len()),
                                span: e.span,
                            });
                        }
                        let (sv, st) = lower_expr(b, lc, &args[0])?.ok_or_else(|| CodegenError::Unsupported {
                            what: "slice start is unit".into(), span: args[0].span,
                        })?;
                        let (ev_, et) = lower_expr(b, lc, &args[1])?.ok_or_else(|| CodegenError::Unsupported {
                            what: "slice end is unit".into(), span: args[1].span,
                        })?;
                        let start_i64 = coerce(b, (sv, st), JitTy::I64, args[0].span)?;
                        let end_i64 = coerce(b, (ev_, et), JitTy::I64, args[1].span)?;
                        let r = lc.module.declare_func_in_func(lc.strfns.slice, b.func);
                        let call = b.ins().call(r, &[obj_v, start_i64, end_i64]);
                        let v = b.inst_results(call)[0];
                        if release_recv { emit_release_string(b, lc, obj_v); }
                        return Ok(Some((v, JitTy::Str)));
                    }
                    "split" => {
                        if args.len() != 1 {
                            return Err(CodegenError::Unsupported {
                                what: format!("split takes 1 arg, got {}", args.len()),
                                span: e.span,
                            });
                        }
                        let (sv, st) = lower_expr(b, lc, &args[0])?.ok_or_else(|| CodegenError::Unsupported {
                            what: "split sep is unit".into(), span: args[0].span,
                        })?;
                        if !matches!(st, JitTy::Str) {
                            return Err(CodegenError::Unsupported {
                                what: "split expects string sep".into(), span: args[0].span,
                            });
                        }
                        let release_s = !is_aliased_heap_source(&args[0].kind);
                        // Result is `string[]` — intern the array kind so the
                        // drop wrapper releases each StringRc on array drop.
                        let kind_id = intern_array_kind(
                            lc.array_kinds,
                            ArrayKind { elem: JitTy::Str, fixed: None },
                        );
                        let drop_fn_ptr = crate::drops::array_drop_fn_ptr(b, lc, kind_id);
                        let r = lc.module.declare_func_in_func(lc.strfns.split, b.func);
                        let call = b.ins().call(r, &[obj_v, sv, drop_fn_ptr]);
                        let v = b.inst_results(call)[0];
                        if release_recv { emit_release_string(b, lc, obj_v); }
                        if release_s { emit_release_string(b, lc, sv); }
                        return Ok(Some((v, JitTy::Array(kind_id))));
                    }
                    _ => {
                        return Err(CodegenError::Unsupported {
                            what: format!("string has no method {method:?}"),
                            span: e.span,
                        });
                    }
                }
            }
            // Built-in Map methods.
            if let JitTy::Map(map_id) = obj_t {
                return lower_map_method(b, lc, map_id, method.as_str(), obj_v, args, e.span);
            }
            let class_id = match obj_t {
                JitTy::Object(id) => id,
                _ => {
                    return Err(CodegenError::Unsupported {
                        what: "method call on non-object".into(),
                        span: obj.span,
                    });
                }
            };
            call_method(b, lc, class_id, method.as_str(), obj_v, args, e.span)
        }
        ExprKind::Call { callee, args } => {
            // Built-in `typeof(x): Type`. The argument is evaluated
            // (so heap retains/releases stay correct), then the
            // resulting `TypeMeta*` is computed by static dispatch on
            // the argument's compile-time JitTy — except for
            // class-typed arguments, which read the dynamic class's
            // TypeMeta pointer from the vtable's leading slot
            // (vtable_ptr - 8).
            if callee.as_str() == "typeof" && args.len() == 1 {
                return Ok(Some(lower_typeof(b, lc, &args[0])?));
            }
            // Indirect call through a function-typed local. Matches the
            // type checker's lookup order — a `let` shadows top-level
            // fns of the same name.
            if let Some(&(var, JitTy::Fn(sig_id))) = lc.env.bindings.get(callee) {
                let sig = lc.fn_signatures[sig_id as usize].clone();
                // Closure protocol: the binding holds a closure
                // struct pointer. Load fn_ptr from offset 0; call
                // with (closure_ptr, args). The wrapper signature
                // has env_ptr (i64) prepended to the user params.
                let closure_ptr = b.use_var(var);
                let fn_ptr = b.ins().load(I64, MemFlags::trusted(), closure_ptr, 0);
                let mut arg_vals = Vec::with_capacity(args.len() + 1);
                arg_vals.push(closure_ptr);
                for (i, a) in args.iter().enumerate() {
                    let (av, at) = lower_expr(b, lc, a)?.ok_or_else(|| {
                        CodegenError::Unsupported {
                            what: "argument is unit".into(),
                            span: a.span,
                        }
                    })?;
                    let coerced = coerce(b, (av, at), sig.params[i], a.span)?;
                    emit_bind_retain(b, lc, &a.kind, at, sig.params[i], coerced);
                    arg_vals.push(coerced);
                }
                // Cranelift sig: env_ptr (i64) + user params + ret.
                let mut cl_sig = lc.module.make_signature();
                cl_sig
                    .params
                    .push(cranelift::prelude::AbiParam::new(I64));
                for p in &sig.params {
                    cl_sig.params.push(cranelift::prelude::AbiParam::new(
                        p.cl().expect("non-unit param"),
                    ));
                }
                if let Some(rt) = sig.ret.cl() {
                    cl_sig.returns.push(cranelift::prelude::AbiParam::new(rt));
                }
                let sig_ref = b.import_signature(cl_sig);
                let call = b.ins().call_indirect(sig_ref, fn_ptr, &arg_vals);
                if matches!(sig.ret, JitTy::Unit) {
                    return Ok(None);
                }
                return Ok(Some((b.inst_results(call)[0], sig.ret)));
            }
            // Built-in helpers callable inside `@extern(C) { ... }`
            // blocks. Recognised before the normal `lc.funcs` lookup
            // so they bypass the regular extern-fn machinery.
            if let Some(result) = try_lower_extern_c_helper(b, lc, callee.as_str(), args, e.span)? {
                return Ok(result);
            }
            // Free function first.
            if let Some(entry) = lc.funcs.get(callee).cloned() {
                let (id, param_tys, ret_ty) = entry;
                let is_native = lc.native_extern_fns.contains(callee);
                let is_variadic = lc.native_extern_variadic.contains(callee);
                let n_fixed = param_tys.len();
                let mut arg_vals = Vec::with_capacity(args.len());
                // sret return: the C ABI wants a pointer to caller-
                // allocated storage as the hidden first arg. Alloc
                // the destination ilang instance up front (its user
                // area becomes the C struct's home), and remember the
                // pointer so we can return it after the call. The
                // signature's first AbiParam is StructReturn; the
                // calling-conv assigns it to X8 / RDI.
                let sret_ptr: Option<cranelift::prelude::Value> =
                    if lc.native_extern_by_value.contains(callee) {
                        if let JitTy::Object(class_id) = ret_ty {
                            let layout = &lc.class_layouts[class_id as usize];
                            if matches!(
                                crate::compiler::repr_c_by_value_kind(layout),
                                crate::compiler::ByValueKind::Indirect
                            ) {
                                let size = layout.size as i64;
                                let drop_fn_ptr =
                                    match lc.class_drops[class_id as usize] {
                                        Some(fid) => {
                                            let fr = lc
                                                .module
                                                .declare_func_in_func(fid, b.func);
                                            b.ins().func_addr(I64, fr)
                                        }
                                        None => b.ins().iconst(I64, 0),
                                    };
                                let vtable_addr = lc
                                    .class_vtable_addrs
                                    .get(class_id as usize)
                                    .copied()
                                    .unwrap_or(0);
                                let vtable_ptr = b.ins().iconst(I64, vtable_addr);
                                let alloc_ref = lc
                                    .module
                                    .declare_func_in_func(lc.alloc_object_id, b.func);
                                let size_v = b.ins().iconst(I64, size);
                                let alloc_call = b.ins().call(
                                    alloc_ref,
                                    &[size_v, drop_fn_ptr, vtable_ptr],
                                );
                                let ptr = b.inst_results(alloc_call)[0];
                                arg_vals.push(ptr);
                                Some(ptr)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                // For native externs, every `string` arg is converted
                // to a freshly-malloc'd C string via the runtime
                // helper; the resulting pointer is freed after the
                // call. Track them here so the post-call cleanup loop
                // sees the right Values.
                let mut c_str_temps: Vec<cranelift::prelude::Value> = Vec::new();
                for (i, a) in args.iter().enumerate() {
                    // Variadic tail: the declared param list is the
                    // fixed prefix (`fmt: string` for printf); extras
                    // pass through with their actual JIT types and
                    // get the same per-type marshalling (string →
                    // C-string, array → data ptr, managed opaque →
                    // unwrap).
                    if is_variadic && i >= n_fixed {
                        let (av, at) = lower_expr(b, lc, a)?.ok_or_else(|| {
                            CodegenError::Unsupported {
                                what: "argument is unit".into(),
                                span: a.span,
                            }
                        })?;
                        let raw = match at {
                            JitTy::Str => {
                                let f = lc
                                    .module
                                    .declare_func_in_func(lc.strfns.to_c_str, b.func);
                                let c = b.ins().call(f, &[av]);
                                let c_ptr = b.inst_results(c)[0];
                                c_str_temps.push(c_ptr);
                                c_ptr
                            }
                            JitTy::Array(_) => b.ins().load(
                                I64,
                                MemFlags::trusted(),
                                av,
                                ARRAY_DATA_OFFSET,
                            ),
                            JitTy::Object(class_id)
                                if lc.class_layouts[class_id as usize]
                                    .extern_lib
                                    .is_some()
                                    && lc.class_methods[class_id as usize]
                                        .contains_key(&"deinit".into()) =>
                            {
                                b.ins().load(I64, MemFlags::trusted(), av, 0)
                            }
                            _ => av,
                        };
                        arg_vals.push(raw);
                        continue;
                    }
                    // C callback: a `fn(...)` parameter on any
                    // `@extern` fn (host or native lib) accepts a
                    // *raw function pointer*, not a closure box.
                    // Support direct top-level fn references
                    // (capture-free) and reject closure values —
                    // the C side has no env-ptr slot to thread
                    // through. Regular ilang fns continue to box.
                    if lc.extern_fn_names.contains(callee)
                        && matches!(param_tys[i], JitTy::Fn(_))
                    {
                        let name = match &a.kind {
                            ExprKind::Var(n) if lc.funcs.contains_key(n) => n.clone(),
                            _ => {
                                return Err(CodegenError::Unsupported {
                                    what: "C callback argument must be a direct \
                                           top-level fn name (closures and let-bound \
                                           fn values can't be passed)".into(),
                                    span: a.span,
                                });
                            }
                        };
                        let (id, _, _) = lc.funcs.get(&name).cloned().unwrap();
                        let func_ref = lc.module.declare_func_in_func(id, b.func);
                        let addr = b.ins().func_addr(I64, func_ref);
                        arg_vals.push(addr);
                        continue;
                    }
                    let (av, at) = lower_expr(b, lc, a)?.ok_or_else(|| {
                        CodegenError::Unsupported {
                            what: "argument is unit".into(),
                            span: a.span,
                        }
                    })?;
                    let coerced = coerce(b, (av, at), param_tys[i], a.span)?;
                    emit_bind_retain(b, lc, &a.kind, at, param_tys[i], coerced);
                    if is_native && matches!(param_tys[i], JitTy::Str) {
                        // StringRc* → malloc'd C string. Save the
                        // temp pointer so we can free it after the
                        // foreign call returns.
                        let f = lc.module.declare_func_in_func(lc.strfns.to_c_str, b.func);
                        let c = b.ins().call(f, &[coerced]);
                        let c_ptr = b.inst_results(c)[0];
                        c_str_temps.push(c_ptr);
                        arg_vals.push(c_ptr);
                    } else if is_native && matches!(param_tys[i], JitTy::Array(_)) {
                        // Numeric array → `void *` buffer pointer.
                        // Hand the C side the raw data slot from
                        // the array's heap header; ARC keeps the
                        // ilang allocation alive across the call.
                        let data = b.ins().load(
                            I64,
                            MemFlags::trusted(),
                            coerced,
                            ARRAY_DATA_OFFSET,
                        );
                        arg_vals.push(data);
                    } else if lc.native_extern_by_value.contains(callee)
                        && matches!(param_tys[i], JitTy::Object(_))
                    {
                        // Pass-by-value `@extern(C) struct` struct: load the
                        // user data area as 1–2 i64 chunks (per the
                        // integer-only ≤ 16 B composite rule). The
                        // sig was built with the same chunk count so
                        // the ABI lines up on AArch64 / SysV.
                        let class_id = match param_tys[i] {
                            JitTy::Object(id) => id,
                            _ => unreachable!(),
                        };
                        let layout = &lc.class_layouts[class_id as usize];
                        let size = layout.size;
                        match crate::compiler::repr_c_by_value_kind(layout) {
                            crate::compiler::ByValueKind::Chunks(n) => {
                                if n >= 1 {
                                    let raw = b.ins().load(I64, MemFlags::trusted(), coerced, 0);
                                    let lo_size = size.min(8);
                                    let chunk = if lo_size == 8 {
                                        raw
                                    } else {
                                        let mask = (1i64 << (lo_size as i64 * 8)) - 1;
                                        b.ins().band_imm(raw, mask)
                                    };
                                    arg_vals.push(chunk);
                                }
                                if n >= 2 {
                                    let upper_size = size - 8;
                                    let raw_hi = b.ins().load(
                                        I64, MemFlags::trusted(), coerced, 8,
                                    );
                                    let hi = if upper_size == 8 {
                                        raw_hi
                                    } else {
                                        let mask = (1i64 << (upper_size as i64 * 8)) - 1;
                                        b.ins().band_imm(raw_hi, mask)
                                    };
                                    arg_vals.push(hi);
                                }
                            }
                            crate::compiler::ByValueKind::Hfa { elem, count } => {
                                // Each HFA element loads as F32 / F64
                                // and goes to its own FP register
                                // (Cranelift's calling-conv handles
                                // V0..V3 / XMM assignment when the
                                // AbiParam type is F32 / F64).
                                let cl_ty = elem.cl().expect("HFA elem cl");
                                let elem_size = elem.size_bytes();
                                let mut entries: Vec<(u32, JitTy)> = layout
                                    .fields
                                    .values()
                                    .map(|&(off, ty)| (off, ty))
                                    .collect();
                                entries.sort_by_key(|(off, _)| *off);
                                for (off, _) in entries.iter().take(count as usize) {
                                    let v = b.ins().load(cl_ty, MemFlags::trusted(), coerced, *off as i32);
                                    arg_vals.push(v);
                                }
                                let _ = elem_size;
                            }
                            crate::compiler::ByValueKind::Indirect => {
                                // x86_64: the signature param is
                                // `StructArgument(size)` and Cranelift
                                // copies the struct onto the stack
                                // per SysV when given the user
                                // pointer.
                                //
                                // aarch64: AAPCS64 wants a caller-
                                // allocated copy whose address we
                                // pass in the next GPR. Allocate a
                                // stack slot, chunk-copy the struct
                                // bytes in, and hand over the slot
                                // address (it's only valid for the
                                // duration of this call, so it's
                                // safe to reuse the same slot for
                                // every indirect by_value site).
                                #[cfg(target_arch = "x86_64")]
                                {
                                    arg_vals.push(coerced);
                                }
                                #[cfg(target_arch = "aarch64")]
                                {
                                    use cranelift::prelude::{StackSlotData, StackSlotKind};
                                    let slot = b.create_sized_stack_slot(StackSlotData::new(
                                        StackSlotKind::ExplicitSlot,
                                        size,
                                        3, // 8-byte aligned
                                    ));
                                    let dst = b.ins().stack_addr(I64, slot, 0);
                                    // 8 / 4 / 2 / 1 byte chunk copy.
                                    let chunks: [(u32, cranelift::prelude::Type); 4] =
                                        [(8, I64), (4, I32), (2, I16), (1, I8)];
                                    let mut copied: u32 = 0;
                                    for (w, cl_ty) in chunks {
                                        while size - copied >= w {
                                            let off = copied as i32;
                                            let v = b.ins().load(cl_ty, MemFlags::trusted(), coerced, off);
                                            b.ins().store(MemFlags::trusted(), v, dst, off);
                                            copied += w;
                                        }
                                    }
                                    arg_vals.push(dst);
                                }
                            }
                        }
                    } else if is_native && is_managed_opaque(lc, param_tys[i]) {
                        // Managed opaque handle (`@extern class Foo {
                        // deinit { ... } }`): the user value is the
                        // ARC box pointer; the C function expects the
                        // raw C pointer at offset 0 of that box.
                        let raw = b.ins().load(I64, MemFlags::trusted(), coerced, 0);
                        arg_vals.push(raw);
                    } else if is_native && is_managed_opaque_optional(lc, param_tys[i]) {
                        // `Foo?` for a managed opaque: 0 → 0 (NULL),
                        // box → load *(box+0). Branch so we never
                        // dereference a null pointer.
                        let zero = b.ins().iconst(I64, 0);
                        let is_null = b.ins().icmp(IntCC::Equal, coerced, zero);
                        let null_bb = b.create_block();
                        let some_bb = b.create_block();
                        let merge = b.create_block();
                        b.append_block_param(merge, I64);
                        b.ins().brif(is_null, null_bb, &[], some_bb, &[]);
                        b.switch_to_block(null_bb);
                        b.seal_block(null_bb);
                        b.ins().jump(merge, &[zero.into()]);
                        b.switch_to_block(some_bb);
                        b.seal_block(some_bb);
                        let raw = b.ins().load(I64, MemFlags::trusted(), coerced, 0);
                        b.ins().jump(merge, &[raw.into()]);
                        b.switch_to_block(merge);
                        b.seal_block(merge);
                        arg_vals.push(b.block_params(merge)[0]);
                    } else {
                        arg_vals.push(coerced);
                    }
                }
                let func_ref = lc.module.declare_func_in_func(id, b.func);
                let call = if is_variadic {
                    build_variadic_call(b, lc, func_ref, &arg_vals, n_fixed, ret_ty)
                } else {
                    b.ins().call(func_ref, &arg_vals)
                };
                // by_value struct return: the call produces 1 or 2
                // i64 chunks (per `repr_c_chunk_count`). Allocate a
                // fresh instance and store the chunks back into its
                // user area; the caller then sees a normal heap
                // Object with rc=1.
                let is_by_value_struct_ret = lc.native_extern_by_value.contains(callee)
                    && matches!(ret_ty, JitTy::Object(_))
                    && sret_ptr.is_none();
                let raw_ret = if matches!(ret_ty, JitTy::Unit) {
                    None
                } else if let Some(ptr) = sret_ptr {
                    // sret: the C side wrote into the buffer we
                    // allocated and passed in. The ilang result is
                    // that buffer's user pointer.
                    Some(ptr)
                } else if is_by_value_struct_ret {
                    let class_id = match ret_ty {
                        JitTy::Object(id) => id,
                        _ => unreachable!(),
                    };
                    let size = lc.class_layouts[class_id as usize].size as i64;
                    let drop_fn_ptr = match lc.class_drops[class_id as usize] {
                        Some(fid) => {
                            let fr = lc.module.declare_func_in_func(fid, b.func);
                            b.ins().func_addr(I64, fr)
                        }
                        None => b.ins().iconst(I64, 0),
                    };
                    let vtable_addr = lc
                        .class_vtable_addrs
                        .get(class_id as usize)
                        .copied()
                        .unwrap_or(0);
                    let vtable_ptr = b.ins().iconst(I64, vtable_addr);
                    let alloc_ref =
                        lc.module.declare_func_in_func(lc.alloc_object_id, b.func);
                    let size_v = b.ins().iconst(I64, size);
                    let alloc_call =
                        b.ins().call(alloc_ref, &[size_v, drop_fn_ptr, vtable_ptr]);
                    let ptr = b.inst_results(alloc_call)[0];
                    let results: Vec<_> = b.inst_results(call).to_vec();
                    let layout = &lc.class_layouts[class_id as usize];
                    match crate::compiler::repr_c_by_value_kind(layout) {
                        crate::compiler::ByValueKind::Hfa { count, .. } => {
                            // Store each FP element back at its own
                            // field offset.
                            let mut entries: Vec<(u32, JitTy)> = layout
                                .fields
                                .values()
                                .map(|&(off, ty)| (off, ty))
                                .collect();
                            entries.sort_by_key(|(off, _)| *off);
                            for (i, (off, _)) in entries.iter().take(count as usize).enumerate() {
                                b.ins().store(MemFlags::trusted(), results[i], ptr, *off as i32);
                            }
                        }
                        _ => {
                            // Chunks(1|2): store at offset 0 (and 8).
                            if !results.is_empty() {
                                b.ins().store(MemFlags::trusted(), results[0], ptr, 0);
                            }
                            if results.len() >= 2 {
                                b.ins().store(MemFlags::trusted(), results[1], ptr, 8);
                            }
                        }
                    }
                    Some(ptr)
                } else {
                    Some(b.inst_results(call)[0])
                };
                // Free the C-string temps now that the foreign function
                // has consumed them. Done before result conversion so
                // even an early-`return` doesn't leak (we don't have
                // unwinding here).
                for c_ptr in c_str_temps {
                    let f = lc.module.declare_func_in_func(lc.strfns.free_c_str, b.func);
                    b.ins().call(f, &[c_ptr]);
                }
                let result = if is_native && matches!(ret_ty, JitTy::Str) {
                    let raw = raw_ret.expect("native string return is non-unit");
                    let f = lc
                        .module
                        .declare_func_in_func(lc.strfns.c_str_to_string, b.func);
                    let c = b.ins().call(f, &[raw]);
                    let copied = b.inst_results(c)[0];
                    Some(copied)
                } else if is_native && is_managed_opaque(lc, ret_ty) {
                    // Wrap the C pointer in a fresh ARC box so deinit
                    // fires when the last reference dies.
                    let raw = raw_ret.expect("non-unit return");
                    let boxed = wrap_managed_opaque(b, lc, raw, opaque_class_id_of(ret_ty));
                    Some(boxed)
                } else if is_native && is_managed_opaque_optional(lc, ret_ty) {
                    // `Foo?` from a managed opaque extern: NULL → 0
                    // (none), non-null → wrap in ARC box.
                    let raw = raw_ret.expect("non-unit return");
                    let zero = b.ins().iconst(I64, 0);
                    let is_null = b.ins().icmp(IntCC::Equal, raw, zero);
                    let null_bb = b.create_block();
                    let some_bb = b.create_block();
                    let merge = b.create_block();
                    b.append_block_param(merge, I64);
                    b.ins().brif(is_null, null_bb, &[], some_bb, &[]);
                    b.switch_to_block(null_bb);
                    b.seal_block(null_bb);
                    b.ins().jump(merge, &[zero.into()]);
                    b.switch_to_block(some_bb);
                    b.seal_block(some_bb);
                    let class_id = managed_opaque_optional_class_id(lc, ret_ty)
                        .expect("checked above");
                    let boxed = wrap_managed_opaque(b, lc, raw, class_id);
                    b.ins().jump(merge, &[boxed.into()]);
                    b.switch_to_block(merge);
                    b.seal_block(merge);
                    Some(b.block_params(merge)[0])
                } else {
                    raw_ret
                };
                return match (result, ret_ty) {
                    (None, JitTy::Unit) => Ok(None),
                    (Some(v), t) => Ok(Some((v, t))),
                    _ => unreachable!(),
                };
            }
            // Implicit method call on `this`.
            if let Some((this_var, class_id)) = lc.this {
                if lc.class_methods[class_id as usize].contains_key(callee) {
                    let this_v = b.use_var(this_var);
                    return call_method(b, lc, class_id, callee.as_str(), this_v, args, e.span);
                }
            }
            Err(CodegenError::Unsupported {
                what: format!("unknown function {callee:?}"),
                span: e.span,
            })
        }
        ExprKind::Tuple(elements) => {
            let mut vals: Vec<TV> = Vec::with_capacity(elements.len());
            for el in elements {
                let v = lower_expr(b, lc, el)?.ok_or_else(|| CodegenError::Unsupported {
                    what: "tuple element is unit".into(),
                    span: el.span,
                })?;
                vals.push(v);
            }
            let elem_jtys: Vec<JitTy> = vals.iter().map(|(_, t)| *t).collect();
            let tuple_id = crate::ty::intern_tuple_kind(lc.tuple_kinds, elem_jtys);
            let kind = lc.tuple_kinds[tuple_id as usize].clone();
            let size_v = b.ins().iconst(I64, kind.size as i64);
            let drop_fn_ptr = crate::drops::tuple_drop_fn_ptr(b, lc, tuple_id);
            let zero_vt = b.ins().iconst(I64, 0);
            let alloc_ref = lc.module.declare_func_in_func(lc.alloc_object_id, b.func);
            let call = b.ins().call(alloc_ref, &[size_v, drop_fn_ptr, zero_vt]);
            let ptr = b.inst_results(call)[0];
            for ((val, vt), &offset) in vals.iter().zip(kind.offsets.iter()) {
                let cl = vt.cl().expect("non-unit tuple element");
                let _ = cl;
                b.ins().store(MemFlags::trusted(), *val, ptr, offset as i32);
            }
            Ok(Some((ptr, JitTy::Tuple(tuple_id))))
        }
        ExprKind::Array(elements) => {
            if elements.is_empty() {
                return Err(CodegenError::Unsupported {
                    what: "JIT array literal must have at least one element \
                           (annotate the binding to allow `[]`)".into(),
                    span: e.span,
                });
            }
            // No type hint here — pick the element type from the first
            // element. The Let path overrides via `lower_array_literal`
            // when an annotation is present.
            let (first_v, first_t) = lower_expr(b, lc, &elements[0])?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "array element is unit".into(),
                    span: elements[0].span,
                }
            })?;
            let mut tail: Vec<(Value, JitTy, ilang_ast::Span)> =
                Vec::with_capacity(elements.len() - 1);
            for el in &elements[1..] {
                let (v, t) = lower_expr(b, lc, el)?.ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: "array element is unit".into(),
                        span: el.span,
                    }
                })?;
                tail.push((v, t, el.span));
            }
            let mut all = Vec::with_capacity(elements.len());
            all.push((first_v, first_t, elements[0].span));
            all.extend(tail);
            build_array(b, lc, all, first_t)
        }
        ExprKind::Index { obj, index } => {
            let (obj_v, obj_t) = lower_expr(b, lc, obj)?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "indexed value is unit".into(),
                    span: obj.span,
                }
            })?;
            // Map indexing: `m[k]` calls into the runtime, which aborts
            // when the key is missing (mirrors interpreter).
            if let JitTy::Map(map_id) = obj_t {
                return lower_map_index_get(b, lc, map_id, obj_v, index);
            }
            // Embedded array (`@extern(C) struct` field of fixed numeric
            // type). `obj_v` holds the base address; compute
            // `base + i * elem_size` and load. Bounds check uses
            // the kind's known length.
            if let JitTy::EmbeddedArray(arr_id) = obj_t {
                let kind = lc.array_kinds[arr_id as usize];
                let len = kind
                    .fixed
                    .expect("embedded array always has a fixed length") as i64;
                let elem_jty = kind.elem;
                let (idx_v, idx_t) = lower_expr(b, lc, index)?.ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: "index is unit".into(),
                        span: index.span,
                    }
                })?;
                let idx_i64 = coerce(b, (idx_v, idx_t), JitTy::I64, index.span)?;
                let len_v = b.ins().iconst(I64, len);
                emit_inline_bounds_check(b, lc, idx_i64, len_v);
                let elem_size = elem_jty.size_bytes() as i64;
                let off = b.ins().imul_imm(idx_i64, elem_size);
                let addr = b.ins().iadd(obj_v, off);
                let v = b.ins().load(
                    elem_jty.cl().expect("non-unit elem"),
                    MemFlags::trusted(),
                    addr,
                    0,
                );
                return Ok(Some((v, elem_jty)));
            }
            // Flexible array member: same offset math as
            // `EmbeddedArray`, but no static length so no bounds
            // check (matches C's FAM semantics; the user maintains
            // the count themselves).
            if let JitTy::FlexArray(arr_id) = obj_t {
                let kind = lc.array_kinds[arr_id as usize];
                let elem_jty = kind.elem;
                let (idx_v, idx_t) = lower_expr(b, lc, index)?.ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: "index is unit".into(),
                        span: index.span,
                    }
                })?;
                let idx_i64 = coerce(b, (idx_v, idx_t), JitTy::I64, index.span)?;
                let elem_size = elem_jty.size_bytes() as i64;
                let off = b.ins().imul_imm(idx_i64, elem_size);
                let addr = b.ins().iadd(obj_v, off);
                let v = b.ins().load(
                    elem_jty.cl().expect("non-unit elem"),
                    MemFlags::trusted(),
                    addr,
                    0,
                );
                return Ok(Some((v, elem_jty)));
            }
            // Tuple indexing: index is a constant integer literal
            // (the type checker enforces this so the element type
            // resolves statically). Load directly from the offset.
            if let JitTy::Tuple(tuple_id) = obj_t {
                let n = match index.kind {
                    ExprKind::Int(n) if n >= 0 => n as usize,
                    _ => {
                        return Err(CodegenError::Unsupported {
                            what: "tuple index must be a non-negative integer literal".into(),
                            span: index.span,
                        });
                    }
                };
                let kind = lc.tuple_kinds[tuple_id as usize].clone();
                if n >= kind.elems.len() {
                    return Err(CodegenError::Unsupported {
                        what: format!("tuple index {n} out of bounds"),
                        span: index.span,
                    });
                }
                let elem_jty = kind.elems[n];
                let off = kind.offsets[n] as i32;
                let cl = elem_jty.cl().expect("tuple element is non-unit");
                let v = b.ins().load(cl, MemFlags::trusted(), obj_v, off);
                return Ok(Some((v, elem_jty)));
            }
            let array_id = match obj_t {
                JitTy::Array(id) => id,
                _ => {
                    return Err(CodegenError::Unsupported {
                        what: "index on non-array".into(),
                        span: obj.span,
                    });
                }
            };
            let elem_jty = lc.array_kinds[array_id as usize].elem;
            let (idx_v, idx_t) = lower_expr(b, lc, index)?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "index is unit".into(),
                    span: index.span,
                }
            })?;
            let idx_i64 = coerce(b, (idx_v, idx_t), JitTy::I64, index.span)?;
            // Bounds check: panic if idx < 0 or idx >= len. Matches the
            // interpreter's "array index N out of bounds" error.
            emit_array_bounds_check(b, lc, obj_v, idx_i64);
            let elem_size = b.ins().iconst(I64, elem_jty.size_bytes() as i64);
            let off = b.ins().imul(idx_i64, elem_size);
            let data = b.ins().load(I64, MemFlags::trusted(), obj_v, ARRAY_DATA_OFFSET);
            let addr = b.ins().iadd(data, off);
            let v = b.ins().load(
                elem_jty.cl().expect("non-unit elem"),
                MemFlags::trusted(),
                addr,
                0,
            );
            Ok(Some((v, elem_jty)))
        }
        ExprKind::AssignIndex { obj, index, value } => {
            let (obj_v, obj_t) = lower_expr(b, lc, obj)?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "indexed value is unit".into(),
                    span: obj.span,
                }
            })?;
            // Map[k] = v: dispatch to ilang_jit_map_set; returns Unit.
            if let JitTy::Map(map_id) = obj_t {
                return lower_map_index_set(b, lc, map_id, obj_v, index, value, &value.kind);
            }
            // Embedded array element write — `obj_v` is the base
            // address of the inline bytes; compute element addr and
            // store. No old-value release because primitive elements
            // aren't heap-managed.
            if let JitTy::EmbeddedArray(arr_id) = obj_t {
                let kind = lc.array_kinds[arr_id as usize];
                let len = kind
                    .fixed
                    .expect("embedded array always has a fixed length") as i64;
                let elem_jty = kind.elem;
                let (idx_v, idx_t) = lower_expr(b, lc, index)?.ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: "index is unit".into(),
                        span: index.span,
                    }
                })?;
                let idx_i64 = coerce(b, (idx_v, idx_t), JitTy::I64, index.span)?;
                let len_v = b.ins().iconst(I64, len);
                emit_inline_bounds_check(b, lc, idx_i64, len_v);
                let (val, vt) = lower_expr(b, lc, value)?.ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: "assigned value is unit".into(),
                        span: value.span,
                    }
                })?;
                let coerced = coerce(b, (val, vt), elem_jty, value.span)?;
                let elem_size = elem_jty.size_bytes() as i64;
                let off = b.ins().imul_imm(idx_i64, elem_size);
                let addr = b.ins().iadd(obj_v, off);
                b.ins().store(MemFlags::trusted(), coerced, addr, 0);
                return Ok(None);
            }
            // Flexible array member write — same offset math, no
            // bounds check (FAM length isn't known statically).
            if let JitTy::FlexArray(arr_id) = obj_t {
                let kind = lc.array_kinds[arr_id as usize];
                let elem_jty = kind.elem;
                let (idx_v, idx_t) = lower_expr(b, lc, index)?.ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: "index is unit".into(),
                        span: index.span,
                    }
                })?;
                let idx_i64 = coerce(b, (idx_v, idx_t), JitTy::I64, index.span)?;
                let (val, vt) = lower_expr(b, lc, value)?.ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: "assigned value is unit".into(),
                        span: value.span,
                    }
                })?;
                let coerced = coerce(b, (val, vt), elem_jty, value.span)?;
                let elem_size = elem_jty.size_bytes() as i64;
                let off = b.ins().imul_imm(idx_i64, elem_size);
                let addr = b.ins().iadd(obj_v, off);
                b.ins().store(MemFlags::trusted(), coerced, addr, 0);
                return Ok(None);
            }
            let array_id = match obj_t {
                JitTy::Array(id) => id,
                _ => {
                    return Err(CodegenError::Unsupported {
                        what: "index assignment on non-array".into(),
                        span: obj.span,
                    });
                }
            };
            let elem_jty = lc.array_kinds[array_id as usize].elem;
            let (idx_v, idx_t) = lower_expr(b, lc, index)?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "index is unit".into(),
                    span: index.span,
                }
            })?;
            let idx_i64 = coerce(b, (idx_v, idx_t), JitTy::I64, index.span)?;
            // Bounds check on assignment too. Match the interpreter:
            // `xs[10] = v` on a length-3 array aborts.
            emit_array_bounds_check(b, lc, obj_v, idx_i64);
            let (val, vt) = lower_expr(b, lc, value)?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "assigned value is unit".into(),
                    span: value.span,
                }
            })?;
            let coerced = coerce(b, (val, vt), elem_jty, value.span)?;
            let elem_size = b.ins().iconst(I64, elem_jty.size_bytes() as i64);
            let off = b.ins().imul(idx_i64, elem_size);
            let data = b.ins().load(I64, MemFlags::trusted(), obj_v, ARRAY_DATA_OFFSET);
            let addr = b.ins().iadd(data, off);
            let is_heap =
                elem_jty.is_heap();
            // Read the old element so we can release it after writing the
            // new one. Aliased rhs gets an extra retain; fresh values
            // arrive with rc=1.
            let old_val = if is_heap {
                Some(b.ins().load(
                    elem_jty.cl().expect("non-unit elem"),
                    MemFlags::trusted(),
                    addr,
                    0,
                ))
            } else {
                None
            };
            emit_bind_retain(b, lc, &value.kind, vt, elem_jty, coerced);
            b.ins().store(MemFlags::trusted(), coerced, addr, 0);
            if let Some(old) = old_val {
                emit_release_heap(b, lc, old, elem_jty);
            }
            Ok(None)
        }
        ExprKind::New { class, type_args, args, init_method } => {
            // Built-in `Map<K, V>` — no class layout, just a header
            // alloc. K and V come in via type_args (kept by the
            // monomorphization pass for built-in generics).
            if class == "Map" {
                return lower_new_map(b, lc, type_args, args, e.span);
            }
            if !type_args.is_empty() {
                return Err(CodegenError::Unsupported {
                    what: "generic class instantiation is not yet supported in JIT \
                           (interpreter only)"
                        .into(),
                    span: e.span,
                });
            }
            let class_id = *lc
                .class_layouts
                .iter()
                .enumerate()
                .find(|(_, l)| l.name == *class)
                .map(|(i, _)| i)
                .map(|i| i as u32)
                .as_ref()
                .ok_or_else(|| CodegenError::Unsupported {
                    what: format!("unknown class {class:?}"),
                    span: e.span,
                })?;
            let size = lc.class_layouts[class_id as usize].size as i64;
            let flex_arr_id = lc.class_layouts[class_id as usize].flex_array;
            // Embed the class's drop wrapper (if non-trivial) in the
            // allocation header. The runtime release_object dispatches
            // to it on rc=0 to run user `deinit` and recursively
            // release heap fields.
            let drop_fn_ptr = match lc.class_drops[class_id as usize] {
                Some(fid) => {
                    let func_ref = lc.module.declare_func_in_func(fid, b.func);
                    b.ins().func_addr(I64, func_ref)
                }
                None => b.ins().iconst(I64, 0),
            };
            let alloc_ref =
                lc.module.declare_func_in_func(lc.alloc_object_id, b.func);
            // FAM: `new ClassName(n)` widens the allocation by
            // `n * elem_size` bytes for the trailing flexible array.
            // The single argument is the trailing element count.
            let size_v = if let Some(arr_id) = flex_arr_id {
                if args.len() != 1 {
                    return Err(CodegenError::Unsupported {
                        what: format!(
                            "FAM class {class}: `new {class}(n)` requires exactly \
                             one i64 argument (the trailing element count)"
                        ),
                        span: e.span,
                    });
                }
                let (n_v, n_t) = lower_expr(b, lc, &args[0])?.ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: "FAM count argument is unit".into(),
                        span: args[0].span,
                    }
                })?;
                let n_i64 = coerce(b, (n_v, n_t), JitTy::I64, args[0].span)?;
                let elem_size = lc.array_kinds[arr_id as usize].elem.size_bytes() as i64;
                let trailing = b.ins().imul_imm(n_i64, elem_size);
                let base = b.ins().iconst(I64, size);
                b.ins().iadd(base, trailing)
            } else {
                b.ins().iconst(I64, size)
            };
            // Per-class vtable pointer (or 0 if none was built).
            let vtable_addr = lc
                .class_vtable_addrs
                .get(class_id as usize)
                .copied()
                .unwrap_or(0);
            let vtable_ptr = b.ins().iconst(I64, vtable_addr);
            let alloc_call =
                b.ins().call(alloc_ref, &[size_v, drop_fn_ptr, vtable_ptr]);
            let ptr = b.inst_results(alloc_call)[0];
            // `@extern(C) struct` `str` fields: alloc_object zero-fills user
            // bytes, but a NULL StringRc would crash on any read
            // (length, concat, etc.). Pre-fill each Str slot with an
            // interned empty string (rc saturated → release is a
            // no-op, so no leak from later overwrites or class drop).
            if lc.class_layouts[class_id as usize].is_repr_c {
                let str_offsets: Vec<u32> = lc.class_layouts[class_id as usize]
                    .fields
                    .values()
                    .filter_map(|(off, jty)| matches!(jty, JitTy::Str).then_some(*off))
                    .collect();
                if !str_offsets.is_empty() {
                    let empty = lc.intern_string("");
                    let empty_v = b.ins().iconst(I64, empty);
                    for off in str_offsets {
                        b.ins().store(MemFlags::trusted(), empty_v, ptr, off as i32);
                    }
                }
            }
            // FAM consumed its single arg above; otherwise dispatch
            // to init / reject stray args.
            if flex_arr_id.is_none() {
                // If init exists, call it. The mangler may have set
                // `init_method` to a specific overload (e.g.
                // "init__i64"); fall back to plain "init" otherwise.
                let init_lookup: Symbol = init_method.unwrap_or_else(|| "init".into());
                if lc.class_methods[class_id as usize].contains_key(&init_lookup) {
                    let _ = call_method(b, lc, class_id, init_lookup.as_str(), ptr, args, e.span)?;
                } else if !args.is_empty() {
                    return Err(CodegenError::Unsupported {
                        what: format!("no `init` for class {class}, but args were given"),
                        span: e.span,
                    });
                }
            }
            Ok(Some((ptr, JitTy::Object(class_id))))
        }
        ExprKind::None => {
            // Represent as null pointer. The inner type id doesn't
            // matter for storage (always i64=0), but we need *some*
            // valid optional id so the type tag is well-formed; pick
            // the first interned inner (or intern a dummy Str).
            let id = if lc.optional_inners.is_empty() {
                lc.optional_inners.push(JitTy::Str);
                0
            } else {
                0
            };
            let zero = b.ins().iconst(I64, 0);
            Ok(Some((zero, JitTy::Optional(id))))
        }
        ExprKind::Some(inner) => {
            let (v, vt) = lower_expr(b, lc, inner)?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "some(...) on unit".into(),
                    span: e.span,
                }
            })?;
            let id = intern_optional_inner(lc.optional_inners, vt);
            if vt.is_heap() {
                // Heap inner: pointer is the value; rc was set when
                // the inner was constructed.
                return Ok(Some((v, JitTy::Optional(id))));
            }
            // Primitive inner: heap-box the payload so we have a
            // distinct "0" sentinel for None. Layout: [rc:i64 | payload].
            let size = vt.size_bytes() as i64;
            let size_v = b.ins().iconst(I64, size);
            let new_ref = lc.module.declare_func_in_func(lc.optional_box_new_id, b.func);
            let call = b.ins().call(new_ref, &[size_v]);
            let ptr = b.inst_results(call)[0];
            // Write the payload at ptr + 8 with the inner's natural width.
            let cl_ty = vt.cl().expect("primitive has a cranelift type");
            let _ = cl_ty; // type used implicitly by store
            b.ins().store(
                cranelift::prelude::MemFlags::trusted(),
                v,
                ptr,
                crate::runtime::OPT_PRIM_PAYLOAD_OFFSET,
            );
            Ok(Some((ptr, JitTy::Optional(id))))
        }
        ExprKind::IfLet {
            name,
            expr,
            then_branch,
            else_branch,
        } => lower_if_let(b, lc, name.as_str(), expr, then_branch, else_branch.as_deref()),
    }
}

/// Lower an early `return` (with or without a value). Mirrors what
/// `define_function_body` / `define_main` do at the natural fall-off
/// point: retain the return value if it borrows from a binding,
/// release every heap-typed binding currently in scope (including
/// function params), then emit the cranelift return. Subsequent
/// instructions go into a fresh dead block.
fn lower_return(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    value: Option<&Expr>,
    span: ilang_ast::Span,
) -> Result<Option<TV>, CodegenError> {
    let body = match value {
        Some(e) => {
            let tv = lower_expr(b, lc, e)?;
            if let Some((v, t)) = tv {
                if t.is_heap() && is_aliased_heap_source(&e.kind) {
                    // Hand the caller +1 ownership when borrowing from
                    // a binding we're about to release below.
                    emit_retain_heap(b, lc, v, t);
                }
            }
            tv
        }
        None => None,
    };
    // Coerce to the declared return type before emitting the heap
    // releases — coerce is pure type-shape work, not rc-affecting.
    let ret_ty = lc.current_ret_ty;
    let coerced = match body {
        Some((v, vt)) => Some(coerce(b, (v, vt), ret_ty, span)?),
        None => None,
    };
    // Release every heap-typed binding currently in scope (params +
    // every enclosing let). LIFO by var id matches scope-end release.
    let mut heap: Vec<(Variable, JitTy)> = lc
        .env
        .bindings
        .values()
        .copied()
        .filter(|(_, t)| t.is_heap())
        .collect();
    heap.sort_by_key(|(var, _)| std::cmp::Reverse(var.as_u32()));
    for (var, jty) in heap {
        let p = b.use_var(var);
        emit_release_heap(b, lc, p, jty);
    }
    // Release `this` for methods, except inside `deinit` where the
    // runtime release_object owns the lifecycle.
    if let Some((this_var, class_id)) = lc.this {
        if !lc.current_fn_is_deinit {
            let p = b.use_var(this_var);
            emit_release_object(b, lc, p, class_id);
        }
    }
    // Emit the cranelift return.
    match (ret_ty, coerced) {
        (JitTy::Unit, _) => {
            b.ins().return_(&[]);
        }
        (_, Some(v)) => {
            b.ins().return_(&[v]);
        }
        _ => {
            return Err(CodegenError::Unsupported {
                what: "`return` without a value in a non-unit function".into(),
                span,
            });
        }
    }
    let dead = b.create_block();
    b.switch_to_block(dead);
    b.seal_block(dead);
    Ok(None)
}

/// Phase 1 enum constructor: emit the variant's ordinal as an i32.
/// Phase 2 (payload variants) will allocate a tagged-union heap node;
/// for now non-Unit ctors surface Unsupported.
fn lower_enum_ctor(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    enum_name: &str,
    variant: &str,
    args: &ilang_ast::CtorArgs,
    span: ilang_ast::Span,
) -> Result<Option<TV>, CodegenError> {
    let id = match enum_ids_from(lc).get(&Symbol::intern(enum_name)).copied() {
        Some(id) => id,
        None => {
            return Err(CodegenError::Unsupported {
                what: format!("unknown enum {enum_name:?}"),
                span,
            });
        }
    };
    let layout = lc.enum_layouts[id as usize].clone();
    let idx = layout
        .variants
        .iter()
        .position(|v| v == variant)
        .ok_or_else(|| CodegenError::Unsupported {
            what: format!("enum {enum_name:?} has no variant {variant:?}"),
            span,
        })?;
    let tag = layout.tags[idx];

    if let Some(repr) = layout.flags_repr {
        if !matches!(args, ilang_ast::CtorArgs::Unit) {
            return Err(CodegenError::Unsupported {
                what: format!("variant {enum_name}::{variant} is unit but ctor args supplied"),
                span,
            });
        }
        let cl = repr.cl().expect("flags repr is a numeric type");
        let v = b.ins().iconst(cl, tag);
        return Ok(Some((v, repr)));
    }

    if layout.all_unit {
        if !matches!(args, ilang_ast::CtorArgs::Unit) {
            return Err(CodegenError::Unsupported {
                what: format!("variant {enum_name}::{variant} is unit but ctor args supplied"),
                span,
            });
        }
        let v = b.ins().iconst(I32, tag);
        return Ok(Some((v, JitTy::Enum(id))));
    }

    // Heap-allocated tagged union. user_size = tag area (8) + max
    // payload bytes. drop_fn dispatches on tag to release per-variant
    // heap payload fields (registered later via finalize).
    let user_size = (ENUM_PAYLOAD_OFFSET as i64) + layout.max_payload_size as i64;
    let alloc_ref = lc.module.declare_func_in_func(lc.alloc_object_id, b.func);
    let size_v = b.ins().iconst(I64, user_size);
    let drop_fn_v = enum_drop_fn_ptr(b, lc, id);
    // Enums don't have user-defined methods, so no vtable.
    let zero_vt = b.ins().iconst(I64, 0);
    let alloc_call = b.ins().call(alloc_ref, &[size_v, drop_fn_v, zero_vt]);
    let ptr = b.inst_results(alloc_call)[0];
    // Write tag.
    let tag_v = b.ins().iconst(I32, tag);
    b.ins()
        .store(MemFlags::trusted(), tag_v, ptr, ENUM_TAG_OFFSET);
    // Write payload fields per variant kind.
    let variant_layout = layout.payloads[idx].clone();
    match (&variant_layout, args) {
        (EnumVariantLayout::Unit, ilang_ast::CtorArgs::Unit) => {}
        (EnumVariantLayout::Tuple(entries), ilang_ast::CtorArgs::Tuple(elems)) => {
            for ((offset, fty), arg) in entries.iter().zip(elems.iter()) {
                let (av, at) = lower_expr(b, lc, arg)?.ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: "enum ctor argument is unit".into(),
                        span: arg.span,
                    }
                })?;
                let coerced = coerce(b, (av, at), *fty, arg.span)?;
                emit_bind_retain(b, lc, &arg.kind, at, *fty, coerced);
                let abs_off = ENUM_PAYLOAD_OFFSET + (*offset as i32);
                b.ins().store(MemFlags::trusted(), coerced, ptr, abs_off);
            }
        }
        (EnumVariantLayout::Struct(map), ilang_ast::CtorArgs::Struct(supplied)) => {
            for (fname, expr) in supplied {
                let (offset, fty) = *map.get(fname).ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: format!("unknown field {fname:?}"),
                        span: expr.span,
                    }
                })?;
                let (av, at) = lower_expr(b, lc, &expr)?.ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: "enum ctor field is unit".into(),
                        span: expr.span,
                    }
                })?;
                let coerced = coerce(b, (av, at), fty, expr.span)?;
                emit_bind_retain(b, lc, &expr.kind, at, fty, coerced);
                let abs_off = ENUM_PAYLOAD_OFFSET + (offset as i32);
                b.ins().store(MemFlags::trusted(), coerced, ptr, abs_off);
            }
        }
        _ => {
            return Err(CodegenError::Unsupported {
                what: format!(
                    "ctor shape mismatch for {enum_name}::{variant} — type checker should have caught this"
                ),
                span,
            });
        }
    }
    Ok(Some((ptr, JitTy::EnumHeap(id))))
}

/// Lazily declare/return the per-enum drop wrapper function pointer
/// (or 0 if the enum has no heap-typed payload fields anywhere).
/// Wrapper definition happens after `define_main` (see `crate::drops`).
fn enum_drop_fn_ptr(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    enum_id: u32,
) -> Value {
    // Determine if any variant has any heap-typed payload.
    let layout = &lc.enum_layouts[enum_id as usize];
    let needs_drop = layout.payloads.iter().any(|p| match p {
        EnumVariantLayout::Unit => false,
        EnumVariantLayout::Tuple(entries) => entries.iter().any(|(_, t)| t.is_heap()),
        EnumVariantLayout::Struct(map) => map.values().any(|(_, t)| t.is_heap()),
    });
    if !needs_drop {
        // Cache the negative result so Phase D's define_*_drops doesn't
        // try to define this one.
        lc.enum_drops.entry(enum_id).or_insert(None);
        return b.ins().iconst(I64, 0);
    }
    let id = if let Some(Some(id)) = lc.enum_drops.get(&enum_id) {
        *id
    } else {
        let symbol = format!("__drop_enum_{}", layout.name);
        let mut sig = lc.module.make_signature();
        sig.params.push(AbiParam::new(I64));
        let id = lc
            .module
            .declare_function(&symbol, cranelift_module::Linkage::Local, &sig)
            .expect("declare enum drop");
        lc.enum_drops.insert(enum_id, Some(id));
        id
    };
    let func_ref = lc.module.declare_func_in_func(id, b.func);
    b.ins().func_addr(I64, func_ref)
}

/// Phase 1 match: a cascade of `icmp + brif` on the i32 tag. Each arm
/// jumps to its own block; the merge block joins everyone with a
/// block-param of the unified result type. A trailing wildcard arm
/// becomes the fallthrough.
/// Match dispatch when the scrutinee is a primitive (int / bool /
/// string). Each non-wildcard arm pattern is an `IntLit`,
/// `BoolLit`, or `StrLit` (or a `Variant` named `true` / `false`
/// when the scrutinee is bool — produced by the parser before the
/// type checker's bool rewrite). A wildcard arm is the
/// fallthrough; the type checker guarantees it exists.
fn lower_match_primitive(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    sv: Value,
    st: JitTy,
    arms: &[ilang_ast::MatchArm],
    _span: ilang_ast::Span,
) -> Result<Option<TV>, CodegenError> {
    let arm_blocks: Vec<Block> = arms.iter().map(|_| b.create_block()).collect();
    let merge = b.create_block();
    let mut merge_param: Option<Value> = None;
    let mut result_ty: Option<JitTy> = None;

    let mut wildcard_idx: Option<usize> = None;
    for (i, arm) in arms.iter().enumerate() {
        if matches!(arm.pattern.kind, ilang_ast::PatternKind::Wildcard) {
            wildcard_idx = Some(i);
            break;
        }
        // Build the per-pattern equality test.
        let eq_v = match (&arm.pattern.kind, st) {
            (ilang_ast::PatternKind::IntLit(n), t) if t.is_int() => {
                let cl = t.cl().expect("int has cranelift type");
                let want = b.ins().iconst(cl, *n);
                b.ins().icmp(IntCC::Equal, sv, want)
            }
            (
                ilang_ast::PatternKind::IntRange { low, high, inclusive },
                t,
            ) if t.is_int() => {
                let cl = t.cl().expect("int has cranelift type");
                let low_v = b.ins().iconst(cl, *low);
                let high_v = b.ins().iconst(cl, *high);
                let signed = t.is_signed_int();
                let lo_cc = if signed { IntCC::SignedGreaterThanOrEqual } else { IntCC::UnsignedGreaterThanOrEqual };
                let hi_cc_inc = if signed { IntCC::SignedLessThanOrEqual } else { IntCC::UnsignedLessThanOrEqual };
                let hi_cc_excl = if signed { IntCC::SignedLessThan } else { IntCC::UnsignedLessThan };
                let lo_ok = b.ins().icmp(lo_cc, sv, low_v);
                let hi_ok = b.ins().icmp(
                    if *inclusive { hi_cc_inc } else { hi_cc_excl },
                    sv, high_v,
                );
                b.ins().band(lo_ok, hi_ok)
            }
            (ilang_ast::PatternKind::BoolLit(p), JitTy::Bool) => {
                let want = b.ins().iconst(I8, *p as i64);
                b.ins().icmp(IntCC::Equal, sv, want)
            }
            // `true` / `false` arrive from the parser as Variant
            // patterns — accept them when matching a bool.
            (
                ilang_ast::PatternKind::Variant {
                    variant,
                    bindings: ilang_ast::PatternBindings::Unit,
                    ..
                },
                JitTy::Bool,
            ) if variant == "true" || variant == "false" => {
                let want = b.ins().iconst(I8, if variant == "true" { 1 } else { 0 });
                b.ins().icmp(IntCC::Equal, sv, want)
            }
            (ilang_ast::PatternKind::StrLit(s), JitTy::Str) => {
                let ptr = lc.intern_string(s);
                let str_v = b.ins().iconst(I64, ptr);
                let r = lc.module.declare_func_in_func(lc.strfns.eq, b.func);
                let call = b.ins().call(r, &[sv, str_v]);
                // `str_eq` returns i8 (0 / 1). Use as the cond.
                b.inst_results(call)[0]
            }
            (other_pat, _) => {
                return Err(CodegenError::Unsupported {
                    what: format!(
                        "primitive match: pattern kind {other_pat:?} not supported"
                    ),
                    span: arm.pattern.span,
                });
            }
        };
        let next = b.create_block();
        b.ins().brif(eq_v, arm_blocks[i], &[], next, &[]);
        b.switch_to_block(next);
        b.seal_block(next);
    }
    // Fallthrough: jump to wildcard arm. The type checker guarantees
    // a wildcard arm exists for primitive scrutinees.
    if let Some(w) = wildcard_idx {
        b.ins().jump(arm_blocks[w], &[]);
    } else {
        b.ins().trap(TrapCode::user(1).expect("trap code"));
    }

    // Lower each body, jumping to merge with the produced value.
    for (i, arm) in arms.iter().enumerate() {
        b.switch_to_block(arm_blocks[i]);
        b.seal_block(arm_blocks[i]);
        let body = lower_expr(b, lc, &arm.body)?;
        match body {
            Some((bv, bt)) => {
                if merge_param.is_none() {
                    let cl = bt.cl().unwrap_or(I64);
                    let p = b.append_block_param(merge, cl);
                    merge_param = Some(p);
                    result_ty = Some(bt);
                }
                b.ins().jump(merge, &[bv.into()]);
            }
            None => {
                b.ins().jump(merge, &[]);
            }
        }
    }
    b.switch_to_block(merge);
    b.seal_block(merge);
    Ok(match (merge_param, result_ty) {
        (Some(p), Some(t)) => Some((p, t)),
        _ => None,
    })
}

fn lower_match(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    scrutinee: &Expr,
    arms: &[ilang_ast::MatchArm],
    span: ilang_ast::Span,
) -> Result<Option<TV>, CodegenError> {
    let (sv, st) = lower_expr(b, lc, scrutinee)?.ok_or_else(|| CodegenError::Unsupported {
        what: "match scrutinee is unit".into(),
        span: scrutinee.span,
    })?;
    // Primitive scrutinee (int / bool / string) — dispatch on
    // equality. Each arm's pattern is an `IntLit` / `BoolLit` /
    // `StrLit` (or `_`); the type checker has already validated.
    if matches!(st, JitTy::Bool) || st.is_int() || matches!(st, JitTy::Str) {
        return lower_match_primitive(b, lc, sv, st, arms, span);
    }
    let (enum_id, is_heap) = match st {
        JitTy::Enum(id) => (id, false),
        JitTy::EnumHeap(id) => (id, true),
        other => {
            return Err(CodegenError::Unsupported {
                what: format!("match on non-enum {other:?}"),
                span: scrutinee.span,
            });
        }
    };
    let layout = lc.enum_layouts[enum_id as usize].clone();
    // The tag is either the value itself (unit-only enum) or loaded
    // from the heap object's tag slot.
    let tag_v = if is_heap {
        b.ins().load(I32, MemFlags::trusted(), sv, ENUM_TAG_OFFSET)
    } else {
        sv
    };
    let _ = span;

    // Pre-create one body block per arm + a merge block for joining.
    // We won't seal the merge until every arm has jumped to it.
    let arm_blocks: Vec<Block> = arms.iter().map(|_| b.create_block()).collect();
    let merge = b.create_block();
    // We don't know the result Cranelift type until we lower the first
    // body; capture it lazily.
    let mut merge_param: Option<Value> = None;
    let mut result_ty: Option<JitTy> = None;

    // Dispatch chain. For each non-wildcard arm: compare tag, branch to
    // its body or fall through to the next compare. A wildcard arm
    // becomes the fallthrough body. If there's no wildcard the type
    // checker has guaranteed exhaustiveness, so the final compare's
    // false-branch goes to a `trap` (unreachable in well-typed code).
    let mut wildcard_idx: Option<usize> = None;
    for (i, arm) in arms.iter().enumerate() {
        if matches!(arm.pattern.kind, ilang_ast::PatternKind::Wildcard) {
            wildcard_idx = Some(i);
            break;
        }
        let variant_name = match &arm.pattern.kind {
            ilang_ast::PatternKind::Variant { variant, .. } => variant.clone(),
            _ => unreachable!(),
        };
        let idx = layout
            .variants
            .iter()
            .position(|v| *v == variant_name)
            .expect("type checker validated variant");
        let want = b.ins().iconst(I32, layout.tags[idx]);
        let cond = b.ins().icmp(IntCC::Equal, tag_v, want);
        let next = b.create_block();
        b.ins().brif(cond, arm_blocks[i], &[], next, &[]);
        b.switch_to_block(next);
        b.seal_block(next);
    }
    // After the chain: the current block is the fallthrough. Either
    // jump to the wildcard's body or trap (exhaustiveness guarantees
    // unreachability for well-typed unit enums).
    if let Some(w) = wildcard_idx {
        b.ins().jump(arm_blocks[w], &[]);
    } else {
        b.ins().trap(TrapCode::user(1).expect("trap code"));
    }

    // Lower each body in its own block, jumping to merge with the
    // produced value (or no value for Unit). For heap-payload enums,
    // each arm binds the variant's payload fields (loaded from the
    // scrutinee's payload area) into the env, then runs the body,
    // then restores the env.
    for (i, arm) in arms.iter().enumerate() {
        b.switch_to_block(arm_blocks[i]);
        b.seal_block(arm_blocks[i]);
        // Payload bindings (only when the scrutinee is a heap enum
        // and the pattern is a Variant with bindings).
        let mut shadows: Vec<(Symbol, Option<(Variable, JitTy)>)> = Vec::new();
        if is_heap {
            if let ilang_ast::PatternKind::Variant {
                variant: pvar,
                bindings,
                ..
            } = &arm.pattern.kind
            {
                if let Some(idx) = layout.variants.iter().position(|v| v == pvar) {
                    let vlayout = layout.payloads[idx].clone();
                    match (&vlayout, bindings) {
                        (EnumVariantLayout::Unit, ilang_ast::PatternBindings::Unit) => {}
                        (
                            EnumVariantLayout::Tuple(entries),
                            ilang_ast::PatternBindings::Tuple(names),
                        ) => {
                            for ((offset, fty), n) in entries.iter().zip(names.iter()) {
                                if n == "_" {
                                    continue;
                                }
                                let cl = fty.cl().expect("non-unit payload field");
                                let abs = ENUM_PAYLOAD_OFFSET + (*offset as i32);
                                let v = b.ins().load(cl, MemFlags::trusted(), sv, abs);
                                let var = b.declare_var(cl);
                                b.def_var(var, v);
                                if fty.is_heap() {
                                    crate::arc::emit_retain_heap(b, lc, v, *fty);
                                }
                                let prev = lc.env.bindings.insert(n.clone(), (var, *fty));
                                shadows.push((n.clone(), prev));
                            }
                        }
                        (
                            EnumVariantLayout::Struct(map),
                            ilang_ast::PatternBindings::Struct(pairs),
                        ) => {
                            for (fname, bname) in pairs {
                                if bname == "_" {
                                    continue;
                                }
                                if let Some((offset, fty)) = map.get(fname).copied() {
                                    let cl = fty.cl().expect("non-unit payload field");
                                    let abs = ENUM_PAYLOAD_OFFSET + (offset as i32);
                                    let v = b.ins().load(cl, MemFlags::trusted(), sv, abs);
                                    let var = b.declare_var(cl);
                                    b.def_var(var, v);
                                    if fty.is_heap() {
                                        crate::arc::emit_retain_heap(b, lc, v, fty);
                                    }
                                    let prev =
                                        lc.env.bindings.insert(bname.clone(), (var, fty));
                                    shadows.push((bname.clone(), prev));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        let body_tv = lower_expr(b, lc, &arm.body)?;
        // Release any heap bindings introduced for this arm's pattern,
        // then restore the env.
        for (n, prev) in shadows.into_iter().rev() {
            if let Some((var, jty)) = lc.env.bindings.remove(&n) {
                if jty.is_heap() {
                    let p = b.use_var(var);
                    emit_release_heap(b, lc, p, jty);
                }
            }
            if let Some(p) = prev {
                lc.env.bindings.insert(n, p);
            }
        }
        match (body_tv, result_ty) {
            (Some((v, vt)), None) => {
                let cl = vt.cl().ok_or_else(|| CodegenError::Unsupported {
                    what: "match arm produces unit while merge expects a value".into(),
                    span: arm.span,
                })?;
                merge_param = Some(b.append_block_param(merge, cl));
                result_ty = Some(vt);
                b.ins().jump(merge, &[v.into()]);
            }
            (Some((v, vt)), Some(prev_ty)) => {
                let v = coerce(b, (v, vt), prev_ty, arm.span)?;
                b.ins().jump(merge, &[v.into()]);
            }
            (None, _) => {
                b.ins().jump(merge, &[]);
            }
        }
    }

    b.switch_to_block(merge);
    b.seal_block(merge);
    Ok(merge_param.zip(result_ty).map(|(v, t)| (v, t)))
}

fn lower_if_let(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    name: &str,
    scrut: &Expr,
    then_branch: &ilang_ast::Block,
    else_branch: Option<&Expr>,
) -> Result<Option<TV>, CodegenError> {
    let (scrut_v, scrut_t) = lower_expr(b, lc, scrut)?.ok_or_else(|| CodegenError::Unsupported {
        what: "if-let scrutinee is unit".into(),
        span: scrut.span,
    })?;
    let inner_id = match scrut_t {
        JitTy::Optional(id) => id,
        other => {
            return Err(CodegenError::Unsupported {
                what: format!("if-let on non-optional {other:?}"),
                span: scrut.span,
            });
        }
    };
    let inner_jty = lc.optional_inners[inner_id as usize];

    // The scrutinee carries +1 ownership iff it's a fresh heap
    // producer (call result, `new`, etc.). When that's the case we
    // must release it after the if-let merges, otherwise the
    // allocation leaks (Optional<heap> with no surviving binding).
    // Borrowed scrutinees (Var/Field/Index/This) read someone
    // else's slot and own no rc — releasing would over-free.
    let release_scrut = scrut_t.is_heap()
        && !crate::arc::is_aliased_heap_source(&scrut.kind);

    let then_block = b.create_block();
    let else_block = b.create_block();

    let zero = b.ins().iconst(I64, 0);
    let cond = b.ins().icmp(IntCC::NotEqual, scrut_v, zero);
    b.ins().brif(cond, then_block, &[], else_block, &[]);

    // Then branch: bind `name` to the unwrapped value.
    //   heap inner: scrut_v IS the pointer; retain so the binding owns
    //               its own +1, then release at then-branch exit.
    //   primitive inner: scrut_v is a Box<[rc | payload]>; load the
    //                    payload at OPT_PRIM_PAYLOAD_OFFSET. No ARC on
    //                    the binding (it's a primitive copy).
    b.switch_to_block(then_block);
    b.seal_block(then_block);
    let cl_ty = inner_jty.cl().expect("non-unit inner");
    let var = b.declare_var(cl_ty);
    if inner_jty.is_heap() {
        b.def_var(var, scrut_v);
        crate::arc::emit_retain_heap(b, lc, scrut_v, inner_jty);
    } else {
        let payload = b.ins().load(
            cl_ty,
            cranelift::prelude::MemFlags::trusted(),
            scrut_v,
            crate::runtime::OPT_PRIM_PAYLOAD_OFFSET,
        );
        b.def_var(var, payload);
    }
    let prev = lc.env.bindings.insert(Symbol::intern(name), (var, inner_jty));
    let then_val = lower_block_value(b, lc, then_branch)?;
    // Restore the prior binding.
    match prev {
        Some(p) => {
            lc.env.bindings.insert(Symbol::intern(name), p);
        }
        None => {
            lc.env.bindings.remove(&Symbol::intern(name));
        }
    }
    // Release the +1 we took on entry. lower_block_value already
    // retained the tail when it borrows from the binding (so the
    // returned Value carries its own rc), so this release just
    // discards the binding-local copy without disturbing the merge
    // result.
    if inner_jty.is_heap() {
        let p = b.use_var(var);
        crate::arc::emit_release_heap(b, lc, p, inner_jty);
    }

    // Merge block: gather a value from both branches if the type is
    // non-unit. Mirrors lower_if.
    let merge = b.create_block();
    let merge_param = match then_val {
        Some((v, _)) => Some(b.append_block_param(merge, b.func.dfg.value_type(v))),
        None => None,
    };
    if let Some((v, _)) = then_val {
        b.ins().jump(merge, &[v.into()]);
    } else {
        b.ins().jump(merge, &[]);
    }

    b.switch_to_block(else_block);
    b.seal_block(else_block);
    let else_val = match else_branch {
        Some(e) => lower_expr(b, lc, e)?,
        None => None,
    };
    let result = match (then_val, else_val) {
        (Some((_, tt)), Some((ev, _et))) => {
            let ev_coerced = coerce(b, (ev, _et), tt, scrut.span)?;
            b.ins().jump(merge, &[ev_coerced.into()]);
            b.switch_to_block(merge);
            b.seal_block(merge);
            Ok(merge_param.map(|p| (p, tt)))
        }
        (Some((_, tt)), None) => {
            let zero = match tt.cl() {
                Some(t) if t.is_float() => b.ins().f64const(0.0),
                Some(t) => b.ins().iconst(t, 0),
                None => unreachable!(),
            };
            b.ins().jump(merge, &[zero.into()]);
            b.switch_to_block(merge);
            b.seal_block(merge);
            Ok(merge_param.map(|p| (p, tt)))
        }
        (None, _) => {
            b.ins().jump(merge, &[]);
            b.switch_to_block(merge);
            b.seal_block(merge);
            Ok(None)
        }
    };
    // Drop the scrutinee's +1 now that both branches have rejoined.
    // emit_release_heap on Optional<heap> is null-safe — the else
    // path's NULL pointer flows through fine.
    if release_scrut {
        crate::arc::emit_release_heap(b, lc, scrut_v, scrut_t);
    }
    result
}

/// Lower an array literal forcing each element to `target_elem_jty`.
/// Used by `let a: T[] = [...]` so the runtime layout matches the
/// declared element width even when the literal would naturally pick
/// a different (wider) type.
pub(crate) fn lower_array_literal(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    elements: &[Expr],
    target_elem_jty: JitTy,
    span: ilang_ast::Span,
) -> Result<TV, CodegenError> {
    let mut lowered: Vec<(Value, JitTy, ilang_ast::Span)> =
        Vec::with_capacity(elements.len());
    for el in elements {
        let (v, t) = lower_expr(b, lc, el)?.ok_or_else(|| CodegenError::Unsupported {
            what: "array element is unit".into(),
            span: el.span,
        })?;
        lowered.push((v, t, el.span));
    }
    let tv = build_array(b, lc, lowered, target_elem_jty)?;
    tv.ok_or_else(|| CodegenError::Unsupported {
        what: "array literal produced no value".into(),
        span,
    })
}

/// Allocate the header + buffer and store every (already-lowered)
/// element, coercing to `elem_jty`. Returns `(header_ptr, Array(id))`.
fn build_array(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    lowered: Vec<(Value, JitTy, ilang_ast::Span)>,
    elem_jty: JitTy,
) -> Result<Option<TV>, CodegenError> {
    let array_id = intern_array_kind(
        lc.array_kinds,
        ArrayKind {
            elem: elem_jty,
            fixed: Some(lowered.len() as u32),
        },
    );
    let new_ref = lc.module.declare_func_in_func(lc.arrfns.new, b.func);
    let elem_size = b.ins().iconst(I64, elem_jty.size_bytes() as i64);
    let len = b.ins().iconst(I64, lowered.len() as i64);
    let drop_fn_v = crate::drops::array_drop_fn_ptr(b, lc, array_id);
    let call = b.ins().call(new_ref, &[elem_size, len, drop_fn_v]);
    let header = b.inst_results(call)[0];
    let data = b.ins().load(I64, MemFlags::trusted(), header, ARRAY_DATA_OFFSET);
    let elem_size_i32 = elem_jty.size_bytes() as i32;
    for (i, (v, t, sp)) in lowered.into_iter().enumerate() {
        let coerced = coerce(b, (v, t), elem_jty, sp)?;
        let offset = (i as i32) * elem_size_i32;
        b.ins().store(MemFlags::trusted(), coerced, data, offset);
    }
    Ok(Some((header, JitTy::Array(array_id))))
}

/// Lower a `console.log(a, b, c, ...)` call: dispatch each argument to
/// the FFI print function for its type, separated by spaces, with a
/// trailing newline. Object args are unsupported for now and surface a
/// clear error.
fn lower_console_log(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    args: &[Expr],
) -> Result<(), CodegenError> {
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            let r = lc.module.declare_func_in_func(lc.print.space, b.func);
            b.ins().call(r, &[]);
        }
        let (av, at) = lower_expr(b, lc, a)?.ok_or_else(|| CodegenError::Unsupported {
            what: "console.log argument is unit".into(),
            span: a.span,
        })?;
        emit_print_value(b, lc, av, at, a.span)?;
    }
    let r = lc.module.declare_func_in_func(lc.print.newline, b.func);
    b.ins().call(r, &[]);
    Ok(())
}

/// Emit code that prints a single value of static type `ty`. Mirrors
/// `JitValue`'s `Display` impl so `console.log` output matches the
/// interpreter's. Recurses through Array, Optional, and (shallowly)
/// Object so nested structures format the same way.
fn emit_print_value(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    v: Value,
    ty: JitTy,
    span: ilang_ast::Span,
) -> Result<(), CodegenError> {
    match ty {
        JitTy::I8 | JitTy::I16 | JitTy::I32 | JitTy::I64 => {
            let v = coerce(b, (v, ty), JitTy::I64, span)?;
            let r = lc.module.declare_func_in_func(lc.print.i64, b.func);
            b.ins().call(r, &[v]);
        }
        JitTy::U8 | JitTy::U16 | JitTy::U32 | JitTy::U64 => {
            let v = coerce(b, (v, ty), JitTy::U64, span)?;
            let r = lc.module.declare_func_in_func(lc.print.u64, b.func);
            b.ins().call(r, &[v]);
        }
        JitTy::F32 => {
            let r = lc.module.declare_func_in_func(lc.print.f32, b.func);
            b.ins().call(r, &[v]);
        }
        JitTy::F64 => {
            let r = lc.module.declare_func_in_func(lc.print.f64, b.func);
            b.ins().call(r, &[v]);
        }
        JitTy::Bool => {
            let r = lc.module.declare_func_in_func(lc.print.bool, b.func);
            b.ins().call(r, &[v]);
        }
        JitTy::Str => {
            let r = lc.module.declare_func_in_func(lc.print.str, b.func);
            b.ins().call(r, &[v]);
        }
        JitTy::Object(class_id) => {
            emit_print_object(b, lc, v, class_id, span)?;
        }
        JitTy::Array(id) => {
            let elem_jty = lc.array_kinds[id as usize].elem;
            emit_print_array(b, lc, v, elem_jty, span)?;
        }
        JitTy::Optional(inner_id) => {
            let inner = lc.optional_inners[inner_id as usize];
            emit_print_optional(b, lc, v, inner, span)?;
        }
        JitTy::Weak(class_id) => {
            // `<weak ClassName alive>` / `<weak ClassName dead>`. The
            // strong_rc lives at offset -24 from the user pointer.
            let class_name = lc.class_layouts[class_id as usize].name.as_str().to_string();
            // Branch on (ptr != 0 && strong_rc > 0).
            let zero = b.ins().iconst(I64, 0);
            let alive_block = b.create_block();
            let dead_block = b.create_block();
            let merge = b.create_block();
            let nonnull = b.ins().icmp(IntCC::NotEqual, v, zero);
            let check_block = b.create_block();
            b.ins().brif(nonnull, check_block, &[], dead_block, &[]);
            b.switch_to_block(check_block);
            b.seal_block(check_block);
            let strong = b.ins().load(I64, MemFlags::trusted(), v, -24);
            let alive_cond = b.ins().icmp(IntCC::SignedGreaterThan, strong, zero);
            b.ins().brif(alive_cond, alive_block, &[], dead_block, &[]);

            b.switch_to_block(alive_block);
            b.seal_block(alive_block);
            emit_print_literal(b, lc, &format!("<weak {class_name} alive>"));
            b.ins().jump(merge, &[]);

            b.switch_to_block(dead_block);
            b.seal_block(dead_block);
            emit_print_literal(b, lc, &format!("<weak {class_name} dead>"));
            b.ins().jump(merge, &[]);

            b.switch_to_block(merge);
            b.seal_block(merge);
        }
        JitTy::Enum(id) => {
            // Branch on the i32 tag and print `EnumName::Variant`.
            // Phase 1 enums have no payload to recurse into.
            let layout = lc.enum_layouts[id as usize].clone();
            let merge = b.create_block();
            for (i, variant) in layout.variants.iter().enumerate() {
                let tag = b.ins().iconst(I32, i as i64);
                let cond = b.ins().icmp(IntCC::Equal, v, tag);
                let body_block = b.create_block();
                let next_block = b.create_block();
                b.ins().brif(cond, body_block, &[], next_block, &[]);
                b.switch_to_block(body_block);
                b.seal_block(body_block);
                emit_print_literal(b, lc, &format!("{}::{variant}", layout.name));
                b.ins().jump(merge, &[]);
                b.switch_to_block(next_block);
                b.seal_block(next_block);
            }
            // Fallthrough (well-typed code never reaches here): print
            // a marker and join to merge.
            emit_print_literal(b, lc, &format!("{}::?", layout.name));
            b.ins().jump(merge, &[]);
            b.switch_to_block(merge);
            b.seal_block(merge);
        }
        JitTy::EnumHeap(id) => {
            // Heap-allocated tagged union. Load the tag, then per
            // variant print `EnumName::Variant` and (if applicable)
            // recurse into payload fields.
            let layout = lc.enum_layouts[id as usize].clone();
            let tag_v = b.ins().load(I32, MemFlags::trusted(), v, ENUM_TAG_OFFSET);
            let merge = b.create_block();
            for (i, vname) in layout.variants.iter().enumerate() {
                let want = b.ins().iconst(I32, i as i64);
                let cond = b.ins().icmp(IntCC::Equal, tag_v, want);
                let body_block = b.create_block();
                let next_block = b.create_block();
                b.ins().brif(cond, body_block, &[], next_block, &[]);
                b.switch_to_block(body_block);
                b.seal_block(body_block);
                let prefix = format!("{}::{vname}", layout.name);
                let vlayout = layout.payloads[i].clone();
                match &vlayout {
                    EnumVariantLayout::Unit => {
                        emit_print_literal(b, lc, &prefix);
                    }
                    EnumVariantLayout::Tuple(entries) => {
                        emit_print_literal(b, lc, &format!("{prefix}("));
                        for (k, (off, fty)) in entries.iter().enumerate() {
                            if k > 0 {
                                emit_print_literal(b, lc, ", ");
                            }
                            let cl = fty.cl().expect("non-unit field");
                            let abs = ENUM_PAYLOAD_OFFSET + (*off as i32);
                            let fv = b.ins().load(cl, MemFlags::trusted(), v, abs);
                            emit_print_value(b, lc, fv, *fty, span)?;
                        }
                        emit_print_literal(b, lc, ")");
                    }
                    EnumVariantLayout::Struct(map) => {
                        emit_print_literal(b, lc, &format!("{prefix} {{ "));
                        let mut sorted: Vec<(&Symbol, &(u32, JitTy))> = map.iter().collect();
                        sorted.sort_by(|a, b| a.0.cmp(b.0));
                        for (k, (name, (off, fty))) in sorted.into_iter().enumerate() {
                            if k > 0 {
                                emit_print_literal(b, lc, ", ");
                            }
                            emit_print_literal(b, lc, &format!("{name}: "));
                            let cl = fty.cl().expect("non-unit field");
                            let abs = ENUM_PAYLOAD_OFFSET + (*off as i32);
                            let fv = b.ins().load(cl, MemFlags::trusted(), v, abs);
                            emit_print_value(b, lc, fv, *fty, span)?;
                        }
                        emit_print_literal(b, lc, " }");
                    }
                }
                b.ins().jump(merge, &[]);
                b.switch_to_block(next_block);
                b.seal_block(next_block);
            }
            emit_print_literal(b, lc, &format!("{}::?", layout.name));
            b.ins().jump(merge, &[]);
            b.switch_to_block(merge);
            b.seal_block(merge);
        }
        JitTy::Fn(_) => {
            // Print as `<fn @ 0x...>`. Reuse the i64 printer for the
            // hex-ish address — good enough for debugging, no need
            // for a dedicated formatter.
            let r = lc.module.declare_func_in_func(lc.print.i64, b.func);
            b.ins().call(r, &[v]);
        }
        JitTy::Map(_) => {
            // Same approach as Fn: print the pointer for now. A proper
            // {key: value, ...} formatter is out of scope for Phase A.
            let r = lc.module.declare_func_in_func(lc.print.i64, b.func);
            b.ins().call(r, &[v]);
        }
        JitTy::Tuple(tuple_id) => {
            emit_print_tuple(b, lc, v, tuple_id, span)?;
        }
        JitTy::Unit => {
            return Err(CodegenError::Unsupported {
                what: "console.log of () (unit)".into(),
                span,
            });
        }
        JitTy::TypeRef => {
            // Print as `Type(<name>)` to match the interpreter.
            // Dedicated helper avoids needing static prefix/suffix
            // strings here.
            let r = lc.module.declare_func_in_func(lc.print.type_ref, b.func);
            b.ins().call(r, &[v]);
        }
        JitTy::EmbeddedArray(_) | JitTy::FlexArray(_) => {
            // Embedded / flex arrays only flow through `Index`
            // access; a bare `outer.arr` value reaching this print
            // path means the user used the field as a value, which
            // we don't support yet.
            return Err(CodegenError::Unsupported {
                what: "printing an embedded `T[N]` / FAM field directly is not supported \
                       — index it (e.g. `arr[0]`) instead"
                    .into(),
                span,
            });
        }
    }
    Ok(())
}

/// Print a static string literal by interning it and routing through
/// `print_str`. Cheap — each unique fragment is interned once.
/// Emit a call to a `@extern("...", variadic)` fn. The declared
/// fixed prefix flows through registers per the host C ABI; the
/// trailing args are passed by their actual JIT-time types.
///
/// On Apple AArch64 the platform variadic ABI requires *all*
/// trailing args to live on the stack regardless of type, but
/// Cranelift's calling convention puts them in X0–X7 / V0–V7
/// like fixed args. To spill them to the stack we pad the
/// signature with dummy I64/F64 args that fill the remaining
/// register slots, so Cranelift's reg allocation runs out and
/// stores the real variadic args on the stack — exactly where
/// the C function's `va_list` reads from. Linux x86_64 (System V)
/// passes variadic args in registers anyway, so the padding is
/// skipped there.
fn build_variadic_call(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    func_ref: cranelift::prelude::codegen::ir::FuncRef,
    arg_vals: &[cranelift::prelude::Value],
    n_fixed: usize,
    ret_ty: JitTy,
) -> cranelift::prelude::codegen::ir::Inst {
    use cranelift::prelude::AbiParam;
    let needs_apple_pad =
        cfg!(target_os = "macos") && cfg!(target_arch = "aarch64");
    let mut cl_sig = lc.module.make_signature();
    let fixed = &arg_vals[..n_fixed];
    let varargs = &arg_vals[n_fixed..];
    for v in fixed {
        cl_sig
            .params
            .push(AbiParam::new(b.func.dfg.value_type(*v)));
    }
    let mut padded: Vec<cranelift::prelude::Value> = fixed.to_vec();
    if needs_apple_pad && !varargs.is_empty() {
        let n_int_fixed = fixed
            .iter()
            .filter(|v| b.func.dfg.value_type(**v).is_int())
            .count();
        let n_fp_fixed = fixed
            .iter()
            .filter(|v| b.func.dfg.value_type(**v).is_float())
            .count();
        let n_int_pad = 8usize.saturating_sub(n_int_fixed);
        let n_fp_pad = 8usize.saturating_sub(n_fp_fixed);
        for _ in 0..n_int_pad {
            cl_sig.params.push(AbiParam::new(I64));
        }
        for _ in 0..n_fp_pad {
            cl_sig.params.push(AbiParam::new(F64));
        }
        let zero_i = b.ins().iconst(I64, 0);
        let zero_f = b.ins().f64const(0.0);
        for _ in 0..n_int_pad {
            padded.push(zero_i);
        }
        for _ in 0..n_fp_pad {
            padded.push(zero_f);
        }
    }
    for v in varargs {
        cl_sig
            .params
            .push(AbiParam::new(b.func.dfg.value_type(*v)));
        padded.push(*v);
    }
    if let Some(rt) = ret_ty.cl() {
        cl_sig.returns.push(AbiParam::new(rt));
    }
    let sig_ref = b.import_signature(cl_sig);
    let func_addr = b.ins().func_addr(I64, func_ref);
    b.ins().call_indirect(sig_ref, func_addr, &padded)
}

/// True when `ty` is `Object(class_id)` for a managed opaque-handle
/// class — `@extern("lib") class Foo { deinit { ... } }`. These
/// values are wrapped in an ARC box at the native-extern boundary.
fn is_managed_opaque(lc: &LowerCtx, ty: JitTy) -> bool {
    match ty {
        JitTy::Object(class_id) => {
            lc.class_layouts[class_id as usize].extern_lib.is_some()
                && lc.class_methods[class_id as usize].contains_key(&"deinit".into())
        }
        _ => false,
    }
}

/// True when `ty` is `Optional<Object(managed-opaque)>`.
fn is_managed_opaque_optional(lc: &LowerCtx, ty: JitTy) -> bool {
    matches!(ty, JitTy::Optional(id) if is_managed_opaque(lc, lc.optional_inners[id as usize]))
}

fn opaque_class_id_of(ty: JitTy) -> u32 {
    match ty {
        JitTy::Object(class_id) => class_id,
        _ => unreachable!("caller checked is_managed_opaque"),
    }
}

fn managed_opaque_optional_class_id(lc: &LowerCtx, ty: JitTy) -> Option<u32> {
    match ty {
        JitTy::Optional(id) => match lc.optional_inners[id as usize] {
            JitTy::Object(class_id) => Some(class_id),
            _ => None,
        },
        _ => None,
    }
}

/// Allocate an ilang ARC box for a managed opaque class, store the
/// raw C pointer at offset 0, return the box's user pointer. The
/// `drop_fn_ptr` slot is filled with the class's `__drop_<name>`
/// wrapper so the deinit body fires when the last reference dies.
fn wrap_managed_opaque(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    c_ptr: cranelift::prelude::Value,
    class_id: u32,
) -> cranelift::prelude::Value {
    let size_v = b.ins().iconst(I64, lc.class_layouts[class_id as usize].size as i64);
    let drop_fn_ptr = match lc.class_drops[class_id as usize] {
        Some(fid) => {
            let func_ref = lc.module.declare_func_in_func(fid, b.func);
            b.ins().func_addr(I64, func_ref)
        }
        None => b.ins().iconst(I64, 0),
    };
    let vtable_addr = lc
        .class_vtable_addrs
        .get(class_id as usize)
        .copied()
        .unwrap_or(0);
    let vtable_ptr = b.ins().iconst(I64, vtable_addr);
    let alloc_ref = lc.module.declare_func_in_func(lc.alloc_object_id, b.func);
    let call = b.ins().call(alloc_ref, &[size_v, drop_fn_ptr, vtable_ptr]);
    let box_ptr = b.inst_results(call)[0];
    b.ins().store(MemFlags::trusted(), c_ptr, box_ptr, 0);
    box_ptr
}

fn emit_print_literal(b: &mut FunctionBuilder, lc: &mut LowerCtx, s: &str) {
    let ptr = lc.intern_string(s);
    let v = b.ins().iconst(I64, ptr);
    let r = lc.module.declare_func_in_func(lc.print.str, b.func);
    b.ins().call(r, &[v]);
}

/// Emit `ClassName { f1: v1, f2: v2 }` for an object — matches the
/// interpreter's `JitValue` / `Value::Object` Display. Fields are
/// printed in alphabetical order so the output is stable. An object
/// with no fields prints as `ClassName {}`.
fn emit_print_object(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    obj: Value,
    class_id: u32,
    span: ilang_ast::Span,
) -> Result<(), CodegenError> {
    let class_name = lc.class_layouts[class_id as usize].name.as_str().to_string();
    // Snapshot the field list so we don't borrow `lc.class_layouts`
    // through the recursive emit_print_value call below.
    let mut fields: Vec<(Symbol, u32, JitTy)> = lc.class_layouts[class_id as usize]
        .fields
        .iter()
        .map(|(name, &(offset, fty))| (*name, offset, fty))
        .collect();
    fields.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));

    if fields.is_empty() {
        emit_print_literal(b, lc, &format!("{class_name} {{}}"));
        return Ok(());
    }
    emit_print_literal(b, lc, &format!("{class_name} {{ "));
    for (i, (fname, offset, fty)) in fields.into_iter().enumerate() {
        if i > 0 {
            emit_print_literal(b, lc, ", ");
        }
        emit_print_literal(b, lc, &format!("{fname}: "));
        let cl_ty = fty.cl().expect("non-unit field");
        let fv = b.ins().load(cl_ty, MemFlags::trusted(), obj, offset as i32);
        emit_print_value(b, lc, fv, fty, span)?;
    }
    emit_print_literal(b, lc, " }");
    Ok(())
}

/// Emit `[e0, e1, e2]` for an array. The element-printing branch can
/// recursively invoke `emit_print_value`, so nested arrays / arrays of
/// objects / arrays of optionals all format correctly.
fn emit_print_array(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    header: Value,
    elem_jty: JitTy,
    span: ilang_ast::Span,
) -> Result<(), CodegenError> {
    emit_print_literal(b, lc, "[");
    // Walk the runtime header to read len + data_ptr.
    let len = b.ins().load(I64, MemFlags::trusted(), header, ARRAY_LEN_OFFSET);
    let data = b.ins().load(I64, MemFlags::trusted(), header, ARRAY_DATA_OFFSET);
    let elem_size_const = elem_jty.size_bytes() as i64;

    // Loop: i = 0; while i < len { if i > 0: print ", "; print elem[i]; i++ }
    let i_var = b.declare_var(I64);
    let zero = b.ins().iconst(I64, 0);
    b.def_var(i_var, zero);

    let header_block = b.create_block();
    let body_block = b.create_block();
    let after_block = b.create_block();

    b.ins().jump(header_block, &[]);
    b.switch_to_block(header_block);
    let i = b.use_var(i_var);
    let cond = b.ins().icmp(IntCC::SignedLessThan, i, len);
    b.ins().brif(cond, body_block, &[], after_block, &[]);

    b.switch_to_block(body_block);
    b.seal_block(body_block);
    let i = b.use_var(i_var);
    // Comma separator before every element except the first.
    let zero = b.ins().iconst(I64, 0);
    let need_comma = b.ins().icmp(IntCC::SignedGreaterThan, i, zero);
    let comma_block = b.create_block();
    let no_comma_block = b.create_block();
    b.ins().brif(need_comma, comma_block, &[], no_comma_block, &[]);
    b.switch_to_block(comma_block);
    b.seal_block(comma_block);
    emit_print_literal(b, lc, ", ");
    b.ins().jump(no_comma_block, &[]);
    b.switch_to_block(no_comma_block);
    b.seal_block(no_comma_block);

    let size_v = b.ins().iconst(I64, elem_size_const);
    let off = b.ins().imul(i, size_v);
    let addr = b.ins().iadd(data, off);
    let elem = b.ins().load(
        elem_jty.cl().expect("non-unit elem"),
        MemFlags::trusted(),
        addr,
        0,
    );
    emit_print_value(b, lc, elem, elem_jty, span)?;

    let one = b.ins().iconst(I64, 1);
    let new_i = b.ins().iadd(i, one);
    b.def_var(i_var, new_i);
    b.ins().jump(header_block, &[]);
    b.seal_block(header_block);

    b.switch_to_block(after_block);
    b.seal_block(after_block);
    emit_print_literal(b, lc, "]");
    Ok(())
}

/// Emit `(e0, e1, ...)` for a tuple. Each element loads from its
/// statically-known offset and recursively prints.
fn emit_print_tuple(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    ptr: Value,
    tuple_id: u32,
    span: ilang_ast::Span,
) -> Result<(), CodegenError> {
    let kind = lc.tuple_kinds[tuple_id as usize].clone();
    emit_print_literal(b, lc, "(");
    for (i, &elem_ty) in kind.elems.iter().enumerate() {
        if i > 0 {
            emit_print_literal(b, lc, ", ");
        }
        let cl = elem_ty.cl().expect("tuple element is non-unit");
        let off = kind.offsets[i] as i32;
        let v = b.ins().load(cl, MemFlags::trusted(), ptr, off);
        emit_print_value(b, lc, v, elem_ty, span)?;
    }
    emit_print_literal(b, lc, ")");
    Ok(())
}

/// Emit `none` or `some(<value>)` depending on the runtime null check.
fn emit_print_optional(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    v: Value,
    inner: JitTy,
    span: ilang_ast::Span,
) -> Result<(), CodegenError> {
    let zero = b.ins().iconst(I64, 0);
    let is_some = b.ins().icmp(IntCC::NotEqual, v, zero);
    let some_block = b.create_block();
    let none_block = b.create_block();
    let merge = b.create_block();
    b.ins().brif(is_some, some_block, &[], none_block, &[]);

    b.switch_to_block(some_block);
    b.seal_block(some_block);
    emit_print_literal(b, lc, "some(");
    emit_print_value(b, lc, v, inner, span)?;
    emit_print_literal(b, lc, ")");
    b.ins().jump(merge, &[]);

    b.switch_to_block(none_block);
    b.seal_block(none_block);
    emit_print_literal(b, lc, "none");
    b.ins().jump(merge, &[]);

    b.switch_to_block(merge);
    b.seal_block(merge);
    Ok(())
}

fn call_method(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    class_id: u32,
    method: &str,
    this_v: Value,
    args: &[Expr],
    span: ilang_ast::Span,
) -> Result<Option<TV>, CodegenError> {
    let info = lc.class_methods[class_id as usize]
        .get(&Symbol::intern(method))
        .cloned()
        .ok_or_else(|| CodegenError::Unsupported {
            what: format!(
                "method {method:?} not found on class {:?}",
                lc.class_layouts[class_id as usize].name
            ),
            span,
        })?;
    // The callee will release `this` and any object params at exit; the
    // caller must retain so its own references survive. (No-op for
    // fresh-alloc receivers/args where rc=1 is already "owned".)
    emit_retain_object(b, lc, this_v);
    let mut arg_vals = Vec::with_capacity(args.len() + 1);
    arg_vals.push(this_v);
    for (i, a) in args.iter().enumerate() {
        let (av, at) = lower_expr(b, lc, a)?.ok_or_else(|| CodegenError::Unsupported {
            what: "argument is unit".into(),
            span: a.span,
        })?;
        let coerced = coerce(b, (av, at), info.params[i], a.span)?;
        emit_bind_retain(b, lc, &a.kind, at, info.params[i], coerced);
        arg_vals.push(coerced);
    }
    // Virtual dispatch: if the typechecker assigned a vtable slot
    // for (class, method), load the function pointer from the
    // object's vtable and call_indirect through it. Inherited
    // overrides are reflected in the object's vtable, so a
    // `Parent` reference holding a `Child` calls the override.
    let class_name = lc.class_layouts[class_id as usize].name.as_str().to_string();
    let slot = lc
        .class_method_slots
        .get(&Symbol::intern(&class_name))
        .and_then(|m| m.get(&Symbol::intern(method)))
        .copied();
    if let Some(slot) = slot {
        // Build the Cranelift signature for call_indirect from
        // info's param/ret types (same wire-format for inherited
        // methods that expect Object(parent_id) — both are i64).
        let mut cl_sig = lc.module.make_signature();
        cl_sig
            .params
            .push(cranelift::prelude::AbiParam::new(I64)); // this
        for p in &info.params {
            cl_sig.params.push(cranelift::prelude::AbiParam::new(
                p.cl().expect("non-unit param"),
            ));
        }
        if let Some(rt) = info.ret.cl() {
            cl_sig.returns.push(cranelift::prelude::AbiParam::new(rt));
        }
        let sig_ref = b.import_signature(cl_sig);
        // Load vtable pointer from object header (offset -8 from
        // user pointer; see runtime::VTABLE_OFFSET).
        let vt_ptr = b.ins().load(
            I64,
            MemFlags::trusted(),
            this_v,
            crate::runtime::VTABLE_OFFSET as i32,
        );
        // Load fn pointer at vtable[slot].
        let fn_ptr = b.ins().load(
            I64,
            MemFlags::trusted(),
            vt_ptr,
            (slot * 8) as i32,
        );
        let call = b.ins().call_indirect(sig_ref, fn_ptr, &arg_vals);
        return if matches!(info.ret, JitTy::Unit) {
            Ok(None)
        } else {
            Ok(Some((b.inst_results(call)[0], info.ret)))
        };
    }
    let func_ref = lc.module.declare_func_in_func(info.id, b.func);
    let call = b.ins().call(func_ref, &arg_vals);
    if matches!(info.ret, JitTy::Unit) {
        Ok(None)
    } else {
        Ok(Some((b.inst_results(call)[0], info.ret)))
    }
}

/// Emit an inline scan loop for `arr.indexOf(needle)` / `arr.includes(needle)`.
/// Returns the i64 index (-1 if not found). Element type is restricted to
/// primitive numeric/bool by the caller (uses icmp/fcmp directly).
fn emit_array_index_of(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    header_ptr: cranelift::prelude::Value,
    needle: cranelift::prelude::Value,
    elem_jty: JitTy,
) -> cranelift::prelude::Value {
    let len = b.ins().load(I64, MemFlags::trusted(), header_ptr, ARRAY_LEN_OFFSET);
    let data = b.ins().load(I64, MemFlags::trusted(), header_ptr, ARRAY_DATA_OFFSET);
    let elem_size = b.ins().iconst(I64, elem_jty.size_bytes() as i64);

    let header = b.create_block();
    let body = b.create_block();
    let exit = b.create_block();
    b.append_block_param(header, I64); // i
    b.append_block_param(exit, I64); // result

    let zero = b.ins().iconst(I64, 0);
    b.ins().jump(header, &[zero.into()]);

    b.switch_to_block(header);
    let i = b.block_params(header)[0];
    let done = b.ins().icmp(IntCC::SignedGreaterThanOrEqual, i, len);
    let neg_one = b.ins().iconst(I64, -1);
    b.ins().brif(done, exit, &[neg_one.into()], body, &[]);

    b.switch_to_block(body);
    let off = b.ins().imul(i, elem_size);
    let addr = b.ins().iadd(data, off);
    let elem = b.ins().load(
        elem_jty.cl().expect("non-unit elem"),
        MemFlags::trusted(),
        addr,
        0,
    );
    // Comparison: floats use fcmp; strings use the runtime's
    // content-equality helper so `["a"].indexOf("a")` matches; all
    // other heap kinds (Object/Array/Map/EnumHeap/Optional/Weak) use
    // pointer equality, mirroring `==` semantics elsewhere.
    let eq = match elem_jty {
        JitTy::F32 | JitTy::F64 => b.ins().fcmp(FloatCC::Equal, elem, needle),
        JitTy::Str => {
            let r = lc.module.declare_func_in_func(lc.strfns.eq, b.func);
            let call = b.ins().call(r, &[elem, needle]);
            // str_eq returns i8 (bool); use as the cond directly.
            b.inst_results(call)[0]
        }
        _ => b.ins().icmp(IntCC::Equal, elem, needle),
    };
    let one = b.ins().iconst(I64, 1);
    let next_i = b.ins().iadd(i, one);
    b.ins().brif(eq, exit, &[i.into()], header, &[next_i.into()]);

    b.seal_block(header);
    b.seal_block(body);
    b.seal_block(exit);
    b.switch_to_block(exit);
    b.block_params(exit)[0]
}

// ─── Map<K, V> lowering ──────────────────────────────────────────────

/// Convert a JIT key value to the i64 bit pattern the runtime expects.
/// Strings pass their `*mut StringRc` pointer; ints/uints/bools sign-
/// or zero-extend into i64.
fn coerce_map_key(
    b: &mut FunctionBuilder,
    _lc: &mut LowerCtx,
    (kv, kt): TV,
    expected: JitTy,
    span: ilang_ast::Span,
) -> Result<cranelift::prelude::Value, CodegenError> {
    let coerced = coerce(b, (kv, kt), expected, span)?;
    // For widths < 64, widen so the runtime always sees a full i64.
    Ok(match expected {
        JitTy::I8 | JitTy::I16 | JitTy::I32 => b.ins().sextend(I64, coerced),
        JitTy::U8 | JitTy::U16 | JitTy::U32 | JitTy::Bool => b.ins().uextend(I64, coerced),
        _ => coerced,
    })
}

/// Resolve the `key_kind` runtime tag for a JitTy key.
fn map_key_kind_tag(k: JitTy, span: ilang_ast::Span) -> Result<i64, CodegenError> {
    Ok(match k {
        JitTy::Str => MAP_KEY_KIND_STR,
        JitTy::I8 | JitTy::I16 | JitTy::I32 | JitTy::I64 => MAP_KEY_KIND_INT,
        JitTy::U8 | JitTy::U16 | JitTy::U32 | JitTy::U64 => MAP_KEY_KIND_UINT,
        JitTy::Bool => MAP_KEY_KIND_BOOL,
        other => {
            return Err(CodegenError::Unsupported {
                what: format!("Map key type {other:?} is not supported"),
                span,
            });
        }
    })
}

pub(crate) fn lower_new_map(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    type_args: &[ilang_ast::Type],
    args: &[ilang_ast::Expr],
    span: ilang_ast::Span,
) -> Result<Option<TV>, CodegenError> {
    if !args.is_empty() {
        return Err(CodegenError::Unsupported {
            what: "new Map<K, V>() takes no constructor args".into(),
            span,
        });
    }
    if type_args.len() != 2 {
        return Err(CodegenError::Unsupported {
            what: "new Map needs explicit <K, V> type args".into(),
            span,
        });
    }
    let class_ids = crate::env::class_ids_from(lc);
    let enum_ids = crate::env::enum_ids_from(lc);
    let key_jty = JitTy::from_ast(
        &type_args[0], span, &class_ids, &enum_ids, lc.enum_layouts,
        lc.array_kinds, lc.optional_inners, lc.fn_signatures, lc.map_kinds, lc.tuple_kinds,
    )?;
    let val_jty = JitTy::from_ast(
        &type_args[1], span, &class_ids, &enum_ids, lc.enum_layouts,
        lc.array_kinds, lc.optional_inners, lc.fn_signatures, lc.map_kinds, lc.tuple_kinds,
    )?;
    let map_id = intern_map_kind(lc.map_kinds, MapKind { key: key_jty, val: val_jty });
    let key_kind = map_key_kind_tag(key_jty, span)?;
    let drop_fn_ptr = crate::drops::map_drop_fn_ptr(b, lc, map_id);
    let key_kind_v = b.ins().iconst(I64, key_kind);
    let new_ref = lc.module.declare_func_in_func(lc.map_new_id, b.func);
    let call = b.ins().call(new_ref, &[key_kind_v, drop_fn_ptr]);
    let ptr = b.inst_results(call)[0];
    Ok(Some((ptr, JitTy::Map(map_id))))
}

pub(crate) fn lower_map_method(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    map_id: u32,
    method: &str,
    obj_v: cranelift::prelude::Value,
    args: &[ilang_ast::Expr],
    span: ilang_ast::Span,
) -> Result<Option<TV>, CodegenError> {
    let kind = lc.map_kinds[map_id as usize];
    let arity = |n: usize| -> Result<(), CodegenError> {
        if args.len() == n { Ok(()) } else {
            Err(CodegenError::Unsupported {
                what: format!("Map.{method} takes {n} args, got {}", args.len()),
                span,
            })
        }
    };
    match method {
        "set" => {
            arity(2)?;
            let key_tv = lower_expr(b, lc, &args[0])?.ok_or_else(|| CodegenError::Unsupported {
                what: "Map.set key is unit".into(), span: args[0].span,
            })?;
            let key_bits = coerce_map_key(b, lc, key_tv, kind.key, args[0].span)?;
            let val_tv = lower_expr(b, lc, &args[1])?.ok_or_else(|| CodegenError::Unsupported {
                what: "Map.set value is unit".into(), span: args[1].span,
            })?;
            let val_coerced = coerce(b, val_tv, kind.val, args[1].span)?;
            // Aliased heap rhs needs a retain so the map owns its own +1.
            emit_bind_retain(b, lc, &args[1].kind, val_tv.1, kind.val, val_coerced);
            // Pad value to i64 for the FFI slot.
            let val_bits = match kind.val {
                JitTy::I8 | JitTy::I16 | JitTy::I32 => b.ins().sextend(I64, val_coerced),
                JitTy::U8 | JitTy::U16 | JitTy::U32 | JitTy::Bool => b.ins().uextend(I64, val_coerced),
                JitTy::F32 => {
                    let bits = b.ins().bitcast(I32, MemFlags::new(), val_coerced);
                    b.ins().uextend(I64, bits)
                }
                JitTy::F64 => b.ins().bitcast(I64, MemFlags::new(), val_coerced),
                _ => val_coerced, // already i64
            };
            let r = lc.module.declare_func_in_func(lc.map_set_id, b.func);
            b.ins().call(r, &[obj_v, key_bits, val_bits]);
            Ok(None)
        }
        "has" => {
            arity(1)?;
            let key_tv = lower_expr(b, lc, &args[0])?.ok_or_else(|| CodegenError::Unsupported {
                what: "Map.has key is unit".into(), span: args[0].span,
            })?;
            let key_bits = coerce_map_key(b, lc, key_tv, kind.key, args[0].span)?;
            let r = lc.module.declare_func_in_func(lc.map_has_id, b.func);
            let call = b.ins().call(r, &[obj_v, key_bits]);
            Ok(Some((b.inst_results(call)[0], JitTy::Bool)))
        }
        "delete" => {
            arity(1)?;
            let key_tv = lower_expr(b, lc, &args[0])?.ok_or_else(|| CodegenError::Unsupported {
                what: "Map.delete key is unit".into(), span: args[0].span,
            })?;
            let key_bits = coerce_map_key(b, lc, key_tv, kind.key, args[0].span)?;
            let r = lc.module.declare_func_in_func(lc.map_delete_id, b.func);
            let call = b.ins().call(r, &[obj_v, key_bits]);
            Ok(Some((b.inst_results(call)[0], JitTy::Bool)))
        }
        "size" => {
            arity(0)?;
            let r = lc.module.declare_func_in_func(lc.map_size_id, b.func);
            let call = b.ins().call(r, &[obj_v]);
            Ok(Some((b.inst_results(call)[0], JitTy::I64)))
        }
        "get" => {
            arity(1)?;
            let key_tv = lower_expr(b, lc, &args[0])?.ok_or_else(|| CodegenError::Unsupported {
                what: "Map.get key is unit".into(), span: args[0].span,
            })?;
            let key_bits = coerce_map_key(b, lc, key_tv, kind.key, args[0].span)?;
            let r = lc.module.declare_func_in_func(lc.map_get_or_null_id, b.func);
            let call = b.ins().call(r, &[obj_v, key_bits]);
            let raw = b.inst_results(call)[0];
            let opt_id = intern_optional_inner(lc.optional_inners, kind.val);
            if kind.val.is_heap() {
                // Heap V: returned pointer IS the value. Bump rc so the
                // caller has its own reference (the runtime did NOT
                // retain — the map's own +1 is preserved). The retain
                // helpers null-check, so a 0 result is safely a no-op.
                crate::arc::emit_retain_heap(b, lc, raw, kind.val);
                return Ok(Some((raw, JitTy::Optional(opt_id))));
            }
            // Primitive V: branch on found/missing. Found → box the
            // payload bits into an Optional<primitive>; missing → 0.
            let zero = b.ins().iconst(I64, 0);
            let found = b.ins().icmp(IntCC::NotEqual, raw, zero);
            let then_blk = b.create_block();
            let else_blk = b.create_block();
            let merge = b.create_block();
            b.append_block_param(merge, I64);
            b.ins().brif(found, then_blk, &[], else_blk, &[]);
            b.switch_to_block(then_blk);
            b.seal_block(then_blk);
            // Box the value bits.
            let size_v = b.ins().iconst(I64, kind.val.size_bytes() as i64);
            let box_ref = lc.module.declare_func_in_func(lc.optional_box_new_id, b.func);
            let box_call = b.ins().call(box_ref, &[size_v]);
            let box_ptr = b.inst_results(box_call)[0];
            // Truncate raw bits to V's natural width before storing.
            let payload = match kind.val {
                JitTy::I8 | JitTy::U8 | JitTy::Bool => b.ins().ireduce(I8, raw),
                JitTy::I16 | JitTy::U16 => b.ins().ireduce(I16, raw),
                JitTy::I32 | JitTy::U32 | JitTy::Enum(_) => b.ins().ireduce(I32, raw),
                JitTy::F32 => {
                    let lo = b.ins().ireduce(I32, raw);
                    b.ins().bitcast(F32, MemFlags::new(), lo)
                }
                JitTy::F64 => b.ins().bitcast(F64, MemFlags::new(), raw),
                _ => raw,
            };
            b.ins().store(
                MemFlags::trusted(),
                payload,
                box_ptr,
                crate::runtime::OPT_PRIM_PAYLOAD_OFFSET,
            );
            b.ins().jump(merge, &[box_ptr.into()]);
            b.switch_to_block(else_blk);
            b.seal_block(else_blk);
            b.ins().jump(merge, &[zero.into()]);
            b.switch_to_block(merge);
            b.seal_block(merge);
            let result = b.block_params(merge)[0];
            Ok(Some((result, JitTy::Optional(opt_id))))
        }
        "keys" => {
            arity(0)?;
            // Build K[] via the runtime helper. The new array's per-elem
            // drop wrapper releases each (string keys are freshly
            // allocated by the runtime; primitive keys hold no resources).
            let elem_jty = kind.key;
            let array_kind_id = intern_array_kind(
                lc.array_kinds,
                ArrayKind { elem: elem_jty, fixed: None },
            );
            let drop_fn_ptr = crate::drops::array_drop_fn_ptr(b, lc, array_kind_id);
            let elem_size = b.ins().iconst(I64, elem_jty.size_bytes() as i64);
            let r = lc.module.declare_func_in_func(lc.map_keys_to_array_id, b.func);
            let call = b.ins().call(r, &[obj_v, elem_size, drop_fn_ptr]);
            Ok(Some((b.inst_results(call)[0], JitTy::Array(array_kind_id))))
        }
        "values" => {
            arity(0)?;
            let elem_jty = kind.val;
            let array_kind_id = intern_array_kind(
                lc.array_kinds,
                ArrayKind { elem: elem_jty, fixed: None },
            );
            let drop_fn_ptr = crate::drops::array_drop_fn_ptr(b, lc, array_kind_id);
            let retain_fn_ptr = crate::drops::map_value_retain_fn_ptr(b, lc, map_id);
            let elem_size = b.ins().iconst(I64, elem_jty.size_bytes() as i64);
            let r = lc.module.declare_func_in_func(lc.map_values_to_array_id, b.func);
            let call = b.ins().call(r, &[obj_v, elem_size, drop_fn_ptr, retain_fn_ptr]);
            Ok(Some((b.inst_results(call)[0], JitTy::Array(array_kind_id))))
        }
        _ => Err(CodegenError::Unsupported {
            what: format!("Map has no method {method:?}"),
            span,
        }),
    }
}

pub(crate) fn lower_map_index_get(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    map_id: u32,
    obj_v: cranelift::prelude::Value,
    index: &ilang_ast::Expr,
) -> Result<Option<TV>, CodegenError> {
    let kind = lc.map_kinds[map_id as usize];
    let key_tv = lower_expr(b, lc, index)?.ok_or_else(|| CodegenError::Unsupported {
        what: "Map index key is unit".into(), span: index.span,
    })?;
    let key_bits = coerce_map_key(b, lc, key_tv, kind.key, index.span)?;
    let r = lc.module.declare_func_in_func(lc.map_index_get_id, b.func);
    let call = b.ins().call(r, &[obj_v, key_bits]);
    let raw = b.inst_results(call)[0];
    // Decode the i64 slot back to V's representation.
    let v = match kind.val {
        JitTy::I8 | JitTy::U8 | JitTy::Bool => b.ins().ireduce(I8, raw),
        JitTy::I16 | JitTy::U16 => b.ins().ireduce(I16, raw),
        JitTy::I32 | JitTy::U32 | JitTy::Enum(_) => b.ins().ireduce(I32, raw),
        JitTy::F32 => {
            let lo = b.ins().ireduce(I32, raw);
            b.ins().bitcast(F32, MemFlags::new(), lo)
        }
        JitTy::F64 => b.ins().bitcast(F64, MemFlags::new(), raw),
        _ => raw, // i64 / u64 / pointers
    };
    Ok(Some((v, kind.val)))
}

pub(crate) fn lower_map_index_set(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    map_id: u32,
    obj_v: cranelift::prelude::Value,
    index: &ilang_ast::Expr,
    value: &ilang_ast::Expr,
    value_kind: &ilang_ast::ExprKind,
) -> Result<Option<TV>, CodegenError> {
    let kind = lc.map_kinds[map_id as usize];
    let key_tv = lower_expr(b, lc, index)?.ok_or_else(|| CodegenError::Unsupported {
        what: "Map index key is unit".into(), span: index.span,
    })?;
    let key_bits = coerce_map_key(b, lc, key_tv, kind.key, index.span)?;
    let val_tv = lower_expr(b, lc, value)?.ok_or_else(|| CodegenError::Unsupported {
        what: "Map index value is unit".into(), span: value.span,
    })?;
    let val_coerced = coerce(b, val_tv, kind.val, value.span)?;
    emit_bind_retain(b, lc, value_kind, val_tv.1, kind.val, val_coerced);
    let val_bits = match kind.val {
        JitTy::I8 | JitTy::I16 | JitTy::I32 => b.ins().sextend(I64, val_coerced),
        JitTy::U8 | JitTy::U16 | JitTy::U32 | JitTy::Bool => b.ins().uextend(I64, val_coerced),
        JitTy::F32 => {
            let bits = b.ins().bitcast(I32, MemFlags::new(), val_coerced);
            b.ins().uextend(I64, bits)
        }
        JitTy::F64 => b.ins().bitcast(I64, MemFlags::new(), val_coerced),
        _ => val_coerced,
    };
    let r = lc.module.declare_func_in_func(lc.map_set_id, b.func);
    b.ins().call(r, &[obj_v, key_bits, val_bits]);
    Ok(None)
}

/// Map literal `{ k1: v1, k2: v2, ... }` — allocate an empty Map then
/// emit one `set` per entry. K and V come from the first entry's
/// expression types (mirrors the type checker's MapLit inference).
pub(crate) fn lower_map_lit(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    entries: &[(ilang_ast::Expr, ilang_ast::Expr)],
    span: ilang_ast::Span,
) -> Result<Option<TV>, CodegenError> {
    if entries.is_empty() {
        // Parser only produces MapLit when at least one entry exists,
        // but be safe.
        return Err(CodegenError::Unsupported {
            what: "empty map literal — use `new Map<K, V>()` instead".into(),
            span,
        });
    }
    // Lower the first entry to discover K and V kinds.
    let (k0, v0) = &entries[0];
    let (k0_v, k0_t) = lower_expr(b, lc, k0)?.ok_or_else(|| CodegenError::Unsupported {
        what: "map key is unit".into(), span: k0.span,
    })?;
    let (v0_v, v0_t) = lower_expr(b, lc, v0)?.ok_or_else(|| CodegenError::Unsupported {
        what: "map value is unit".into(), span: v0.span,
    })?;
    let map_id = intern_map_kind(lc.map_kinds, MapKind { key: k0_t, val: v0_t });
    let key_kind = map_key_kind_tag(k0_t, span)?;
    let drop_fn_ptr = crate::drops::map_drop_fn_ptr(b, lc, map_id);
    let key_kind_v = b.ins().iconst(I64, key_kind);
    let new_ref = lc.module.declare_func_in_func(lc.map_new_id, b.func);
    let new_call = b.ins().call(new_ref, &[key_kind_v, drop_fn_ptr]);
    let map_ptr = b.inst_results(new_call)[0];

    // Helper that takes a (k_v, k_t) / (v_v, v_t) pair and emits one set.
    let emit_set = |b: &mut FunctionBuilder,
                    lc: &mut LowerCtx,
                        kv: cranelift::prelude::Value,
                        kt: JitTy,
                        k_span: ilang_ast::Span,
                        vv: cranelift::prelude::Value,
                        vt: JitTy,
                        v_span: ilang_ast::Span,
                        v_kind: &ilang_ast::ExprKind|
     -> Result<(), CodegenError> {
        let key_bits = coerce_map_key(b, lc, (kv, kt), k0_t, k_span)?;
        let val_coerced = coerce(b, (vv, vt), v0_t, v_span)?;
        emit_bind_retain(b, lc, v_kind, vt, v0_t, val_coerced);
        let val_bits = match v0_t {
            JitTy::I8 | JitTy::I16 | JitTy::I32 => b.ins().sextend(I64, val_coerced),
            JitTy::U8 | JitTy::U16 | JitTy::U32 | JitTy::Bool => b.ins().uextend(I64, val_coerced),
            JitTy::F32 => {
                let bits = b.ins().bitcast(I32, MemFlags::new(), val_coerced);
                b.ins().uextend(I64, bits)
            }
            JitTy::F64 => b.ins().bitcast(I64, MemFlags::new(), val_coerced),
            _ => val_coerced,
        };
        let r = lc.module.declare_func_in_func(lc.map_set_id, b.func);
        b.ins().call(r, &[map_ptr, key_bits, val_bits]);
        Ok(())
    };

    // First entry — already lowered.
    emit_set(b, lc, k0_v, k0_t, k0.span, v0_v, v0_t, v0.span, &v0.kind)?;
    // Remaining entries.
    for (k, v) in &entries[1..] {
        let (kv, kt) = lower_expr(b, lc, k)?.ok_or_else(|| CodegenError::Unsupported {
            what: "map key is unit".into(), span: k.span,
        })?;
        let (vv, vt) = lower_expr(b, lc, v)?.ok_or_else(|| CodegenError::Unsupported {
            what: "map value is unit".into(), span: v.span,
        })?;
        emit_set(b, lc, kv, kt, k.span, vv, vt, v.span, &v.kind)?;
    }
    Ok(Some((map_ptr, JitTy::Map(map_id))))
}

/// Lower `xs.pop(): T?`. Inline branching on `len > 0`. For empty
/// arrays returns 0 (None). For heap V the popped pointer flows
/// through unchanged (the array's drop_fn iterates `[0, len)` so the
/// no-longer-tracked slot won't be re-released). For primitive V the
/// popped bits are heap-boxed so the result fits the same nullable-
/// pointer shape used by every Optional in the JIT.
pub(crate) fn lower_array_pop(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    arr_v: cranelift::prelude::Value,
    obj: &ilang_ast::Expr,
    array_id: u32,
    elem_jty: JitTy,
) -> Result<Option<TV>, CodegenError> {
    let opt_id = intern_optional_inner(lc.optional_inners, elem_jty);

    let then_blk = b.create_block();
    let else_blk = b.create_block();
    let merge = b.create_block();
    b.append_block_param(merge, I64);

    let len = b.ins().load(I64, MemFlags::trusted(), arr_v, ARRAY_LEN_OFFSET);
    let zero = b.ins().iconst(I64, 0);
    let nonempty = b.ins().icmp(IntCC::SignedGreaterThan, len, zero);
    b.ins().brif(nonempty, then_blk, &[], else_blk, &[]);

    // Non-empty: read the last element and decrement len.
    b.switch_to_block(then_blk);
    b.seal_block(then_blk);
    let one = b.ins().iconst(I64, 1);
    let new_len = b.ins().isub(len, one);
    b.ins().store(MemFlags::trusted(), new_len, arr_v, ARRAY_LEN_OFFSET);
    let data = b.ins().load(I64, MemFlags::trusted(), arr_v, ARRAY_DATA_OFFSET);
    let elem_size = b.ins().iconst(I64, elem_jty.size_bytes() as i64);
    let off = b.ins().imul(new_len, elem_size);
    let addr = b.ins().iadd(data, off);
    let elem = b.ins().load(
        elem_jty.cl().expect("non-unit elem"),
        MemFlags::trusted(),
        addr,
        0,
    );
    let some_v: cranelift::prelude::Value = if elem_jty.is_heap() {
        // The element pointer's rc was set when push'd; ownership now
        // transfers to the caller (array no longer "owns" it via
        // drop_fn iteration). No retain needed.
        elem
    } else {
        // Box the primitive payload.
        let size_v = b.ins().iconst(I64, elem_jty.size_bytes() as i64);
        let new_ref = lc.module.declare_func_in_func(lc.optional_box_new_id, b.func);
        let call = b.ins().call(new_ref, &[size_v]);
        let ptr = b.inst_results(call)[0];
        b.ins().store(
            MemFlags::trusted(),
            elem,
            ptr,
            crate::runtime::OPT_PRIM_PAYLOAD_OFFSET,
        );
        ptr
    };
    b.ins().jump(merge, &[some_v.into()]);

    // Empty: yield 0 (None).
    b.switch_to_block(else_blk);
    b.seal_block(else_blk);
    let none = b.ins().iconst(I64, 0);
    b.ins().jump(merge, &[none.into()]);

    b.switch_to_block(merge);
    b.seal_block(merge);
    let result = b.block_params(merge)[0];

    // If the receiver was a fresh allocation (e.g. `[1,2,3].pop()`),
    // release it now — the popped value is independent.
    if !is_aliased_heap_source(&obj.kind) {
        emit_release_heap(b, lc, arr_v, JitTy::Array(array_id));
    }

    Ok(Some((result, JitTy::Optional(opt_id))))
}

/// Lower `xs.slice(start, end)`. Allocates a new array and copies the
/// element bytes one slot at a time. For heap V each copied element
/// gets retained so the new array owns its own +1.
pub(crate) fn lower_array_slice(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    arr_v: cranelift::prelude::Value,
    obj: &ilang_ast::Expr,
    start_e: &ilang_ast::Expr,
    end_e: &ilang_ast::Expr,
    array_id: u32,
    elem_jty: JitTy,
) -> Result<Option<TV>, CodegenError> {
    let (sv, st) = lower_expr(b, lc, start_e)?.ok_or_else(|| CodegenError::Unsupported {
        what: "slice start is unit".into(), span: start_e.span,
    })?;
    let start_i64 = coerce(b, (sv, st), JitTy::I64, start_e.span)?;
    let (ev, et) = lower_expr(b, lc, end_e)?.ok_or_else(|| CodegenError::Unsupported {
        what: "slice end is unit".into(), span: end_e.span,
    })?;
    let end_i64 = coerce(b, (ev, et), JitTy::I64, end_e.span)?;

    let len = b.ins().load(I64, MemFlags::trusted(), arr_v, ARRAY_LEN_OFFSET);
    let zero = b.ins().iconst(I64, 0);

    // Clamp: s = max(0, min(start, len)); e = max(s, min(end, len)).
    let start_clamped_lo = clamp_max(b, start_i64, zero);
    let start_clamped = clamp_min(b, start_clamped_lo, len);
    let end_clamped_lo = clamp_max(b, end_i64, zero);
    let end_clamped = clamp_min(b, end_clamped_lo, len);
    let end_final = clamp_max(b, end_clamped, start_clamped);
    let new_len = b.ins().isub(end_final, start_clamped);

    let drop_fn_ptr = crate::drops::array_drop_fn_ptr(b, lc, array_id);
    let elem_size = b.ins().iconst(I64, elem_jty.size_bytes() as i64);
    let new_ref = lc.module.declare_func_in_func(lc.arrfns.new, b.func);
    let call = b.ins().call(new_ref, &[elem_size, new_len, drop_fn_ptr]);
    let new_arr = b.inst_results(call)[0];

    let src_data = b.ins().load(I64, MemFlags::trusted(), arr_v, ARRAY_DATA_OFFSET);
    let dst_data = b.ins().load(I64, MemFlags::trusted(), new_arr, ARRAY_DATA_OFFSET);

    // for i in 0..new_len: copy src[start+i] to dst[i] + retain if heap
    let header = b.create_block();
    let body = b.create_block();
    let exit = b.create_block();
    b.append_block_param(header, I64);
    b.ins().jump(header, &[zero.into()]);

    b.switch_to_block(header);
    let i = b.block_params(header)[0];
    let done = b.ins().icmp(IntCC::SignedGreaterThanOrEqual, i, new_len);
    b.ins().brif(done, exit, &[], body, &[]);

    b.switch_to_block(body);
    b.seal_block(body);
    let src_idx = b.ins().iadd(start_clamped, i);
    let src_off = b.ins().imul(src_idx, elem_size);
    let src_addr = b.ins().iadd(src_data, src_off);
    let dst_off = b.ins().imul(i, elem_size);
    let dst_addr = b.ins().iadd(dst_data, dst_off);
    let cl_ty = elem_jty.cl().expect("non-unit elem");
    let elem = b.ins().load(cl_ty, MemFlags::trusted(), src_addr, 0);
    if elem_jty.is_heap() {
        crate::arc::emit_retain_heap(b, lc, elem, elem_jty);
    }
    b.ins().store(MemFlags::trusted(), elem, dst_addr, 0);
    let one = b.ins().iconst(I64, 1);
    let next = b.ins().iadd(i, one);
    b.ins().jump(header, &[next.into()]);
    b.seal_block(header);

    b.switch_to_block(exit);
    b.seal_block(exit);

    if !is_aliased_heap_source(&obj.kind) {
        emit_release_heap(b, lc, arr_v, JitTy::Array(array_id));
    }
    Ok(Some((new_arr, JitTy::Array(array_id))))
}

fn clamp_max(b: &mut FunctionBuilder, v: cranelift::prelude::Value, lo: cranelift::prelude::Value)
    -> cranelift::prelude::Value
{
    // max(v, lo)
    let cond = b.ins().icmp(IntCC::SignedLessThan, v, lo);
    b.ins().select(cond, lo, v)
}

fn clamp_min(b: &mut FunctionBuilder, v: cranelift::prelude::Value, hi: cranelift::prelude::Value)
    -> cranelift::prelude::Value
{
    // min(v, hi)
    let cond = b.ins().icmp(IntCC::SignedGreaterThan, v, hi);
    b.ins().select(cond, hi, v)
}

/// Lower `xs.map(f)` / `xs.filter(f)` / `xs.forEach(f)`. The callback
/// `f` is a first-class fn value (`JitTy::Fn(sig_id)`), invoked via
/// indirect call per element. Result type:
///   map     → `U[]` (U = f's return type)
///   filter  → `T[]`
///   forEach → unit
pub(crate) fn lower_array_higher_order(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    arr_v: cranelift::prelude::Value,
    obj: &ilang_ast::Expr,
    fn_arg: &ilang_ast::Expr,
    array_id: u32,
    elem_jty: JitTy,
    method: &str,
) -> Result<Option<TV>, CodegenError> {
    let (fv, ft) = lower_expr(b, lc, fn_arg)?.ok_or_else(|| CodegenError::Unsupported {
        what: format!("array.{method} fn arg is unit"), span: fn_arg.span,
    })?;
    let sig_id = match ft {
        JitTy::Fn(id) => id,
        _ => return Err(CodegenError::Unsupported {
            what: format!("array.{method} expects a function value"),
            span: fn_arg.span,
        }),
    };
    let sig = lc.fn_signatures[sig_id as usize].clone();
    if sig.params.len() != 1 {
        return Err(CodegenError::Unsupported {
            what: format!("array.{method} expects fn(T): U taking exactly one arg"),
            span: fn_arg.span,
        });
    }
    let ret_jty = sig.ret;

    // Build cranelift signature for indirect call. Closure
    // calling convention: env_ptr (i64) is prepended to the user
    // params. `fv` itself IS the env_ptr (the closure struct
    // pointer), so we load fn_ptr from offset 0 and call with
    // (fv, user_arg).
    let mut cl_sig = lc.module.make_signature();
    cl_sig
        .params
        .push(cranelift::prelude::AbiParam::new(I64));
    cl_sig.params.push(cranelift::prelude::AbiParam::new(
        sig.params[0].cl().ok_or_else(|| CodegenError::Unsupported {
            what: format!("{method} fn param has unit type"), span: fn_arg.span,
        })?,
    ));
    if let Some(rt) = ret_jty.cl() {
        cl_sig.returns.push(cranelift::prelude::AbiParam::new(rt));
    }
    let sig_ref = b.import_signature(cl_sig);
    let closure_fn_ptr = b.ins().load(I64, MemFlags::trusted(), fv, 0);

    let len = b.ins().load(I64, MemFlags::trusted(), arr_v, ARRAY_LEN_OFFSET);
    let src_data = b.ins().load(I64, MemFlags::trusted(), arr_v, ARRAY_DATA_OFFSET);
    let elem_size = b.ins().iconst(I64, elem_jty.size_bytes() as i64);
    let zero = b.ins().iconst(I64, 0);

    // For map / filter: allocate result array up front. Map sizes it
    // to len; filter starts at 0 and uses push.
    let (out_arr, out_kind_id, out_elem_size_v) = match method {
        "map" => {
            let out_kind_id = intern_array_kind(
                lc.array_kinds,
                ArrayKind { elem: ret_jty, fixed: None },
            );
            let drop_fn_ptr = crate::drops::array_drop_fn_ptr(b, lc, out_kind_id);
            let out_elem_size = b.ins().iconst(I64, ret_jty.size_bytes() as i64);
            let new_ref = lc.module.declare_func_in_func(lc.arrfns.new, b.func);
            let call = b.ins().call(new_ref, &[out_elem_size, len, drop_fn_ptr]);
            let arr = b.inst_results(call)[0];
            (Some(arr), Some(out_kind_id), Some(out_elem_size))
        }
        "filter" => {
            // Start with an empty array of T (same kind as input).
            let drop_fn_ptr = crate::drops::array_drop_fn_ptr(b, lc, array_id);
            let new_ref = lc.module.declare_func_in_func(lc.arrfns.new, b.func);
            let call = b.ins().call(new_ref, &[elem_size, zero, drop_fn_ptr]);
            let arr = b.inst_results(call)[0];
            (Some(arr), Some(array_id), Some(elem_size))
        }
        _ => (None, None, None),
    };

    // Loop: for i in 0..len { call f(elem); ... }
    let header = b.create_block();
    let body = b.create_block();
    let exit = b.create_block();
    b.append_block_param(header, I64);
    b.ins().jump(header, &[zero.into()]);

    b.switch_to_block(header);
    let i = b.block_params(header)[0];
    let done = b.ins().icmp(IntCC::SignedGreaterThanOrEqual, i, len);
    b.ins().brif(done, exit, &[], body, &[]);

    b.switch_to_block(body);
    b.seal_block(body);
    let off = b.ins().imul(i, elem_size);
    let addr = b.ins().iadd(src_data, off);
    let cl_ty = elem_jty.cl().expect("non-unit elem");
    let elem = b.ins().load(cl_ty, MemFlags::trusted(), addr, 0);
    // Calling convention for first-class fn calls: caller retains
    // each heap arg before the call so the callee can release at exit
    // (matching how regular fn params are wired). Without this the
    // callee's exit-release would drop the source array's +1 and
    // leave a freed pointer in the next iteration's slot.
    if elem_jty.is_heap() {
        crate::arc::emit_retain_heap(b, lc, elem, elem_jty);
    }
    let call = b.ins().call_indirect(sig_ref, closure_fn_ptr, &[fv, elem]);
    let result = if matches!(ret_jty, JitTy::Unit) {
        None
    } else {
        Some(b.inst_results(call)[0])
    };

    let one = b.ins().iconst(I64, 1);
    match method {
        "map" => {
            // Result is the value to store. For heap U the callee
            // returned with rc=1 (fresh allocation) — we own it and
            // hand it to the new array. For heap V WHERE U == V is
            // possible too; same rule.
            let arr = out_arr.unwrap();
            let out_data = b.ins().load(I64, MemFlags::trusted(), arr, ARRAY_DATA_OFFSET);
            let out_size = out_elem_size_v.unwrap();
            let dst_off = b.ins().imul(i, out_size);
            let dst_addr = b.ins().iadd(out_data, dst_off);
            b.ins().store(MemFlags::trusted(), result.expect("map fn returns a value"), dst_addr, 0);
            // Bump the result array's len so map_drops sees the slot.
            let new_len = b.ins().iadd(i, one);
            b.ins().store(MemFlags::trusted(), new_len, arr, ARRAY_LEN_OFFSET);
        }
        "filter" => {
            let arr = out_arr.unwrap();
            let kept_blk = b.create_block();
            let skip_blk = b.create_block();
            let res = result.expect("filter fn returns a bool");
            b.ins().brif(res, kept_blk, &[], skip_blk, &[]);
            b.switch_to_block(kept_blk);
            b.seal_block(kept_blk);
            // Kept path: the new array stores the elem and needs its
            // own +1. The source still holds its +1, so no leak/double-
            // free either way.
            if elem_jty.is_heap() {
                crate::arc::emit_retain_heap(b, lc, elem, elem_jty);
            }
            let push_id = match elem_jty.size_bytes() {
                1 => lc.arrfns.push_i8,
                2 => lc.arrfns.push_i16,
                4 => match elem_jty {
                    JitTy::F32 => lc.arrfns.push_f32,
                    _ => lc.arrfns.push_i32,
                },
                8 => match elem_jty {
                    JitTy::F64 => lc.arrfns.push_f64,
                    _ => lc.arrfns.push_i64,
                },
                _ => unreachable!("primitive widths covered"),
            };
            let r = lc.module.declare_func_in_func(push_id, b.func);
            b.ins().call(r, &[arr, elem]);
            b.ins().jump(skip_blk, &[]);
            b.switch_to_block(skip_blk);
            b.seal_block(skip_blk);
        }
        "forEach" => {
            // Callee's return value is discarded. If heap-typed (and
            // came back with rc=1), release it so it doesn't leak.
            if let Some(rv) = result {
                if ret_jty.is_heap() {
                    crate::arc::emit_release_heap(b, lc, rv, ret_jty);
                }
            }
        }
        _ => unreachable!(),
    }
    let next = b.ins().iadd(i, one);
    b.ins().jump(header, &[next.into()]);
    b.seal_block(header);

    b.switch_to_block(exit);
    b.seal_block(exit);

    // Release fn-arg if it was a fresh allocation. (Plain Var/named fn
    // refs are aliased.) Fn pointers don't have rc — emit_release_heap
    // skips Fn — so this is just for symmetry.

    if !is_aliased_heap_source(&obj.kind) {
        emit_release_heap(b, lc, arr_v, JitTy::Array(array_id));
    }

    Ok(match method {
        "map" => Some((out_arr.unwrap(), JitTy::Array(out_kind_id.unwrap()))),
        "filter" => Some((out_arr.unwrap(), JitTy::Array(out_kind_id.unwrap()))),
        "forEach" => None,
        _ => unreachable!(),
    })
}

/// Emit `if idx < 0 || idx >= len { panic_index_oob(idx, len) }` so
/// out-of-range array reads / writes match the interpreter's runtime
/// error rather than silently corrupting memory.
fn emit_array_bounds_check(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    arr_v: cranelift::prelude::Value,
    idx_i64: cranelift::prelude::Value,
) {
    let len = b.ins().load(I64, MemFlags::trusted(), arr_v, ARRAY_LEN_OFFSET);
    emit_inline_bounds_check(b, lc, idx_i64, len);
}

/// Same panic dispatch as `emit_array_bounds_check`, but the length
/// comes from a caller-provided i64 Value (e.g. an iconst for an
/// embedded fixed-length array).
fn emit_inline_bounds_check(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    idx_i64: cranelift::prelude::Value,
    len: cranelift::prelude::Value,
) {
    let zero = b.ins().iconst(I64, 0);
    let neg = b.ins().icmp(IntCC::SignedLessThan, idx_i64, zero);
    let too_big = b.ins().icmp(IntCC::SignedGreaterThanOrEqual, idx_i64, len);
    let bad = b.ins().bor(neg, too_big);
    let oob = b.create_block();
    let ok = b.create_block();
    b.ins().brif(bad, oob, &[], ok, &[]);
    b.switch_to_block(oob);
    b.seal_block(oob);
    let r = lc.module.declare_func_in_func(lc.panic_index_oob_id, b.func);
    b.ins().call(r, &[idx_i64, len]);
    b.ins().trap(cranelift_codegen::ir::TrapCode::user(1).expect("trap code"));
    b.switch_to_block(ok);
    b.seal_block(ok);
}

/// Ensure a trampoline wrapper exists for top-level fn `name` so a
/// `let f = name` reference can produce a closure value. The
/// trampoline takes `(env_ptr, ...args)` (ignoring env_ptr) and
/// tail-calls the real fn. Cached per fn name.
/// Map an AST primitive type to its `JitTy`. Used by `@extern static`
/// where the AST type is known but `JitTy::from_ast`'s class-id /
/// generic plumbing is overkill.
fn jit_ty_from_primitive(t: &ilang_ast::Type) -> JitTy {
    use ilang_ast::Type as T;
    match t {
        T::I8 => JitTy::I8,
        T::I16 => JitTy::I16,
        T::I32 => JitTy::I32,
        T::I64 => JitTy::I64,
        T::U8 => JitTy::U8,
        T::U16 => JitTy::U16,
        T::U32 => JitTy::U32,
        T::U64 => JitTy::U64,
        T::F32 => JitTy::F32,
        T::F64 => JitTy::F64,
        T::Bool => JitTy::Bool,
        _ => unreachable!("type checker restricts @extern static to primitives"),
    }
}

fn ensure_trampoline(
    _b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    name: &str,
    target_id: cranelift_module::FuncId,
    target_params: &[JitTy],
    target_ret: JitTy,
) -> Result<cranelift_module::FuncId, CodegenError> {
    use cranelift::prelude::*;
    use cranelift_codegen::ir::types::I64;
    use cranelift_module::Module as _;
    if let Some(&id) = lc.closure_trampolines.get(&Symbol::intern(name)) {
        return Ok(id);
    }
    // Build the wrapper Cranelift signature: env_ptr (i64) + target
    // params, return target ret.
    let mut sig = lc.module.make_signature();
    sig.params.push(AbiParam::new(I64)); // env_ptr (ignored)
    for p in target_params {
        sig.params
            .push(AbiParam::new(p.cl().expect("non-unit param")));
    }
    if let Some(rt) = target_ret.cl() {
        sig.returns.push(AbiParam::new(rt));
    }
    let symbol = format!("__closure_trampoline_{name}");
    let id = lc
        .module
        .declare_function(
            &symbol,
            cranelift_module::Linkage::Local,
            &sig,
        )
        .map_err(|e| CodegenError::Module(e.to_string()))?;
    // Define the body — a one-block function that loads each user
    // param and calls the target directly.
    let mut ctx = lc.module.make_context();
    ctx.func.signature = sig.clone();
    let mut bctx = FunctionBuilderContext::new();
    {
        let mut tb = FunctionBuilder::new(&mut ctx.func, &mut bctx);
        let entry = tb.create_block();
        tb.append_block_params_for_function_params(entry);
        tb.switch_to_block(entry);
        tb.seal_block(entry);
        let block_params: Vec<Value> = tb.block_params(entry).to_vec();
        // Skip block_params[0] (env_ptr); pass the rest to target.
        let target_ref = lc.module.declare_func_in_func(target_id, tb.func);
        let call = tb.ins().call(target_ref, &block_params[1..]);
        let results = tb.inst_results(call).to_vec();
        if matches!(target_ret, JitTy::Unit) {
            tb.ins().return_(&[]);
        } else {
            tb.ins().return_(&results);
        }
        tb.finalize();
    }
    lc.module
        .define_function(id, &mut ctx)
        .map_err(|e| CodegenError::Module(e.to_string()))?;
    lc.module.clear_context(&mut ctx);
    lc.closure_trampolines.insert(name.into(), id);
    Ok(id)
}

/// Allocate a closure struct via the standard object allocator
/// (so it gets ARC + a drop fn) and write `[fn_ptr | env_field0 |
/// ...]`. The drop fn (one per wrapper) releases each heap-typed
/// capture. Returns the closure pointer as `JitTy::Fn(sig_id)`.
fn lower_closure_construct(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    fn_name: &str,
    captures: &[(Symbol, ilang_ast::Type)],
    span: ilang_ast::Span,
) -> Result<Option<TV>, CodegenError> {
    use cranelift_codegen::ir::types::I64;
    let meta = lc
        .closure_meta
        .get(&Symbol::intern(fn_name))
        .cloned()
        .ok_or_else(|| CodegenError::Unsupported {
            what: format!("closure metadata missing for {fn_name:?}"),
            span,
        })?;
    let n = meta.captures.len() as i64;
    let drop_fn_ptr = crate::drops::closure_drop_fn_ptr(b, lc, fn_name, &meta.captures)?;
    let alloc_ref = lc.module.declare_func_in_func(lc.alloc_closure_id, b.func);
    let n_v = b.ins().iconst(I64, n);
    let call = b.ins().call(alloc_ref, &[n_v, drop_fn_ptr]);
    let closure_ptr = b.inst_results(call)[0];
    // Write the wrapper's function pointer at offset 0.
    let (wrapper_id, _, _) = lc
        .funcs
        .get(&Symbol::intern(fn_name))
        .cloned()
        .ok_or_else(|| CodegenError::Unsupported {
            what: format!("closure wrapper {fn_name:?} not declared"),
            span,
        })?;
    let func_ref = lc.module.declare_func_in_func(wrapper_id, b.func);
    let fn_addr = b.ins().func_addr(I64, func_ref);
    b.ins().store(MemFlags::trusted(), fn_addr, closure_ptr, 0);
    // Write each capture at offset 8, 16, 24, ...
    for (i, (cap_name, _cap_ty)) in captures.iter().enumerate() {
        let offset = 8 + (i as i32) * 8;
        // Look in regular env first, then in the surrounding
        // closure's capture env (so a nested closure can re-capture
        // an outer closure's captured value).
        let (v, jty) = if let Some(&(var, vt)) = lc.env.bindings.get(cap_name) {
            (b.use_var(var), vt)
        } else if let Some(env) = lc.closure_capture_env.as_ref() {
            if let Some(entry) = env.captures.iter().find(|(n, _, _)| n == cap_name) {
                let outer_offset = entry.1 as i32;
                let outer_jty = entry.2;
                let env_ptr = b.use_var(env.env_var);
                let raw = b.ins().load(I64, MemFlags::trusted(), env_ptr, outer_offset);
                let v = match outer_jty {
                    JitTy::I64 | JitTy::U64 => raw,
                    JitTy::F64 => b.ins().bitcast(F64, MemFlags::new(), raw),
                    JitTy::Bool => b.ins().ireduce(I8, raw),
                    t if t.is_heap() => raw,
                    _ => unreachable!(),
                };
                (v, outer_jty)
            } else {
                return Err(CodegenError::Unsupported {
                    what: format!(
                        "closure capture {cap_name:?} not in scope at construction site"
                    ),
                    span,
                });
            }
        } else {
            return Err(CodegenError::Unsupported {
                what: format!(
                    "closure capture {cap_name:?} not in scope at construction site"
                ),
                span,
            });
        };
        let bits = match jty {
            JitTy::I64 | JitTy::U64 => v,
            JitTy::F64 => b.ins().bitcast(I64, MemFlags::new(), v),
            JitTy::Bool => b.ins().uextend(I64, v),
            t if t.is_heap() => {
                // Heap capture: retain so the closure owns a
                // reference. The drop fn will release on close.
                crate::arc::emit_retain_heap(b, lc, v, t);
                v
            }
            _ => {
                return Err(CodegenError::Unsupported {
                    what: format!(
                        "closure capture of type {jty:?} not yet supported"
                    ),
                    span,
                });
            }
        };
        b.ins().store(MemFlags::trusted(), bits, closure_ptr, offset);
    }
    let sig_id = crate::ty::intern_fn_sig(
        lc.fn_signatures,
        crate::ty::FnSignature {
            params: meta.user_params.clone(),
            ret: meta.ret,
        },
    );
    Ok(Some((closure_ptr, JitTy::Fn(sig_id))))
}

/// Built-in helpers callable inside `@extern(C) { ... }` blocks.
/// Returns `Some(...)` if the callee name matched a recognised
/// helper and lowering succeeded; `None` to fall through to the
/// regular fn-call path.
fn try_lower_extern_c_helper(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    callee: &str,
    args: &[ilang_ast::Expr],
    call_span: ilang_ast::Span,
) -> Result<Option<Option<TV>>, CodegenError> {
    match callee {
        // stringFromCstr(p: *const char): string
        "stringFromCstr" => {
            let (av, _) = lower_expr(b, lc, &args[0])?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "stringFromCstr arg is unit".into(),
                    span: args[0].span,
                }
            })?;
            let f = lc.module.declare_func_in_func(lc.strfns.c_str_to_string, b.func);
            let c = b.ins().call(f, &[av]);
            Ok(Some(Some((b.inst_results(c)[0], JitTy::Str))))
        }
        // cstrFromString(s: string): *char  (raw pointer = i64)
        "cstrFromString" => {
            let (av, _) = lower_expr(b, lc, &args[0])?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "cstrFromString arg is unit".into(),
                    span: args[0].span,
                }
            })?;
            let f = lc.module.declare_func_in_func(lc.strfns.to_c_str, b.func);
            let c = b.ins().call(f, &[av]);
            Ok(Some(Some((b.inst_results(c)[0], JitTy::I64))))
        }
        // freeCstr(p: *char)
        "freeCstr" => {
            let (av, _) = lower_expr(b, lc, &args[0])?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "freeCstr arg is unit".into(),
                    span: args[0].span,
                }
            })?;
            let f = lc.module.declare_func_in_func(lc.strfns.free_c_str, b.func);
            b.ins().call(f, &[av]);
            Ok(Some(None))
        }
        // bytesFromBuffer(p: *const void, n: size_t): u8[]
        "bytesFromBuffer" => {
            let (pv, _) = lower_expr(b, lc, &args[0])?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "bytesFromBuffer ptr is unit".into(),
                    span: args[0].span,
                }
            })?;
            let (nv, nt) = lower_expr(b, lc, &args[1])?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "bytesFromBuffer len is unit".into(),
                    span: args[1].span,
                }
            })?;
            let n_i64 = coerce(b, (nv, nt), JitTy::I64, args[1].span)?;
            let arr = lower_extern_c_array_copy(b, lc, pv, n_i64, JitTy::U8);
            Ok(Some(Some((arr, JitTy::Array(intern_array_kind_u8(lc))))))
        }
        // read{IN,UN,FN}(p: *const void, offset: i64): TN —
        // primitive load at `p + offset` (offset in bytes).
        "readI8" | "readI16" | "readI32" | "readI64" | "readU8" | "readU16" | "readU32"
        | "readU64" | "readF32" | "readF64" => {
            let (pv, _) = lower_expr(b, lc, &args[0])?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: format!("{callee} ptr is unit"),
                    span: args[0].span,
                }
            })?;
            let (ov, ot) = lower_expr(b, lc, &args[1])?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: format!("{callee} offset is unit"),
                    span: args[1].span,
                }
            })?;
            let off_i64 = coerce(b, (ov, ot), JitTy::I64, args[1].span)?;
            let addr = b.ins().iadd(pv, off_i64);
            let (cl, jty) = match callee.as_ref() {
                "readI8" => (I8, JitTy::I8),
                "readI16" => (I16, JitTy::I16),
                "readI32" => (I32, JitTy::I32),
                "readI64" => (I64, JitTy::I64),
                "readU8" => (I8, JitTy::U8),
                "readU16" => (I16, JitTy::U16),
                "readU32" => (I32, JitTy::U32),
                "readU64" => (I64, JitTy::U64),
                "readF32" => (F32, JitTy::F32),
                "readF64" => (F64, JitTy::F64),
                _ => unreachable!(),
            };
            let v = b.ins().load(cl, MemFlags::trusted(), addr, 0);
            Ok(Some(Some((v, jty))))
        }
        // write{IN,UN,FN}(p: *void, offset: i64, value: TN)
        "writeI8" | "writeI16" | "writeI32" | "writeI64" | "writeU8" | "writeU16"
        | "writeU32" | "writeU64" | "writeF32" | "writeF64" => {
            let (pv, _) = lower_expr(b, lc, &args[0])?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: format!("{callee} ptr is unit"),
                    span: args[0].span,
                }
            })?;
            let (ov, ot) = lower_expr(b, lc, &args[1])?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: format!("{callee} offset is unit"),
                    span: args[1].span,
                }
            })?;
            let off_i64 = coerce(b, (ov, ot), JitTy::I64, args[1].span)?;
            let addr = b.ins().iadd(pv, off_i64);
            let (vv, vt) = lower_expr(b, lc, &args[2])?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: format!("{callee} value is unit"),
                    span: args[2].span,
                }
            })?;
            let target = match callee.as_ref() {
                "writeI8" => JitTy::I8,
                "writeI16" => JitTy::I16,
                "writeI32" => JitTy::I32,
                "writeI64" => JitTy::I64,
                "writeU8" => JitTy::U8,
                "writeU16" => JitTy::U16,
                "writeU32" => JitTy::U32,
                "writeU64" => JitTy::U64,
                "writeF32" => JitTy::F32,
                "writeF64" => JitTy::F64,
                _ => unreachable!(),
            };
            let coerced = coerce(b, (vv, vt), target, args[2].span)?;
            b.ins().store(MemFlags::trusted(), coerced, addr, 0);
            Ok(Some(None))
        }
        // fnAddr(f): i64 — code-pointer of an ilang fn for passing
        // into C as a callback. The argument must be a bare fn name
        // (Var); we look it up in the JIT's fn table and emit a
        // `func_addr` instruction so the address is materialised at
        // runtime (post-finalize).
        "fnAddr" => {
            let name = match &args[0].kind {
                ExprKind::Var(n) => n.clone(),
                _ => {
                    return Err(CodegenError::Unsupported {
                        what: "fnAddr argument must be a bare fn name".into(),
                        span: args[0].span,
                    });
                }
            };
            // Try the bare name first, then the module-qualified
            // form a prefix pass might have produced.
            let resolved = lc.funcs.get(&name).map(|t| t.0).or_else(|| {
                lc.funcs.iter().find_map(|(k, t)| {
                    if k == &name || k.as_str().ends_with(&format!(".{name}")) {
                        Some(t.0)
                    } else {
                        None
                    }
                })
            });
            let func_id = resolved.ok_or_else(|| CodegenError::Unsupported {
                what: format!("fnAddr: unknown fn `{name}`"),
                span: args[0].span,
            })?;
            let func_ref = lc.module.declare_func_in_func(func_id, b.func);
            let addr = b.ins().func_addr(I64, func_ref);
            Ok(Some(Some((addr, JitTy::I64))))
        }
        // arrayFromCArray<T>(p: *const T, n: size_t): T[]
        "arrayFromCArray" => {
            let elem = resolve_type_arg_t(lc, call_span)?;
            let elem_jty = jit_ty_from_primitive_for_helper(&elem, call_span)?;
            let (pv, _) = lower_expr(b, lc, &args[0])?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "arrayFromCArray ptr is unit".into(),
                    span: args[0].span,
                }
            })?;
            let (nv, nt) = lower_expr(b, lc, &args[1])?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "arrayFromCArray len is unit".into(),
                    span: args[1].span,
                }
            })?;
            let n_i64 = coerce(b, (nv, nt), JitTy::I64, args[1].span)?;
            let arr = lower_extern_c_array_copy(b, lc, pv, n_i64, elem_jty);
            let arr_id = crate::ty::intern_array_kind(
                lc.array_kinds,
                crate::ty::ArrayKind { elem: elem_jty, fixed: None },
            );
            Ok(Some(Some((arr, JitTy::Array(arr_id)))))
        }
        // cstrArrayToStrings(p: *const *const char): string[]
        "cstrArrayToStrings" => {
            let (pv, _) = lower_expr(b, lc, &args[0])?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "cstrArrayToStrings ptr is unit".into(),
                    span: args[0].span,
                }
            })?;
            // Element is JitTy::Str; per-element drop_fn comes from
            // the array's per-kind drop wrapper (release_string).
            let arr_id = crate::ty::intern_array_kind(
                lc.array_kinds,
                crate::ty::ArrayKind { elem: JitTy::Str, fixed: None },
            );
            let drop_fn_ptr = crate::drops::array_drop_fn_ptr(b, lc, arr_id);
            let f = lc
                .module
                .declare_func_in_func(lc.strfns.cstr_array_to_strings, b.func);
            let c = b.ins().call(f, &[pv, drop_fn_ptr]);
            Ok(Some(Some((b.inst_results(c)[0], JitTy::Array(arr_id)))))
        }
        // errnoCheck(rc: i32): i32?
        "errnoCheck" => {
            let (rcv, _) = lower_expr(b, lc, &args[0])?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "errnoCheck arg is unit".into(),
                    span: args[0].span,
                }
            })?;
            Ok(Some(Some(lower_errno_check(b, lc, rcv, JitTy::I32))))
        }
        // errnoCheckI64(rc: i64): i64?
        "errnoCheckI64" => {
            let (rcv, _) = lower_expr(b, lc, &args[0])?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "errnoCheckI64 arg is unit".into(),
                    span: args[0].span,
                }
            })?;
            Ok(Some(Some(lower_errno_check(b, lc, rcv, JitTy::I64))))
        }
        _ => Ok(None),
    }
}

/// Allocate an ilang `T[]` of length `n_i64` and memcpy
/// `n_i64 * sizeof(elem)` bytes from `src_ptr`. Returns the array
/// header pointer.
fn lower_extern_c_array_copy(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    src_ptr: cranelift::prelude::Value,
    n_i64: cranelift::prelude::Value,
    elem: JitTy,
) -> cranelift::prelude::Value {
    let elem_size = elem.size_bytes() as i64;
    let new_ref = lc.module.declare_func_in_func(lc.arrfns.new, b.func);
    let elem_size_v = b.ins().iconst(I64, elem_size);
    let drop_fn = b.ins().iconst(I64, 0);
    let alloc_call = b.ins().call(new_ref, &[elem_size_v, n_i64, drop_fn]);
    let header = b.inst_results(alloc_call)[0];
    let dst = b.ins().load(
        I64,
        MemFlags::trusted(),
        header,
        crate::runtime::ARRAY_DATA_OFFSET,
    );
    let total_bytes = b.ins().imul_imm(n_i64, elem_size);
    b.call_memcpy(lc.module.target_config(), dst, src_ptr, total_bytes);
    header
}

/// Branch on `rc < 0` (signed) and produce a heap-boxed `Optional<T>`:
/// failure → 0 (None sentinel), success → boxed primitive.
fn lower_errno_check(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    rc: cranelift::prelude::Value,
    inner: JitTy,
) -> TV {
    let zero_inner = match inner {
        JitTy::I32 => b.ins().iconst(I32, 0),
        JitTy::I64 => b.ins().iconst(I64, 0),
        _ => unreachable!("errnoCheck inner restricted to i32/i64"),
    };
    let is_fail = b.ins().icmp(IntCC::SignedLessThan, rc, zero_inner);
    let fail_bb = b.create_block();
    let ok_bb = b.create_block();
    let merge = b.create_block();
    b.append_block_param(merge, I64);
    b.ins().brif(is_fail, fail_bb, &[], ok_bb, &[]);
    b.switch_to_block(fail_bb);
    b.seal_block(fail_bb);
    let none_v = b.ins().iconst(I64, 0);
    b.ins().jump(merge, &[none_v.into()]);
    b.switch_to_block(ok_bb);
    b.seal_block(ok_bb);
    let size_v = b.ins().iconst(I64, inner.size_bytes() as i64);
    let new_ref = lc.module.declare_func_in_func(lc.optional_box_new_id, b.func);
    let alloc_call = b.ins().call(new_ref, &[size_v]);
    let box_ptr = b.inst_results(alloc_call)[0];
    b.ins().store(
        MemFlags::trusted(),
        rc,
        box_ptr,
        crate::runtime::OPT_PRIM_PAYLOAD_OFFSET,
    );
    b.ins().jump(merge, &[box_ptr.into()]);
    b.switch_to_block(merge);
    b.seal_block(merge);
    let opt_id = crate::ty::intern_optional_inner(lc.optional_inners, inner);
    (b.block_params(merge)[0], JitTy::Optional(opt_id))
}

/// Lower `x is T` (`downcast = false`) and `x as? T`
/// (`downcast = true`).
///
/// Currently restricted to **class targets**: the whole point of
/// `is` is the parent-chain walk, and that machinery only exists
/// for object types. Non-class T is rejected at codegen.
fn lower_type_test_or_downcast(
    b: &mut FunctionBuilder<'_>,
    lc: &mut LowerCtx,
    expr: &Expr,
    target: &ilang_ast::Type,
    downcast: bool,
    span: ilang_ast::Span,
) -> Result<TV, CodegenError> {
    let target_class = match target {
        ilang_ast::Type::Object(name) => *name,
        _ => {
            return Err(CodegenError::Unsupported {
                what: "is / as? require a class target type".into(),
                span,
            });
        }
    };
    let target_class_id = match lc.class_layouts.iter().position(|c| c.name == target_class) {
        Some(i) => i as u32,
        None => {
            return Err(CodegenError::Unsupported {
                what: format!("is / as? unknown class {target_class:?}"),
                span,
            });
        }
    };
    let target_meta_addr =
        lc.class_layouts[target_class_id as usize].name.as_str();
    let _ = target_meta_addr; // (silence unused; actual address fetched below)
    // Address of the target class's TypeMeta — stored in the
    // vtable header for that class. Computed as
    // `class_vtable_addrs[id] - 8`.
    let target_meta_ptr_addr = lc.class_vtable_addrs[target_class_id as usize] - 8;
    let target_meta_ptr = unsafe { *(target_meta_ptr_addr as *const i64) };

    let (val_v, val_t) = lower_expr(b, lc, expr)?.ok_or_else(|| CodegenError::Unsupported {
        what: "is / as? receiver is unit".into(),
        span,
    })?;
    let _val_class_id = match val_t {
        JitTy::Object(id) => id,
        _ => {
            return Err(CodegenError::Unsupported {
                what: "is / as? receiver must be a class instance".into(),
                span,
            });
        }
    };
    // Read the receiver's dynamic TypeMeta from its vtable header.
    let vt = b.ins().load(
        I64,
        MemFlags::trusted(),
        val_v,
        crate::runtime::VTABLE_OFFSET as i32,
    );
    let dyn_meta = b.ins().load(I64, MemFlags::trusted(), vt, -8);
    let target_meta_v = b.ins().iconst(I64, target_meta_ptr);
    let r = lc.module.declare_func_in_func(lc.type_is_subtype, b.func);
    let call = b.ins().call(r, &[dyn_meta, target_meta_v]);
    let matched_i8 = b.inst_results(call)[0];
    if downcast {
        // Build Optional<Object(target_class_id)> as nullable
        // pointer: matched ? val_v : 0. Retain on success so the
        // resulting Optional owns its own reference.
        let zero = b.ins().iconst(I64, 0);
        let cond_i8 = matched_i8;
        let cond = b.ins().icmp_imm(IntCC::NotEqual, cond_i8, 0);
        // Use cranelift `select` for the value choice; if matched,
        // we also need to bump the rc once. Easiest: do retain
        // unconditionally on val_v before the select (we'll be
        // dropping the `none` branch's retained ref ... no, that's
        // wrong). Use brif to two blocks.
        let then_blk = b.create_block();
        let else_blk = b.create_block();
        let join_blk = b.create_block();
        b.append_block_param(join_blk, I64);
        b.ins().brif(cond, then_blk, &[], else_blk, &[]);
        b.switch_to_block(then_blk);
        b.seal_block(then_blk);
        // Matched: retain val (it's already a JitTy::Object, no
        // class-id sub-dispatch needed) so the Optional owns its ref.
        emit_retain_heap(b, lc, val_v, val_t);
        b.ins().jump(join_blk, &[val_v.into()]);
        b.switch_to_block(else_blk);
        b.seal_block(else_blk);
        b.ins().jump(join_blk, &[zero.into()]);
        b.switch_to_block(join_blk);
        b.seal_block(join_blk);
        let opt_id = crate::ty::intern_optional_inner(
            lc.optional_inners,
            JitTy::Object(target_class_id),
        );
        // Release the original receiver (we either retained
        // through the then-branch into the new Optional, or it
        // wasn't transferred — same release rule as `as`).
        if !is_aliased_heap_source(&expr.kind) {
            emit_release_heap(b, lc, val_v, val_t);
        }
        let result = b.block_params(join_blk)[0];
        Ok((result, JitTy::Optional(opt_id)))
    } else {
        // `is` returns bool. Release the receiver as usual.
        if !is_aliased_heap_source(&expr.kind) {
            emit_release_heap(b, lc, val_v, val_t);
        }
        // matched_i8 is i8 (0 or 1); JitTy::Bool is also i8 in cl.
        Ok((matched_i8, JitTy::Bool))
    }
}

/// Lower `Type.fieldType(name)` / `methodReturn(name)` /
/// `methodParams(name)`. The receiver TypeMeta carries parallel
/// `(names, values)` saturated arrays per kind; the runtime
/// helper does a linear-scan lookup and returns 0 (none) on miss.
fn lower_type_member_lookup(
    b: &mut FunctionBuilder<'_>,
    lc: &mut LowerCtx,
    obj_v: cranelift::prelude::Value,
    method: &str,
    arg_expr: &Expr,
    span: ilang_ast::Span,
) -> Result<TV, CodegenError> {
    let (names_off, values_off, ret_inner) = match method {
        "fieldType" => (
            crate::runtime::TYPE_META_FIELDS_OFFSET,
            crate::runtime::TYPE_META_FIELD_TYPES_OFFSET,
            JitTy::TypeRef,
        ),
        "methodReturn" => (
            crate::runtime::TYPE_META_METHODS_OFFSET,
            crate::runtime::TYPE_META_METHOD_RETURNS_OFFSET,
            JitTy::TypeRef,
        ),
        "methodParams" => {
            // Inner element type is `Type[]` itself; the Optional
            // wraps the outer array kind.
            let inner_array_id = crate::ty::intern_array_kind(
                lc.array_kinds,
                crate::ty::ArrayKind { elem: JitTy::TypeRef, fixed: None },
            );
            (
                crate::runtime::TYPE_META_METHODS_OFFSET,
                crate::runtime::TYPE_META_METHOD_PARAMS_OFFSET,
                JitTy::Array(inner_array_id),
            )
        }
        _ => unreachable!("caller filtered"),
    };
    // Lower the (string) argument and feed it to the lookup helper.
    let (arg_v, arg_t) =
        lower_expr(b, lc, arg_expr)?.ok_or_else(|| CodegenError::Unsupported {
            what: "Type lookup arg is unit".into(),
            span,
        })?;
    if arg_t != JitTy::Str {
        return Err(CodegenError::Unsupported {
            what: format!("Type.{method} expects a string arg"),
            span,
        });
    }
    let names = b.ins().load(I64, MemFlags::trusted(), obj_v, names_off);
    let values = b.ins().load(I64, MemFlags::trusted(), obj_v, values_off);
    let r = lc.module.declare_func_in_func(lc.type_lookup, b.func);
    let call = b.ins().call(r, &[names, values, arg_v]);
    let result_ptr = b.inst_results(call)[0];
    if !is_aliased_heap_source(&arg_expr.kind) {
        emit_release_string(b, lc, arg_v);
    }
    let opt_id = crate::ty::intern_optional_inner(lc.optional_inners, ret_inner);
    Ok((result_ptr, JitTy::Optional(opt_id)))
}

/// Lower `typeof(arg)`: evaluate `arg` (so heap retains/releases
/// happen normally) and produce a `JitTy::TypeRef` whose pointer
/// is the `TypeMeta*` for the value's runtime type.
///
/// All non-class types resolve to a compile-time-known TypeMeta
/// pointer (their static and dynamic types coincide). Class
/// receivers go through the vtable: each class's vtable header
/// stores its TypeMeta pointer at `vtable_ptr - 8`, so a
/// `Parent`-typed slot holding a `Child` value reports the
/// dynamic class.
fn lower_typeof(
    b: &mut FunctionBuilder<'_>,
    lc: &mut LowerCtx,
    arg: &Expr,
) -> Result<TV, CodegenError> {
    let (arg_v, arg_t) = lower_expr(b, lc, arg)?.ok_or_else(|| CodegenError::Unsupported {
        what: "typeof argument is unit".into(),
        span: arg.span,
    })?;
    let release = !is_aliased_heap_source(&arg.kind);
    let meta_addr = match arg_t {
        JitTy::Object(_) => {
            // Read the dynamic class's TypeMeta pointer from the
            // vtable header (slot at `vtable_ptr - 8`). The vtable
            // pointer itself lives at `obj_ptr - 8` (VTABLE_OFFSET).
            let vt = b.ins().load(
                I64,
                MemFlags::trusted(),
                arg_v,
                crate::runtime::VTABLE_OFFSET as i32,
            );
            let meta = b.ins().load(I64, MemFlags::trusted(), vt, -8);
            if release {
                emit_release_heap(b, lc, arg_v, arg_t);
            }
            return Ok((meta, JitTy::TypeRef));
        }
        JitTy::Weak(_) => *lc.prim_type_meta_addrs.get("weak").expect("weak meta"),
        JitTy::Map(_) => *lc.prim_type_meta_addrs.get("Map").expect("Map meta"),
        JitTy::Enum(eid) | JitTy::EnumHeap(eid) => lc.enum_type_meta_addrs[eid as usize],
        JitTy::Optional(_) => *lc.prim_type_meta_addrs.get("optional").expect("optional meta"),
        JitTy::Array(_) | JitTy::EmbeddedArray(_) | JitTy::FlexArray(_) => {
            *lc.prim_type_meta_addrs.get("array").expect("array meta")
        }
        JitTy::Tuple(_) => *lc.prim_type_meta_addrs.get("tuple").expect("tuple meta"),
        JitTy::Fn(_) => *lc.prim_type_meta_addrs.get("fn").expect("fn meta"),
        JitTy::Str => *lc.prim_type_meta_addrs.get("string").expect("string meta"),
        JitTy::Bool => *lc.prim_type_meta_addrs.get("bool").expect("bool meta"),
        JitTy::I8 => *lc.prim_type_meta_addrs.get("i8").expect("i8 meta"),
        JitTy::I16 => *lc.prim_type_meta_addrs.get("i16").expect("i16 meta"),
        JitTy::I32 => *lc.prim_type_meta_addrs.get("i32").expect("i32 meta"),
        JitTy::I64 => *lc.prim_type_meta_addrs.get("i64").expect("i64 meta"),
        JitTy::U8 => *lc.prim_type_meta_addrs.get("u8").expect("u8 meta"),
        JitTy::U16 => *lc.prim_type_meta_addrs.get("u16").expect("u16 meta"),
        JitTy::U32 => *lc.prim_type_meta_addrs.get("u32").expect("u32 meta"),
        JitTy::U64 => *lc.prim_type_meta_addrs.get("u64").expect("u64 meta"),
        JitTy::F32 => *lc.prim_type_meta_addrs.get("f32").expect("f32 meta"),
        JitTy::F64 => *lc.prim_type_meta_addrs.get("f64").expect("f64 meta"),
        JitTy::TypeRef => *lc.prim_type_meta_addrs.get("Type").expect("Type meta"),
        JitTy::Unit => *lc.prim_type_meta_addrs.get("()").expect("unit meta"),
    };
    if release && arg_t.is_heap() {
        emit_release_heap(b, lc, arg_v, arg_t);
    }
    let meta = b.ins().iconst(I64, meta_addr);
    Ok((meta, JitTy::TypeRef))
}

fn intern_array_kind_u8(lc: &mut LowerCtx) -> u32 {
    crate::ty::intern_array_kind(
        lc.array_kinds,
        crate::ty::ArrayKind { elem: JitTy::U8, fixed: None },
    )
}

/// Resolve the inferred type argument T for a generic helper call
/// (e.g. `arrayFromCArray<T>(...)`) from the type checker's recorded
/// `fn_call_type_args[span]`. Errors clearly if T wasn't inferred.
fn resolve_type_arg_t(
    lc: &LowerCtx,
    call_span: ilang_ast::Span,
) -> Result<ilang_ast::Type, CodegenError> {
    match lc.fn_call_type_args.get(&call_span) {
        Some((_, args)) if args.len() == 1 => Ok(args[0].clone()),
        _ => Err(CodegenError::Unsupported {
            what: "generic helper requires an explicit type argument (e.g. arrayFromCArray<i32>(p, n))".into(),
            span: call_span,
        }),
    }
}

/// Map an AST primitive type to a `JitTy` for use as the element
/// type of `arrayFromCArray<T>`. Rejects non-primitive Ts since
/// the helper only does flat memcpy (no per-element marshalling).
fn jit_ty_from_primitive_for_helper(
    t: &ilang_ast::Type,
    span: ilang_ast::Span,
) -> Result<JitTy, CodegenError> {
    use ilang_ast::Type as T;
    match t {
        T::I8 => Ok(JitTy::I8),
        T::I16 => Ok(JitTy::I16),
        T::I32 => Ok(JitTy::I32),
        T::I64 => Ok(JitTy::I64),
        T::U8 => Ok(JitTy::U8),
        T::U16 => Ok(JitTy::U16),
        T::U32 => Ok(JitTy::U32),
        T::U64 => Ok(JitTy::U64),
        T::F32 => Ok(JitTy::F32),
        T::F64 => Ok(JitTy::F64),
        T::Bool => Ok(JitTy::Bool),
        other => Err(CodegenError::Unsupported {
            what: format!(
                "arrayFromCArray<T>: T must be a numeric primitive or bool (got {other})"
            ),
            span,
        }),
    }
}
