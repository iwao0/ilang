//! Unit tests for the ARC peephole pass: cancels Retain/Release pairs
//! within a single basic block when nothing observable happens between
//! them.

use ilang_ast::Symbol;
use ilang_mir::{
    BinOp, FuncId, FuncParam, FuncRef, FunctionBuilder, FunctionKind, Inst, MirTy, Terminator,
};

fn build_one_block<F>(setup: F) -> ilang_mir::Function
where
    F: FnOnce(&mut FunctionBuilder, ilang_mir::ValueId),
{
    let mut fb = FunctionBuilder::new(
        Symbol::intern("t"),
        Symbol::intern("t"),
        MirTy::I64,
        FunctionKind::Local,
    );
    let entry = fb.new_block();
    fb.switch_to(entry);
    let v = fb.add_block_param(entry, MirTy::I64);
    setup(&mut fb, v);
    fb.set_terminator(Terminator::Return { value: Some(v) });
    fb.finish(
        vec![FuncParam {
            name: Symbol::intern("v"),
            ty: MirTy::I64,
            value: v,
        }]
        .into_boxed_slice(),
    )
}

#[test]
fn adjacent_pair_is_removed() {
    let mut f = build_one_block(|fb, v| {
        fb.push_inst(Inst::Retain { value: v });
        fb.push_inst(Inst::Release { value: v });
    });
    let stats = ilang_mir::passes::arc_peephole::run_function(&mut f);
    assert_eq!(stats.pairs_removed, 1);
    assert!(f.blocks[0].insts.is_empty());
}

#[test]
fn pure_inst_between_pair_is_crossed() {
    let mut f = build_one_block(|fb, v| {
        let one = fb.new_value(MirTy::I64);
        let two = fb.new_value(MirTy::I64);
        let _sum = fb.new_value(MirTy::I64);
        fb.push_inst(Inst::Retain { value: v });
        fb.push_inst(Inst::Const {
            dst: one,
            value: ilang_mir::MirConst::Int(1),
        });
        fb.push_inst(Inst::Const {
            dst: two,
            value: ilang_mir::MirConst::Int(2),
        });
        fb.push_inst(Inst::BinOp {
            dst: _sum,
            op: BinOp::IAdd,
            lhs: one,
            rhs: two,
        });
        fb.push_inst(Inst::Release { value: v });
    });
    let stats = ilang_mir::passes::arc_peephole::run_function(&mut f);
    assert_eq!(stats.pairs_removed, 1);
    // Retain/Release gone, the 3 pure insts remain.
    assert_eq!(f.blocks[0].insts.len(), 3);
    for inst in &f.blocks[0].insts {
        assert!(
            !matches!(inst, Inst::Retain { .. } | Inst::Release { .. }),
            "ARC inst should be gone, found {inst:?}"
        );
    }
}

#[test]
fn call_between_pair_is_a_barrier() {
    let mut f = build_one_block(|fb, v| {
        fb.push_inst(Inst::Retain { value: v });
        fb.push_inst(Inst::Call {
            dst: None,
            callee: FuncRef::Builtin(Symbol::intern("noop")),
            args: Box::new([]),
        });
        fb.push_inst(Inst::Release { value: v });
    });
    let stats = ilang_mir::passes::arc_peephole::run_function(&mut f);
    assert_eq!(stats.pairs_removed, 0);
    assert_eq!(f.blocks[0].insts.len(), 3);
}

#[test]
fn pure_use_of_value_between_pair_is_crossed() {
    // Retain v ; LoadField v ; Release v — the pair *can* be removed.
    // LoadField is a pure read, so it doesn't observe v's refcount.
    // The load result has its own refcount lifecycle independent of
    // the parent, so dropping v back to its original count is safe.
    let mut f = build_one_block(|fb, v| {
        let dst = fb.new_value(MirTy::I64);
        fb.push_inst(Inst::Retain { value: v });
        fb.push_inst(Inst::LoadField {
            dst,
            obj: v,
            field: ilang_mir::FieldId(0),
        });
        fb.push_inst(Inst::Release { value: v });
    });
    let stats = ilang_mir::passes::arc_peephole::run_function(&mut f);
    assert_eq!(stats.pairs_removed, 1);
    // Only the LoadField remains.
    assert_eq!(f.blocks[0].insts.len(), 1);
    assert!(matches!(f.blocks[0].insts[0], Inst::LoadField { .. }));
}

