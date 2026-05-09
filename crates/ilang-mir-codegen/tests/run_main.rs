//! End-to-end: build a tiny MIR program by hand, lower to clif, JIT,
//! and verify the entry fn returns the expected value.

use ilang_ast::Symbol;
use ilang_mir::{
    BinOp, FuncId, FuncParam, FuncRef, FunctionBuilder, FunctionKind, Inst, MirTy, Program,
    SwitchCase, Terminator,
};
use ilang_mir_codegen::{compile_program, compile_with_builtins, run_main, BuiltinDecl};

fn build_const_program(value: i64) -> Program {
    let main = Symbol::intern("__main");
    let mut fb = FunctionBuilder::new(main, main, MirTy::I64, FunctionKind::Local);
    let entry = fb.new_block();
    fb.switch_to(entry);
    let v = fb.new_value(MirTy::I64);
    fb.push_inst(Inst::Const {
        dst: v,
        value: ilang_mir::MirConst::Int(value),
    });
    fb.set_terminator(Terminator::Return { value: Some(v) });
    let mut p = Program::new(FuncId(0));
    p.functions.push(fb.finish(Box::new([])));
    p
}

#[test]
fn run_const_returns_value() {
    let p = build_const_program(42);
    let c = compile_program(&p).expect("compile");
    let r = run_main(&c);
    assert_eq!(r, 42);
}

#[test]
fn run_arithmetic() {
    let main = Symbol::intern("__main");
    let mut fb = FunctionBuilder::new(main, main, MirTy::I64, FunctionKind::Local);
    let entry = fb.new_block();
    fb.switch_to(entry);
    let a = fb.new_value(MirTy::I64);
    fb.push_inst(Inst::Const {
        dst: a,
        value: ilang_mir::MirConst::Int(20),
    });
    let b = fb.new_value(MirTy::I64);
    fb.push_inst(Inst::Const {
        dst: b,
        value: ilang_mir::MirConst::Int(22),
    });
    let sum = fb.new_value(MirTy::I64);
    fb.push_inst(Inst::BinOp {
        dst: sum,
        op: BinOp::IAdd,
        lhs: a,
        rhs: b,
    });
    fb.set_terminator(Terminator::Return { value: Some(sum) });

    let mut p = Program::new(FuncId(0));
    p.functions.push(fb.finish(Box::new([])));

    let c = compile_program(&p).expect("compile");
    let r = run_main(&c);
    assert_eq!(r, 42);
}

#[test]
fn run_call_between_fns() {
    // Build:
    //   fn add(a: i64, b: i64): i64 { a + b }
    //   fn main(): i64 { add(20, 22) }
    let add_name = Symbol::intern("add");
    let mut fb_add = FunctionBuilder::new(add_name, add_name, MirTy::I64, FunctionKind::Local);
    let entry = fb_add.new_block();
    fb_add.switch_to(entry);
    let a = fb_add.add_block_param(entry, MirTy::I64);
    let b = fb_add.add_block_param(entry, MirTy::I64);
    let s = fb_add.new_value(MirTy::I64);
    fb_add.push_inst(Inst::BinOp { dst: s, op: BinOp::IAdd, lhs: a, rhs: b });
    fb_add.set_terminator(Terminator::Return { value: Some(s) });
    let add_fn = fb_add.finish(
        vec![
            FuncParam { name: Symbol::intern("a"), ty: MirTy::I64, value: a },
            FuncParam { name: Symbol::intern("b"), ty: MirTy::I64, value: b },
        ]
        .into_boxed_slice(),
    );

    let main = Symbol::intern("__main");
    let mut fb_main = FunctionBuilder::new(main, main, MirTy::I64, FunctionKind::Local);
    let me = fb_main.new_block();
    fb_main.switch_to(me);
    let twenty = fb_main.new_value(MirTy::I64);
    fb_main.push_inst(Inst::Const { dst: twenty, value: ilang_mir::MirConst::Int(20) });
    let twenty_two = fb_main.new_value(MirTy::I64);
    fb_main.push_inst(Inst::Const { dst: twenty_two, value: ilang_mir::MirConst::Int(22) });
    let r = fb_main.new_value(MirTy::I64);
    fb_main.push_inst(Inst::Call {
        dst: Some(r),
        callee: ilang_mir::FuncRef::Local(FuncId(0)),
        args: Box::new([twenty, twenty_two]),
    });
    fb_main.set_terminator(Terminator::Return { value: Some(r) });
    let main_fn = fb_main.finish(Box::new([]));

    let mut p = Program::new(FuncId(1));
    p.functions.push(add_fn);
    p.functions.push(main_fn);

    let c = compile_program(&p).expect("compile");
    let result = run_main(&c);
    assert_eq!(result, 42);
}

