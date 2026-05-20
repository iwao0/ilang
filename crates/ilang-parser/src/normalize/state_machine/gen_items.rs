//! AST generators for the state-machine items.
//!
//! Given the segment list from `segments::build_segments`, produce
//! the four items that materialize the state machine:
//!
//! - `gen_state_enum`: one variant per segment, payload is the
//!   segment's live-in set as a struct.
//! - `gen_state_ref_class`: a small heap wrapper holding the
//!   current enum variant and the result promise.
//! - `gen_poll_fn`: a `loop { match state_ref.current { ... } }`
//!   driver. Each arm runs the segment's stmts and then dispatches
//!   on the terminator (Suspend / Branch / Jump / MatchT /
//!   JumpBind / Settle).
//! - `gen_wrapper_fn`: the original-named entry that allocates the
//!   state, kicks off the initial poll, and returns the result
//!   promise.
//!
//! `emit_segment_arm` is the per-arm builder used by `gen_poll_fn`;
//! `mk_transition_block(_override)` is the small helper that emits
//! `state_ref.current = S{N}{...}; __poll; return` blocks for the
//! various terminator kinds.

use ilang_ast::{
    Block, ClassDecl, EnumDecl, Expr, ExprKind, FieldDecl, FnDecl, MatchArm, Param, Pattern,
    PatternBindings, PatternKind, Span, Stmt, StmtKind, Symbol, Type, Variant, VariantPayload,
};

use super::segments::{SegTerm, Segment};
use super::{
    mk_assign_field, mk_call, mk_enum_ctor_struct, mk_expr_stmt, mk_field, mk_int, mk_let,
    mk_method_call, mk_var,
};

// --- AST generators -------------------------------------------------

/// Generate the state enum: one variant per segment, each carrying
/// the segment's field layout as a Struct payload.
pub fn gen_state_enum(
    enum_name: Symbol,
    segments: &[Segment],
    span: Span,
    type_params: Box<[Symbol]>,
) -> EnumDecl {
    let mut variants: Vec<Variant> = Vec::new();
    for seg in segments {
        let fields: Vec<FieldDecl> = seg
            .fields
            .iter()
            .map(|(n, t)| FieldDecl {
                is_pub: true,
                name: *n,
                ty: t.clone(),
                span,
                bits: None,
            })
            .collect();
        variants.push(Variant {
            name: Symbol::intern(&format!("S{}", seg.idx)),
            payload: VariantPayload::Struct(fields.into_boxed_slice()),
            discriminant: None,
            span,
        });
    }
    EnumDecl {
        is_pub: false,
        name: enum_name,
        type_params,
        repr_ty: None,
        flags: false,
        variants: variants.into_boxed_slice(),
        span,
    }
}

