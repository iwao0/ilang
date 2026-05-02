//! Statement and block lowering. Statements feed into block-value
//! lowering, which is where scope-exit ARC release lives.

use cranelift::prelude::*;
use ilang_ast::{ExprKind, Stmt, StmtKind, Type};

use crate::arc::{emit_bind_retain, emit_release_heap, emit_retain_heap, is_aliased_heap_source};
use crate::env::{class_ids_from, enum_ids_from, LowerCtx};
use crate::error::CodegenError;
use crate::lower_expr::{lower_array_literal, lower_expr};
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
                )?;
                Some(lower_array_literal(b, lc, elements, target_elem_jty, value.span)?)
            } else {
                None
            };
            let lowered_or_raw = match lowered {
                Some(tv) => Some(tv),
                None => lower_expr(b, lc, value)?,
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
                )?,
                None => vt,
            };
            let coerced = coerce(b, (val, vt), bind_ty, s.span)?;
            // Aliased binding (`let y = x` where x came from a Var/
            // Field/Index of heap type) needs an extra retain so the
            // new binding has its own reference. Fresh allocations
            // (`new C(...)`, fn results, "literal" + "literal", `[...]`)
            // already start at rc=1.
            emit_bind_retain(b, lc, &value.kind, vt, bind_ty, coerced);
            let var = Variable::new(lc.env.next_var_id());
            b.declare_var(
                var,
                bind_ty.cl().ok_or_else(|| CodegenError::Unsupported {
                    what: "unit-typed let binding".into(),
                    span: s.span,
                })?,
            );
            b.def_var(var, coerced);
            lc.env.bindings.insert(name.clone(), (var, bind_ty));
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
    let before_bindings: std::collections::HashMap<String, (Variable, JitTy)> =
        lc.env.bindings.clone();
    let unit_before: std::collections::HashSet<String> =
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
    let mut introduced_heap: Vec<(String, Variable, JitTy)> = lc
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
    let new_units: Vec<String> = lc
        .env
        .unit_bindings
        .iter()
        .filter(|n| !unit_before.contains(n.as_str()))
        .cloned()
        .collect();
    for n in new_units {
        lc.env.unit_bindings.remove(&n);
    }
    Ok(tail)
}
