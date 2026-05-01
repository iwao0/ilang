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
    assert_eq!(jit("1 + 2 * 3"), JitValue::I64(7));
    assert_eq!(jit("(10 - 3) * 4"), JitValue::I64(28));
    assert_eq!(jit("100 / 7"), JitValue::I64(14));
    assert_eq!(jit("100 % 7"), JitValue::I64(2));
}

#[test]
fn bitwise() {
    assert_eq!(jit("12 & 10"), JitValue::I64(8));
    assert_eq!(jit("12 | 10"), JitValue::I64(14));
    assert_eq!(jit("12 ^ 10"), JitValue::I64(6));
    assert_eq!(jit("1 << 4"), JitValue::I64(16));
    assert_eq!(jit("256 >> 2"), JitValue::I64(64));
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
        JitValue::I64(15)
    );
}

#[test]
fn if_expression() {
    assert_eq!(
        jit("let n = 7; if n > 5 { n * 10 } else { n * 100 }"),
        JitValue::I64(70)
    );
}

#[test]
fn while_loop() {
    let src = "let n = 0; let i = 1; while i <= 5 { n = n + i; i = i + 1; } n";
    assert_eq!(jit(src), JitValue::I64(15));
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
    assert_eq!(jit(src), JitValue::I64(25)); // 1+3+5+7+9
}

#[test]
fn function_calls() {
    let src = "fn add(a: i64, b: i64): i64 { a + b } add(2, 3)";
    assert_eq!(jit(src), JitValue::I64(5));
}

#[test]
fn recursive_fib() {
    let src = "fn fib(n: i64): i64 { if n < 2 { n } else { fib(n - 1) + fib(n - 2) } } fib(20)";
    assert_eq!(jit(src), JitValue::I64(6765));
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

#[test]
fn narrow_int_types() {
    assert_eq!(jit("let a: i32 = 100; let b: i32 = 200; a + b"), JitValue::I32(300));
    assert_eq!(jit("let a: i16 = 1000; a * 2 as i16"), JitValue::I16(2000));
    assert_eq!(jit("let a: i8 = -5; a + 3 as i8"), JitValue::I8(-2));
}

#[test]
fn unsigned_int_types() {
    assert_eq!(jit("let a: u8 = 100; let b: u8 = 50; a + b"), JitValue::U8(150));
    assert_eq!(jit("let a: u16 = 1000; a + 1 as u16"), JitValue::U16(1001));
    assert_eq!(jit("let a: u32 = 1_000_000; a"), JitValue::U32(1_000_000));
    assert_eq!(
        jit("(0xFFFF_FFFF_FFFF_FFFF as u64)"),
        JitValue::U64(u64::MAX)
    );
}

#[test]
fn float_arithmetic() {
    assert_eq!(jit("1.5 + 2.5"), JitValue::F64(4.0));
    assert_eq!(jit("let x: f32 = 1.5; x + 2.5_f32"), JitValue::F32(4.0));
    assert_eq!(jit("10.0 / 4.0"), JitValue::F64(2.5));
}

#[test]
fn float_comparison() {
    assert_eq!(jit("1.5 < 2.0"), JitValue::Bool(true));
    assert_eq!(jit("3.14 == 3.14"), JitValue::Bool(true));
}

#[test]
fn cast_lowering() {
    assert_eq!(jit("3.7 as i32"), JitValue::I32(3));
    assert_eq!(jit("100_i32 as f64"), JitValue::F64(100.0));
    assert_eq!(jit("(-1_i32) as u32"), JitValue::U32(u32::MAX));
    assert_eq!(jit("true as i32"), JitValue::I32(1));
}

#[test]
fn mixed_width_promotes() {
    // i32 + i64 → i64
    assert_eq!(jit("let a: i32 = 5; a + 10"), JitValue::I64(15));
    // f32 + f64 → f64
    assert_eq!(jit("let a: f32 = 1.5; a + 1.0"), JitValue::F64(2.5));
}

#[test]
fn unsigned_arithmetic_uses_unsigned_ops() {
    // u32 division and comparison go through udiv / unsigned icmp.
    assert_eq!(
        jit("let a: u32 = 4_000_000_000; a / 2 as u32"),
        JitValue::U32(2_000_000_000)
    );
    // 0xFFFFFFFF as u32 > 1 (would be -1 < 1 if treated signed)
    assert_eq!(
        jit("let a: u32 = 0xFFFF_FFFF as u32; a > 1 as u32"),
        JitValue::Bool(true)
    );
}
