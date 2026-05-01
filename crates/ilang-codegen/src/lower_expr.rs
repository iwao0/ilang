//! Expression lowering — the big `match e.kind` plus its handful of
//! companion helpers (`lower_array_literal`, `build_array`,
//! `lower_console_log`, `call_method`).

use cranelift::prelude::*;
use cranelift_codegen::ir::types::{I64, I8};
use cranelift_module::Module;
use ilang_ast::{Expr, ExprKind};

use crate::arc::{emit_bind_retain, emit_release_heap, emit_retain_object};
use crate::env::{class_ids_from, LowerCtx};
use crate::error::CodegenError;
use crate::lower_ctrl::{lower_if, lower_loop, lower_while};
use crate::lower_op::{coerce, lower_binary, lower_logical, lower_unary};
use crate::lower_stmt::lower_block_value;
use crate::runtime::{ARRAY_DATA_OFFSET, ARRAY_LEN_OFFSET};
use crate::ty::{intern_array_kind, intern_optional_inner, ArrayKind, JitTy, TV};

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
            Err(CodegenError::Unsupported {
                what: format!("unknown variable {name:?}"),
                span: e.span,
            })
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
                lc.array_kinds,
                lc.optional_inners,
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
        ExprKind::Loop { body } => {
            lower_loop(b, lc, body)?;
            Ok(None)
        }
        ExprKind::Break => {
            let target = lc.loops.last().ok_or_else(|| CodegenError::Unsupported {
                what: "break outside loop".into(),
                span: e.span,
            })?.1;
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
            let class_id = match obj_t {
                JitTy::Object(id) => id,
                _ => {
                    return Err(CodegenError::Unsupported {
                        what: "field access on non-object".into(),
                        span: obj.span,
                    });
                }
            };
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
                    "is_some" => {
                        let v = b.ins().icmp(IntCC::NotEqual, obj_v, zero);
                        return Ok(Some((v, JitTy::Bool)));
                    }
                    "is_none" => {
                        let v = b.ins().icmp(IntCC::Equal, obj_v, zero);
                        return Ok(Some((v, JitTy::Bool)));
                    }
                    "unwrap" => {
                        // No null-check codegen — relies on the user
                        // having checked `is_some()` first. Calling on
                        // `none` reads address 0 downstream (segfault).
                        let inner = lc.optional_inners[id as usize];
                        return Ok(Some((obj_v, inner)));
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
            // Coerce index to i64; bounds-checking elided in MVP.
            let idx_i64 = coerce(b, (idx_v, idx_t), JitTy::I64, index.span)?;
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
        ExprKind::New { class, args } => {
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
            // If init exists, call it.
            if lc.class_methods[class_id as usize].contains_key("init") {
                let _ = call_method(b, lc, class_id, "init", ptr, args, e.span)?;
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
            if !vt.is_heap() {
                return Err(CodegenError::Unsupported {
                    what: format!("some({vt:?}) — JIT only supports Optional of heap types"),
                    span: e.span,
                });
            }
            let id = intern_optional_inner(lc.optional_inners, vt);
            Ok(Some((v, JitTy::Optional(id))))
        }
        ExprKind::IfLet {
            name,
            expr,
            then_branch,
            else_branch,
        } => lower_if_let(b, lc, name, expr, then_branch, else_branch.as_deref()),
    }
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

    // Then branch: bind `name` to the unwrapped pointer with the inner
    // heap type. Retain so the binding has its own +1 (block-end release
    // in lower_block_value will balance it).
    b.switch_to_block(then_block);
    b.seal_block(then_block);
    let var = Variable::new(lc.env.next_var_id());
    b.declare_var(var, I64);
    b.def_var(var, scrut_v);
    crate::arc::emit_retain_heap(b, lc, scrut_v, inner_jty);
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
            // Format as `<ClassName @ 0xPTR>` to match JitValue::Display.
            let prefix = format!("<{} @ ", lc.class_layouts[class_id as usize].name);
            emit_print_literal(b, lc, &prefix);
            let r = lc.module.declare_func_in_func(lc.print.ptr_hex, b.func);
            b.ins().call(r, &[v]);
            emit_print_literal(b, lc, ">");
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
