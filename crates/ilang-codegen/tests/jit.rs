use ilang_codegen::{jit_run, JitValue};
use ilang_lexer::tokenize;
use ilang_parser::parse;
use ilang_types::TypeChecker;

fn jit(src: &str) -> JitValue {
    let toks = tokenize(src).expect("lex");
    let prog = parse(&toks).expect("parse");
    TypeChecker::new().check(&prog).expect("typecheck");
    jit_run(&prog).expect("jit")
}

#[test]
fn arithmetic() {
    assert_eq!(jit("1 + 2 * 3"), JitValue::Int(7));
    assert_eq!(jit("(10 - 3) * 4"), JitValue::Int(28));
    assert_eq!(jit("100 / 7"), JitValue::Int(14));
    assert_eq!(jit("100 % 7"), JitValue::Int(2));
}

#[test]
fn bitwise() {
    assert_eq!(jit("12 & 10"), JitValue::Int(8));
    assert_eq!(jit("12 | 10"), JitValue::Int(14));
    assert_eq!(jit("12 ^ 10"), JitValue::Int(6));
    assert_eq!(jit("1 << 4"), JitValue::Int(16));
    assert_eq!(jit("256 >> 2"), JitValue::Int(64));
}

#[test]
fn comparison_and_logical() {
    assert_eq!(jit("1 < 2"), JitValue::Bool(true));
    assert_eq!(jit("1 == 1"), JitValue::Bool(true));
    assert_eq!(jit("true && false"), JitValue::Bool(false));
    assert_eq!(jit("true || false"), JitValue::Bool(true));
    assert_eq!(jit("!false"), JitValue::Bool(true));
}

#[test]
fn let_and_assign() {
    assert_eq!(
        jit("let x = 10; x = x + 5; x"),
        JitValue::Int(15)
    );
}

#[test]
fn if_expression() {
    assert_eq!(
        jit("let n = 7; if n > 5 { n * 10 } else { n * 100 }"),
        JitValue::Int(70)
    );
}

#[test]
fn while_loop() {
    let src = "let n = 0; let i = 1; while i <= 5 { n = n + i; i = i + 1; } n";
    assert_eq!(jit(src), JitValue::Int(15));
}

#[test]
fn loop_break_continue() {
    let src = r#"
        let i = 0
        let sum = 0
        loop {
            i = i + 1
            if i > 10 { break }
            if i % 2 == 0 { continue }
            sum = sum + i
        }
        sum
    "#;
    assert_eq!(jit(src), JitValue::Int(25)); // 1+3+5+7+9
}

#[test]
fn function_calls() {
    let src = "fn add(a: i64, b: i64): i64 { a + b } add(2, 3)";
    assert_eq!(jit(src), JitValue::Int(5));
}

#[test]
fn recursive_fib() {
    let src = "fn fib(n: i64): i64 { if n < 2 { n } else { fib(n - 1) + fib(n - 2) } } fib(20)";
    assert_eq!(jit(src), JitValue::Int(6765));
}

#[test]
fn bool_returning_function() {
    let src = "fn even(n: i64): bool { n % 2 == 0 } even(42)";
    assert_eq!(jit(src), JitValue::Bool(true));
}

#[test]
fn unsupported_string_errors() {
    // Strings aren't in the JIT subset yet; the type checker passes but
    // the codegen rejects with a clear error.
    let toks = tokenize(r#"let s = "hi""#).unwrap();
    let prog = parse(&toks).unwrap();
    TypeChecker::new().check(&prog).unwrap();
    assert!(jit_run(&prog).is_err());
}