#[test]
fn local_aliased_pair_is_removed() {
    // Mirrors the lowered pattern that the M2-α/β identity-based
    // pass missed entirely:
    //   def_local %0 = v       (heap alloc bound to slot)
    //   v_a = use_local %0     (alias of v through the slot)
    //   retain v_a
    //   def_local %1 = v_a
    //   v_b = use_local %1     (alias of v_a through %1)
    //   release v_b            <- ValueId differs from retain target
    //
    // With local-aware equivalence the retain/release pair collapses.
    let mut f = build_one_block(|fb, v| {
        let l0 = fb.new_local(MirTy::I64);
        let l1 = fb.new_local(MirTy::I64);
        let v_a = fb.new_value(MirTy::I64);
        let v_b = fb.new_value(MirTy::I64);
        fb.push_inst(Inst::DefLocal { local: l0, value: v });
        fb.push_inst(Inst::UseLocal { dst: v_a, local: l0 });
        fb.push_inst(Inst::Retain { value: v_a });
        fb.push_inst(Inst::DefLocal { local: l1, value: v_a });
        fb.push_inst(Inst::UseLocal { dst: v_b, local: l1 });
        fb.push_inst(Inst::Release { value: v_b });
    });
    let stats = ilang_mir::passes::arc_peephole::run_function(&mut f);
    assert_eq!(stats.pairs_removed, 1);
    // Retain and Release gone; the four DefLocal/UseLocal stay.
    assert_eq!(f.blocks[0].insts.len(), 4);
    for inst in &f.blocks[0].insts {
        assert!(
            !matches!(inst, Inst::Retain { .. } | Inst::Release { .. }),
            "ARC inst should be gone, found {inst:?}"
        );
    }
}

#[test]
fn local_rebind_breaks_equivalence() {
    // After `def_local %0 = v_other`, reads from %0 give a value
    // unrelated to the originally-retained one. The pair must NOT
    // be removed.
    let mut f = build_one_block(|fb, v| {
        let l0 = fb.new_local(MirTy::I64);
        let v_a = fb.new_value(MirTy::I64);
        let v_other = fb.new_value(MirTy::I64);
        let v_b = fb.new_value(MirTy::I64);
        fb.push_inst(Inst::DefLocal { local: l0, value: v });
        fb.push_inst(Inst::UseLocal { dst: v_a, local: l0 });
        fb.push_inst(Inst::Retain { value: v_a });
        fb.push_inst(Inst::Const {
            dst: v_other,
            value: ilang_mir::MirConst::Int(42),
        });
        fb.push_inst(Inst::DefLocal {
            local: l0,
            value: v_other,
        });
        fb.push_inst(Inst::UseLocal { dst: v_b, local: l0 });
        fb.push_inst(Inst::Release { value: v_b });
    });
    let stats = ilang_mir::passes::arc_peephole::run_function(&mut f);
    assert_eq!(stats.pairs_removed, 0);
}

#[test]
fn unrelated_retain_release_is_crossed() {
    // Retain v ; Retain w ; Release w ; Release v — the inner pair
    // matches first (it's also a candidate), then the outer pair
    // collapses too. Two pairs removed, all four insts gone.
    let mut f = build_one_block(|fb, v| {
        let w = fb.add_block_param(fb.current_block(), MirTy::I64);
        fb.push_inst(Inst::Retain { value: v });
        fb.push_inst(Inst::Retain { value: w });
        fb.push_inst(Inst::Release { value: w });
        fb.push_inst(Inst::Release { value: v });
    });
    let stats = ilang_mir::passes::arc_peephole::run_function(&mut f);
    assert_eq!(stats.pairs_removed, 2);
    assert!(f.blocks[0].insts.is_empty());
}

