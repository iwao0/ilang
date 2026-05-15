//! Segment-graph construction.
//!
//! The body of an async fn splits into N+1 "segments" — straight-
//! line chunks of code separated by `await`s and other control-flow
//! boundaries. `build_segments` walks the body and produces the
//! segment list that `gen` then materializes as an `enum + class +
//! poll fn` triple.
//!
//! Each `Segment` carries:
//!
//! - `idx`: the variant index in the generated state enum
//!   (`S{idx}`).
//! - `fields`: the live-in set at segment entry (params, `__this`,
//!   and let-bindings introduced upstream — over-approximate but
//!   sound).
//! - `stmts`: the sync stmts that run when this variant is matched.
//! - `terminator`: one of `Suspend` / `Branch` / `Jump` / `MatchT`
//!   / `JumpBind` / `Settle` — determines what happens after the
//!   stmts.
//! - `loop_info`: when set, the enclosing `while`'s (header_idx,
//!   after_idx); used to rewrite `break` / `continue` inside the
//!   segment's stmts.

use std::collections::HashMap;

use ilang_ast::{
    Block, EnumDecl, Expr, ExprKind, Param, Pattern, Span, Stmt, StmtKind, Symbol, Type,
};

use super::pattern::{coerce_to_block, mid_body_join_kind, pattern_binding_types, resolve_var_ty};
use super::{block_has_await, expr_has_await, mk_expr_stmt, mk_int};

/// One segment of an async fn body — code between two awaits (or
/// between the body start and the first await, or between the last
/// await and the body end). Identified by `idx` (0-based), which
/// corresponds to the state-enum variant `S{idx}`.
#[derive(Debug, Clone)]
pub(super) struct Segment {
    pub idx: u32,
    /// Field layout for the variant payload that represents this
    /// segment. Over-approximate (params + every let-binding
    /// introduced before this segment, in source order). Always
    /// includes `__this` for class-method asyncs.
    pub fields: Vec<(Symbol, Type)>,
    /// Sync stmts to execute when this variant is matched.
    pub stmts: Vec<Stmt>,
    /// Terminator action.
    pub terminator: SegTerm,
    /// If this segment lives inside a `while` body, the (header_idx,
    /// after_idx) of the enclosing loop. Used by emit_segment_arm
    /// to rewrite `break` → transition-to-after and `continue` →
    /// transition-to-header within the segment's stmts.
    pub loop_info: Option<(u32, u32)>,
}

#[derive(Debug, Clone)]
pub(super) enum SegTerm {
    /// `let <binding>: <binding_ty> = await <promise>` — schedule
    /// `promise` with a continuation that builds variant
    /// `S{next_idx}` from the destructured-and-new locals.
    Suspend {
        promise: Expr,
        binding: Symbol,
        binding_ty: Type,
        next_idx: u32,
    },
    /// Tail-position `if cond { ... } else { ... }`. Each branch
    /// has its own segment chain (rooted at `then_idx` / `else_idx`)
    /// that independently settles the result promise.
    Branch {
        cond: Expr,
        then_idx: u32,
        else_idx: u32,
    },
    /// Unconditional transition to `target_idx`. Used as the
    /// "back edge" of a `while` body (back to the header) and as
    /// the "fall-through" from before a `while` into the header.
    Jump { target_idx: u32 },
    /// Tail-position `match scrutinee { arm1 => target1, ... }`.
    /// Each arm picks a target segment to continue at; pattern
    /// bindings introduced by the arm become locals of the target
    /// variant's payload.
    MatchT {
        scrutinee: Expr,
        arms: Vec<MatchTArm>,
    },
    /// Mid-body join: transition to `target_idx` after binding
    /// `binding = value` into the destination variant's payload.
    /// Used to merge per-arm results of a `let r = if-else/match`
    /// back into the post-construct segment chain.
    JumpBind {
        target_idx: u32,
        binding: Symbol,
        value: Expr,
    },
    /// Final segment: settle the result promise with `value`.
    Settle { value: Expr },
}

#[derive(Debug, Clone)]
pub(super) struct MatchTArm {
    pub pattern: Pattern,
    pub target_idx: u32,
}

// --- Body shape detection -------------------------------------------

