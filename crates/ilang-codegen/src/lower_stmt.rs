//! Statement and block lowering. Statements feed into block-value
//! lowering, which is where scope-exit ARC release lives.

use cranelift::prelude::*;
use ilang_ast::{ExprKind, Stmt, StmtKind, Type, Symbol};

use cranelift_module::Module;

use crate::arc::{emit_bind_retain, emit_release_heap, emit_retain_heap, is_aliased_heap_source};
use crate::env::{class_ids_from, enum_ids_from, LowerCtx};
use crate::error::CodegenError;
use crate::lower_expr::{lower_array_literal, lower_expr, lower_value_with_target};
use crate::lower_op::coerce;
use crate::ty::{JitTy, TV};

pub(crate) fn lower_stmt(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    s: &Stmt,
) -> Result<(), CodegenError> {
    match &s.kind {
        StmtKind::Let { name, ty, value } => {
            // Special-case `let a: T[] = [...]` so the literal is built
            // with the annotated element type from the start. Otherwise
            // the array would be allocated with the literal's natural
            // element type (i64 from `1`) and the strides wouldn't match
            // the bind type's element width.
            let lowered = if let (
                Some(Type::Array { elem: target_elem, .. }),
                ExprKind::Array(elements),
            ) = (ty.as_ref(), &value.kind)
            {
                let target_elem_jty = JitTy::from_ast(
                    target_elem,
                    value.span,
                    &class_ids_from(lc),
                    &enum_ids_from(lc),
                    lc.enum_layouts,
                    lc.array_kinds,
                    lc.optional_inners,
                lc.fn_signatures,
                lc.map_kinds,
                lc.tuple_kinds,
                )?;
                Some(lower_array_literal(b, lc, elements, target_elem_jty, value.span)?)
            } else {
                None
            };
            // Convert the annotation (if any) to a JitTy and use
            // `lower_value_with_target` so empty array literals
            // wrapped inside `some(...)` see their element type
            // from the let's annotation.
            let target_jty: Option<JitTy> = match ty.as_ref() {
                Some(t) => Some(JitTy::from_ast(
                    t,
                    s.span,
                    &class_ids_from(lc),
                    &enum_ids_from(lc),
                    lc.enum_layouts,
                    lc.array_kinds,
                    lc.optional_inners,
                    lc.fn_signatures,
                    lc.map_kinds,
                    lc.tuple_kinds,
                )?),
                None => None,
            };
            let lowered_or_raw = match lowered {
                Some(tv) => Some(tv),
                None => match target_jty {
                    Some(t) => lower_value_with_target(b, lc, value, t)?,
                    None => lower_expr(b, lc, value)?,
                },
            };
            // Unit RHS (`let x = loop {...}`, `let x = console.log(...)`,
            // `let x = if true {} else {}`, etc.): the RHS produces no
            // Cranelift value. Match the interpreter, which binds `x` to
            // Unit. We don't allocate a Variable (Unit has no width);
            // the name is tracked in `unit_bindings` so later references
            // resolve to `Ok(None)`.
            let (val, vt) = match lowered_or_raw {
                Some(tv) => tv,
                None => {
                    lc.env.unit_bindings.insert(name.clone());
                    return Ok(());
                }
            };
            let bind_ty = match ty {
                Some(t) => JitTy::from_ast(
                    t,
                    s.span,
                    &class_ids_from(lc),
                    &enum_ids_from(lc),
                    lc.enum_layouts,
                    lc.array_kinds,
                    lc.optional_inners,
                lc.fn_signatures,
                lc.map_kinds,
                lc.tuple_kinds,
                )?,
                None => vt,
            };
            // Primitive auto-wrap (`let x: i64? = 7`): box the value on
            // the heap before storing. coerce can't see lc, so this
            // happens here. For heap inner types the existing path in
            // coerce already maps T → T?.
            let (val, vt) = maybe_box_to_optional_prim(b, lc, val, vt, bind_ty);
            let coerced = coerce(b, (val, vt), bind_ty, s.span)?;
            // Aliased binding (`let y = x` where x came from a Var/
            // Field/Index of heap type) needs an extra retain so the
            // new binding has its own reference. Fresh allocations
            // (`new C(...)`, fn results, "literal" + "literal", `[...]`)
            // already start at rc=1.
            emit_bind_retain(b, lc, &value.kind, vt, bind_ty, coerced);
            let var = b.declare_var(bind_ty.cl().ok_or_else(|| CodegenError::Unsupported {
                what: "unit-typed let binding".into(),
                span: s.span,
            })?);
            b.def_var(var, coerced);
            lc.env.bindings.insert(name.clone(), (var, bind_ty));
        }
        StmtKind::LetTuple { elems, value } => {
            let (val, vt) = match lower_expr(b, lc, value)? {
                Some(tv) => tv,
                None => {
                    return Err(CodegenError::Unsupported {
                        what: "tuple destructure on unit value".into(),
                        span: s.span,
                    });
                }
            };
            let tuple_id = match vt {
                JitTy::Tuple(id) => id,
                _ => {
                    return Err(CodegenError::Unsupported {
                        what: "tuple destructure on non-tuple value".into(),
                        span: s.span,
                    });
                }
            };
            let kind = lc.tuple_kinds[tuple_id as usize].clone();
            for (i, slot) in elems.iter().enumerate() {
                let Some(name) = slot else { continue };
                let elem_jty = kind.elems[i];
                let off = kind.offsets[i] as i32;
                let cl = elem_jty.cl().ok_or_else(|| CodegenError::Unsupported {
                    what: "tuple slot of unit type".into(),
                    span: s.span,
                })?;
                let v = b.ins().load(cl, MemFlags::trusted(), val, off);
                if elem_jty.is_heap() {
                    emit_retain_heap(b, lc, v, elem_jty);
                }
                let var = b.declare_var(cl);
                b.def_var(var, v);
                lc.env.bindings.insert(name.clone(), (var, elem_jty));
            }
            // Release the tuple if it was a fresh allocation.
            if !is_aliased_heap_source(&value.kind) {
                emit_release_heap(b, lc, val, vt);
            }
        }
        StmtKind::LetStruct { class: _, fields, value } => {
            let (val, vt) = match lower_expr(b, lc, value)? {
                Some(tv) => tv,
                None => {
                    return Err(CodegenError::Unsupported {
                        what: "struct destructure on unit value".into(),
                        span: s.span,
                    });
                }
            };
            let class_id = match vt {
                JitTy::Object(id) => id,
                _ => {
                    return Err(CodegenError::Unsupported {
                        what: "struct destructure on non-object value".into(),
                        span: s.span,
                    });
                }
            };
            let layout = lc.class_layouts[class_id as usize].fields.clone();
            for fname in fields.iter() {
                let (off, fty) = match layout.get(fname) {
                    Some(&(off, fty)) => (off, fty),
                    None => {
                        return Err(CodegenError::Unsupported {
                            what: format!("unknown field {fname:?} in destructure"),
                            span: s.span,
                        });
                    }
                };
                let cl = fty.cl().ok_or_else(|| CodegenError::Unsupported {
                    what: "field of unit type".into(),
                    span: s.span,
                })?;
                let v = b.ins().load(cl, MemFlags::trusted(), val, off as i32);
                if fty.is_heap() {
                    emit_retain_heap(b, lc, v, fty);
                }
                let var = b.declare_var(cl);
                b.def_var(var, v);
                lc.env.bindings.insert(fname.clone(), (var, fty));
            }
            if !is_aliased_heap_source(&value.kind) {
                emit_release_heap(b, lc, val, vt);
            }
        }
        StmtKind::Expr(e) => {
            // Discarded result. If it's a fresh heap value (call result,
            // `new`, `[..]`, "a"+"b"), nothing else owns it — release so
            // it doesn't leak. Aliased heap sources (Var/Field/Index/
            // This) are still owned by their binding, so leave them.
            if let Some((v, t)) = lower_expr(b, lc, e)? {
                if t.is_heap()
                    && !is_aliased_heap_source(&e.kind)
                {
                    emit_release_heap(b, lc, v, t);
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn lower_block_value(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    block: &ilang_ast::Block,
) -> Result<Option<TV>, CodegenError> {
    // Snapshot the entire binding map (not just the key set) so we can
    // restore both new bindings AND shadowed bindings on block exit.
    // Without this, `let y = 5; { let y: string = "hi"; ... }` would
    // leave `y` mapped to the inner string Variable after the block,
    // diverging from the interpreter.
    let before_bindings: std::collections::HashMap<Symbol, (Variable, JitTy)> =
        lc.env.bindings.clone();
    let unit_before: std::collections::HashSet<Symbol> =
        lc.env.unit_bindings.iter().cloned().collect();
    for s in &block.stmts {
        lower_stmt(b, lc, s)?;
    }
    let tail_kind = block.tail.as_ref().map(|e| &e.kind);
    let tail = match &block.tail {
        Some(t) => lower_expr(b, lc, t)?,
        None => None,
    };
    // Retain the tail value only when it's an aliased heap reference
    // (Var/Field/Index/This): the binding it borrows from is about to
    // be released, so we need our own +1 to hand to the caller.
    // Fresh heap values (call result, `new`, `[..]`, "a"+"b") already
    // come with rc=1, and a second retain would leak.
    if let Some((v, t)) = tail {
        if t.is_heap()
            && tail_kind.map(is_aliased_heap_source).unwrap_or(false)
        {
            emit_retain_heap(b, lc, v, t);
        }
    }
    // Release every heap-typed binding INTRODUCED in this block —
    // including names that shadowed an outer binding (which need their
    // OWN release before the outer is restored). Sort LIFO so
    // dependents drop first.
    let mut introduced_heap: Vec<(Symbol, Variable, JitTy)> = lc
        .env
        .bindings
        .iter()
        .filter(|(k, current)| {
            // Either the name is new, or its (Variable, JitTy) differs
            // from what was here before the block (a shadow).
            before_bindings.get(*k).map(|prev| prev != *current).unwrap_or(true)
        })
        .filter_map(|(k, &(var, jty))| {
            if jty.is_heap() {
                Some((k.clone(), var, jty))
            } else {
                None
            }
        })
        .collect();
    introduced_heap.sort_by_key(|(_, var, _)| std::cmp::Reverse(var.as_u32()));
    for (_, var, jty) in &introduced_heap {
        let p = b.use_var(*var);
        emit_release_heap(b, lc, p, *jty);
    }
    // Restore the binding map to its pre-block state. This drops every
    // new binding AND restores shadowed names to their outer value.
    lc.env.bindings = before_bindings;
    // Drop unit bindings introduced in this block. No release needed
    // (Unit holds no resources); just unregister so an outer-scope name
    // collision doesn't leak in.
    let new_units: Vec<Symbol> = lc
        .env
        .unit_bindings
        .iter()
        .filter(|n| !unit_before.contains(n))
        .cloned()
        .collect();
    for n in new_units {
        lc.env.unit_bindings.remove(&n);
    }
    Ok(tail)
}

/// If `bind_ty` is `Optional<P>` for a primitive P and the value
/// matches P, allocate a `[rc | payload]` box and return its pointer
/// + `Optional<P>` tagged type. Otherwise return the input unchanged.
/// Lets `let auto: i64? = 7` work the same as `let auto: i64? = 7`
/// already does in the interpreter.
fn maybe_box_to_optional_prim(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    val: cranelift::prelude::Value,
    vt: JitTy,
    bind_ty: JitTy,
) -> (cranelift::prelude::Value, JitTy) {
    use cranelift_codegen::ir::types::I64;
    if let JitTy::Optional(id) = bind_ty {
        let inner = lc.optional_inners[id as usize];
        if !inner.is_heap() && vt == inner {
            let size = b.ins().iconst(I64, inner.size_bytes() as i64);
            let new_ref = lc.module.declare_func_in_func(lc.optional_box_new_id, b.func);
            let call = b.ins().call(new_ref, &[size]);
            let ptr = b.inst_results(call)[0];
            b.ins().store(
                cranelift::prelude::MemFlags::trusted(),
                val,
                ptr,
                crate::runtime::OPT_PRIM_PAYLOAD_OFFSET,
            );
            return (ptr, bind_ty);
        }
    }
    (val, vt)
}
