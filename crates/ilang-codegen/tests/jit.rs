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
fn string_literal_concat_compare() {
    assert_eq!(
        jit(r#""hello, " + "world""#),
        JitValue::Str("hello, world".into())
    );
    assert_eq!(jit(r#""a" == "a""#), JitValue::Bool(true));
    assert_eq!(jit(r#""a" == "b""#), JitValue::Bool(false));
    assert_eq!(jit(r#""a" != "b""#), JitValue::Bool(true));
}

#[test]
fn string_round_trip_through_function() {
    let src = r#"
        fn shout(s: string): string { s + "!!!" }
        shout("wow")
    "#;
    assert_eq!(jit(src), JitValue::Str("wow!!!".into()));
}

#[test]
fn console_log_runs() {
    // Output isn't captured; just verify it compiles and exits cleanly.
    assert_eq!(jit("console.log(1, 2, 3)"), JitValue::Unit);
    assert_eq!(jit(r#"console.log("hello", true, 3.14)"#), JitValue::Unit);
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

#[test]
fn class_basic_counter() {
    let src = r#"
        class Counter {
            count: i64
            init(start: i64) { this.count = start }
            increment(): i64 {
                this.count = this.count + 1
                this.count
            }
            get(): i64 { this.count }
        }
        let c = new Counter(10)
        c.increment()
        c.increment()
        c.get()
    "#;
    assert_eq!(jit(src), JitValue::I64(12));
}

#[test]
fn class_implicit_this() {
    let src = r#"
        class Point {
            x: i64
            y: i64
            init(a: i64, b: i64) { this.x = a; this.y = b }
            sum(): i64 { x + y }
            scale(k: i64) { x = x * k; y = y * k }
        }
        let p = new Point(3, 4)
        p.scale(10)
        p.sum()
    "#;
    assert_eq!(jit(src), JitValue::I64(70));
}

#[test]
fn class_mixed_field_types() {
    let src = r#"
        class Mixed {
            a: i32
            b: f64
            c: bool
            init() { this.a = 100; this.b = 3.14; this.c = true }
            show(): i32 { a }
        }
        new Mixed().show()
    "#;
    assert_eq!(jit(src), JitValue::I32(100));
}

#[test]
fn class_method_calls_method() {
    let src = r#"
        class Calc {
            n: i64
            init(x: i64) { this.n = x }
            doubled(): i64 { n * 2 }
            quadrupled(): i64 { doubled() * 2 }
        }
        new Calc(5).quadrupled()
    "#;
    assert_eq!(jit(src), JitValue::I64(20));
}

#[test]
fn class_returned_as_object() {
    let src = r#"
        class P { init() {} }
        new P()
    "#;
    let v = jit(src);
    assert!(matches!(v, JitValue::Object { ref class, .. } if class == "P"));
}

#[test]
fn array_literal_index_length() {
    assert_eq!(
        jit("let a: i32[] = [10, 20, 30]; a.length"),
        JitValue::I64(3)
    );
    assert_eq!(
        jit("let a: i32[] = [10, 20, 30]; a[1]"),
        JitValue::I32(20)
    );
}

#[test]
fn array_index_assignment() {
    let src = "let a: i32[] = [1, 2, 3]; a[0] = 100; a[0]";
    assert_eq!(jit(src), JitValue::I32(100));
}

#[test]
fn array_push_grows() {
    let src = "let a: i32[] = [1]; a.push(2); a.push(3); a.length";
    assert_eq!(jit(src), JitValue::I64(3));
}

#[test]
fn array_returned_to_host() {
    let v = jit("let a: i32[] = [10, 20, 30]; a");
    assert_eq!(
        v,
        JitValue::Array(vec![
            JitValue::I32(10),
            JitValue::I32(20),
            JitValue::I32(30),
        ])
    );
}

#[test]
fn array_of_f64() {
    assert_eq!(jit("let a: f64[] = [1.5, 2.5, 3.5]; a[2]"), JitValue::F64(3.5));
}

#[test]
fn jit_deinit_runs_on_block_exit() {
    let src = r#"
        class Counter {
            n: i64
            init() { this.n = 0 }
            inc() { n = n + 1 }
        }
        class Tracked {
            c: Counter
            init(cc: Counter) { this.c = cc }
            deinit() { c.inc() }
        }
        let counter = new Counter()
        {
            let _t = new Tracked(counter)
        }
        counter.n
    "#;
    assert_eq!(jit(src), JitValue::I64(1));
}

#[test]
fn jit_deinit_skipped_when_aliased() {
    // The inner `_b = a` retains; when `_b` drops, rc still > 0 because
    // `a` outlives it. deinit shouldn't fire mid-program.
    let src = r#"
        class Counter {
            n: i64
            init() { this.n = 0 }
            inc() { n = n + 1 }
        }
        class Tracked {
            c: Counter
            init(cc: Counter) { this.c = cc }
            deinit() { c.inc() }
        }
        let counter = new Counter()
        let a = new Tracked(counter)
        {
            let _b = a
        }
        counter.n
    "#;
    assert_eq!(jit(src), JitValue::I64(0));
}