/// Body shape that the segment builder supports — straight-line
/// stmts plus tail-If / tail-Match with awaits in branches /
/// arms, mid-body `let X = if-else / match` joins, stmt-position
/// if-with-await, and while-with-await.
pub(super) fn body_is_supported(body: &Block) -> bool {
    for s in &body.stmts {
        if !stmt_is_supported_for_body(s) {
            return false;
        }
    }
    let Some(t) = body.tail.as_deref() else {
        return true;
    };
    if let ExprKind::If { cond, then_branch, else_branch } = &t.kind {
        let has_await_in_branches = block_has_await(then_branch)
            || else_branch.as_deref().is_some_and(expr_has_await);
        if has_await_in_branches {
            // Tail-If with awaits: needs Branch terminator. Requires
            // else (which may be a Block OR an `if` expr — else-if
            // chains coerce into a Block whose tail is the chained If).
            if expr_has_await(cond) {
                return false;
            }
            let Some(eb) = else_branch.as_deref() else {
                return false;
            };
            let else_block = coerce_to_block(eb);
            return body_is_supported(then_branch) && body_is_supported(&else_block);
        }
        // Sync tail-If (no awaits in branches) — fall through.
    }
    if let ExprKind::Match { scrutinee, arms } = &t.kind {
        let has_arm_await = arms.iter().any(|a| expr_has_await(&a.body));
        if has_arm_await {
            if expr_has_await(scrutinee) {
                return false;
            }
            for a in arms.iter() {
                let arm_block = match &a.body.kind {
                    ExprKind::Block(b) => b.clone(),
                    _ => Block {
                        stmts: Vec::new(),
                        tail: Some(Box::new(a.body.clone())),
                    },
                };
                if !body_is_supported(&arm_block) {
                    return false;
                }
            }
            return true;
        }
        // Sync tail-Match — fall through.
    }
    !expr_contains_control_flow_with_await(t)
}

fn stmt_is_straight_line(s: &Stmt) -> bool {
    match &s.kind {
        StmtKind::Let { value, .. } => !expr_contains_control_flow_with_await(value),
        StmtKind::LetTuple { value, .. } | StmtKind::LetStruct { value, .. } => {
            !expr_contains_control_flow_with_await(value)
        }
        StmtKind::Expr(e) => match &e.kind {
            // While/Loop/For as statements don't qualify for the
            // straight-line case — but `body_is_supported` adds a
            // separate clause for them.
            ExprKind::While { .. } | ExprKind::Loop { .. } | ExprKind::ForIn { .. } => {
                !block_or_expr_has_await_inside(e)
            }
            _ => !expr_contains_control_flow_with_await(e),
        },
    }
}

/// A stmt is "supported in a body" if it's straight-line, OR a
/// supported while-with-await, OR a stmt-position if-with-await,
/// OR a mid-body `let X = if-else / match` whose branches / arms
/// have awaits.
fn stmt_is_supported_for_body(s: &Stmt) -> bool {
    if stmt_is_straight_line(s) {
        return true;
    }
    if let StmtKind::Expr(e) = &s.kind {
        if let ExprKind::While { cond, body } = &e.kind {
            if expr_has_await(cond) {
                return false;
            }
            return loop_body_is_supported(body);
        }
        // Stmt-position `if cond { ...await... } [else { ... }]` —
        // both arms flow to a synthesized after segment. else may
        // be omitted (then we just synthesize an empty else→after
        // jump).
        if let ExprKind::If { cond, then_branch, else_branch } = &e.kind {
            let any_await = block_has_await(then_branch)
                || else_branch.as_deref().is_some_and(expr_has_await);
            if any_await {
                if expr_has_await(cond) {
                    return false;
                }
                if !body_is_supported(then_branch) {
                    return false;
                }
                if let Some(eb) = else_branch.as_deref() {
                    let else_blk = coerce_to_block(eb);
                    if !body_is_supported(&else_blk) {
                        return false;
                    }
                }
                return true;
            }
        }
    }
    if let StmtKind::Let { value, .. } = &s.kind {
        // mid-body `let X = if-else { ...await... }` (join).
        if let ExprKind::If { cond, then_branch, else_branch } = &value.kind {
            if expr_has_await(cond) {
                return false;
            }
            let Some(eb) = else_branch.as_deref() else {
                return false;
            };
            let else_block = coerce_to_block(eb);
            return body_is_supported(then_branch) && body_is_supported(&else_block);
        }
        // mid-body `let X = match { ...await... }` (join).
        if let ExprKind::Match { scrutinee, arms } = &value.kind {
            if expr_has_await(scrutinee) {
                return false;
            }
            for a in arms.iter() {
                let arm_block = match &a.body.kind {
                    ExprKind::Block(b) => b.clone(),
                    _ => Block {
                        stmts: Vec::new(),
                        tail: Some(Box::new(a.body.clone())),
                    },
                };
                if !body_is_supported(&arm_block) {
                    return false;
                }
            }
            return true;
        }
    }
    false
}