#[test]
fn store_between_pair_is_a_barrier() {
    // ArrayStore can alias v's heap state; treat as barrier even
    // though the operands themselves don't contain v.
    let mut f = build_one_block(|fb, v| {
        let arr = fb.add_block_param(fb.current_block(), MirTy::I64);
        let idx = fb.add_block_param(fb.current_block(), MirTy::I64);
        let val = fb.add_block_param(fb.current_block(), MirTy::I64);
        fb.push_inst(Inst::Retain { value: v });
        fb.push_inst(Inst::ArrayStore {
            arr,
            idx,
            value: val,
        });
        fb.push_inst(Inst::Release { value: v });
    });
    let stats = ilang_mir::passes::arc_peephole::run_function(&mut f);
    assert_eq!(stats.pairs_removed, 0);
    assert_eq!(f.blocks[0].insts.len(), 3);
}

/// Extended-BB peephole: Retain at end of B0, Release at start of B1
/// (B1's only predecessor is B0). Both are removed.
#[test]
fn cross_block_pair_is_removed_when_b2_single_pred() {
    let mut fb = FunctionBuilder::new(
        Symbol::intern("t"),
        Symbol::intern("t"),
        MirTy::I64,
        FunctionKind::Local,
    );
    let b0 = fb.new_block();
    let b1 = fb.new_block();
    fb.switch_to(b0);
    let v = fb.add_block_param(b0, MirTy::I64);
    fb.push_inst(Inst::Retain { value: v });
    fb.set_terminator(Terminator::Br {
        dst: b1,
        args: Box::new([v]),
    });
    fb.switch_to(b1);
    let v1 = fb.add_block_param(b1, MirTy::I64);
    fb.push_inst(Inst::Release { value: v1 });
    fb.set_terminator(Terminator::Return { value: Some(v1) });
    let mut f = fb.finish(
        vec![FuncParam {
            name: Symbol::intern("v"),
            ty: MirTy::I64,
            value: v,
        }]
        .into_boxed_slice(),
    );
    let stats = ilang_mir::passes::arc_peephole::run_function(&mut f);
    assert_eq!(stats.pairs_removed, 1);
    assert!(f.blocks[0].insts.is_empty());
    assert!(f.blocks[1].insts.is_empty());
}

/// Extended-BB does NOT fire when the successor has multiple
/// predecessors — we'd need a dominator argument the simple pass
/// isn't doing.
#[test]
fn cross_block_pair_kept_when_b2_has_multiple_preds() {
    let mut fb = FunctionBuilder::new(
        Symbol::intern("t"),
        Symbol::intern("t"),
        MirTy::I64,
        FunctionKind::Local,
    );
    let b0 = fb.new_block();
    let b_alt = fb.new_block();
    let b1 = fb.new_block();
    // B0 → B1
    fb.switch_to(b0);
    let v = fb.add_block_param(b0, MirTy::I64);
    let cond = fb.add_block_param(b0, MirTy::Bool);
    fb.push_inst(Inst::Retain { value: v });
    fb.set_terminator(Terminator::CondBr {
        cond,
        then_block: b1,
        then_args: Box::new([v]),
        else_block: b_alt,
        else_args: Box::new([]),
    });
    // B_alt → B1 (second pred, makes B1 multi-pred)
    fb.switch_to(b_alt);
    fb.set_terminator(Terminator::Br {
        dst: b1,
        args: Box::new([v]),
    });
    // B1
    fb.switch_to(b1);
    let v1 = fb.add_block_param(b1, MirTy::I64);
    fb.push_inst(Inst::Release { value: v1 });
    fb.set_terminator(Terminator::Return { value: Some(v1) });
    let mut f = fb.finish(
        vec![
            FuncParam {
                name: Symbol::intern("v"),
                ty: MirTy::I64,
                value: v,
            },
            FuncParam {
                name: Symbol::intern("c"),
                ty: MirTy::Bool,
                value: cond,
            },
        ]
        .into_boxed_slice(),
    );
    let stats = ilang_mir::passes::arc_peephole::run_function(&mut f);
    assert_eq!(stats.pairs_removed, 0);
    assert_eq!(f.blocks[0].insts.len(), 1);
    assert_eq!(f.blocks[2].insts.len(), 1);
}

