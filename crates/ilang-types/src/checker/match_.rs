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
    /// Type-check `match` over a primitive scrutinee (integer /
    /// bool / string). Each arm's pattern must be a literal of the
    /// same shape, the wildcard `_`, or — for bool scrutinees —
    /// a `Variant` pattern whose name parses as `true` / `false`.
    /// A wildcard arm is mandatory: literal patterns can never be
    /// proven exhaustive over a primitive.
    pub(super) fn check_match_primitive(
        &self,
        st: &Type,
        arms: &[ilang_ast::MatchArm],
        match_span: Span,
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
    ) -> Result<Type, TypeError> {
        let mut has_wildcard = false;
        let mut bool_true_covered = false;
        let mut bool_false_covered = false;
        let mut result_ty: Option<Type> = None;
        for arm in arms {
            if has_wildcard {
                return Err(TypeError::Unsupported {
                    what: "match arm after wildcard `_` is unreachable".into(),
                    span: arm.span,
                });
            }
            let pspan = arm.pattern.span;
            let ok = match &arm.pattern.kind {
                PatternKind::Wildcard => {
                    has_wildcard = true;
                    true
                }
                PatternKind::IntLit(_) => st.is_numeric(),
                PatternKind::IntRange { low, high, inclusive } => {
                    if !st.is_numeric() {
                        false
                    } else if low.is_none() && high.is_none() {
                        return Err(TypeError::Unsupported {
                            what: "integer range pattern needs at least one bound".into(),
                            span: pspan,
                        });
                    } else if let (Some(lo), Some(hi)) = (low, high) {
                        if *lo > *hi || (!*inclusive && *lo == *hi) {
                            let l_s = lo.to_string();
                            let h_s = hi.to_string();
                            return Err(TypeError::Unsupported {
                                what: format!(
                                    "empty integer range pattern `{l_s}{}{h_s}`",
                                    if *inclusive { "..=" } else { ".." }
                                ),
                                span: pspan,
                            });
                        }
                        true
                    } else {
                        true
                    }
                }
                PatternKind::BoolLit(p) => {
                    if *st == Type::Bool {
                        if *p { bool_true_covered = true; } else { bool_false_covered = true; }
                        true
                    } else { false }
                }
                PatternKind::StrLit(_) => *st == Type::Str,
                // Bare `true` / `false` arrive from the parser as a
                // unit `Variant{name:"true"|"false"}`. Accept them
                // when matching a bool scrutinee.
                PatternKind::Variant { enum_name: None, variant, bindings: ilang_ast::PatternBindings::Unit }
                    if *st == Type::Bool && (variant == "true" || variant == "false") => {
                    if variant == "true" { bool_true_covered = true; } else { bool_false_covered = true; }
                    true
                }
                _ => false,
            };
            if !ok {
                return Err(TypeError::Unsupported {
                    what: format!(
                        "pattern type doesn't match scrutinee `{st}`"
                    ),
                    span: pspan,
                });
            }
            let bt = self.check_expr(&arm.body, env, ret_ty, in_class, loop_depth)?;
            result_ty = Some(match result_ty {
                None => bt,
                Some(prev) => self.unify_branch_obj(prev, bt, arm.body.span)?,
            });
        }
        // Bool is the only primitive whose value space is enumerable
        // — `true` + `false` together count as exhaustive, no `_`
        // arm needed.
        let bool_exhaustive =
            *st == Type::Bool && bool_true_covered && bool_false_covered;
        if !has_wildcard && !bool_exhaustive {
            return Err(TypeError::Unsupported {
                what: format!(
                    "non-exhaustive match on `{st}`: literal patterns require a `_` wildcard arm"
                ),
                span: match_span,
            });
        }
        let rt = result_ty.unwrap_or(Type::Unit);
        self.refine_match_arm_ctors(arms, &rt);
        Ok(rt)
    }

}
