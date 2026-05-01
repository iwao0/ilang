use ilang_eval::{run_program, Interpreter, RuntimeError, Value};
use ilang_lexer::tokenize;
use ilang_parser::parse;

fn run(src: &str) -> Result<Value, RuntimeError> {
    let toks = tokenize(src).unwrap();
    let prog = parse(&toks).unwrap();
    run_program(&prog)
}

#[test]
fn int_arithmetic() {
    assert_eq!(run("1 + 2 * 3").unwrap(), Value::Int(7));
    assert_eq!(run("(1 + 2) * 3").unwrap(), Value::Int(9));
    assert_eq!(run("7 / 2").unwrap(), Value::Int(3));
}

#[test]
fn float_promotion() {
    assert_eq!(run("7.0 / 2").unwrap(), Value::Float(3.5));
    assert_eq!(run("1 + 2.0").unwrap(), Value::Float(3.0));
}

#[test]
fn let_and_use() {
    assert_eq!(run("let x = 1 + 2; x * 3").unwrap(), Value::Int(9));
    assert_eq!(run("let x = 1; let y = 2; x + y").unwrap(), Value::Int(3));
}

#[test]
fn undefined_variable() {
    assert_eq!(
        run("x + 1"),
        Err(RuntimeError::UndefinedVariable("x".into()))
    );
}

#[test]
fn fn_call_basic() {
    let src = "fn add(a: i64, b: i64) -> i64 { a + b } add(2, 3)";
    assert_eq!(run(src).unwrap(), Value::Int(5));
}

#[test]
fn fn_recursive() {
    let src = "fn double(x: i64) -> i64 { x * 2 } fn quad(x: i64) -> i64 { double(double(x)) } quad(3)";
    assert_eq!(run(src).unwrap(), Value::Int(12));
}

#[test]
fn block_scoping() {
    let src = "let x = 1; { let x = 99; x }";
    assert_eq!(run(src).unwrap(), Value::Int(99));
    let src = "let x = 1; { let y = 2; }; x";
    assert_eq!(run(src).unwrap(), Value::Int(1));
}

#[test]
fn arity_mismatch() {
    let src = "fn id(x: i64) -> i64 { x } id(1, 2)";
    assert!(matches!(run(src), Err(RuntimeError::ArityMismatch { .. })));
}

#[test]
fn attribute_parses_but_does_not_enforce() {
    let src = "#[requires(net)] fn f(x: i64) -> i64 { x + 1 } f(41)";
    assert_eq!(run(src).unwrap(), Value::Int(42));
}

#[test]
fn division_by_zero_int() {
    assert_eq!(run("1 / 0"), Err(RuntimeError::DivisionByZero));
}

#[test]
fn overflow_detected() {
    let src = format!("{} + 1", i64::MAX);
    assert_eq!(run(&src), Err(RuntimeError::Overflow));
}

#[test]
fn if_expression() {
    assert_eq!(run("if true { 1 } else { 2 }").unwrap(), Value::Int(1));
    assert_eq!(run("if false { 1 } else { 2 }").unwrap(), Value::Int(2));
    assert_eq!(
        run("if 1 < 2 { 10 } else if 1 > 2 { 20 } else { 30 }").unwrap(),
        Value::Int(10)
    );
}

#[test]
fn while_loop_with_assignment() {
    let src = "let n = 0; let i = 1; while i <= 5 { n = n + i; i = i + 1; } n";
    assert_eq!(run(src).unwrap(), Value::Int(15));
}

#[test]
fn fn_with_if_and_while() {
    let src = "fn sum_to(n: i64) -> i64 { let s = 0; let i = 1; while i <= n { s = s + i; i = i + 1; } s } sum_to(10)";
    assert_eq!(run(src).unwrap(), Value::Int(55));
}

#[test]
fn short_circuit_and() {
    let src = "let x = 0; false && (1 / x == 0)";
    assert_eq!(run(src).unwrap(), Value::Bool(false));
}

#[test]
fn short_circuit_or() {
    let src = "let x = 0; true || (1 / x == 0)";
    assert_eq!(run(src).unwrap(), Value::Bool(true));
}

#[test]
fn comparison_ops() {
    assert_eq!(run("1 == 1").unwrap(), Value::Bool(true));
    assert_eq!(run("1.5 < 2").unwrap(), Value::Bool(true));
    assert_eq!(run("true != false").unwrap(), Value::Bool(true));
}

#[test]
fn assignment_persists_across_block() {
    let src = "let x = 1; { x = 99; let y = 5; } x";
    assert_eq!(run(src).unwrap(), Value::Int(99));
    let src = "let x = 1; { let x = 99; } x";
    assert_eq!(run(src).unwrap(), Value::Int(1));
}

#[test]
fn assign_to_undefined_fails() {
    assert_eq!(
        run("y = 1;"),
        Err(RuntimeError::UndefinedVariable("y".into()))
    );
}

#[test]
fn repl_persistence() {
    let mut interp = Interpreter::new();
    let toks = tokenize("let x = 10;").unwrap();
    let p = parse(&toks).unwrap();
    assert_eq!(interp.run(&p).unwrap(), Value::Unit);

    let toks = tokenize("x + 5").unwrap();
    let p = parse(&toks).unwrap();
    assert_eq!(interp.run(&p).unwrap(), Value::Int(15));
}