/// Generate the state-ref class. Two fields: `current: EnumT` and
/// `__async_promise: Promise<T>`. The init writes both verbatim.
pub fn gen_state_ref_class(
    class_name: Symbol,
    enum_name: Symbol,
    promise_ret: &Type,
    span: Span,
    type_params: Box<[Symbol]>,
) -> ClassDecl {
    let enum_ty = if type_params.is_empty() {
        Type::Object(enum_name)
    } else {
        Type::generic(
            enum_name,
            type_params.iter().map(|p| Type::Object(*p)).collect(),
        )
    };
    let fields = vec![
        FieldDecl {
            is_pub: true,
            name: Symbol::intern("current"),
            ty: enum_ty.clone(),
            span,
            bits: None,
        },
        FieldDecl {
            is_pub: true,
            name: Symbol::intern("__async_promise"),
            ty: promise_ret.clone(),
            span,
            bits: None,
        },
    ];
    let init_initial = Symbol::intern("__init_state");
    let init_prom = Symbol::intern("__init_prom");
    let init_params = vec![
        Param {
            name: init_initial,
            ty: enum_ty.clone(),
            span,
            default: None,
        },
        Param {
            name: init_prom,
            ty: promise_ret.clone(),
            span,
            default: None,
        },
    ];
    let this_e = || Expr::new(ExprKind::This, span);
    let init_stmts = vec![
        mk_expr_stmt(
            mk_assign_field(
                this_e(),
                Symbol::intern("current"),
                mk_var(init_initial, span),
                span,
            ),
            span,
        ),
        mk_expr_stmt(
            mk_assign_field(
                this_e(),
                Symbol::intern("__async_promise"),
                mk_var(init_prom, span),
                span,
            ),
            span,
        ),
    ];
    let init_method = FnDecl {
        attrs: Box::new([]),
        is_pub: true,
        name: Symbol::intern("init"),
        type_params: Box::new([]),
        params: init_params.into_boxed_slice(),
        ret: None,
        body: Block { stmts: init_stmts, tail: None },
        span,
        is_override: false,
        is_async: false,
    };
    ClassDecl {
        extern_lib: None,
        is_repr_c: false,
        is_packed: false,
        is_handle: false,
        is_union: false,
        is_pub: false,
        name: class_name,
        parent: None,
        interfaces: Box::new([]),
        type_params,
        fields: fields.into_boxed_slice(),
        methods: Box::new([init_method]),
        static_methods: Box::new([]),
        static_fields: Box::new([]),
        properties: Box::new([]),
        attrs: Box::new([]),
        span,
    }
}

/// Generate the poll fn. Body: `loop { match state_ref.current { ... } }`
/// where each arm runs the segment's stmts, then either suspends
/// (`.then` registration + return) or settles + returns.
pub fn gen_poll_fn(
    poll_name: Symbol,
    state_ref_class: Symbol,
    state_enum: Symbol,
    segments: &[Segment],
    enclosing_class: Option<Symbol>,
    span: Span,
    type_params: Box<[Symbol]>,
) -> FnDecl {
    let state_ref_param = Symbol::intern("__state_ref");
    let dummy_awaited_param = Symbol::intern("__awaited_value");

    // Build a `idx -> &Segment` lookup. Segments are appended to
    // the vec in DFS push order, not variant-index order (the
    // Branch terminator allocates `then_idx` / `else_idx` before
    // recursing into either branch), so direct `segments[idx]` is
    // wrong.
    let mut by_idx: Vec<Option<&Segment>> =
        std::iter::repeat_with(|| None).take(segments.len()).collect();
    for s in segments {
        let i = s.idx as usize;
        if i >= by_idx.len() {
            // Defensive: grow if needed (shouldn't happen given
            // segments.len() == # variants).
            by_idx.resize_with(i + 1, || None);
        }
        by_idx[i] = Some(s);
    }
    let by_idx: Vec<&Segment> = by_idx.into_iter().flatten().collect();

    let mut match_arms: Vec<MatchArm> = Vec::new();
    for seg in segments {
        let arm_body = emit_segment_arm(
            seg,
            &by_idx,
            state_ref_param,
            poll_name,
            state_enum,
            enclosing_class,
            span,
        );
        let pattern = Pattern {
            kind: PatternKind::Variant {
                enum_name: Some(state_enum),
                variant: Symbol::intern(&format!("S{}", seg.idx)),
                bindings: PatternBindings::Struct(
                    seg.fields
                        .iter()
                        .map(|(n, _)| (*n, *n))
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                ),
            },
            span,
        };
        match_arms.push(MatchArm {
            pattern,
            body: Expr::new(ExprKind::Block(arm_body), span),
            span,
        });
    }

    let match_expr = Expr::new(
        ExprKind::Match {
            scrutinee: Box::new(mk_field(
                mk_var(state_ref_param, span),
                Symbol::intern("current"),
                span,
            )),
            arms: match_arms.into_boxed_slice(),
        },
        span,
    );
    let loop_body = Block {
        stmts: vec![mk_expr_stmt(match_expr, span)],
        tail: None,
    };
    let body = Block {
        stmts: vec![mk_expr_stmt(
            Expr::new(ExprKind::Loop { body: loop_body }, span),
            span,
        )],
        tail: None,
    };
    let state_ref_ty = if type_params.is_empty() {
        Type::Object(state_ref_class)
    } else {
        Type::generic(
            state_ref_class,
            type_params.iter().map(|p| Type::Object(*p)).collect(),
        )
    };
    FnDecl {
        attrs: Box::new([]),
        is_pub: false,
        name: poll_name,
        type_params,
        params: Box::new([
            Param {
                name: state_ref_param,
                ty: state_ref_ty,
                span,
                default: None,
            },
            Param {
                name: dummy_awaited_param,
                ty: Type::I64,
                span,
                default: None,
            },
        ]),
        ret: None,
        body,
        span,
        is_override: false,
        is_async: false,
    }
}

