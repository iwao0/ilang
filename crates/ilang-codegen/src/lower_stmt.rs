//! Statement and block lowering. Statements feed into block-value
//! lowering, which is where scope-exit ARC release lives.

use cranelift::prelude::*;
use ilang_ast::{ExprKind, Stmt, StmtKind, Type};

use crate::arc::{emit_release_heap, emit_retain_heap, is_aliased_heap_source};
use crate::env::{class_ids_from, LowerCtx};
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
                    lc.array_kinds,
                )?;
                Some(lower_array_literal(b, lc, elements, target_elem_jty, value.span)?)
            } else {
                None
            };
            let (val, vt) = match lowered {
                Some(tv) => tv,
                None => lower_expr(b, lc, value)?.ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: "let value produces no value".into(),
                        span: value.span,
                    }
                })?,
            };
            let bind_ty = match ty {
                Some(t) => JitTy::from_ast(
                    t,
                    s.span,
                    &class_ids_from(lc),
                    lc.array_kinds,
                )?,
                None => vt,
            };
            let coerced = coerce(b, (val, vt), bind_ty, s.span)?;
            // Aliased binding (`let y = x` where x came from a Var/
            // Field/Index of heap type) needs an extra retain so the
            // new binding has its own reference. Fresh allocations
            // (`new C(...)`, fn results, "literal" + "literal", `[...]`)
            // already start at rc=1.
            if matches!(bind_ty, JitTy::Object(_) | JitTy::Str | JitTy::Array(_))
                && is_aliased_heap_source(&value.kind)
            {
                emit_retain_heap(b, lc, coerced, bind_ty);
            }
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
            let _ = lower_expr(b, lc, e)?;
        }
    }
    Ok(())
}

pub(crate) fn lower_block_value(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    block: &ilang_ast::Block,
) -> Result<Option<TV>, CodegenError> {
    let before: std::collections::HashSet<String> =
        lc.env.bindings.keys().cloned().collect();
    for s in &block.stmts {
        lower_stmt(b, lc, s)?;
    }
    let tail = match &block.tail {
        Some(t) => lower_expr(b, lc, t)?,
        None => None,
    };
    // Retain the tail value if it's heap-typed, so the upcoming releases
    // of this block's heap-typed bindings don't free the value the
    // caller is about to consume.
    if let Some((v, t)) = tail {
        if matches!(t, JitTy::Object(_) | JitTy::Str | JitTy::Array(_)) {
            emit_retain_heap(b, lc, v, t);
        }
    }
    // Release any heap-typed bindings introduced by this block, then
    // drop them from the env so an outer-scope release pass doesn't see
    // the freed value a second time. Release in LIFO order
    // (most-recently-bound first) so a later binding can depend on an
    // earlier one's heap-held data without the earlier one freeing first.
    let mut new_heap: Vec<(String, Variable, JitTy)> = lc
        .env
        .bindings
        .iter()
        .filter(|(k, _)| !before.contains(k.as_str()))
        .filter_map(|(k, &(var, jty))| {
            if matches!(jty, JitTy::Object(_) | JitTy::Str | JitTy::Array(_)) {
                Some((k.clone(), var, jty))
            } else {
                None
            }
        })
        .collect();
    new_heap.sort_by_key(|(_, var, _)| std::cmp::Reverse(var.as_u32()));
    for (k, var, jty) in new_heap {
        let p = b.use_var(var);
        emit_release_heap(b, lc, p, jty);
        lc.env.bindings.remove(&k);
    }
    Ok(tail)
}
