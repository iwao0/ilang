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
                let vt = self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
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
                        if !self.value_assignable(value, &vt, &ann_rewritten) {
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