#[test]
fn run_switch() {
    // switch on i64: match 2 { 1 -> 100; 2 -> 200; default -> 999 }
    let main = Symbol::intern("__main");
    let mut fb = FunctionBuilder::new(main, main, MirTy::I64, FunctionKind::Local);
    let entry = fb.new_block();
    let case1 = fb.new_block();
    let case2 = fb.new_block();
    let default_b = fb.new_block();
    let cont = fb.new_block();
    let result = fb.add_block_param(cont, MirTy::I64);

    fb.switch_to(entry);
    let scrut = fb.new_value(MirTy::I64);
    fb.push_inst(Inst::Const { dst: scrut, value: ilang_mir::MirConst::Int(2) });
    fb.set_terminator(Terminator::Switch {
        scrutinee: scrut,
        cases: Box::new([
            SwitchCase { value: 1, dst: case1, args: Box::new([]) },
            SwitchCase { value: 2, dst: case2, args: Box::new([]) },
        ]),
        default: default_b,
        default_args: Box::new([]),
    });

    fb.switch_to(case1);
    let v1 = fb.new_value(MirTy::I64);
    fb.push_inst(Inst::Const { dst: v1, value: ilang_mir::MirConst::Int(100) });
    fb.set_terminator(Terminator::Br { dst: cont, args: Box::new([v1]) });

    fb.switch_to(case2);
    let v2 = fb.new_value(MirTy::I64);
    fb.push_inst(Inst::Const { dst: v2, value: ilang_mir::MirConst::Int(200) });
    fb.set_terminator(Terminator::Br { dst: cont, args: Box::new([v2]) });

    fb.switch_to(default_b);
    let v3 = fb.new_value(MirTy::I64);
    fb.push_inst(Inst::Const { dst: v3, value: ilang_mir::MirConst::Int(999) });
    fb.set_terminator(Terminator::Br { dst: cont, args: Box::new([v3]) });

    fb.switch_to(cont);
    fb.set_terminator(Terminator::Return { value: Some(result) });

    let mut p = Program::new(FuncId(0));
    p.functions.push(fb.finish(Box::new([])));

    let c = compile_program(&p).expect("compile");
    let r = run_main(&c);
    assert_eq!(r, 200);
}

extern "C" fn host_double(n: i64) -> i64 {
    n * 2
}

extern "C" fn host_strlen(p: i64) -> i64 {
    if p == 0 {
        return 0;
    }
    let mut n: i64 = 0;
    unsafe {
        let mut q = p as *const u8;
        while *q != 0 {
            n += 1;
            q = q.add(1);
        }
    }
    n
}

#[test]
fn run_builtin_call() {
    // Build:
    //   fn main(): i64 { __double_builtin(21) }
    let main = Symbol::intern("__main");
    let mut fb = FunctionBuilder::new(main, main, MirTy::I64, FunctionKind::Local);
    let entry = fb.new_block();
    fb.switch_to(entry);
    let arg = fb.new_value(MirTy::I64);
    fb.push_inst(Inst::Const { dst: arg, value: ilang_mir::MirConst::Int(21) });
    let r = fb.new_value(MirTy::I64);
    fb.push_inst(Inst::Call {
        dst: Some(r),
        callee: FuncRef::Builtin(Symbol::intern("host_double")),
        args: Box::new([arg]),
    });
    fb.set_terminator(Terminator::Return { value: Some(r) });
    let main_fn = fb.finish(Box::new([]));

    let mut p = Program::new(FuncId(0));
    p.functions.push(main_fn);

    let builtins = vec![BuiltinDecl {
        name: "host_double",
        params: vec![MirTy::I64],
        ret: MirTy::I64,
        ptr: host_double as *const u8,
    }];
    let c = compile_with_builtins(&p, &builtins).expect("compile");
    let result = run_main(&c);
    assert_eq!(result, 42);
}