/// Extended-BB fires only when v is actually forwarded as a
/// block-arg. If v isn't passed, the candidate is dropped.
#[test]
fn cross_block_pair_kept_when_v_not_in_args() {
    let mut fb = FunctionBuilder::new(
        Symbol::intern("t"),
        Symbol::intern("t"),
        MirTy::I64,
        FunctionKind::Local,
    );
    let b0 = fb.new_block();
    let b1 = fb.new_block();
    fb.switch_to(b0);
    let v = fb.add_block_param(b0, MirTy::I64);
    let other = fb.add_block_param(b0, MirTy::I64);
    fb.push_inst(Inst::Retain { value: v });
    // forward only `other`, not `v`
    fb.set_terminator(Terminator::Br {
        dst: b1,
        args: Box::new([other]),
    });
    fb.switch_to(b1);
    let p = fb.add_block_param(b1, MirTy::I64);
    fb.push_inst(Inst::Release { value: p });
    fb.set_terminator(Terminator::Return { value: Some(p) });
    let mut f = fb.finish(
        vec![
            FuncParam {
                name: Symbol::intern("v"),
                ty: MirTy::I64,
                value: v,
            },
            FuncParam {
                name: Symbol::intern("o"),
                ty: MirTy::I64,
                value: other,
            },
        ]
        .into_boxed_slice(),
    );
    let stats = ilang_mir::passes::arc_peephole::run_function(&mut f);
    assert_eq!(stats.pairs_removed, 0);
    assert_eq!(f.blocks[0].insts.len(), 1);
    assert_eq!(f.blocks[1].insts.len(), 1);
}

/// A Call between Retain (B0 tail) and the terminator is a barrier:
/// the Call could observe v's refcount, so the Retain stays.
#[test]
fn cross_block_pair_kept_when_b1_tail_has_barrier() {
    let mut fb = FunctionBuilder::new(
        Symbol::intern("t"),
        Symbol::intern("t"),
        MirTy::I64,
        FunctionKind::Local,
    );
    let b0 = fb.new_block();
    let b1 = fb.new_block();
    fb.switch_to(b0);
    let v = fb.add_block_param(b0, MirTy::I64);
    fb.push_inst(Inst::Retain { value: v });
    fb.push_inst(Inst::Call {
        dst: None,
        callee: FuncRef::Builtin(Symbol::intern("noop")),
        args: Box::new([]),
    });
    fb.set_terminator(Terminator::Br {
        dst: b1,
        args: Box::new([v]),
    });
    fb.switch_to(b1);
    let p = fb.add_block_param(b1, MirTy::I64);
    fb.push_inst(Inst::Release { value: p });
    fb.set_terminator(Terminator::Return { value: Some(p) });
    let mut f = fb.finish(
        vec![FuncParam {
            name: Symbol::intern("v"),
            ty: MirTy::I64,
            value: v,
        }]
        .into_boxed_slice(),
    );
    let stats = ilang_mir::passes::arc_peephole::run_function(&mut f);
    assert_eq!(stats.pairs_removed, 0);
}

/// Extended-BB chains through three blocks (B0 → B1 → B2), each
/// with a single predecessor for the next. The chain walker follows
/// the rename through the empty intermediate block and cancels the
/// pair across two hops.
#[test]
fn cross_block_chain_is_peeled() {
    let mut fb = FunctionBuilder::new(
        Symbol::intern("t"),
        Symbol::intern("t"),
        MirTy::I64,
        FunctionKind::Local,
    );
    let b0 = fb.new_block();
    let b1 = fb.new_block();
    let b2 = fb.new_block();
    fb.switch_to(b0);
    let v = fb.add_block_param(b0, MirTy::I64);
    fb.push_inst(Inst::Retain { value: v });
    fb.set_terminator(Terminator::Br {
        dst: b1,
        args: Box::new([v]),
    });
    fb.switch_to(b1);
    let v1 = fb.add_block_param(b1, MirTy::I64);
    fb.set_terminator(Terminator::Br {
        dst: b2,
        args: Box::new([v1]),
    });
    fb.switch_to(b2);
    let v2 = fb.add_block_param(b2, MirTy::I64);
    fb.push_inst(Inst::Release { value: v2 });
    fb.set_terminator(Terminator::Return { value: Some(v2) });
    let mut f = fb.finish(
        vec![FuncParam {
            name: Symbol::intern("v"),
            ty: MirTy::I64,
            value: v,
        }]
        .into_boxed_slice(),
    );
    let stats = ilang_mir::passes::arc_peephole::run_function(&mut f);
    assert_eq!(stats.pairs_removed, 1);
    assert!(f.blocks[0].insts.is_empty());
    assert!(f.blocks[1].insts.is_empty());
    assert!(f.blocks[2].insts.is_empty());
}