/// Build one segment's arm body. Runs the segment's sync stmts,
/// then handles the terminator (Suspend or Settle), then `return`s
/// to exit the outer poll fn (preventing the `loop { ... }` from
/// iterating again).
/// Build a "transition to `target_idx` and re-enter __poll" Block:
/// `{ state_ref.current = S{target}{...locals...}; __poll(state_ref, 0); return; }`.
/// The ctor args are pulled from the destination variant's field list
/// — each one defaults to `Var(field_name)` (the local must be in
/// scope), but `overrides` may supply an arbitrary expression for
/// specific fields (used by `JumpBind` to thread the arm's tail
/// value into a join variant's binding field).
fn mk_transition_block_override(
    target_idx: u32,
    all_segments: &[&Segment],
    state_ref_param: Symbol,
    poll_name: Symbol,
    state_enum: Symbol,
    span: Span,
    overrides: &[(Symbol, Expr)],
) -> Block {
    let target_seg = &all_segments[target_idx as usize];
    let ctor_args: Vec<(Symbol, Expr)> = target_seg
        .fields
        .iter()
        .map(|(n, _)| {
            if let Some((_, v)) = overrides.iter().find(|(name, _)| name == n) {
                let mut e = v.clone();
                rewrite_this_in_expr(&mut e);
                (*n, e)
            } else {
                let mut e = mk_var(*n, span);
                rewrite_this_in_expr(&mut e);
                (*n, e)
            }
        })
        .collect();
    let new_variant = mk_enum_ctor_struct(
        state_enum,
        Symbol::intern(&format!("S{}", target_idx)),
        ctor_args,
        span,
    );
    Block {
        stmts: vec![
            mk_expr_stmt(
                mk_assign_field(
                    mk_var(state_ref_param, span),
                    Symbol::intern("current"),
                    new_variant,
                    span,
                ),
                span,
            ),
            mk_expr_stmt(
                mk_call(
                    poll_name,
                    vec![mk_var(state_ref_param, span), mk_int(0, span)],
                    span,
                ),
                span,
            ),
            mk_expr_stmt(
                Expr::new(ExprKind::Return(None), span),
                span,
            ),
        ],
        tail: None,
    }
}

/// Shorthand for `mk_transition_block_override` with no overrides.
fn mk_transition_block(
    target_idx: u32,
    all_segments: &[&Segment],
    state_ref_param: Symbol,
    poll_name: Symbol,
    state_enum: Symbol,
    span: Span,
) -> Block {
    mk_transition_block_override(
        target_idx, all_segments, state_ref_param, poll_name, state_enum, span, &[],
    )
}

/// Walk `b` and replace every `break` / `continue` (at any nesting
/// depth, but NOT crossing a nested loop) with a transition Block to
/// `after_idx` / `header_idx` respectively. Phase 2b doesn't allow
/// nested loops inside an async while body, so we don't currently
/// need to skip nested while bodies — but the walker is defensive
/// and stops at them.
fn rewrite_loop_jumps_block(
    b: &mut Block,
    header_idx: u32,
    after_idx: u32,
    all_segments: &[&Segment],
    state_ref_param: Symbol,
    poll_name: Symbol,
    state_enum: Symbol,
    span: Span,
) {
    for s in b.stmts.iter_mut() {
        rewrite_loop_jumps_stmt(
            s, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum, span,
        );
    }
    if let Some(t) = b.tail.as_mut() {
        rewrite_loop_jumps_expr(
            t, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum, span,
        );
    }
}