#[test]
fn run_string_const_via_builtin() {
    // Build a tiny MIR program that creates a string const and
    // passes it to a host-registered C-style strlen-like fn.
    let main = Symbol::intern("__main");
    let mut fb = FunctionBuilder::new(main, main, MirTy::I64, FunctionKind::Local);
    let entry = fb.new_block();
    fb.switch_to(entry);
    let s = fb.new_value(MirTy::Str);
    fb.push_inst(Inst::Const {
        dst: s,
        value: ilang_mir::MirConst::Str(Symbol::intern("hello world")),
    });
    let r = fb.new_value(MirTy::I64);
    fb.push_inst(Inst::Call {
        dst: Some(r),
        callee: FuncRef::Builtin(Symbol::intern("host_strlen")),
        args: Box::new([s]),
    });
    fb.set_terminator(Terminator::Return { value: Some(r) });
    let main_fn = fb.finish(Box::new([]));

    let mut p = Program::new(FuncId(0));
    p.functions.push(main_fn);

    let builtins = vec![BuiltinDecl {
        name: "host_strlen",
        params: vec![MirTy::Str],
        ret: MirTy::I64,
        ptr: host_strlen as *const u8,
    }];
    let c = compile_with_builtins(&p, &builtins).expect("compile");
    let result = run_main(&c);
    assert_eq!(result, 11);
}

#[test]
fn run_branching() {
    // if 1 > 0 { 100 } else { 200 }  → 100
    let main = Symbol::intern("__main");
    let mut fb = FunctionBuilder::new(main, main, MirTy::I64, FunctionKind::Local);
    let entry = fb.new_block();
    let then_b = fb.new_block();
    let else_b = fb.new_block();
    let cont = fb.new_block();
    let result = fb.add_block_param(cont, MirTy::I64);

    fb.switch_to(entry);
    let one = fb.new_value(MirTy::I64);
    fb.push_inst(Inst::Const { dst: one, value: ilang_mir::MirConst::Int(1) });
    let zero = fb.new_value(MirTy::I64);
    fb.push_inst(Inst::Const { dst: zero, value: ilang_mir::MirConst::Int(0) });
    let cmp = fb.new_value(MirTy::Bool);
    fb.push_inst(Inst::BinOp {
        dst: cmp,
        op: BinOp::IGtS,
        lhs: one,
        rhs: zero,
    });
    fb.set_terminator(Terminator::CondBr {
        cond: cmp,
        then_block: then_b,
        then_args: Box::new([]),
        else_block: else_b,
        else_args: Box::new([]),
    });

    fb.switch_to(then_b);
    let a = fb.new_value(MirTy::I64);
    fb.push_inst(Inst::Const { dst: a, value: ilang_mir::MirConst::Int(100) });
    fb.set_terminator(Terminator::Br { dst: cont, args: Box::new([a]) });

    fb.switch_to(else_b);
    let b = fb.new_value(MirTy::I64);
    fb.push_inst(Inst::Const { dst: b, value: ilang_mir::MirConst::Int(200) });
    fb.set_terminator(Terminator::Br { dst: cont, args: Box::new([b]) });

    fb.switch_to(cont);
    fb.set_terminator(Terminator::Return { value: Some(result) });

    let mut p = Program::new(FuncId(0));
    p.functions.push(fb.finish(Box::new([])));
    let c = compile_program(&p).expect("compile");
    let r = run_main(&c);
    assert_eq!(r, 100);
}