/// Chain walking gives up if any intermediate block contains a
/// barrier (here, a Call) — even if the value being chained isn't
/// touched by the barrier.
#[test]
fn cross_block_chain_kept_when_intermediate_has_barrier() {
    let mut fb = FunctionBuilder::new(
        Symbol::intern("t"),
        Symbol::intern("t"),
        MirTy::I64,
        FunctionKind::Local,
    );
    let b0 = fb.new_block();
    let b1 = fb.new_block();
    let b2 = fb.new_block();
    fb.switch_to(b0);
    let v = fb.add_block_param(b0, MirTy::I64);
    fb.push_inst(Inst::Retain { value: v });
    fb.set_terminator(Terminator::Br {
        dst: b1,
        args: Box::new([v]),
    });
    fb.switch_to(b1);
    let v1 = fb.add_block_param(b1, MirTy::I64);
    fb.push_inst(Inst::Call {
        dst: None,
        callee: FuncRef::Builtin(Symbol::intern("noop")),
        args: Box::new([]),
    });
    fb.set_terminator(Terminator::Br {
        dst: b2,
        args: Box::new([v1]),
    });
    fb.switch_to(b2);
    let v2 = fb.add_block_param(b2, MirTy::I64);
    fb.push_inst(Inst::Release { value: v2 });
    fb.set_terminator(Terminator::Return { value: Some(v2) });
    let mut f = fb.finish(
        vec![FuncParam {
            name: Symbol::intern("v"),
            ty: MirTy::I64,
            value: v,
        }]
        .into_boxed_slice(),
    );
    let stats = ilang_mir::passes::arc_peephole::run_function(&mut f);
    assert_eq!(stats.pairs_removed, 0);
}

/// Chain walking gives up if any intermediate block stops
/// forwarding `v` as a block-arg.
#[test]
fn cross_block_chain_kept_when_v_dropped_midchain() {
    let mut fb = FunctionBuilder::new(
        Symbol::intern("t"),
        Symbol::intern("t"),
        MirTy::I64,
        FunctionKind::Local,
    );
    let b0 = fb.new_block();
    let b1 = fb.new_block();
    let b2 = fb.new_block();
    fb.switch_to(b0);
    let v = fb.add_block_param(b0, MirTy::I64);
    let other = fb.add_block_param(b0, MirTy::I64);
    fb.push_inst(Inst::Retain { value: v });
    fb.set_terminator(Terminator::Br {
        dst: b1,
        args: Box::new([v, other]),
    });
    fb.switch_to(b1);
    let _v1 = fb.add_block_param(b1, MirTy::I64);
    let other1 = fb.add_block_param(b1, MirTy::I64);
    // forward only `other` past this block — `v` doesn't survive.
    fb.set_terminator(Terminator::Br {
        dst: b2,
        args: Box::new([other1]),
    });
    fb.switch_to(b2);
    let v2 = fb.add_block_param(b2, MirTy::I64);
    fb.push_inst(Inst::Release { value: v2 });
    fb.set_terminator(Terminator::Return { value: Some(v2) });
    let mut f = fb.finish(
        vec![
            FuncParam {
                name: Symbol::intern("v"),
                ty: MirTy::I64,
                value: v,
            },
            FuncParam {
                name: Symbol::intern("o"),
                ty: MirTy::I64,
                value: other,
            },
        ]
        .into_boxed_slice(),
    );
    let stats = ilang_mir::passes::arc_peephole::run_function(&mut f);
    assert_eq!(stats.pairs_removed, 0);
}

#[test]
fn unused_funcid() {
    // Sanity — keep a use of FuncId so the import isn't dead. (We
    // don't otherwise touch FuncId in this file.)
    let _ = FuncId(0);
}
