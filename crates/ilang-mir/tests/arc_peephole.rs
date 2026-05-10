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

#[test]
fn unused_funcid() {
    // Sanity — keep a use of FuncId so the import isn't dead. (We
    // don't otherwise touch FuncId in this file.)
    let _ = FuncId(0);
}