/// A loop body's shape is the same as a fn body's: tail-If/match
/// with awaits, mid-body let-if/match join, nested while, etc.
fn loop_body_is_supported(body: &Block) -> bool {
    body_is_supported(body)
}

fn block_or_expr_has_await_inside(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::While { body, .. } | ExprKind::Loop { body, .. } => block_has_await(body),
        ExprKind::ForIn { body, .. } => block_has_await(body),
        _ => false,
    }
}

fn expr_contains_control_flow_with_await(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::If { then_branch, else_branch, .. } => {
            block_has_await(then_branch)
                || else_branch.as_deref().is_some_and(expr_has_await)
        }
        ExprKind::While { body, .. } | ExprKind::Loop { body, .. } => block_has_await(body),
        ExprKind::ForIn { body, .. } => block_has_await(body),
        ExprKind::Match { arms, .. } => arms.iter().any(|a| expr_has_await(&a.body)),
        _ => false,
    }
}

// --- Segment construction -------------------------------------------

/// What terminator caps the final segment of a block walked by
/// `build_block`. `SettleTail` produces `Settle{value: tail}` from
/// the block's tail expression (the default for fn-body tails and
/// for if-else arms). `JumpTo(idx)` ignores the tail value and
/// unconditionally jumps — used for while-body tails to flow the
/// back-edge into the loop header.
#[derive(Debug, Clone, Copy)]
enum FinalTerm {
    SettleTail,
    JumpTo(u32),
    /// Bind the block's tail value into `binding` of variant
    /// `target_idx`'s payload, then jump. Used for arms that feed a
    /// mid-body join (e.g. `let r = if-else { ...await... }`).
    JumpBindTail { target_idx: u32, binding: Symbol },
}

/// Stateful builder used while walking a block tree.
struct SegBuilder<'a> {
    segments: Vec<Segment>,
    next_idx: u32,
    let_ty: &'a HashMap<Symbol, Type>,
    span: Span,
    /// Innermost enclosing loop's (header_idx, after_idx). Newly
    /// pushed segments inherit this as their `loop_info` so
    /// emit_segment_arm knows where break/continue should jump.
    cur_loop: Option<(u32, u32)>,
    /// Enum declarations in scope, used to resolve match-pattern
    /// binding types from a variant's payload spec. Keyed by enum
    /// name. Built from `Item::Enum` entries by the caller.
    enums: &'a HashMap<Symbol, EnumDecl>,
}

impl<'a> SegBuilder<'a> {
    fn alloc_idx(&mut self) -> u32 {
        let i = self.next_idx;
        self.next_idx += 1;
        i
    }

    /// Push a segment, attaching the current `loop_info` so the
    /// arm-emission pass can resolve break/continue inside its
    /// stmts.
    fn push_seg(&mut self, idx: u32, fields: Vec<(Symbol, Type)>, stmts: Vec<Stmt>, term: SegTerm) {
        self.segments.push(Segment {
            idx,
            fields,
            stmts,
            terminator: term,
            loop_info: self.cur_loop,
        });
    }

