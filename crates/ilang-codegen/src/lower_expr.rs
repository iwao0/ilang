//! Expression lowering — the big `match e.kind` plus its handful of
//! companion helpers (`lower_array_literal`, `build_array`,
//! `lower_console_log`, `call_method`).

use cranelift::prelude::*;
use cranelift_codegen::ir::types::{F32, F64, I16, I32, I64, I8};
use cranelift_module::Module;
use ilang_ast::{Expr, ExprKind};

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
            if let Some(&(var, vt)) = lc.env.bindings.get(name) {
                return Ok(Some((b.use_var(var), vt)));
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
            // First-class function: a bare reference to a top-level fn
            // becomes its code-address as a function pointer.
            if let Some(entry) = lc.funcs.get(name).cloned() {
                let (id, params, ret) = entry;
                let func_ref = lc.module.declare_func_in_func(id, b.func);
                let addr = b.ins().func_addr(I64, func_ref);
                let sig_id = crate::ty::intern_fn_sig(
                    lc.fn_signatures,
                    crate::ty::FnSignature { params, ret },
                );
                return Ok(Some((addr, JitTy::Fn(sig_id))));
            }
            Err(CodegenError::Unsupported {
                what: format!("unknown variable {name:?}"),
                span: e.span,
            })
        }
        ExprKind::FnExpr { .. } => {
            // Hoisting pass should have replaced this with Var(synth).
            Err(CodegenError::Unsupported {
                what: "anonymous function reached lowering — hoist pass failed".into(),
                span: e.span,
            })
        }
        ExprKind::MapLit(entries) => lower_map_lit(b, lc, entries, e.span),
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
            )?;
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
            lower_for_in(b, lc, var, iter, body)?;
            Ok(None)
        }
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
        } => lower_enum_ctor(b, lc, enum_name, variant, args, e.span),
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
                lc.class_methods[class_id as usize].get(&prop_key).cloned()
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
                lc.class_methods[class_id as usize].get(&prop_key).cloned()
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
            }
            let (obj_v, obj_t) = lower_expr(b, lc, obj)?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "method receiver is unit".into(),
                    span: obj.span,
                }
            })?;
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
            // Built-in Optional methods. The Optional value is a
            // nullable pointer (i64); these inspect it without
            // touching rc.
            if let JitTy::Optional(id) = obj_t {
                let zero = b.ins().iconst(I64, 0);
                match method.as_str() {
                    "isSome" => {
                        let v = b.ins().icmp(IntCC::NotEqual, obj_v, zero);
                        return Ok(Some((v, JitTy::Bool)));
                    }
                    "isNone" => {
                        let v = b.ins().icmp(IntCC::Equal, obj_v, zero);
                        return Ok(Some((v, JitTy::Bool)));
                    }
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
                        b, lc, obj_v, obj, &args[0], id, elem_jty, method,
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
                return lower_map_method(b, lc, map_id, method, obj_v, args, e.span);
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
            call_method(b, lc, class_id, method, obj_v, args, e.span)
        }
        ExprKind::Call { callee, args } => {
            // Indirect call through a function-typed local. Matches the
            // type checker's lookup order — a `let` shadows top-level
            // fns of the same name.
            if let Some(&(var, JitTy::Fn(sig_id))) = lc.env.bindings.get(callee) {
                let sig = lc.fn_signatures[sig_id as usize].clone();
                let mut arg_vals = Vec::with_capacity(args.len());
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
                // Build the Cranelift signature for call_indirect.
                let mut cl_sig = lc.module.make_signature();
                for p in &sig.params {
                    cl_sig.params.push(cranelift::prelude::AbiParam::new(
                        p.cl().expect("non-unit param"),
                    ));
                }
                if let Some(rt) = sig.ret.cl() {
                    cl_sig.returns.push(cranelift::prelude::AbiParam::new(rt));
                }
                let sig_ref = b.import_signature(cl_sig);
                let callee_v = b.use_var(var);
                let call = b.ins().call_indirect(sig_ref, callee_v, &arg_vals);
                if matches!(sig.ret, JitTy::Unit) {
                    return Ok(None);
                }
                return Ok(Some((b.inst_results(call)[0], sig.ret)));
            }
            // Free function first.
            if let Some(entry) = lc.funcs.get(callee).cloned() {
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
            // Implicit method call on `this`.
            if let Some((this_var, class_id)) = lc.this {
                if lc.class_methods[class_id as usize].contains_key(callee) {
                    let this_v = b.use_var(this_var);
                    return call_method(b, lc, class_id, callee, this_v, args, e.span);
                }
            }
            Err(CodegenError::Unsupported {
                what: format!("unknown function {callee:?}"),
                span: e.span,
            })
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
            let size_v = b.ins().iconst(I64, size);
            let alloc_call = b.ins().call(alloc_ref, &[size_v, drop_fn_ptr]);
            let ptr = b.inst_results(alloc_call)[0];
            // If init exists, call it. The mangler may have set
            // `init_method` to a specific overload (e.g. "init__i64");
            // fall back to plain "init" otherwise.
            let init_lookup: &str = init_method.as_deref().unwrap_or("init");
            if lc.class_methods[class_id as usize].contains_key(init_lookup) {
                let _ = call_method(b, lc, class_id, init_lookup, ptr, args, e.span)?;
            } else if !args.is_empty() {
                return Err(CodegenError::Unsupported {
                    what: format!("no `init` for class {class}, but args were given"),
                    span: e.span,
                });
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
        } => lower_if_let(b, lc, name, expr, then_branch, else_branch.as_deref()),
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
    let id = match enum_ids_from(lc).get(enum_name).copied() {
        Some(id) => id,
        None => {
            return Err(CodegenError::Unsupported {
                what: format!("unknown enum {enum_name:?}"),
                span,
            });
        }
    };
    let layout = lc.enum_layouts[id as usize].clone();
    let tag = layout
        .variants
        .iter()
        .position(|v| v == variant)
        .ok_or_else(|| CodegenError::Unsupported {
            what: format!("enum {enum_name:?} has no variant {variant:?}"),
            span,
        })? as i64;

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
    let alloc_call = b.ins().call(alloc_ref, &[size_v, drop_fn_v]);
    let ptr = b.inst_results(alloc_call)[0];
    // Write tag.
    let tag_v = b.ins().iconst(I32, tag);
    b.ins()
        .store(MemFlags::trusted(), tag_v, ptr, ENUM_TAG_OFFSET);
    // Write payload fields per variant kind.
    let variant_layout = layout.payloads[tag as usize].clone();
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
                let (av, at) = lower_expr(b, lc, expr)?.ok_or_else(|| {
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
        let tag = layout
            .variants
            .iter()
            .position(|v| *v == variant_name)
            .expect("type checker validated variant") as i64;
        let want = b.ins().iconst(I32, tag);
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
        let mut shadows: Vec<(String, Option<(Variable, JitTy)>)> = Vec::new();
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
                                let var = Variable::new(lc.env.next_var_id());
                                b.declare_var(var, cl);
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
                                    let var = Variable::new(lc.env.next_var_id());
                                    b.declare_var(var, cl);
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
                b.ins().jump(merge, &[v]);
            }
            (Some((v, vt)), Some(prev_ty)) => {
                let v = coerce(b, (v, vt), prev_ty, arm.span)?;
                b.ins().jump(merge, &[v]);
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

    let then_block = b.create_block();
    let else_block = b.create_block();

    let zero = b.ins().iconst(I64, 0);
    let cond = b.ins().icmp(IntCC::NotEqual, scrut_v, zero);
    b.ins().brif(cond, then_block, &[], else_block, &[]);

    // Then branch: bind `name` to the unwrapped value.
    //   heap inner: scrut_v IS the pointer; retain so the binding owns
    //               its own +1 (block-end release balances).
    //   primitive inner: scrut_v is a Box<[rc | payload]>; load the
    //                    payload at OPT_PRIM_PAYLOAD_OFFSET. No ARC on
    //                    the binding (it's a primitive copy).
    b.switch_to_block(then_block);
    b.seal_block(then_block);
    let var = Variable::new(lc.env.next_var_id());
    let cl_ty = inner_jty.cl().expect("non-unit inner");
    b.declare_var(var, cl_ty);
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
    let prev = lc.env.bindings.insert(name.to_string(), (var, inner_jty));
    let then_val = lower_block_value(b, lc, then_branch)?;
    // Restore the prior binding.
    match prev {
        Some(p) => {
            lc.env.bindings.insert(name.to_string(), p);
        }
        None => {
            lc.env.bindings.remove(name);
        }
    }

    // Merge block: gather a value from both branches if the type is
    // non-unit. Mirrors lower_if.
    let merge = b.create_block();
    let merge_param = match then_val {
        Some((v, _)) => Some(b.append_block_param(merge, b.func.dfg.value_type(v))),
        None => None,
    };
    if let Some((v, _)) = then_val {
        b.ins().jump(merge, &[v]);
    } else {
        b.ins().jump(merge, &[]);
    }

    b.switch_to_block(else_block);
    b.seal_block(else_block);
    let else_val = match else_branch {
        Some(e) => lower_expr(b, lc, e)?,
        None => None,
    };
    match (then_val, else_val) {
        (Some((_, tt)), Some((ev, _et))) => {
            let ev_coerced = coerce(b, (ev, _et), tt, scrut.span)?;
            b.ins().jump(merge, &[ev_coerced]);
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
            b.ins().jump(merge, &[zero]);
            b.switch_to_block(merge);
            b.seal_block(merge);
            Ok(None)
        }
        (None, _) => {
            b.ins().jump(merge, &[]);
            b.switch_to_block(merge);
            b.seal_block(merge);
            Ok(None)
        }
    }
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
            let class_name = lc.class_layouts[class_id as usize].name.clone();
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
                        let mut sorted: Vec<(&String, &(u32, JitTy))> = map.iter().collect();
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
        JitTy::Unit => {
            return Err(CodegenError::Unsupported {
                what: "console.log of () (unit)".into(),
                span,
            });
        }
    }
    Ok(())
}

/// Print a static string literal by interning it and routing through
/// `print_str`. Cheap — each unique fragment is interned once.
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
    let class_name = lc.class_layouts[class_id as usize].name.clone();
    // Snapshot the field list so we don't borrow `lc.class_layouts`
    // through the recursive emit_print_value call below.
    let mut fields: Vec<(String, u32, JitTy)> = lc.class_layouts[class_id as usize]
        .fields
        .iter()
        .map(|(name, &(offset, fty))| (name.clone(), offset, fty))
        .collect();
    fields.sort_by(|a, b| a.0.cmp(&b.0));

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
    let i_var = Variable::new(lc.env.next_var_id());
    b.declare_var(i_var, I64);
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
        .get(method)
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
        lc.array_kinds, lc.optional_inners, lc.fn_signatures, lc.map_kinds,
    )?;
    let val_jty = JitTy::from_ast(
        &type_args[1], span, &class_ids, &enum_ids, lc.enum_layouts,
        lc.array_kinds, lc.optional_inners, lc.fn_signatures, lc.map_kinds,
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

    // Build cranelift signature for indirect call.
    let mut cl_sig = lc.module.make_signature();
    cl_sig.params.push(cranelift::prelude::AbiParam::new(
        sig.params[0].cl().ok_or_else(|| CodegenError::Unsupported {
            what: format!("{method} fn param has unit type"), span: fn_arg.span,
        })?,
    ));
    if let Some(rt) = ret_jty.cl() {
        cl_sig.returns.push(cranelift::prelude::AbiParam::new(rt));
    }
    let sig_ref = b.import_signature(cl_sig);

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
    let call = b.ins().call_indirect(sig_ref, fv, &[elem]);
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
    // The panic helper is `extern "C" fn(...) -> !` but Cranelift can't
    // see that — emit a trap so the verifier knows we don't fall through.
    b.ins().trap(cranelift_codegen::ir::TrapCode::user(1).expect("trap code"));
    b.switch_to_block(ok);
    b.seal_block(ok);
}
