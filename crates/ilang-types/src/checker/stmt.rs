//! Extracted from `checker/mod.rs`.

#![allow(unused_imports)]

use std::collections::{HashMap, HashSet};

use ilang_ast::{
    Block, ClassDecl, CtorArgs, EnumDecl, Expr, ExprKind, FieldDecl, FnDecl, Item, Param,
    PatternBindings, PatternKind, Program, Span, Stmt, StmtKind, Symbol, Type, UnOp,
    VariantPayload,
};

use crate::error::TypeError;
use crate::ops::{assignable, bin_result, int_literal_fits};

use super::*;

impl TypeChecker {
    pub(super) fn check_block(
        &self,
        block: &Block,
        outer: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
    ) -> Result<Type, TypeError> {
        let mut env = outer.clone();
        let mut last = Type::Unit;
        for s in &block.stmts {
            last = self.check_stmt(s, &mut env, ret_ty, in_class, loop_depth)?;
        }
        if let Some(t) = &block.tail {
            last = self.check_expr(t, &env, ret_ty, in_class, loop_depth)?;
        }
        Ok(last)
    }

    pub(super) fn check_stmt(
        &self,
        stmt: &Stmt,
        env: &mut Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
    ) -> Result<Type, TypeError> {
        match &stmt.kind {
            StmtKind::Let { name, ty, value, is_const, .. } => {
                // `let a = []` cannot pick an element type. Force the
                // user to annotate before we lose the chance to do so.
                if ty.is_none() {
                    if let ExprKind::Array(elements) = &value.kind {
                        if elements.is_empty() {
                            return Err(TypeError::EmptyArrayNeedsAnnotation {
                                span: value.span,
                            });
                        }
                    }
                }
                // Array literal hinted by an `T[]` / `T[N]`
                // annotation: each element is checked against `T`
                // directly so subclass / interface-implementing
                // siblings can share the same literal. The unhinted
                // path unifies on the first element's class only.
                // Self-recursive closure: `let f: fn(..): T =
                // fn(..) { ... f(...) ... }` — the body must see `f`
                // while it's being checked. Pre-bind the annotated fn
                // type into the env used for the RHS. Requires the
                // explicit annotation (the recursive type can't be
                // inferred); unannotated self-references still report
                // an unknown name.
                let self_rec_env: Option<Vars> = match (ty.as_ref(), &value.kind) {
                    (Some(ann @ Type::Fn(_)), ExprKind::FnExpr { .. }) => {
                        let tps = self.current_type_params.borrow();
                        let ann_rewritten = if tps.is_empty() {
                            ann.clone()
                        } else {
                            rewrite_type_params(ann, &tps)
                        };
                        drop(tps);
                        let mut e = env.clone();
                        e.insert(*name, ann_rewritten);
                        Some(e)
                    }
                    _ => None,
                };
                let mut vt = if let (Some(Type::Array { elem, .. }), ExprKind::Array(items)) =
                    (ty.as_ref(), &value.kind)
                {
                    if items.is_empty() {
                        self.check_expr(value, env, ret_ty, in_class, loop_depth)?
                    } else {
                        let tps = self.current_type_params.borrow();
                        let elem_rewritten = if tps.is_empty() {
                            (**elem).clone()
                        } else {
                            rewrite_type_params(elem, &tps)
                        };
                        drop(tps);
                        self.check_array_with_hint(
                            items,
                            Some(&elem_rewritten),
                            env,
                            ret_ty,
                            in_class,
                            loop_depth,
                            value.span,
                        )?
                    }
                } else if let (Some(Type::Generic(g)), ExprKind::MapLit(entries)) =
                    (ty.as_ref(), &value.kind)
                {
                    // Map literal hinted by a `Map<K, V>` annotation:
                    // each key / value is checked against K / V directly
                    // so subclass values and `some(child)` / `none`
                    // mixes land in the parent slot (mirrors the array
                    // case above). Falls back to the inferred path for a
                    // non-Map generic or an empty literal.
                    if g.base.as_str() == "Map" && g.args.len() == 2 && !entries.is_empty() {
                        let tps = self.current_type_params.borrow();
                        let (k_h, v_h) = if tps.is_empty() {
                            (g.args[0].clone(), g.args[1].clone())
                        } else {
                            (
                                rewrite_type_params(&g.args[0], &tps),
                                rewrite_type_params(&g.args[1], &tps),
                            )
                        };
                        drop(tps);
                        self.check_map_lit_with_hint(
                            entries, &k_h, &v_h, env, ret_ty, in_class, loop_depth,
                        )?
                    } else {
                        self.check_expr(value, env, ret_ty, in_class, loop_depth)?
                    }
                } else {
                    match self.check_expr(
                        value,
                        self_rec_env.as_ref().unwrap_or(env),
                        ret_ty,
                        in_class,
                        loop_depth,
                    ) {
                        Ok(t) => t,
                        // Unannotated self-reference: the closure
                        // body named its own binding, but with no
                        // annotation there's no type to pre-bind.
                        // Point at the actual fix instead of a bare
                        // "undefined function".
                        Err(
                            TypeError::UndefinedFunction { name: n, span }
                            | TypeError::UndefinedVariable { name: n, span },
                        ) if n == *name
                            && ty.is_none()
                            && matches!(value.kind, ExprKind::FnExpr { .. }) =>
                        {
                            return Err(
                                TypeError::SelfRecursiveClosureNeedsAnnotation {
                                    name: *name,
                                    span,
                                },
                            );
                        }
                        Err(e) => return Err(e),
                    }
                };
                // `let f = fn(...) { ... f(...) ... }` — drop a self-
                // reference from the FnExpr's capture list. The closure
                // can't capture its own value (it's still being built);
                // the body's `f` is left as a free name so codegen
                // resolves it through the global-let slot at call time
                // (the slot is initialised by the time any call site
                // fires).
                if let ExprKind::FnExpr { .. } = &value.kind {
                    let mut tbl = self.fn_expr_captures.borrow_mut();
                    if let Some(caps) = tbl.get_mut(&value.span) {
                        caps.retain(|(n, _)| n != name);
                    }
                }
                let bind = match ty {
                    Some(ann) => {
                        self.validate_type(ann, stmt.span, &[])?;
                        // Rewrite Object(T) → TypeVar(T) using the
                        // active fn's type params so a body-local
                        // `let y: T = x` matches the param-side
                        // representation (which goes through the
                        // same rewrite at fn entry).
                        let tps = self.current_type_params.borrow();
                        let ann_rewritten = if tps.is_empty() {
                            ann.clone()
                        } else {
                            rewrite_type_params(ann, &tps)
                        };
                        drop(tps);
                        // A generic fn call whose type param is fixed only
                        // by the return position (`let xs: i64[] =
                        // makeArr()`) infers `Any` from the args alone —
                        // solve it from the annotation so both the value
                        // type and the stashed type-args become concrete.
                        if let Some(corrected) =
                            self.refine_fn_call_type_args(value, &ann_rewritten)
                        {
                            vt = corrected;
                        }
                        // A covariant LITERAL value (an if/match of ctor
                        // literals that all built the same subclass, e.g.
                        // `Box<Dog>`, into a `let r: Box<Animal>`) is a real
                        // widening `value_assignable` doesn't model. Gated on
                        // the literal check so an aliased generic stays
                        // invariant; numeric narrowing stays rejected.
                        if !self.value_assignable(value, &vt, &ann_rewritten)
                            && !(self.is_covariant_join_literal(value)
                                && self.covariant_widening(&vt, &ann_rewritten))
                        {
                            return Err(TypeError::Mismatch {
                                expected: ann_rewritten.clone(),
                                got: vt,
                                span: value.span,
                            });
                        }
                        // Refine any enum-ctor side-table entries inside
                        // `value` whose inferred args contain `Any`,
                        // using the let annotation as the target. This
                        // is what lets the JIT monomorphizer pick a
                        // single concrete enum instantiation when an
                        // EnumCtor only provides args for some of T/E.
                        self.refine_enum_ctor_args(value, ann);
                        ann_rewritten
                    }
                    None => vt,
                };
                // Outside `@extern(C) {}`, raw C pointer / `char` /
                // `void` / `size_t` / `ssize_t` values cannot be
                // bound to a name — that would let them escape into
                // user code, defeating the encapsulation. Wrap the
                // FFI call in an `@extern(C)` fn instead.
                if !*self.in_extern_c.borrow() {
                    if let Some(c_only) = first_c_only_type(&bind) {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "value of type {bind} cannot be bound outside an \
                                 @extern(C) {{ ... }} block (contains the C-only type \
                                 {c_only}); wrap the FFI call in an @extern(C) fn"
                            ),
                            span: value.span,
                        });
                    }
                }
                env.insert(name.clone(), bind);
                if *is_const {
                    self.const_names.borrow_mut().insert(name.clone());
                } else {
                    // A `let` of the same name shadows / drops any
                    // previous const flag in this fn body.
                    self.const_names.borrow_mut().remove(name);
                }
                Ok(Type::Unit)
            }
            StmtKind::LetTuple { elems, value } => {
                let vt = self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
                let tys = match &vt {
                    Type::Tuple(ts) => ts,
                    _ => {
                        return Err(TypeError::Mismatch {
                            expected: Type::Tuple(Box::new([])),
                            got: vt.clone(),
                            span: value.span,
                        });
                    }
                };
                if tys.len() != elems.len() {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "tuple destructure expects {} slots, got {}",
                            tys.len(),
                            elems.len()
                        ),
                        span: stmt.span,
                    });
                }
                for (slot, t) in elems.iter().zip(tys.iter()) {
                    if let Some(name) = slot {
                        env.insert(name.clone(), t.clone());
                    }
                }
                Ok(Type::Unit)
            }
            StmtKind::LetStruct { class, fields, value } => {
                let vt = self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
                let (vname, vargs) = match &vt {
                    Type::Object(name) => (*name, Vec::<Type>::new()),
                    Type::Generic(g) => (g.base, g.args.to_vec()),
                    _ => {
                        return Err(TypeError::Mismatch {
                            expected: Type::Object(*class),
                            got: vt.clone(),
                            span: value.span,
                        });
                    }
                };
                if vname != *class {
                    return Err(TypeError::Mismatch {
                        expected: Type::Object(*class),
                        got: vt.clone(),
                        span: value.span,
                    });
                }
                let cls = self.classes.get(class).ok_or_else(|| {
                    TypeError::UndefinedClass {
                        name: (*class).into(),
                        span: stmt.span,
                    }
                })?;
                for f in fields.iter() {
                    let raw = cls.fields.get(f).cloned().ok_or_else(|| {
                        TypeError::UnknownField {
                            class: (*class).into(),
                            field: *f,
                            span: stmt.span,
                        }
                    })?;
                    let ty = subst_type(&raw, &cls.type_params, &vargs);
                    env.insert(f.clone(), ty);
                }
                Ok(Type::Unit)
            }
            StmtKind::Expr(e) => self.check_expr(e, env, ret_ty, in_class, loop_depth),
        }
    }

}