fn rewrite_loop_jumps_stmt(
    s: &mut Stmt,
    header_idx: u32,
    after_idx: u32,
    all_segments: &[&Segment],
    state_ref_param: Symbol,
    poll_name: Symbol,
    state_enum: Symbol,
    span: Span,
) {
    match &mut s.kind {
        StmtKind::Let { value, .. }
        | StmtKind::LetTuple { value, .. }
        | StmtKind::LetStruct { value, .. } => rewrite_loop_jumps_expr(
            value, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
            span,
        ),
        StmtKind::Expr(e) => rewrite_loop_jumps_expr(
            e, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum, span,
        ),
    }
}

fn rewrite_loop_jumps_expr(
    e: &mut Expr,
    header_idx: u32,
    after_idx: u32,
    all_segments: &[&Segment],
    state_ref_param: Symbol,
    poll_name: Symbol,
    state_enum: Symbol,
    span: Span,
) {
    match &mut e.kind {
        ExprKind::Break(_) => {
            let blk = mk_transition_block(
                after_idx, all_segments, state_ref_param, poll_name, state_enum, span,
            );
            e.kind = ExprKind::Block(blk);
        }
        ExprKind::Continue => {
            let blk = mk_transition_block(
                header_idx, all_segments, state_ref_param, poll_name, state_enum, span,
            );
            e.kind = ExprKind::Block(blk);
        }
        // Nested loops would shadow break/continue — stop recursing.
        ExprKind::While { .. } | ExprKind::Loop { .. } | ExprKind::ForIn { .. } => {}
        ExprKind::Block(b) => rewrite_loop_jumps_block(
            b, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum, span,
        ),
        ExprKind::If { cond, then_branch, else_branch } => {
            rewrite_loop_jumps_expr(
                cond, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
                span,
            );
            rewrite_loop_jumps_block(
                then_branch, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                state_enum, span,
            );
            if let Some(eb) = else_branch {
                rewrite_loop_jumps_expr(
                    eb, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                    state_enum, span,
                );
            }
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            rewrite_loop_jumps_expr(
                expr, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
                span,
            );
            rewrite_loop_jumps_block(
                then_branch, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                state_enum, span,
            );
            if let Some(eb) = else_branch {
                rewrite_loop_jumps_expr(
                    eb, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                    state_enum, span,
                );
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            rewrite_loop_jumps_expr(
                scrutinee, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                state_enum, span,
            );
            for a in arms.iter_mut() {
                rewrite_loop_jumps_expr(
                    &mut a.body, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                    state_enum, span,
                );
            }
        }
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            rewrite_loop_jumps_expr(
                lhs, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
                span,
            );
            rewrite_loop_jumps_expr(
                rhs, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
                span,
            );
        }
        ExprKind::Unary { expr, .. }
        | ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. } => rewrite_loop_jumps_expr(
            expr, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
            span,
        ),
        ExprKind::Some(inner) | ExprKind::Await(inner) => rewrite_loop_jumps_expr(
            inner, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
            span,
        ),
        ExprKind::Return(opt) => {
            if let Some(inner) = opt {
                rewrite_loop_jumps_expr(
                    inner, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                    state_enum, span,
                );
            }
        }
        ExprKind::Assign { value, .. } => rewrite_loop_jumps_expr(
            value, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
            span,
        ),
        ExprKind::AssignField { obj, value, .. } => {
            rewrite_loop_jumps_expr(
                obj, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
                span,
            );
            rewrite_loop_jumps_expr(
                value, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                state_enum, span,
            );
        }
        ExprKind::AssignIndex { obj, index, value } => {
            rewrite_loop_jumps_expr(
                obj, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
                span,
            );
            rewrite_loop_jumps_expr(
                index, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                state_enum, span,
            );
            rewrite_loop_jumps_expr(
                value, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                state_enum, span,
            );
        }
        ExprKind::Call { args, .. }
        | ExprKind::SuperCall { args, .. }
        | ExprKind::New { args, .. } => {
            for a in args.iter_mut() {
                rewrite_loop_jumps_expr(
                    a, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                    state_enum, span,
                );
            }
        }
        ExprKind::MethodCall { obj, args, .. } => {
            rewrite_loop_jumps_expr(
                obj, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
                span,
            );
            for a in args.iter_mut() {
                rewrite_loop_jumps_expr(
                    a, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                    state_enum, span,
                );
            }
        }
        ExprKind::Field { obj, .. } => rewrite_loop_jumps_expr(
            obj, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
            span,
        ),
        ExprKind::Index { obj, index } => {
            rewrite_loop_jumps_expr(
                obj, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
                span,
            );
            rewrite_loop_jumps_expr(
                index, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                state_enum, span,
            );
        }
        ExprKind::Tuple(es) | ExprKind::Array(es) => {
            for e in es.iter_mut() {
                rewrite_loop_jumps_expr(
                    e, header_idx, after_idx, all_segments, state_ref_param, poll_name,
                    state_enum, span,
                );
            }
        }
        ExprKind::FnExpr { body, .. } => rewrite_loop_jumps_block(
            body, header_idx, after_idx, all_segments, state_ref_param, poll_name, state_enum,
            span,
        ),
        _ => {}
    }
}

