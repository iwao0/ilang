//! `match` and `if let` lowering on `BodyCx`.
//!
//! - `lower_match` dispatches on the scrutinee's MirTy to one of
//!   the per-shape lowerers: `lower_match_enum` (the most complex —
//!   tag switch + per-variant payload destructure), `lower_match_int`
//!   (integer-ranged switch with explicit / range / wildcard arms),
//!   `lower_match_bool` / `lower_match_str` (compact 2-arm and
//!   eq-chain forms).
//! - `lower_if_let` is the convenient one-arm `if let some(x) =
//!   opt { ... } else { ... }` form.

use ilang_ast::{self as ast, Block as AstBlock, Expr, ExprKind, StmtKind, Symbol};

use crate::inst::{BinOp, BlockId, Inst, MirConst, Terminator, ValueId};
use crate::types::MirTy;

use super::{BodyCx, LowerError, VariantPayloadMeta};

/// Same shape as `ilang-types`'s `arm_body_diverges`. An arm whose
/// body transfers control out (early `return` / `break` /
/// `continue`) never reaches the match's join — record it as such
/// so the join wiring skips it (otherwise the post-Return "dead"
/// block emits a `Br` to the join with the wrong argument shape
/// and Cranelift rejects the function).
pub(super) fn arm_body_diverges(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Return(_) | ExprKind::Break(_) | ExprKind::Continue => true,
        ExprKind::Block(b) => {
            for s in &b.stmts {
                if let StmtKind::Expr(inner) = &s.kind {
                    if arm_body_diverges(inner) {
                        return true;
                    }
                }
            }
            b.tail.as_ref().map(|t| arm_body_diverges(t)).unwrap_or(false)
        }
        _ => false,
    }
}

