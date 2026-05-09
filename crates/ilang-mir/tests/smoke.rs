//! Smoke test: build a tiny `add(a, b) = a + b` MIR function by hand
//! and confirm it round-trips through the printer + validator.

use ilang_ast::Symbol;
use ilang_mir::{
    BinOp, FuncId, FuncParam, FunctionBuilder, FunctionKind, Inst, MirTy, Program, Terminator,
    validate_program,
};

#[test]
fn build_add_function() {
    let mut fb = FunctionBuilder::new(
        Symbol::intern("add__i64_i64"),
        Symbol::intern("add"),
        MirTy::I64,
        FunctionKind::Local,
    );

    let entry = fb.new_block();
    fb.switch_to(entry);
    let a = fb.add_block_param(entry, MirTy::I64);
    let b = fb.add_block_param(entry, MirTy::I64);
    let sum = fb.new_value(MirTy::I64);
    fb.push_inst(Inst::BinOp {
        dst: sum,
        op: BinOp::IAdd,
        lhs: a,
        rhs: b,
    });
    fb.set_terminator(Terminator::Return { value: Some(sum) });

    let func = fb.finish(
        vec![
            FuncParam {
                name: Symbol::intern("a"),
                ty: MirTy::I64,
                value: a,
            },
            FuncParam {
                name: Symbol::intern("b"),
                ty: MirTy::I64,
                value: b,
            },
        ]
        .into_boxed_slice(),
    );

    let mut p = Program::new(FuncId(0));
    p.functions.push(func);

    validate_program(&p).expect("valid MIR");

    let dump = ilang_mir::print_program(&p);
    assert!(dump.contains("fn add"), "missing add header:\n{dump}");
    assert!(dump.contains("(v0: i64, v1: i64) -> i64"), "missing sig:\n{dump}");
    assert!(dump.contains("iadd v0, v1"));
    assert!(dump.contains("return v2"));
}