fn emit_segment_arm(
    seg: &Segment,
    all_segments: &[&Segment],
    state_ref_param: Symbol,
    poll_name: Symbol,
    state_enum: Symbol,
    _enclosing_class: Option<Symbol>,
    span: Span,
) -> Block {
    let mut stmts: Vec<Stmt> = Vec::new();
    for s in &seg.stmts {
        let mut s2 = s.clone();
        rewrite_this_in_stmt(&mut s2);
        // If this segment lives inside a loop, rewrite break/continue
        // inside its stmts to transition Blocks.
        if let Some((header_idx, after_idx)) = seg.loop_info {
            rewrite_loop_jumps_stmt(
                &mut s2,
                header_idx,
                after_idx,
                all_segments,
                state_ref_param,
                poll_name,
                state_enum,
                span,
            );
        }
        stmts.push(s2);
    }
    match &seg.terminator {
        SegTerm::Suspend {
            promise,
            binding,
            binding_ty,
            next_idx,
        } => {
            // The continuation closure builds the next variant
            // with: (a) the binding name = the resolved value,
            // (b) every other field of the next variant carried
            // over from the current arm body's locals.
            //
            // The destination variant's field set is computed at
            // build time but we don't have it here — the segments
            // vec encodes it on segment[next_idx]. We approximate
            // by including every current-arm local that the
            // surrounding fn has visibility of.
            // (See enum_state_machine module's call site for the
            // resolved next_fields list.)
            //
            // For now we pass the resolved next_fields via the
            // SegTerm; emit it as the EnumCtor args.
            // Build ctor args for V_{next}: every field of the next
            // variant. For the await binding itself, use the closure
            // parameter; for all others, use the local of the same
            // name (either destructured from V_K or bound by S_K's
            // sync stmts above).
            let next_seg = &all_segments[*next_idx as usize];
            let mut ctor_args: Vec<(Symbol, Expr)> = next_seg
                .fields
                .iter()
                .map(|(n, _)| {
                    if n == binding {
                        (*n, mk_var(*n, span))
                    } else {
                        let mut expr = mk_var(*n, span);
                        rewrite_this_in_expr(&mut expr);
                        (*n, expr)
                    }
                })
                .collect();
            // If next_seg didn't include `binding` (shouldn't happen),
            // append defensively.
            if !next_seg.fields.iter().any(|(n, _)| n == binding) {
                ctor_args.push((*binding, mk_var(*binding, span)));
            }

            let mut prom_expr = promise.clone();
            rewrite_this_in_expr(&mut prom_expr);

            let v_name = *binding;
            let new_variant = mk_enum_ctor_struct(
                state_enum,
                Symbol::intern(&format!("S{}", next_idx)),
                ctor_args,
                span,
            );
            let closure_body = Block {
                stmts: vec![
                    mk_expr_stmt(
                        mk_assign_field(
                            mk_var(state_ref_param, span),
                            Symbol::intern("current"),
                            new_variant,
                            span,
                        ),
                        span,
                    ),
                    mk_expr_stmt(
                        mk_call(
                            poll_name,
                            vec![mk_var(state_ref_param, span), mk_int(0, span)],
                            span,
                        ),
                        span,
                    ),
                ],
                tail: Some(Box::new(mk_var(v_name, span))),
            };
            let closure = Expr::new(
                ExprKind::FnExpr {
                    params: Box::new([Param {
                        name: v_name,
                        ty: binding_ty.clone(),
                        span,
                        default: None,
                    }]),
                    ret: Some(binding_ty.clone()),
                    body: closure_body,
                },
                span,
            );
            let then_call = mk_method_call(
                prom_expr,
                Symbol::intern("then"),
                vec![closure],
                span,
            );
            stmts.push(mk_let(Symbol::intern("_"), None, then_call, span));
            stmts.push(mk_expr_stmt(
                Expr::new(ExprKind::Return(None), span),
                span,
            ));
        }
        SegTerm::Branch { cond, then_idx, else_idx } => {
            let mut cond_e = cond.clone();
            rewrite_this_in_expr(&mut cond_e);
            // If this Branch lives inside a loop body (only happens
            // when the Branch is itself the tail of the loop body —
            // not currently emitted but supported defensively),
            // rewrite break/continue in cond too.
            if let Some((header_idx, after_idx)) = seg.loop_info {
                rewrite_loop_jumps_expr(
                    &mut cond_e,
                    header_idx,
                    after_idx,
                    all_segments,
                    state_ref_param,
                    poll_name,
                    state_enum,
                    span,
                );
            }
            let then_blk = mk_transition_block(
                *then_idx, all_segments, state_ref_param, poll_name, state_enum, span,
            );
            let else_blk = mk_transition_block(
                *else_idx, all_segments, state_ref_param, poll_name, state_enum, span,
            );
            let if_expr = Expr::new(
                ExprKind::If {
                    cond: Box::new(cond_e),
                    then_branch: then_blk,
                    else_branch: Some(Box::new(Expr::new(
                        ExprKind::Block(else_blk),
                        span,
                    ))),
                },
                span,
            );
            stmts.push(mk_expr_stmt(if_expr, span));
            stmts.push(mk_expr_stmt(
                Expr::new(ExprKind::Return(None), span),
                span,
            ));
        }
        SegTerm::Jump { target_idx } => {
            let blk = mk_transition_block(
                *target_idx, all_segments, state_ref_param, poll_name, state_enum, span,
            );
            for s in blk.stmts {
                stmts.push(s);
            }
        }
        SegTerm::JumpBind { target_idx, binding, value } => {
            let mut v = value.clone();
            rewrite_this_in_expr(&mut v);
            let blk = mk_transition_block_override(
                *target_idx,
                all_segments,
                state_ref_param,
                poll_name,
                state_enum,
                span,
                &[(*binding, v)],
            );
            for s in blk.stmts {
                stmts.push(s);
            }
        }
        SegTerm::MatchT { scrutinee, arms } => {
            let mut scrut_e = scrutinee.clone();
            rewrite_this_in_expr(&mut scrut_e);
            let match_arms: Vec<MatchArm> = arms
                .iter()
                .map(|a| {
                    let transition = mk_transition_block(
                        a.target_idx,
                        all_segments,
                        state_ref_param,
                        poll_name,
                        state_enum,
                        span,
                    );
                    MatchArm {
                        pattern: a.pattern.clone(),
                        body: Expr::new(ExprKind::Block(transition), span),
                        span,
                    }
                })
                .collect();
            let match_expr = Expr::new(
                ExprKind::Match {
                    scrutinee: Box::new(scrut_e),
                    arms: match_arms.into_boxed_slice(),
                },
                span,
            );
            stmts.push(mk_expr_stmt(match_expr, span));
            stmts.push(mk_expr_stmt(
                Expr::new(ExprKind::Return(None), span),
                span,
            ));
        }
        SegTerm::Settle { value } => {
            let mut v = value.clone();
            rewrite_this_in_expr(&mut v);
            stmts.push(mk_expr_stmt(
                mk_method_call(
                    mk_var(Symbol::intern("Promise"), span),
                    Symbol::intern("__settleResolve"),
                    vec![
                        mk_field(
                            mk_var(state_ref_param, span),
                            Symbol::intern("__async_promise"),
                            span,
                        ),
                        v,
                    ],
                    span,
                ),
                span,
            ));
            stmts.push(mk_expr_stmt(
                Expr::new(ExprKind::Return(None), span),
                span,
            ));
        }
    }
    Block { stmts, tail: None }
}