impl<'a> BodyCx<'a> {
    pub(super) fn lower_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[ast::MatchArm],
    ) -> Result<(ValueId, MirTy), LowerError> {
        // Optional needs the fresh-vs-borrowed bit before
        // `lower_expr` consumes the AST, so peek first.
        let scrut_is_fresh = self.is_fresh_object_expr(scrutinee);
        let (sv, sty) = self.lower_expr(scrutinee)?;

        // The `?` desugar's `__try_*` temp ALWAYS owns the matched
        // value (its `let` either adopted a fresh expr or retained
        // an alias), and the desugar block's scope exit releases it
        // right after the match — so an arm tail that hands back a
        // pattern binding must mint the caller's +1 even though the
        // Var scrutinee reads as non-fresh. Without this the outer
        // `let v = e?` retained an already-freed payload
        // (use-after-free for BOTH Result and Optional `?`).
        let force_tail_retain =
            matches!(&scrutinee.kind, ExprKind::Var(n) if n.as_str().starts_with("__try_"));
        match &sty {
            MirTy::Enum(eid) => {
                self.lower_match_enum(sv, *eid, arms, scrut_is_fresh, force_tail_retain)
            }
            MirTy::I8 | MirTy::I16 | MirTy::I32 | MirTy::I64
            | MirTy::U8 | MirTy::U16 | MirTy::U32 | MirTy::U64
            | MirTy::Size | MirTy::SSize => self.lower_match_int(sv, sty.clone(), arms),
            MirTy::Bool => self.lower_match_bool(sv, arms),
            MirTy::Str => self.lower_match_str(sv, arms, scrut_is_fresh),
            MirTy::Optional(inner) => {
                let inner_ty = (**inner).clone();
                // `e?` on an Optional: the parser's type-blind `?`
                // desugar produced Result-shaped arms (`ok(v){v}` /
                // `err(e){return Result.err(e)}`) over a synthetic
                // `__try_*` temp. Rewrite them to the Optional
                // meaning — unwrap, or `return none` — and lower
                // through the regular optional-match path so the
                // early-return sweep / ARC rules apply unchanged.
                // (The checker validated the enclosing fn returns
                // an Optional.)
                if is_try_desugar(scrutinee, arms) {
                    let span = arms[0].span;
                    let ok_binding = match &arms[0].pattern.kind {
                        ast::PatternKind::Variant {
                            bindings: ast::PatternBindings::Tuple(names),
                            ..
                        } => names[0],
                        _ => unreachable!("try desugar shape checked"),
                    };
                    let rewritten = vec![
                        ast::MatchArm {
                            pattern: ast::Pattern {
                                kind: ast::PatternKind::Variant {
                                    enum_name: None,
                                    variant: Symbol::intern("some"),
                                    bindings: ast::PatternBindings::Tuple(
                                        Box::new([ok_binding]),
                                    ),
                                },
                                span,
                            },
                            body: arms[0].body.clone(),
                            span,
                        },
                        ast::MatchArm {
                            pattern: ast::Pattern {
                                kind: ast::PatternKind::Variant {
                                    enum_name: None,
                                    variant: Symbol::intern("none"),
                                    bindings: ast::PatternBindings::Unit,
                                },
                                span,
                            },
                            body: Expr::new(
                                ExprKind::Return(Some(Box::new(Expr::new(
                                    ExprKind::None,
                                    span,
                                )))),
                                span,
                            ),
                            span,
                        },
                    ];
                    return self.lower_match_optional(
                        sv,
                        inner_ty,
                        &rewritten,
                        scrut_is_fresh,
                        force_tail_retain,
                    );
                }
                self.lower_match_optional(sv, inner_ty, arms, scrut_is_fresh, force_tail_retain)
            }
            other => Err(LowerError::Other(format!(
                "match on unsupported scrutinee type: {other}"
            ))),
        }
    }

    fn lower_match_optional(
        &mut self,
        sv: ValueId,
        inner_ty: MirTy,
        arms: &[ast::MatchArm],
        scrut_is_fresh: bool,
        force_tail_retain: bool,
    ) -> Result<(ValueId, MirTy), LowerError> {
        // Find the some / none / wildcard arms.
        let mut some_arm: Option<&ast::MatchArm> = None;
        let mut none_arm: Option<&ast::MatchArm> = None;
        let mut wildcard: Option<&ast::MatchArm> = None;
        let mut some_binding: Option<Symbol> = None;
        for arm in arms {
            match &arm.pattern.kind {
                ast::PatternKind::Wildcard => wildcard = Some(arm),
                ast::PatternKind::Variant { variant, bindings, .. } => {
                    match variant.as_str() {
                        "some" => {
                            some_arm = Some(arm);
                            if let ast::PatternBindings::Tuple(names) = bindings {
                                if let Some(n) = names.first() {
                                    if n.as_str() != "_" {
                                        some_binding = Some(*n);
                                    }
                                }
                            }
                        }
                        "none" => none_arm = Some(arm),
                        other => {
                            return Err(LowerError::Other(format!(
                                "Optional match has no variant {other}"
                            )))
                        }
                    }
                }
                _ => {
                    return Err(LowerError::Other(
                        "non-variant pattern in Optional match".into(),
                    ))
                }
            }
        }
        let some_arm = some_arm.or(wildcard);
        let none_arm = none_arm.or(wildcard);

        let is_some = self.fb.new_value(MirTy::Bool);
        self.fb.push_inst(Inst::OptionalIsSome { dst: is_some, opt: sv });

        let some_blk = self.fb.new_block();
        let none_blk = self.fb.new_block();
        let cont = self.fb.new_block();
        self.fb.set_terminator(Terminator::CondBr {
            cond: is_some,
            then_block: some_blk,
            then_args: Box::new([]),
            else_block: none_blk,
            else_args: Box::new([]),
        });

        let mut joins: Vec<(BlockId, ValueId)> = Vec::new();
        let mut result_ty: Option<MirTy> = None;

        // Some branch — unwrap, bind name (if any), lower body.
        // ARC: see `lower_if_let` — release fresh scrutinee at
        // some-branch exit so the cell's cascade reclaims the
        // inner value the unwrap aliased.
        self.fb.switch_to(some_blk);
        if let Some(arm) = some_arm {
            self.env.enter_scope();
            if let Some(name) = some_binding {
                // The pattern binding is a borrow into the
                // Optional cell's inner slot — ownership flows
                // through whatever the arm body returns (caller's
                // let-retain or post-call release closes the
                // loop). Same policy as `lower_match_enum` after
                // the EnumPayload codegen-retain removal:
                // `OptionalUnwrap` codegen MUST NOT call
                // `__retain_*` here, otherwise the +1 has no
                // matching release path and leaks. The
                // env.exit_scope below skips the
                // `release_top_scope_objects` sweep on purpose —
                // the binding is `Binding::Ssa` and carries no
                // rc share to drop.
                let unwrapped = self.fb.new_value(inner_ty.clone());
                self.fb.push_inst(Inst::OptionalUnwrap { dst: unwrapped, opt: sv });
                self.env.bind_pattern(name, unwrapped, inner_ty.clone(), scrut_is_fresh);
            }
            let diverges = arm_body_diverges(&arm.body);
            if scrut_is_fresh {
                let depth = self.env.scopes.len();
                self.live_fresh_scrutinees.push((sv, depth));
            }
            let (bv, bty) = self.lower_expr(&arm.body)?;
            if scrut_is_fresh {
                self.live_fresh_scrutinees.pop();
            }
            // `?` desugar (`__try_*` scrutinee): the arm body is the
            // bare binding Var, which bypasses the block-tail retain
            // pairing — mint the caller's +1 here, BEFORE the
            // desugar block's scope exit releases the `__try` temp
            // and cascades the cell (the payload would be freed
            // under the outer `let`).
            if force_tail_retain
                && matches!(&arm.body.kind, ExprKind::Var(_))
                && self.is_arc_slot(&bty)
            {
                self.fb.push_inst(Inst::Retain { value: bv });
            }
            // `Release(sv)` below cascades the Optional cell. The
            // pattern binding was registered with
            // `needs_retain_on_tail = scrut_is_fresh`, so
            // `lower_block_hinted` has already paired any tail
            // `Var(binding)` with the matching `Retain` — no extra
            // accounting needed here. (On a diverging body this
            // lands in the dead block; the early-exit sweep emitted
            // the live-path Release via `live_fresh_scrutinees`.)
            if scrut_is_fresh {
                self.fb.push_inst(Inst::Release { value: sv });
            }
            self.env.exit_scope();
            if !diverges {
                if result_ty.is_none() && !matches!(bty, MirTy::Unit) {
                    result_ty = Some(bty.clone());
                }
                joins.push((self.fb.current_block(), bv));
            }
        } else {
            self.fb.set_terminator(Terminator::Unreachable);
        }

        // None branch.
        self.fb.switch_to(none_blk);
        if let Some(arm) = none_arm {
            let diverges = arm_body_diverges(&arm.body);
            let (bv, bty) = self.lower_expr(&arm.body)?;
            if !diverges {
                if result_ty.is_none() && !matches!(bty, MirTy::Unit) {
                    result_ty = Some(bty.clone());
                }
                joins.push((self.fb.current_block(), bv));
            }
        } else {
            self.fb.set_terminator(Terminator::Unreachable);
        }

        let result_ty = result_ty.unwrap_or(MirTy::Unit);
        let result_val = if matches!(result_ty, MirTy::Unit) {
            None
        } else {
            Some(self.fb.add_block_param(cont, result_ty.clone()))
        };
        for (blk, val) in joins {
            self.fb.switch_to(blk);
            let args: Box<[ValueId]> = if matches!(result_ty, MirTy::Unit) {
                Box::new([])
            } else {
                Box::new([val])
            };
            self.fb.set_terminator(Terminator::Br { dst: cont, args });
        }
        self.fb.switch_to(cont);
        Ok(match result_val {
            Some(v) => (v, result_ty),
            None => (self.const_unit(), MirTy::Unit),
        })
    }

    fn lower_match_enum(
        &mut self,
        sv: ValueId,
        eid: crate::types::EnumId,
        arms: &[ast::MatchArm],
        scrut_is_fresh: bool,
        force_tail_retain: bool,
    ) -> Result<(ValueId, MirTy), LowerError> {
        let layout = &self.enums[eid.0 as usize];
        // For each arm, find which variant it matches (or wildcard).
        let mut cases: Vec<crate::inst::SwitchCase> = Vec::new();
        let mut default: Option<crate::inst::BlockId> = None;
        let cont = self.fb.new_block();
        let mut result_ty: Option<MirTy> = None;
        // Lazy attach to cont once we know the result type.

        // Tag value (i64).
        let tag = self.fb.new_value(MirTy::I64);
        self.fb.push_inst(Inst::EnumTag { dst: tag, value: sv });

        // We must terminate the current block once we set the switch
        // — but we don't know cases yet. Defer terminator setting:
        // collect (variant_idx, arm) pairs, then emit switch.
        let mut arm_blocks: Vec<(BlockId, &ast::MatchArm)> = Vec::new();
        let mut wildcard_blk: Option<(BlockId, &ast::MatchArm)> = None;

        for arm in arms {
            match &arm.pattern.kind {
                ast::PatternKind::Wildcard => {
                    let blk = self.fb.new_block();
                    wildcard_blk = Some((blk, arm));
                    default = Some(blk);
                }
                ast::PatternKind::Variant { variant, .. } => {
                    let vmeta_id = layout
                        .variants
                        .iter()
                        .find(|v| v.name == *variant)
                        .ok_or_else(|| {
                            LowerError::Other(format!("variant {variant} not in enum"))
                        })?
                        .id;
                    let blk = self.fb.new_block();
                    let disc = layout.variants[vmeta_id.0 as usize].discriminant;
                    cases.push(crate::inst::SwitchCase {
                        value: disc,
                        dst: blk,
                        args: Box::new([]),
                    });
                    arm_blocks.push((blk, arm));
                }
                _ => {
                    return Err(LowerError::Other(format!(
                        "non-variant pattern in enum match"
                    )))
                }
            }
        }

        // If no wildcard, synthesise an unreachable default.
        let default = default.unwrap_or_else(|| {
            let b = self.fb.new_block();
            // (We'll set its terminator after switch creation.)
            b
        });

        self.fb.set_terminator(Terminator::Switch {
            scrutinee: tag,
            cases: cases.clone().into_boxed_slice(),
            default,
            default_args: Box::new([]),
        });

        // Lower each arm body.
        let mut joins: Vec<(BlockId, ValueId)> = Vec::new();
        for (blk, arm) in &arm_blocks {
            self.fb.switch_to(*blk);
            self.env.enter_scope();
            // Bind variant payload if any.
            if let ast::PatternKind::Variant { variant, bindings, .. } = &arm.pattern.kind {
                let vmeta = self.enum_meta.get(&eid).unwrap().variants.get(variant).unwrap();
                let vid = vmeta.id;
                match (&vmeta.payload, bindings) {
                    (VariantPayloadMeta::Unit, ast::PatternBindings::Unit) => {}
                    (VariantPayloadMeta::Tuple(tys), ast::PatternBindings::Tuple(names)) => {
                        for (i, n) in names.iter().enumerate() {
                            if n.as_str() == "_" {
                                continue;
                            }
                            let ty = tys.get(i).cloned().ok_or_else(|| {
                                LowerError::Other("tuple binding length > variant arity".into())
                            })?;
                            let v = self.fb.new_value(ty.clone());
                            self.fb.push_inst(Inst::EnumPayload {
                                dst: v,
                                value: sv,
                                variant: vid,
                                idx: i as u32,
                            });
                            self.env.bind_pattern(*n, v, ty, scrut_is_fresh);
                        }
                    }
                    (VariantPayloadMeta::Struct(fields), ast::PatternBindings::Struct(named)) => {
                        for (decl_name, bind_name) in named.iter() {
                            let idx = fields
                                .iter()
                                .position(|(n, _)| n == decl_name)
                                .ok_or_else(|| {
                                    LowerError::Other(format!("no field {decl_name}"))
                                })?;
                            let ty = fields[idx].1.clone();
                            let v = self.fb.new_value(ty.clone());
                            self.fb.push_inst(Inst::EnumPayload {
                                dst: v,
                                value: sv,
                                variant: vid,
                                idx: idx as u32,
                            });
                            self.env.bind_pattern(*bind_name, v, ty, scrut_is_fresh);
                        }
                    }
                    _ => {
                        return Err(LowerError::Other(
                            "variant pattern shape doesn't match payload".into(),
                        ))
                    }
                }
            }
            let diverges = arm_body_diverges(&arm.body);
            // Register the fresh scrutinee for the early-exit
            // sweeps: a `return` / `break` / `continue` inside the
            // arm body bypasses the arm-end Release below.
            if scrut_is_fresh {
                let depth = self.env.scopes.len();
                self.live_fresh_scrutinees.push((sv, depth));
            }
            let (bv, bty) = self.lower_expr(&arm.body)?;
            if scrut_is_fresh {
                self.live_fresh_scrutinees.pop();
            }
            // `?` desugar — see `lower_match_optional`: the bare
            // Var arm body bypasses the block-tail retain pairing,
            // so mint the caller's +1 before the `__try` temp's
            // scope-exit release cascades the cell.
            if force_tail_retain
                && matches!(&arm.body.kind, ExprKind::Var(_))
                && self.is_arc_slot(&bty)
            {
                self.fb.push_inst(Inst::Retain { value: bv });
            }
            // Mirror `lower_match_optional`: when the scrutinee
            // was fresh, release the enum cell at arm exit so its
            // cascade reclaims the payload the binding aliased.
            // Without this, the fresh enum cell stays at rc=1
            // forever (caller-releases-fresh isn't a Match
            // scrutinee contract — the match takes ownership).
            if scrut_is_fresh && !diverges {
                self.fb.push_inst(Inst::Release { value: sv });
            }
            self.env.exit_scope();
            // Pin the result type from the first arm we encounter.
            // A diverging arm (early `return` / `break` / `continue`)
            // never reaches the join, so its lowered value is
            // irrelevant — skip both the type-pinning and the
            // joins-list push.
            if !diverges {
                if result_ty.is_none() && !matches!(bty, MirTy::Unit) {
                    result_ty = Some(bty.clone());
                }
                joins.push((self.fb.current_block(), bv));
            }
        }
        // Wildcard arm.
        if let Some((blk, arm)) = wildcard_blk {
            self.fb.switch_to(blk);
            let diverges = arm_body_diverges(&arm.body);
            if scrut_is_fresh {
                let depth = self.env.scopes.len();
                self.live_fresh_scrutinees.push((sv, depth));
            }
            let (bv, bty) = self.lower_expr(&arm.body)?;
            if scrut_is_fresh {
                self.live_fresh_scrutinees.pop();
            }
            if scrut_is_fresh && !diverges {
                self.fb.push_inst(Inst::Release { value: sv });
            }
            if !diverges {
                if result_ty.is_none() && !matches!(bty, MirTy::Unit) {
                    result_ty = Some(bty.clone());
                }
                joins.push((self.fb.current_block(), bv));
            }
        } else {
            // No user wildcard: the synthesised default is unreachable.
            self.fb.switch_to(default);
            self.fb.set_terminator(Terminator::Unreachable);
        }

        let result_ty = result_ty.unwrap_or(MirTy::Unit);
        let result_val = if matches!(result_ty, MirTy::Unit) {
            None
        } else {
            Some(self.fb.add_block_param(cont, result_ty.clone()))
        };
        for (blk, val) in joins {
            self.fb.switch_to(blk);
            let args: Box<[ValueId]> = if matches!(result_ty, MirTy::Unit) {
                Box::new([])
            } else {
                Box::new([val])
            };
            self.fb.set_terminator(Terminator::Br { dst: cont, args });
        }

        self.fb.switch_to(cont);
        Ok(match result_val {
            Some(v) => (v, result_ty),
            None => (self.const_unit(), MirTy::Unit),
        })
    }

    fn lower_match_int(
        &mut self,
        sv: ValueId,
        sty: MirTy,
        arms: &[ast::MatchArm],
    ) -> Result<(ValueId, MirTy), LowerError> {
        // Lower as a chain of if/else compares; ranges and wildcards
        // are handled in-line. A jump-table optimisation can replace
        // this later.
        let cont = self.fb.new_block();
        let mut result_ty: Option<MirTy> = None;
        let mut joins: Vec<(BlockId, ValueId)> = Vec::new();

        let int_signed = sty.is_signed_int();
        for (i, arm) in arms.iter().enumerate() {
            let is_last = i == arms.len() - 1;
            match &arm.pattern.kind {
                ast::PatternKind::Wildcard => {
                    // Body unconditionally.
                    let (bv, bty) = self.lower_expr(&arm.body)?;
                    if result_ty.is_none() && !matches!(bty, MirTy::Unit) {
                        result_ty = Some(bty.clone());
                    }
                    joins.push((self.fb.current_block(), bv));
                    break;
                }
                ast::PatternKind::IntLit(n) => {
                    let cval = self.const_int(sty.clone(), *n);
                    let cmp = self.fb.new_value(MirTy::Bool);
                    self.fb.push_inst(Inst::BinOp {
                        dst: cmp,
                        op: BinOp::IEq,
                        lhs: sv,
                        rhs: cval,
                    });
                    let body_blk = self.fb.new_block();
                    let next_blk = self.fb.new_block();
                    self.fb.set_terminator(Terminator::CondBr {
                        cond: cmp,
                        then_block: body_blk,
                        then_args: Box::new([]),
                        else_block: next_blk,
                        else_args: Box::new([]),
                    });
                    self.fb.switch_to(body_blk);
                    let (bv, bty) = self.lower_expr(&arm.body)?;
                    if result_ty.is_none() && !matches!(bty, MirTy::Unit) {
                        result_ty = Some(bty.clone());
                    }
                    joins.push((self.fb.current_block(), bv));
                    self.fb.switch_to(next_blk);
                    if is_last {
                        // No more arms — unreachable (type-checker
                        // should have rejected non-exhaustive).
                        self.fb.set_terminator(Terminator::Unreachable);
                    }
                }
                ast::PatternKind::IntRange { low, high, inclusive } => {
                    let mut all_one = self.fb.new_value(MirTy::Bool);
                    self.fb.push_inst(Inst::Const {
                        dst: all_one,
                        value: MirConst::Bool(true),
                    });
                    if let Some(l) = low {
                        let lv = self.const_int(sty.clone(), *l);
                        let g = self.fb.new_value(MirTy::Bool);
                        let op = if int_signed { BinOp::IGeS } else { BinOp::IGeU };
                        self.fb.push_inst(Inst::BinOp { dst: g, op, lhs: sv, rhs: lv });
                        let nm = self.fb.new_value(MirTy::Bool);
                        self.fb.push_inst(Inst::BinOp {
                            dst: nm,
                            op: BinOp::IAnd,
                            lhs: all_one,
                            rhs: g,
                        });
                        all_one = nm;
                    }
                    if let Some(h) = high {
                        let hv = self.const_int(sty.clone(), *h);
                        let cond = self.fb.new_value(MirTy::Bool);
                        let op = if *inclusive {
                            if int_signed { BinOp::ILeS } else { BinOp::ILeU }
                        } else if int_signed {
                            BinOp::ILtS
                        } else {
                            BinOp::ILtU
                        };
                        self.fb.push_inst(Inst::BinOp { dst: cond, op, lhs: sv, rhs: hv });
                        let nm = self.fb.new_value(MirTy::Bool);
                        self.fb.push_inst(Inst::BinOp {
                            dst: nm,
                            op: BinOp::IAnd,
                            lhs: all_one,
                            rhs: cond,
                        });
                        all_one = nm;
                    }
                    let body_blk = self.fb.new_block();
                    let next_blk = self.fb.new_block();
                    self.fb.set_terminator(Terminator::CondBr {
                        cond: all_one,
                        then_block: body_blk,
                        then_args: Box::new([]),
                        else_block: next_blk,
                        else_args: Box::new([]),
                    });
                    self.fb.switch_to(body_blk);
                    let (bv, bty) = self.lower_expr(&arm.body)?;
                    if result_ty.is_none() && !matches!(bty, MirTy::Unit) {
                        result_ty = Some(bty.clone());
                    }
                    joins.push((self.fb.current_block(), bv));
                    self.fb.switch_to(next_blk);
                    if is_last {
                        self.fb.set_terminator(Terminator::Unreachable);
                    }
                }
                _ => {
                    return Err(LowerError::Other(
                        "non-int pattern in integer match".into(),
                    ))
                }
            }
        }

        let result_ty = result_ty.unwrap_or(MirTy::Unit);
        let result_val = if matches!(result_ty, MirTy::Unit) {
            None
        } else {
            Some(self.fb.add_block_param(cont, result_ty.clone()))
        };
        for (blk, val) in joins {
            self.fb.switch_to(blk);
            let args: Box<[ValueId]> = if matches!(result_ty, MirTy::Unit) {
                Box::new([])
            } else {
                Box::new([val])
            };
            self.fb.set_terminator(Terminator::Br { dst: cont, args });
        }
        self.fb.switch_to(cont);
        Ok(match result_val {
            Some(v) => (v, result_ty),
            None => (self.const_unit(), MirTy::Unit),
        })
    }

    fn lower_match_bool(
        &mut self,
        sv: ValueId,
        arms: &[ast::MatchArm],
    ) -> Result<(ValueId, MirTy), LowerError> {
        // Convert to two-arm if/else (true / false) lookup.
        let mut true_arm: Option<&ast::MatchArm> = None;
        let mut false_arm: Option<&ast::MatchArm> = None;
        let mut wildcard: Option<&ast::MatchArm> = None;
        for arm in arms {
            match &arm.pattern.kind {
                ast::PatternKind::BoolLit(true) => true_arm = Some(arm),
                ast::PatternKind::BoolLit(false) => false_arm = Some(arm),
                // Parser produces Variant("true"/"false") since they
                // could also be enum variant names; the type checker
                // would rewrite. We do the same lookup here.
                ast::PatternKind::Variant { variant, .. } if variant.as_str() == "true" => {
                    true_arm = Some(arm)
                }
                ast::PatternKind::Variant { variant, .. } if variant.as_str() == "false" => {
                    false_arm = Some(arm)
                }
                ast::PatternKind::Wildcard => wildcard = Some(arm),
                _ => {
                    return Err(LowerError::Other(
                        "non-bool pattern in bool match".into(),
                    ))
                }
            }
        }
        let true_arm = true_arm.or(wildcard);
        let false_arm = false_arm.or(wildcard);
        let then_blk = self.fb.new_block();
        let else_blk = self.fb.new_block();
        let cont = self.fb.new_block();
        self.fb.set_terminator(Terminator::CondBr {
            cond: sv,
            then_block: then_blk,
            then_args: Box::new([]),
            else_block: else_blk,
            else_args: Box::new([]),
        });

        let mut joins: Vec<(BlockId, ValueId)> = Vec::new();
        let mut result_ty: Option<MirTy> = None;
        if let Some(arm) = true_arm {
            self.fb.switch_to(then_blk);
            let (bv, bty) = self.lower_expr(&arm.body)?;
            if !matches!(bty, MirTy::Unit) {
                result_ty.get_or_insert(bty);
            }
            joins.push((self.fb.current_block(), bv));
        } else {
            self.fb.switch_to(then_blk);
            self.fb.set_terminator(Terminator::Unreachable);
        }
        if let Some(arm) = false_arm {
            self.fb.switch_to(else_blk);
            let (bv, bty) = self.lower_expr(&arm.body)?;
            if !matches!(bty, MirTy::Unit) {
                result_ty.get_or_insert(bty);
            }
            joins.push((self.fb.current_block(), bv));
        } else {
            self.fb.switch_to(else_blk);
            self.fb.set_terminator(Terminator::Unreachable);
        }

        let result_ty = result_ty.unwrap_or(MirTy::Unit);
        let result_val = if matches!(result_ty, MirTy::Unit) {
            None
        } else {
            Some(self.fb.add_block_param(cont, result_ty.clone()))
        };
        for (blk, val) in joins {
            self.fb.switch_to(blk);
            let args: Box<[ValueId]> = if matches!(result_ty, MirTy::Unit) {
                Box::new([])
            } else {
                Box::new([val])
            };
            self.fb.set_terminator(Terminator::Br { dst: cont, args });
        }
        self.fb.switch_to(cont);
        Ok(match result_val {
            Some(v) => (v, result_ty),
            None => (self.const_unit(), MirTy::Unit),
        })
    }

    fn lower_match_str(
        &mut self,
        sv: ValueId,
        arms: &[ast::MatchArm],
        scrut_is_fresh: bool,
    ) -> Result<(ValueId, MirTy), LowerError> {
        let cont = self.fb.new_block();
        let mut result_ty: Option<MirTy> = None;
        let mut joins: Vec<(BlockId, ValueId)> = Vec::new();

        for (i, arm) in arms.iter().enumerate() {
            let is_last = i == arms.len() - 1;
            match &arm.pattern.kind {
                ast::PatternKind::Wildcard => {
                    // A fresh scrutinee string has no other owner —
                    // the match consumes it (same contract as
                    // `lower_match_enum`). This path released
                    // NOTHING before: `match "k" + s { ... }` leaked
                    // one registry string per evaluation.
                    let diverges = arm_body_diverges(&arm.body);
                    if scrut_is_fresh {
                        let depth = self.env.scopes.len();
                        self.live_fresh_scrutinees.push((sv, depth));
                    }
                    let (bv, bty) = self.lower_expr(&arm.body)?;
                    if scrut_is_fresh {
                        self.live_fresh_scrutinees.pop();
                    }
                    if scrut_is_fresh && !diverges {
                        self.fb.push_inst(Inst::Release { value: sv });
                    }
                    if !diverges {
                        if result_ty.is_none() && !matches!(bty, MirTy::Unit) {
                            result_ty = Some(bty.clone());
                        }
                        joins.push((self.fb.current_block(), bv));
                    }
                    break;
                }
                ast::PatternKind::StrLit(s) => {
                    let lit = self.fb.new_value(MirTy::Str);
                    self.fb.push_inst(Inst::Const {
                        dst: lit,
                        value: MirConst::Str(Symbol::intern(s)),
                    });
                    let cmp = self.fb.new_value(MirTy::Bool);
                    self.fb.push_inst(Inst::BinOp {
                        dst: cmp,
                        op: BinOp::StrEq,
                        lhs: sv,
                        rhs: lit,
                    });
                    let body_blk = self.fb.new_block();
                    let next_blk = self.fb.new_block();
                    self.fb.set_terminator(Terminator::CondBr {
                        cond: cmp,
                        then_block: body_blk,
                        then_args: Box::new([]),
                        else_block: next_blk,
                        else_args: Box::new([]),
                    });
                    self.fb.switch_to(body_blk);
                    let diverges = arm_body_diverges(&arm.body);
                    if scrut_is_fresh {
                        let depth = self.env.scopes.len();
                        self.live_fresh_scrutinees.push((sv, depth));
                    }
                    let (bv, bty) = self.lower_expr(&arm.body)?;
                    if scrut_is_fresh {
                        self.live_fresh_scrutinees.pop();
                    }
                    if scrut_is_fresh && !diverges {
                        self.fb.push_inst(Inst::Release { value: sv });
                    }
                    if !diverges {
                        if result_ty.is_none() && !matches!(bty, MirTy::Unit) {
                            result_ty = Some(bty.clone());
                        }
                        joins.push((self.fb.current_block(), bv));
                    }
                    self.fb.switch_to(next_blk);
                    if is_last {
                        self.fb.set_terminator(Terminator::Unreachable);
                    }
                }
                _ => return Err(LowerError::Other("non-string pattern in string match".into())),
            }
        }
        let result_ty = result_ty.unwrap_or(MirTy::Unit);
        let result_val = if matches!(result_ty, MirTy::Unit) {
            None
        } else {
            Some(self.fb.add_block_param(cont, result_ty.clone()))
        };
        for (blk, val) in joins {
            self.fb.switch_to(blk);
            let args: Box<[ValueId]> = if matches!(result_ty, MirTy::Unit) {
                Box::new([])
            } else {
                Box::new([val])
            };
            self.fb.set_terminator(Terminator::Br { dst: cont, args });
        }
        self.fb.switch_to(cont);
        Ok(match result_val {
            Some(v) => (v, result_ty),
            None => (self.const_unit(), MirTy::Unit),
        })
    }

    pub(super) fn lower_if_let(
        &mut self,
        name: Symbol,
        scrut: &Expr,
        then_branch: &AstBlock,
        else_branch: Option<&Expr>,
    ) -> Result<(ValueId, MirTy), LowerError> {
        let scrut_is_fresh = self.is_fresh_object_expr(scrut);
        let (sv, sty) = self.lower_expr(scrut)?;
        let inner_ty = match &sty {
            MirTy::Optional(t) => (**t).clone(),
            other => {
                return Err(LowerError::Other(format!(
                    "`if let some(...)` requires Optional, got {other}"
                )))
            }
        };

        let is_some = self.fb.new_value(MirTy::Bool);
        self.fb.push_inst(Inst::OptionalIsSome { dst: is_some, opt: sv });

        let some_blk = self.fb.new_block();
        let none_blk = self.fb.new_block();
        self.fb.set_terminator(Terminator::CondBr {
            cond: is_some,
            then_block: some_blk,
            then_args: Box::new([]),
            else_block: none_blk,
            else_args: Box::new([]),
        });

        // Some branch: unwrap and bind. The unwrapped value aliases
        // the Optional cell's slot — the cell already owns the
        // inner's +1. For a borrowed scrutinee (param / field load),
        // the caller will release the cell later and the cascade
        // will reclaim the inner; we mustn't release the inner here
        // or it would double-drop and UAF (`linked_list_via_optional_field`
        // / `recursive_method_optional_tree`). For a *fresh*
        // scrutinee (the value came from `make(...)` etc. and has
        // no other owner), no later release happens — we have to
        // release the scrutinee opt at the some-branch exit so the
        // cell's cascade fires and reclaims the inner ourselves
        // (`iflet_heap_release`).
        //
        // Mirrors the `lower_match_enum` policy: pattern bindings
        // are borrows, `OptionalUnwrap` codegen MUST NOT call
        // `__retain_*` on its dst. If that contract gets broken,
        // the +1 has no matching release path here and leaks
        // (the same bug class as the pre-fix `EnumPayload`
        // auto-retain, see commit 08989aba).
        self.fb.switch_to(some_blk);
        let unwrapped = self.fb.new_value(inner_ty.clone());
        self.fb.push_inst(Inst::OptionalUnwrap { dst: unwrapped, opt: sv });
        self.env.enter_scope();
        self.env.bind_pattern(name, unwrapped, inner_ty.clone(), scrut_is_fresh);
        if scrut_is_fresh {
            let depth = self.env.scopes.len();
            self.live_fresh_scrutinees.push((sv, depth));
        }
        let then_tail = self.lower_block(then_branch)?;
        if scrut_is_fresh {
            self.live_fresh_scrutinees.pop();
        }
        // `Release(sv)` below cascades the Optional cell. The
        // pattern binding was registered with
        // `needs_retain_on_tail = scrut_is_fresh`, so
        // `lower_block_hinted` has already paired any tail
        // `Var(name)` with the matching `Retain`. (On a diverging
        // body this lands dead; the early-exit sweep covered the
        // live path via `live_fresh_scrutinees`.)
        if scrut_is_fresh {
            self.fb.push_inst(Inst::Release { value: sv });
        }
        self.env.exit_scope();

        // `if let some(x) = ... { ... }` without an else clause is
        // a statement; the result is `Unit` and the then-branch's
        // trailing value (if any) is silently discarded, matching
        // the type checker (see `check_if_let` in
        // `crates/ilang-types/.../match_ctrl.rs` "No else: the
        // result is Unit even if then has a value").
        let result_ty = if else_branch.is_none() {
            MirTy::Unit
        } else {
            match &then_tail {
                Some((_, t)) => t.clone(),
                None => MirTy::Unit,
            }
        };
        let cont = self.fb.new_block();
        let result_val = if matches!(result_ty, MirTy::Unit) {
            None
        } else {
            Some(self.fb.add_block_param(cont, result_ty.clone()))
        };
        let then_arg: Box<[ValueId]> = match (&result_ty, then_tail) {
            (MirTy::Unit, _) => Box::new([]),
            (_, Some((v, _))) => Box::new([v]),
            (_, None) => Box::new([self.const_unit()]),
        };
        self.fb.set_terminator(Terminator::Br { dst: cont, args: then_arg });

        // None branch.
        self.fb.switch_to(none_blk);
        let else_arg: Box<[ValueId]> = match else_branch {
            Some(e) => {
                let (v, _) = self.lower_expr(e)?;
                if matches!(result_ty, MirTy::Unit) {
                    Box::new([])
                } else {
                    Box::new([v])
                }
            }
            None => Box::new([]),
        };
        self.fb.set_terminator(Terminator::Br { dst: cont, args: else_arg });

        self.fb.switch_to(cont);
        Ok(match result_val {
            Some(v) => (v, result_ty),
            None => (self.const_unit(), MirTy::Unit),
        })
    }
}

/// `true` for the parser's `?` desugar shape — a `__try_*` Var
/// scrutinee with pristine `ok(v){v}` / `err(e){return ...}` arms
/// (the names are synthetic, so the prefix can't collide with user
/// bindings). Mirrors `try_sugar_sync_shape` in the checker.
fn is_try_desugar(scrutinee: &Expr, arms: &[ast::MatchArm]) -> bool {
    let is_try_var =
        matches!(&scrutinee.kind, ExprKind::Var(n) if n.as_str().starts_with("__try_"));
    if !is_try_var || arms.len() != 2 {
        return false;
    }
    let ok_ok = matches!(
        &arms[0].pattern.kind,
        ast::PatternKind::Variant { variant, bindings: ast::PatternBindings::Tuple(names), .. }
            if variant.as_str() == "ok" && names.len() == 1
    ) && matches!(&arms[0].body.kind, ExprKind::Var(_));
    let err_ok = matches!(
        &arms[1].pattern.kind,
        ast::PatternKind::Variant { variant, .. } if variant.as_str() == "err"
    ) && matches!(&arms[1].body.kind, ExprKind::Return(Some(_)));
    ok_ok && err_ok
}