    /// Build segments for `block`. On entry, `self_idx` is the
    /// variant index reserved for the FIRST segment this call will
    /// push (allocated by the caller). `cumulative_fields` is the
    /// live-in set for that first segment. `final_term` decides how
    /// the last segment (the one holding the block's tail) is capped.
    fn build_block(
        &mut self,
        block: &Block,
        self_idx: u32,
        mut cumulative_fields: Vec<(Symbol, Type)>,
        final_term: FinalTerm,
    ) {
        let mut cur_stmts: Vec<Stmt> = Vec::new();
        let mut pending_lets: Vec<(Symbol, Type)> = Vec::new();
        let mut idx = self_idx;
        let stmts_slice = &block.stmts[..];
        let mut stmt_i = 0usize;
        while stmt_i < stmts_slice.len() {
            let s = &stmts_slice[stmt_i];
            stmt_i += 1;
            // Mid-body `let X = if-else { ...await... }` / `let X =
            // match { ...await... }`: branch into per-arm chains
            // that converge on a join segment carrying `X` as a new
            // live local. The rest of the outer block is then
            // processed FROM the join idx via a tail-recursive call.
            if let StmtKind::Let { name, value, ty, .. } = &s.kind {
                let join_kind = mid_body_join_kind(value);
                if join_kind.is_some() {
                    let r_ty = ty
                        .clone()
                        .or_else(|| self.let_ty.get(name).cloned())
                        .unwrap_or(Type::I64);
                    let join_idx = self.alloc_idx();
                    let mut branch_live = cumulative_fields.clone();
                    branch_live.append(&mut pending_lets);
                    match &value.kind {
                        ExprKind::If { cond, then_branch, else_branch } => {
                            let then_idx = self.alloc_idx();
                            let else_idx = self.alloc_idx();
                            self.push_seg(
                                idx,
                                cumulative_fields,
                                std::mem::take(&mut cur_stmts),
                                SegTerm::Branch {
                                    cond: (**cond).clone(),
                                    then_idx,
                                    else_idx,
                                },
                            );
                            self.build_block(
                                then_branch,
                                then_idx,
                                branch_live.clone(),
                                FinalTerm::JumpBindTail { target_idx: join_idx, binding: *name },
                            );
                            if let Some(eb) = else_branch.as_deref() {
                                let else_blk = coerce_to_block(eb);
                                self.build_block(
                                    &else_blk,
                                    else_idx,
                                    branch_live.clone(),
                                    FinalTerm::JumpBindTail { target_idx: join_idx, binding: *name },
                                );
                            }
                        }
                        ExprKind::Match { scrutinee, arms } => {
                            let scrut_ty = resolve_var_ty(scrutinee, &branch_live);
                            let mut term_arms: Vec<MatchTArm> = Vec::new();
                            let mut per_arm: Vec<(u32, Vec<(Symbol, Type)>, Block)> = Vec::new();
                            for a in arms.iter() {
                                let target_idx = self.alloc_idx();
                                let typed_bindings = pattern_binding_types(
                                    &a.pattern,
                                    scrut_ty.as_ref(),
                                    self.enums,
                                );
                                let arm_block = match &a.body.kind {
                                    ExprKind::Block(b) => b.clone(),
                                    _ => Block {
                                        stmts: Vec::new(),
                                        tail: Some(Box::new(a.body.clone())),
                                    },
                                };
                                term_arms.push(MatchTArm {
                                    pattern: a.pattern.clone(),
                                    target_idx,
                                });
                                per_arm.push((target_idx, typed_bindings, arm_block));
                            }
                            self.push_seg(
                                idx,
                                cumulative_fields,
                                std::mem::take(&mut cur_stmts),
                                SegTerm::MatchT {
                                    scrutinee: (**scrutinee).clone(),
                                    arms: term_arms,
                                },
                            );
                            for (target_idx, typed_bindings, arm_block) in per_arm {
                                let mut arm_live = branch_live.clone();
                                for (b, t) in &typed_bindings {
                                    arm_live.push((*b, t.clone()));
                                }
                                self.build_block(
                                    &arm_block,
                                    target_idx,
                                    arm_live,
                                    FinalTerm::JumpBindTail { target_idx: join_idx, binding: *name },
                                );
                            }
                        }
                        _ => unreachable!("join_kind matched but RHS isn't If/Match"),
                    }
                    // Continue the outer block at join_idx with `X`
                    // added as a live local. Tail-recurse.
                    let rest = Block {
                        stmts: stmts_slice[stmt_i..].to_vec(),
                        tail: block.tail.clone(),
                    };
                    let mut join_live = branch_live;
                    join_live.push((*name, r_ty));
                    self.build_block(&rest, join_idx, join_live, final_term);
                    return;
                }
            }
            // `let X = await E` — Suspend terminator boundary.
            if let StmtKind::Let { name, value, .. } = &s.kind {
                if let ExprKind::Await(p) = &value.kind {
                    let binding_ty =
                        self.let_ty.get(name).cloned().unwrap_or(Type::I64);
                    let next_idx = self.alloc_idx();
                    self.push_seg(
                        idx,
                        cumulative_fields.clone(),
                        std::mem::take(&mut cur_stmts),
                        SegTerm::Suspend {
                            promise: (**p).clone(),
                            binding: *name,
                            binding_ty: binding_ty.clone(),
                            next_idx,
                        },
                    );
                    cumulative_fields.append(&mut pending_lets);
                    cumulative_fields.push((*name, binding_ty));
                    idx = next_idx;
                    continue;
                }
            }
            // Stmt-position `if cond { ...await... } [else { ... }]`.
            if let StmtKind::Expr(e) = &s.kind {
                if let ExprKind::If { cond, then_branch, else_branch } = &e.kind {
                    let any_await = block_has_await(then_branch)
                        || else_branch.as_deref().is_some_and(expr_has_await);
                    if any_await {
                        let then_idx = self.alloc_idx();
                        let else_idx = self.alloc_idx();
                        let after_idx = self.alloc_idx();
                        let mut branch_live = cumulative_fields.clone();
                        branch_live.append(&mut pending_lets);
                        self.push_seg(
                            idx,
                            cumulative_fields,
                            std::mem::take(&mut cur_stmts),
                            SegTerm::Branch {
                                cond: (**cond).clone(),
                                then_idx,
                                else_idx,
                            },
                        );
                        self.build_block(
                            then_branch,
                            then_idx,
                            branch_live.clone(),
                            FinalTerm::JumpTo(after_idx),
                        );
                        if let Some(eb) = else_branch.as_deref() {
                            let else_blk = coerce_to_block(eb);
                            self.build_block(
                                &else_blk,
                                else_idx,
                                branch_live.clone(),
                                FinalTerm::JumpTo(after_idx),
                            );
                        } else {
                            // Synth empty else segment: jump straight to after.
                            self.push_seg(
                                else_idx,
                                branch_live.clone(),
                                Vec::new(),
                                SegTerm::Jump { target_idx: after_idx },
                            );
                        }
                        cumulative_fields = branch_live;
                        idx = after_idx;
                        continue;
                    }
                }
            }
            // While-with-await statement.
            if let StmtKind::Expr(e) = &s.kind {
                if let ExprKind::While { cond, body } = &e.kind {
                    if block_has_await(body) {
                        let header_idx = self.alloc_idx();
                        let body_idx = self.alloc_idx();
                        let after_idx = self.alloc_idx();
                        let mut live = cumulative_fields.clone();
                        live.append(&mut pending_lets);
                        self.push_seg(
                            idx,
                            cumulative_fields.clone(),
                            std::mem::take(&mut cur_stmts),
                            SegTerm::Jump { target_idx: header_idx },
                        );
                        self.push_seg(
                            header_idx,
                            live.clone(),
                            Vec::new(),
                            SegTerm::Branch {
                                cond: (**cond).clone(),
                                then_idx: body_idx,
                                else_idx: after_idx,
                            },
                        );
                        let saved_loop = self.cur_loop;
                        self.cur_loop = Some((header_idx, after_idx));
                        self.build_block(
                            body,
                            body_idx,
                            live.clone(),
                            FinalTerm::JumpTo(header_idx),
                        );
                        self.cur_loop = saved_loop;
                        cumulative_fields = live;
                        idx = after_idx;
                        continue;
                    }
                }
            }
            if let StmtKind::Let { name, ty, .. } = &s.kind {
                let resolved = ty
                    .clone()
                    .or_else(|| self.let_ty.get(name).cloned())
                    .unwrap_or(Type::I64);
                pending_lets.push((*name, resolved));
            }
            cur_stmts.push(s.clone());
        }

        // Tail handling: tail-Match with awaits → MatchT terminator.
        if let Some(t) = block.tail.as_deref() {
            if let ExprKind::Match { scrutinee, arms } = &t.kind {
                let has_arm_await = arms.iter().any(|a| expr_has_await(&a.body));
                if has_arm_await {
                    let mut branch_live = cumulative_fields.clone();
                    branch_live.append(&mut pending_lets);
                    let scrut_ty = resolve_var_ty(scrutinee, &branch_live);
                    let mut term_arms: Vec<MatchTArm> = Vec::new();
                    let mut per_arm: Vec<(u32, Vec<(Symbol, Type)>, Block)> = Vec::new();
                    for a in arms.iter() {
                        let target_idx = self.alloc_idx();
                        let typed_bindings = pattern_binding_types(
                            &a.pattern,
                            scrut_ty.as_ref(),
                            self.enums,
                        );
                        let arm_block = match &a.body.kind {
                            ExprKind::Block(b) => b.clone(),
                            _ => Block {
                                stmts: Vec::new(),
                                tail: Some(Box::new(a.body.clone())),
                            },
                        };
                        term_arms.push(MatchTArm {
                            pattern: a.pattern.clone(),
                            target_idx,
                        });
                        per_arm.push((target_idx, typed_bindings, arm_block));
                    }
                    self.push_seg(
                        idx,
                        cumulative_fields,
                        cur_stmts,
                        SegTerm::MatchT {
                            scrutinee: (**scrutinee).clone(),
                            arms: term_arms,
                        },
                    );
                    for (target_idx, typed_bindings, arm_block) in per_arm {
                        let mut arm_live = branch_live.clone();
                        for (b, t) in &typed_bindings {
                            arm_live.push((*b, t.clone()));
                        }
                        self.build_block(&arm_block, target_idx, arm_live, final_term);
                    }
                    return;
                }
            }
            if let ExprKind::If { cond, then_branch, else_branch } = &t.kind {
                let has_branch_await = block_has_await(then_branch)
                    || else_branch.as_deref().is_some_and(expr_has_await);
                if has_branch_await {
                    let mut branch_live = cumulative_fields.clone();
                    branch_live.append(&mut pending_lets);
                    let then_idx = self.alloc_idx();
                    let else_idx = self.alloc_idx();
                    self.push_seg(
                        idx,
                        cumulative_fields,
                        cur_stmts,
                        SegTerm::Branch {
                            cond: (**cond).clone(),
                            then_idx,
                            else_idx,
                        },
                    );
                    self.build_block(then_branch, then_idx, branch_live.clone(), final_term);
                    if let Some(eb) = else_branch.as_deref() {
                        let else_blk = coerce_to_block(eb);
                        self.build_block(&else_blk, else_idx, branch_live, final_term);
                    }
                    return;
                }
            }
        }

        // Default cap: depends on final_term.
        let mut final_stmts = cur_stmts;
        let term = match final_term {
            FinalTerm::SettleTail => {
                let tail_val = block
                    .tail
                    .as_deref()
                    .cloned()
                    .unwrap_or_else(|| mk_int(0, self.span));
                SegTerm::Settle { value: tail_val }
            }
            FinalTerm::JumpTo(target) => {
                // Loop body: tail value is discarded but the tail
                // expr may have side effects (`if cond { break }`).
                if let Some(t) = block.tail.as_deref() {
                    final_stmts.push(mk_expr_stmt(t.clone(), self.span));
                }
                SegTerm::Jump { target_idx: target }
            }
            FinalTerm::JumpBindTail { target_idx, binding } => {
                // Mid-body join arm: the block's tail is the value
                // bound into `binding` at the join variant.
                let value = block
                    .tail
                    .as_deref()
                    .cloned()
                    .unwrap_or_else(|| mk_int(0, self.span));
                SegTerm::JumpBind { target_idx, binding, value }
            }
        };
        self.push_seg(idx, cumulative_fields, final_stmts, term);
    }
}

/// Walk a (supported) body and produce one Segment per state.
/// `body_lets` is the (name, type) list previously computed by
/// `collect_let_types` — we reuse it for liveness over-approx.
pub(super) fn build_segments(
    body: &Block,
    params: &[Param],
    body_lets: &[(Symbol, Type)],
    enclosing_class: Option<Symbol>,
    span: Span,
    enums: &HashMap<Symbol, EnumDecl>,
) -> Vec<Segment> {
    // V_0's live-in: params (+ __this if class method).
    let mut initial_fields: Vec<(Symbol, Type)> = Vec::new();
    if let Some(class) = enclosing_class {
        initial_fields.push((Symbol::intern("__this"), Type::Object(class)));
    }
    for p in params {
        initial_fields.push((p.name, p.ty.clone()));
    }

    let let_ty: HashMap<Symbol, Type> = body_lets.iter().cloned().collect();
    let mut builder = SegBuilder {
        segments: Vec::new(),
        next_idx: 1,
        let_ty: &let_ty,
        span,
        cur_loop: None,
        enums,
    };
    builder.build_block(body, 0, initial_fields, FinalTerm::SettleTail);
    builder.segments
}