/// Rewrite every `this` to `Var(__this)` so the variant-destructured
/// local picks it up. Used in class-method lowering only; safe to
/// run for free async fns (their bodies don't contain `this`).
fn rewrite_this_in_stmt(s: &mut Stmt) {
    match &mut s.kind {
        StmtKind::Let { value, .. } => rewrite_this_in_expr(value),
        StmtKind::LetTuple { value, .. } | StmtKind::LetStruct { value, .. } => {
            rewrite_this_in_expr(value)
        }
        StmtKind::Expr(e) => rewrite_this_in_expr(e),
    }
}

fn rewrite_this_in_expr(e: &mut Expr) {
    match &mut e.kind {
        ExprKind::This => {
            e.kind = ExprKind::Var(Symbol::intern("__this"));
        }
        ExprKind::Block(b) => rewrite_this_in_block(b),
        ExprKind::If { cond, then_branch, else_branch } => {
            rewrite_this_in_expr(cond);
            rewrite_this_in_block(then_branch);
            if let Some(eb) = else_branch {
                rewrite_this_in_expr(eb);
            }
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            rewrite_this_in_expr(expr);
            rewrite_this_in_block(then_branch);
            if let Some(eb) = else_branch {
                rewrite_this_in_expr(eb);
            }
        }
        ExprKind::While { cond, body } => {
            rewrite_this_in_expr(cond);
            rewrite_this_in_block(body);
        }
        ExprKind::Loop { body } => rewrite_this_in_block(body),
        ExprKind::ForIn { iter, body, .. } => {
            rewrite_this_in_expr(iter);
            rewrite_this_in_block(body);
        }
        ExprKind::Match { scrutinee, arms } => {
            rewrite_this_in_expr(scrutinee);
            for a in arms.iter_mut() {
                rewrite_this_in_expr(&mut a.body);
            }
        }
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            rewrite_this_in_expr(lhs);
            rewrite_this_in_expr(rhs);
        }
        ExprKind::Unary { expr, .. }
        | ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. } => rewrite_this_in_expr(expr),
        ExprKind::Some(e) | ExprKind::Await(e) => rewrite_this_in_expr(e),
        ExprKind::Return(opt) | ExprKind::Break(opt) => {
            if let Some(e) = opt {
                rewrite_this_in_expr(e);
            }
        }
        ExprKind::Assign { value, .. } => rewrite_this_in_expr(value),
        ExprKind::AssignField { obj, value, .. } => {
            rewrite_this_in_expr(obj);
            rewrite_this_in_expr(value);
        }
        ExprKind::AssignIndex { obj, index, value } => {
            rewrite_this_in_expr(obj);
            rewrite_this_in_expr(index);
            rewrite_this_in_expr(value);
        }
        ExprKind::Call { args, .. }
        | ExprKind::SuperCall { args, .. }
        | ExprKind::New { args, .. } => {
            for a in args.iter_mut() {
                rewrite_this_in_expr(a);
            }
        }
        ExprKind::MethodCall { obj, args, .. } => {
            rewrite_this_in_expr(obj);
            for a in args.iter_mut() {
                rewrite_this_in_expr(a);
            }
        }
        ExprKind::Field { obj, .. } => rewrite_this_in_expr(obj),
        ExprKind::Index { obj, index } => {
            rewrite_this_in_expr(obj);
            rewrite_this_in_expr(index);
        }
        ExprKind::Tuple(es) | ExprKind::Array(es) => {
            for e in es.iter_mut() {
                rewrite_this_in_expr(e);
            }
        }
        ExprKind::FnExpr { body, .. } => rewrite_this_in_block(body),
        _ => {}
    }
}

