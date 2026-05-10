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
fn use_of_value_between_pair_is_a_barrier() {
    // Retain v ; LoadField v ; Release v — pair must NOT be removed
    // because the borrowed +1 is what guarantees v is still alive
    // during the load (in general — for this load instruction it
    // happens to be safe, but the peephole is conservative).
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
    assert_eq!(stats.pairs_removed, 0);
    assert_eq!(f.blocks[0].insts.len(), 3);
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
fn cross_block_pair_is_not_touched() {
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
    assert_eq!(stats.pairs_removed, 0);
    assert_eq!(f.blocks[0].insts.len(), 1);
    assert_eq!(f.blocks[1].insts.len(), 1);
}

#[test]
fn unused_funcid() {
    // Sanity — keep a use of FuncId so the import isn't dead. (We
    // don't otherwise touch FuncId in this file.)
    let _ = FuncId(0);
}