fn rewrite_this_in_block(b: &mut Block) {
    for s in b.stmts.iter_mut() {
        rewrite_this_in_stmt(s);
    }
    if let Some(t) = b.tail.as_mut() {
        rewrite_this_in_expr(t);
    }
}

/// Generate the wrapper fn that allocates the StateRef + initial
/// variant + result promise, kicks `__<name>_poll(state_ref, 0)`,
/// and returns the result promise.
pub fn gen_wrapper_fn(
    orig: &FnDecl,
    state_ref_class: Symbol,
    state_enum: Symbol,
    poll_fn_name: Symbol,
    initial_fields: &[(Symbol, Type)],
    promise_ret: &Type,
    enclosing_class: Option<Symbol>,
    span: Span,
) -> FnDecl {
    let prom_local = Symbol::intern("__async_prom");
    let initial_local = Symbol::intern("__async_initial");
    let state_local = Symbol::intern("__async_state");

    let mut wrapper_stmts: Vec<Stmt> = Vec::new();
    wrapper_stmts.push(mk_let(
        prom_local,
        Some(promise_ret.clone()),
        mk_method_call(
            mk_var(Symbol::intern("Promise"), span),
            Symbol::intern("__pending"),
            vec![],
            span,
        ),
        span,
    ));
    // Initial variant ctor args: every field from V_0's field list.
    // For class methods, V_0 includes __this — pass `this` literal.
    let ctor_args: Vec<(Symbol, Expr)> = initial_fields
        .iter()
        .map(|(n, _)| {
            if enclosing_class.is_some() && n.as_str() == "__this" {
                (*n, Expr::new(ExprKind::This, span))
            } else {
                (*n, mk_var(*n, span))
            }
        })
        .collect();
    wrapper_stmts.push(mk_let(
        initial_local,
        None,
        mk_enum_ctor_struct(state_enum, Symbol::intern("S0"), ctor_args, span),
        span,
    ));
    let state_ref_type_args: Box<[Type]> = orig
        .type_params
        .iter()
        .map(|p| Type::Object(*p))
        .collect();
    wrapper_stmts.push(mk_let(
        state_local,
        None,
        Expr::new(
            ExprKind::New {
                class: state_ref_class,
                type_args: state_ref_type_args,
                args: Box::new([
                    mk_var(initial_local, span),
                    mk_var(prom_local, span),
                ]),
                init_method: None,
            },
            span,
        ),
        span,
    ));
    wrapper_stmts.push(mk_expr_stmt(
        mk_call(
            poll_fn_name,
            vec![mk_var(state_local, span), mk_int(0, span)],
            span,
        ),
        span,
    ));
    let wrapper_body = Block {
        stmts: wrapper_stmts,
        tail: Some(Box::new(mk_var(prom_local, span))),
    };
    FnDecl {
        attrs: orig.attrs.clone(),
        is_pub: orig.is_pub,
        name: orig.name,
        type_params: orig.type_params.clone(),
        params: orig.params.clone(),
        ret: Some(promise_ret.clone()),
        body: wrapper_body,
        span: orig.span,
        is_override: orig.is_override,
        is_async: false,
    }
}
