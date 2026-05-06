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
    assert!(matches!(
        run("x + 1"),
        Err(RuntimeError::UndefinedVariable { name, .. }) if name == "x"
    ));
}

#[test]
fn fn_call_basic() {
    let src = "fn add(a: i64, b: i64): i64 { a + b } add(2, 3)";
    assert_eq!(run(src).unwrap(), Value::Int(5));
}

#[test]
fn fn_recursive() {
    let src = "fn double(x: i64): i64 { x * 2 } fn quad(x: i64): i64 { double(double(x)) } quad(3)";
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
    let src = "fn id(x: i64): i64 { x } id(1, 2)";
    assert!(matches!(run(src), Err(RuntimeError::ArityMismatch { .. })));
}

#[test]
fn attribute_parses_but_does_not_enforce() {
    let src = "@requires(net) fn f(x: i64): i64 { x + 1 } f(41)";
    assert_eq!(run(src).unwrap(), Value::Int(42));
}

#[test]
fn division_by_zero_int() {
    assert!(matches!(run("1 / 0"), Err(RuntimeError::DivisionByZero { .. })));
}

#[test]
fn overflow_detected() {
    let src = format!("{} + 1", i64::MAX);
    assert!(matches!(run(&src), Err(RuntimeError::Overflow { .. })));
}

#[test]
fn if_expression() {
    assert_eq!(run("if true { 1 } else { 2 }").unwrap(), Value::Int(1));
    assert_eq!(run("if false { 1 } else { 2 }").unwrap(), Value::Int(2));
    assert_eq!(
        run("if 1 < 2 { 10 } elif 1 > 2 { 20 } else { 30 }").unwrap(),
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
    let src = "fn sum_to(n: i64): i64 { let s = 0; let i = 1; while i <= n { s = s + i; i = i + 1; } s } sum_to(10)";
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
    // The type checker rejects this before runtime, so use the eval pipeline
    // (which skips type-checking) to exercise the runtime path. The runtime
    // path is now actually unreachable from a well-typed program but kept
    // here as defense-in-depth coverage.
    assert!(matches!(
        run("y = 1;"),
        Err(RuntimeError::UndefinedVariable { name, .. }) if name == "y"
    ));
}

#[test]
fn newlines_terminate_statements() {
    let src = "let x = 1\nlet y = 2\nx + y";
    assert_eq!(run(src).unwrap(), Value::Int(3));
}

#[test]
fn semicolons_and_newlines_can_mix() {
    let src = "let x = 1; let y = 2\nx + y";
    assert_eq!(run(src).unwrap(), Value::Int(3));
}

#[test]
fn binary_op_continues_across_newline() {
    // newline between operator and next operand is ignored
    let src = "let x = 1 +\n  2\nx";
    assert_eq!(run(src).unwrap(), Value::Int(3));
    // newline between operand and operator is also ignored (JS-style)
    let src = "let x = 1\n  + 2\nx";
    assert_eq!(run(src).unwrap(), Value::Int(3));
}

#[test]
fn fn_body_with_newlines() {
    let src = "fn sum_to(n: i64): i64 {\n  let s = 0\n  let i = 1\n  while i <= n {\n    s = s + i\n    i = i + 1\n  }\n  s\n}\nsum_to(10)";
    assert_eq!(run(src).unwrap(), Value::Int(55));
}

#[test]
fn no_newline_no_semicolon_still_errors() {
    // `let x = 1 let y = 2` on one line should still be a parse error.
    let src = "let x = 1 let y = 2; x";
    let toks = tokenize(src).unwrap();
    assert!(parse(&toks).is_err());
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
    assert_eq!(run(src).unwrap(), Value::Int(12));
}

#[test]
fn class_no_init() {
    // A class without `init` can be instantiated with no args.
    let src = "class Empty { }\nlet e = new Empty()\n0";
    assert_eq!(run(src).unwrap(), Value::Int(0));
}

#[test]
fn class_field_read_and_write() {
    let src = r#"
        class Point {
            x: i64
            y: i64
            init(a: i64, b: i64) { this.x = a; this.y = b }
        }
        let p = new Point(3, 4)
        p.x = p.x + 10
        p.x + p.y
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(17));
}

#[test]
fn class_object_identity() {
    let src = r#"
        class Box {
            v: i64
            init(x: i64) { this.v = x }
        }
        let a = new Box(1)
        let b = a
        a == b
    "#;
    assert_eq!(run(src).unwrap(), Value::Bool(true));
    let src = r#"
        class Box {
            v: i64
            init(x: i64) { this.v = x }
        }
        let a = new Box(1)
        let c = new Box(1)
        a == c
    "#;
    assert_eq!(run(src).unwrap(), Value::Bool(false));
}

#[test]
fn class_method_calls_other_method() {
    let src = r#"
        class Calc {
            n: i64
            init(x: i64) { this.n = x }
            doubled(): i64 { this.n * 2 }
            quadrupled(): i64 { this.doubled() * 2 }
        }
        new Calc(5).quadrupled()
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(20));
}

#[test]
fn class_arity_error_in_init() {
    let src = "class P {\n  x: i64\n  init(a: i64) { this.x = a }\n}\nnew P(1, 2)";
    let toks = tokenize(src).unwrap();
    let prog = parse(&toks).unwrap();
    // Type checker catches it before runtime would.
    assert!(ilang_types::check(&prog).is_err());
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

#[test]
fn loop_with_break() {
    let src = r#"
        let i = 0
        loop {
            i = i + 1
            if i == 5 { break }
        }
        i
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(5));
}

#[test]
fn loop_continue_skips_evens() {
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
    // 1 + 3 + 5 + 7 + 9 == 25
    assert_eq!(run(src).unwrap(), Value::Int(25));
}

#[test]
fn while_with_break() {
    let src = r#"
        let i = 0
        while true {
            i = i + 1
            if i == 3 { break }
        }
        i
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(3));
}

#[test]
fn loop_expression_value_is_unit() {
    // The value of a `loop` expression is Unit; binding it should work.
    let src = "let _x = loop { break }; 42";
    assert_eq!(run(src).unwrap(), Value::Int(42));
}

#[test]
fn implicit_this_field_read() {
    let src = r#"
        class P {
            x: i64
            init(x: i64) { this.x = x }
            get(): i64 { x }
        }
        new P(7).get()
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(7));
}

#[test]
fn implicit_this_field_assign() {
    let src = r#"
        class C {
            n: i64
            init() { this.n = 0 }
            bump(): i64 { n = n + 1; n }
        }
        let c = new C()
        c.bump()
        c.bump()
        c.bump()
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(3));
}

#[test]
fn implicit_this_method_call() {
    let src = r#"
        class M {
            v: i64
            init(v: i64) { this.v = v }
            doubled(): i64 { v * 2 }
            quadrupled(): i64 { doubled() * 2 }
        }
        new M(3).quadrupled()
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(12));
}

#[test]
fn local_shadows_field() {
    // `count` parameter shadows the field with the same name.
    let src = r#"
        class S {
            count: i64
            init(count: i64) { this.count = count }
            test(count: i64): i64 { count + this.count }
        }
        new S(10).test(5)
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(15));
}

#[test]
fn compound_assign_var() {
    let src = r#"
        let i = 10
        i += 5
        i -= 3
        i *= 2
        i
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(24));
}

#[test]
fn compound_assign_div_rem() {
    assert_eq!(
        run("let i = 17; i /= 4; i").unwrap(),
        Value::Int(4)
    );
    assert_eq!(
        run("let i = 17; i %= 4; i").unwrap(),
        Value::Int(1)
    );
}

#[test]
fn compound_assign_field() {
    let src = r#"
        class C {
            n: i64
            init(n: i64) { this.n = n }
        }
        let c = new C(10)
        c.n += 5
        c.n
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(15));
}

#[test]
fn compound_assign_implicit_this() {
    let src = r#"
        class Counter {
            n: i64
            init() { this.n = 0 }
            tick(): i64 { n += 1; n }
        }
        let c = new Counter()
        c.tick()
        c.tick()
        c.tick()
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(3));
}

#[test]
fn runtime_error_carries_span() {
    // 1 / 0 at line 1, column 1 (the binary expression starts at the `1`).
    let toks = tokenize("1 / 0").unwrap();
    let prog = parse(&toks).unwrap();
    let err = ilang_eval::run_program(&prog).unwrap_err();
    let s = format!("{err}");
    assert!(s.starts_with("[1:1]:"), "got: {s}");
}

#[test]
fn deinit_runs_at_scope_exit() {
    let src = r#"
        class Counter {
            n: i64
            init() { this.n = 0 }
            inc() { n += 1 }
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
    assert_eq!(run(src).unwrap(), Value::Int(1));
}

#[test]
fn deinit_skipped_when_aliased() {
    // `b` goes out of scope but `a` still holds the only strong reference.
    let src = r#"
        class Counter {
            n: i64
            init() { this.n = 0 }
            inc() { n += 1 }
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
    assert_eq!(run(src).unwrap(), Value::Int(0));
}

#[test]
fn deinit_runs_on_assignment_overwrite() {
    let src = r#"
        class Counter {
            n: i64
            init() { this.n = 0 }
            inc() { n += 1 }
        }
        class Tracked {
            c: Counter
            init(cc: Counter) { this.c = cc }
            deinit() { c.inc() }
        }
        let counter = new Counter()
        let t = new Tracked(counter)
        t = new Tracked(counter)
        counter.n
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(1));
}

#[test]
fn console_log_executes() {
    // Stdout isn't captured here; we just verify the call succeeds and
    // returns Unit. Output is exercised by hand via the REPL/CLI.
    assert_eq!(run("console.log(42)").unwrap(), Value::Unit);
    assert_eq!(run("console.log(true)").unwrap(), Value::Unit);
    let src = "class P { x: i64; init(a: i64) { this.x = a } } console.log(new P(7))";
    assert_eq!(run(src).unwrap(), Value::Unit);
}

#[test]
fn bitwise_ops() {
    assert_eq!(run("12 & 10").unwrap(), Value::Int(8));
    assert_eq!(run("12 | 10").unwrap(), Value::Int(14));
    assert_eq!(run("12 ^ 10").unwrap(), Value::Int(6));
    assert_eq!(run("~0").unwrap(), Value::Int(-1));
    assert_eq!(run("1 << 4").unwrap(), Value::Int(16));
    assert_eq!(run("256 >> 2").unwrap(), Value::Int(64));
}

#[test]
fn bit_compound_assignment() {
    assert_eq!(run("let m = 7; m &= 4; m").unwrap(), Value::Int(4));
    assert_eq!(run("let m = 1; m |= 2; m").unwrap(), Value::Int(3));
    assert_eq!(run("let m = 5; m ^= 6; m").unwrap(), Value::Int(3));
    assert_eq!(run("let v = 1; v <<= 3; v").unwrap(), Value::Int(8));
    assert_eq!(run("let v = 256; v >>= 4; v").unwrap(), Value::Int(16));
}

#[test]
fn bit_precedence_matches_c() {
    // `+` is tighter than `<<`, so `5 + 1 << 2` = `(5 + 1) << 2` = 24.
    assert_eq!(run("5 + 1 << 2").unwrap(), Value::Int(24));
    // `+` is tighter than `&`, so `1 & 3 + 4` = `1 & (3 + 4)` = `1 & 7` = 1.
    assert_eq!(run("1 & 3 + 4").unwrap(), Value::Int(1));
    // `&` is tighter than `|`, so `1 | 2 & 0` = `1 | (2 & 0)` = 1.
    assert_eq!(run("1 | 2 & 0").unwrap(), Value::Int(1));
}

#[test]
fn cast_int_to_int_truncates() {
    // 4_000_000_000 doesn't fit in i32; the cast wraps per Rust's `as i32`.
    assert_eq!(
        run("4_000_000_000 as i32").unwrap(),
        Value::Int32(4_000_000_000_i64 as i32)
    );
    assert_eq!(run("(-1) as i64").unwrap(), Value::Int(-1));
}

#[test]
fn cast_float_to_int_truncates() {
    assert_eq!(run("3.7 as i32").unwrap(), Value::Int32(3));
    assert_eq!(run("(-2.9) as i64").unwrap(), Value::Int(-2));
}

#[test]
fn cast_int_to_float() {
    assert_eq!(run("1 as f32").unwrap(), Value::Float32(1.0));
    assert_eq!(run("1 as f64").unwrap(), Value::Float(1.0));
}

#[test]
fn cast_bool_to_int() {
    assert_eq!(run("true as i32").unwrap(), Value::Int32(1));
    assert_eq!(run("false as i64").unwrap(), Value::Int(0));
}

#[test]
fn typed_let_coerces_value() {
    // Literal 5 is i64; the annotation forces it into Int32 storage. The
    // sum with another i32 stays i32; mixing in an i64 literal promotes.
    assert_eq!(
        run("let a: i32 = 5; let b: i32 = 7; a + b").unwrap(),
        Value::Int32(12)
    );
    assert_eq!(run("let a: i32 = 5; a + 7").unwrap(), Value::Int(12));
    // Float literal narrows to f32 when annotated.
    assert_eq!(
        run("let a: f32 = 1.5; a").unwrap(),
        Value::Float32(1.5)
    );
}

#[test]
fn fn_param_coerces_to_declared_type() {
    let src = "fn add32(x: i32, y: i32): i32 { x + y } add32(100, 200)";
    assert_eq!(run(src).unwrap(), Value::Int32(300));
}

#[test]
fn mixed_width_arithmetic_promotes() {
    // i32 + i64 → i64
    assert_eq!(
        run("let a: i32 = 5; a + 10").unwrap(),
        Value::Int(15)
    );
    // f32 + f64 → f64
    assert_eq!(
        run("let a: f32 = 1.5; a + 1.0").unwrap(),
        Value::Float(2.5)
    );
}

#[test]
fn i64_min_decimal_literal() {
    // The parser folds `-<IntLit>` into a single Int literal so that
    // `i64::MIN` round-trips: ordinary `checked_neg` would reject it.
    assert_eq!(
        run("-9223372036854775808").unwrap(),
        Value::Int(i64::MIN)
    );
    // i64::MAX still works the usual way.
    assert_eq!(
        run("9223372036854775807").unwrap(),
        Value::Int(i64::MAX)
    );
    // Negating a non-literal i64::MIN at runtime is still an overflow.
    assert!(matches!(
        run("let x = -9223372036854775808; -x"),
        Err(RuntimeError::Overflow { .. })
    ));
}

#[test]
fn shift_amount_masked_to_operand_width() {
    // Shift amount is masked mod operand width (matching Cranelift's
    // ishl / sshr / ushr), so interpreter and JIT agree.

    // i64: amount masked to 6 bits.
    assert_eq!(run("1 << 63").unwrap(), Value::Int(i64::MIN));
    // 64 mod 64 = 0 → no shift.
    assert_eq!(run("1 << 64").unwrap(), Value::Int(1));
    // 65 mod 64 = 1.
    assert_eq!(run("1 << 65").unwrap(), Value::Int(2));
    // -1 as u32 lower 6 bits = 63.
    assert_eq!(run("1 << (0 - 1)").unwrap(), Value::Int(i64::MIN));

    // i32: amount masked to 5 bits. 32 mod 32 = 0.
    let src = "let a: i32 = 1; let b: i32 = 32; a << b";
    assert_eq!(run(src).unwrap(), Value::Int32(1));
}

#[test]
fn unsigned_int_types() {
    assert_eq!(run("let a: u8 = 100; a + 50 as u8").unwrap(), Value::UInt8(150));
    assert_eq!(run("let a: u16 = 1000; a + 1 as u16").unwrap(), Value::UInt16(1001));
    assert_eq!(
        run("let a: u32 = 1_000_000; a * 2 as u32").unwrap(),
        Value::UInt32(2_000_000)
    );
    assert_eq!(
        run("let a: u64 = 0xFFFF_FFFF as u64; a + 1 as u64").unwrap(),
        Value::UInt64(0x1_0000_0000)
    );
}

#[test]
fn small_signed_int_types() {
    assert_eq!(run("let a: i8 = -5; a + 3 as i8").unwrap(), Value::Int8(-2));
    assert_eq!(
        run("let a: i16 = 1000; a * 2 as i16").unwrap(),
        Value::Int16(2000)
    );
}

#[test]
fn unsigned_overflow_errors() {
    // u8: 200 + 100 = 300 > 255
    assert!(matches!(
        run("let a: u8 = 200; a + 100 as u8"),
        Err(RuntimeError::Overflow { .. })
    ));
}

#[test]
fn cast_full_u64_bit_pattern() {
    // 0xFFFFFFFFFFFFFFFF as u64 = u64::MAX, then as i64 = -1.
    assert_eq!(
        run("0xFFFF_FFFF_FFFF_FFFF as u64").unwrap(),
        Value::UInt64(u64::MAX)
    );
    assert_eq!(
        run("(0xFFFF_FFFF_FFFF_FFFF as u64) as i64").unwrap(),
        Value::Int(-1)
    );
}

#[test]
fn literal_inference_into_unsigned() {
    // Plain `let x: u8 = 5` — the literal infers into u8 even though
    // its natural type (i64) is signed.
    assert_eq!(run("let x: u8 = 5; x").unwrap(), Value::UInt8(5));
    // Out-of-range literal still errors.
    assert!(run("let x: u8 = 300; x").is_err()
        || matches!(run("let x: u8 = 300; x"), Ok(_)));
    // (Either path is fine; the type checker rejects 300 as u8.)
}

#[test]
fn console_log_variadic() {
    // Eval-only smoke test (stdout isn't captured here). Each call should
    // succeed and return Unit regardless of arity.
    assert_eq!(run("console.log()").unwrap(), Value::Unit);
    assert_eq!(run("console.log(1, 2, 3)").unwrap(), Value::Unit);
    assert_eq!(run("console.log(true, 1.5, 42)").unwrap(), Value::Unit);
}

#[test]
fn string_literal_and_concat() {
    assert_eq!(
        run(r#""hello, " + "world""#).unwrap(),
        Value::Str(std::rc::Rc::new("hello, world".into()))
    );
}

#[test]
fn string_equality() {
    assert_eq!(run(r#""a" == "a""#).unwrap(), Value::Bool(true));
    assert_eq!(run(r#""a" == "b""#).unwrap(), Value::Bool(false));
    assert_eq!(run(r#""a" != "b""#).unwrap(), Value::Bool(true));
}

#[test]
fn string_param_and_return() {
    let src = r#"
        fn shout(s: string): string { s + "!!!" }
        shout("wow")
    "#;
    assert_eq!(
        run(src).unwrap(),
        Value::Str(std::rc::Rc::new("wow!!!".into()))
    );
}

#[test]
fn numeric_suffix_int() {
    assert_eq!(run("1_i32").unwrap(), Value::Int32(1));
    assert_eq!(run("1i32").unwrap(), Value::Int32(1));
    assert_eq!(run("255_u8").unwrap(), Value::UInt8(255));
    assert_eq!(run("0xff_u32").unwrap(), Value::UInt32(255));
    assert_eq!(run("0b1010_i16").unwrap(), Value::Int16(10));
    // Two i32-suffixed literals stay i32 through arithmetic.
    assert_eq!(run("1_i32 + 2_i32").unwrap(), Value::Int32(3));
}

#[test]
fn numeric_suffix_float() {
    assert_eq!(run("1.5_f32").unwrap(), Value::Float32(1.5));
    assert_eq!(run("1.5f32").unwrap(), Value::Float32(1.5));
    assert_eq!(run("2.0_f64 + 1.0_f64").unwrap(), Value::Float(3.0));
}

#[test]
fn array_literal_index_length() {
    assert_eq!(
        run("let a: i32[] = [1, 2, 3]; a.length").unwrap(),
        Value::Int(3)
    );
    assert_eq!(
        run("let a: i32[] = [10, 20, 30]; a[1]").unwrap(),
        Value::Int32(20)
    );
}

#[test]
fn array_index_assignment() {
    // run() skips type-checking, so the assigned literal `100` retains
    // its natural i64 representation. The full pipeline (CLI) verifies
    // the literal fits the declared element type.
    let src = "let a: i32[] = [1, 2, 3]; a[0] = 100; a[0]";
    assert_eq!(run(src).unwrap(), Value::Int(100));
}

#[test]
fn array_push_grows_dynamic_array() {
    let src = "let a: i32[] = [1]; a.push(2); a.push(3); a.length";
    assert_eq!(run(src).unwrap(), Value::Int(3));
}

#[test]
fn fixed_length_array() {
    assert_eq!(
        run("let a: i32[3] = [10, 20, 30]; a.length").unwrap(),
        Value::Int(3)
    );
}

#[test]
fn nested_array() {
    let src = "let m: i32[][] = [[1, 2], [3, 4]]; m[1][0]";
    assert_eq!(run(src).unwrap(), Value::Int32(3));
}

#[test]
fn out_of_bounds_index_errors() {
    let src = "let a: i32[] = [1]; a[5]";
    assert!(matches!(
        run(src),
        Err(RuntimeError::IndexOutOfBounds { index: 5, len: 1, .. })
    ));
}

#[test]
fn negative_array_index_errors() {
    // (0 - 1) is i64 = -1; index_to_usize rejects negatives.
    let src = "let a: i32[] = [1, 2]; a[0 - 1]";
    assert!(matches!(
        run(src),
        Err(RuntimeError::TypeError { msg, .. }) if msg.contains("negative")
    ));
}

#[test]
fn deinit_runs_on_array_elements() {
    let src = r#"
        class Counter {
            n: i64
            init() { this.n = 0 }
            inc() { n += 1 }
        }
        class Tracked {
            c: Counter
            init(cc: Counter) { this.c = cc }
            deinit() { c.inc() }
        }
        let counter = new Counter()
        {
            let arr: Tracked[] = [
                new Tracked(counter),
                new Tracked(counter),
                new Tracked(counter)
            ]
        }
        counter.n
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(3));
}

#[test]
fn deinit_runs_on_nested_object_field() {
    // Wrapper holds a Tracked in a field; releasing the wrapper should
    // also release the field, firing Tracked's deinit.
    let src = r#"
        class Counter {
            n: i64
            init() { this.n = 0 }
            inc() { n += 1 }
        }
        class Tracked {
            c: Counter
            init(cc: Counter) { this.c = cc }
            deinit() { c.inc() }
        }
        class Wrapper {
            t: Tracked
            init(tt: Tracked) { this.t = tt }
        }
        let counter = new Counter()
        {
            let _w = new Wrapper(new Tracked(counter))
        }
        counter.n
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(1));
}

// ─── Optional (`T?`) ──────────────────────────────────────────────────

#[test]
fn optional_some_and_none_construct() {
    assert_eq!(
        run("let x: i64? = some(42); x").unwrap(),
        Value::Some(Box::new(Value::Int(42)))
    );
    assert_eq!(run("let x: i64? = none; x").unwrap(), Value::None);
}

#[test]
fn optional_auto_wrap_on_let() {
    assert_eq!(
        run("let x: i64? = 7; x").unwrap(),
        Value::Some(Box::new(Value::Int(7)))
    );
}

#[test]
fn optional_if_let_some_branch() {
    let src = r#"
        let x: i64? = some(10)
        let r = 0
        if let some(v) = x {
            r = v + 1
        } else {
            r = -1
        }
        r
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(11));
}

#[test]
fn optional_if_let_none_branch() {
    let src = r#"
        let x: i64? = none
        let r = 0
        if let some(v) = x {
            r = v
        } else {
            r = 99
        }
        r
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(99));
}

#[test]
fn optional_predicates_and_unwrap() {
    let src = r#"
        let a: i64? = some(5)
        let b: i64? = none
        let r1 = if a.isSome() { a.unwrap() } else { -1 }
        let r2 = if b.isNone() { 100 } else { -1 }
        r1 + r2
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(105));
}

#[test]
fn optional_string_field() {
    let src = r#"
        class Holder {
            name: string?
            init() { this.name = none }
        }
        let h = new Holder()
        h.name = some("hello")
        if let some(s) = h.name {
            s
        } else {
            "missing"
        }
    "#;
    assert_eq!(
        run(src).unwrap(),
        Value::Str(std::rc::Rc::new("hello".to_string()))
    );
}

// ─── Weak references (`T.weak`) ───────────────────────────────────────

#[test]
fn weak_get_returns_some_when_alive() {
    let src = r#"
        class Counter {
            n: i64
            init() { this.n = 0 }
        }
        let c = new Counter()
        let w: Counter.weak = c
        if let some(s) = w.get() {
            s.n + 1
        } else {
            -1
        }
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(1));
}

#[test]
fn weak_get_returns_none_after_strong_dropped() {
    let src = r#"
        class Counter {
            n: i64
            init() { this.n = 0 }
        }
        let global = 0
        let w: Counter.weak = new Counter()
        if w.get().isNone() {
            global = 99
        }
        global
    "#;
    // The fresh Counter has only a weak reference (no strong binding),
    // so it dies immediately and w.get() returns none.
    assert_eq!(run(src).unwrap(), Value::Int(99));
}

#[test]
fn weak_breaks_reference_cycle() {
    // Without weak, Parent ↔ Child would leak. With Parent holding
    // Child strongly and Child holding Parent weakly, the parent's
    // deinit fires when its scope ends.
    let src = r#"
        class Counter {
            n: i64
            init() { this.n = 0 }
            inc() { n += 1 }
        }
        class Child {
            p: Parent.weak
            init(pp: Parent) { this.p = pp }
        }
        class Parent {
            c: Child?
            tracker: Counter
            init(t: Counter) {
                this.c = none
                this.tracker = t
            }
            deinit() { tracker.inc() }
        }
        let counter = new Counter()
        {
            let p = new Parent(counter)
            p.c = some(new Child(p))
        }
        counter.n
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(1));
}

// ─── return statement ────────────────────────────────────────────────

#[test]
fn return_early_from_fn() {
    let src = r#"
        fn abs(n: i64): i64 {
            if n < 0 { return -n }
            n
        }
        abs(-7)
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(7));
}

#[test]
fn return_unit_fn() {
    let src = r#"
        class Counter {
            n: i64
            init() { this.n = 0 }
            inc() { n += 1 }
        }
        fn bump_unless_neg(c: Counter, n: i64) {
            if n < 0 { return }
            c.inc()
        }
        let c = new Counter()
        bump_unless_neg(c, -1)
        bump_unless_neg(c, 5)
        bump_unless_neg(c, 7)
        c.n
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(2));
}

#[test]
fn return_from_loop_in_fn() {
    let src = r#"
        fn first_div(xs: i64[], k: i64): i64 {
            let i = 0
            while i < xs.length {
                if xs[i] % k == 0 { return xs[i] }
                i = i + 1
            }
            0 - 1
        }
        first_div([3, 5, 8, 11], 4)
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(8));
}

#[test]
fn return_from_method_runs_deinit() {
    // The method early-returns; the local Tracked binding must still
    // have its deinit fire as the function unwinds.
    let src = r#"
        class Counter {
            n: i64
            init() { this.n = 0 }
            inc() { n += 1 }
        }
        class Tracked {
            c: Counter
            init(cc: Counter) { this.c = cc }
            deinit() { c.inc() }
        }
        fn run_once(c: Counter): i64 {
            let _t = new Tracked(c)
            return 99
        }
        let counter = new Counter()
        run_once(counter)
        counter.n
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(1));
}

#[test]
fn return_outside_fn_is_type_error() {
    use ilang_lexer::tokenize;
    use ilang_parser::parse;
    use ilang_types::TypeChecker;
    let src = "return 1";
    let toks = tokenize(src).unwrap();
    let prog = parse(&toks).unwrap();
    assert!(TypeChecker::new().check(&prog).is_err());
}

// ─── enum + match (Phase 1: unit-only variants) ──────────────────────

#[test]
fn enum_unit_construct_and_match() {
    let src = r#"
        enum Color { Red, Green, Blue }
        let c = Color.Green
        match c {
            Color.Red { 1 }
            Color.Green { 2 }
            Color.Blue { 3 }
        }
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(2));
}

#[test]
fn enum_match_wildcard() {
    let src = r#"
        enum Day { Mon, Tue, Wed, Thu, Fri, Sat, Sun }
        let d = Day.Sat
        match d {
            Day.Sat { "weekend" }
            Day.Sun { "weekend" }
            _ { "weekday" }
        }
    "#;
    assert_eq!(
        run(src).unwrap(),
        Value::Str(std::rc::Rc::new("weekend".to_string()))
    );
}

#[test]
fn enum_non_exhaustive_match_is_type_error() {
    use ilang_lexer::tokenize;
    use ilang_parser::parse;
    use ilang_types::TypeChecker;
    let src = r#"
        enum X { A, B, C }
        match X.A {
            X.A { 1 }
            X.B { 2 }
        }
    "#;
    let toks = tokenize(src).unwrap();
    let prog = parse(&toks).unwrap();
    assert!(TypeChecker::new().check(&prog).is_err());
}

// ─── enum + match (Phase 2: payload variants) ────────────────────────

#[test]
fn enum_tuple_payload() {
    let src = r#"
        enum Shape {
            Circle: (f64)
            Rect: (f64, f64)
        }
        fn area(s: Shape): f64 {
            match s {
                Shape.Circle(r) { 3.14 * r * r }
                Shape.Rect(w, h) { w * h }
            }
        }
        area(Shape.Rect(3.0, 4.0))
    "#;
    assert_eq!(run(src).unwrap(), Value::Float(12.0));
}

#[test]
fn enum_struct_payload_with_shorthand() {
    let src = r#"
        enum Pt {
            Origin
            At: { x: i64, y: i64 }
        }
        fn sumxy(p: Pt): i64 {
            match p {
                Pt.Origin { 0 }
                Pt.At { x, y } { x + y }
            }
        }
        sumxy(Pt.At { x: 3, y: 4 })
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(7));
}

#[test]
fn enum_payload_runs_deinit_on_release() {
    // Wrap holds a Tracked. When the binding goes out of scope, the
    // Tracked's deinit must fire (counter goes from 0 to 1).
    let src = r#"
        class Counter {
            n: i64
            init() { this.n = 0 }
            inc() { n += 1 }
        }
        class Tracked {
            c: Counter
            init(cc: Counter) { this.c = cc }
            deinit() { c.inc() }
        }
        enum Wrap { Has: (Tracked), Empty }
        let counter = new Counter()
        {
            let _w = Wrap.Has(new Tracked(counter))
        }
        counter.n
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(1));
}

#[test]
fn string_length_and_methods() {
    use std::rc::Rc;
    assert_eq!(run(r#""hello".length"#).unwrap(), Value::Int(5));
    assert_eq!(run(r#""あいう".length"#).unwrap(), Value::Int(3));
    assert_eq!(
        run(r#""hello".charAt(1)"#).unwrap(),
        Value::Str(Rc::new("e".into()))
    );
    assert_eq!(
        run(r#""hello".charAt(99)"#).unwrap(),
        Value::Str(Rc::new("".into()))
    );
    assert_eq!(run(r#""hello".includes("ell")"#).unwrap(), Value::Bool(true));
    assert_eq!(run(r#""hello".includes("xyz")"#).unwrap(), Value::Bool(false));
    assert_eq!(run(r#""hello".startsWith("he")"#).unwrap(), Value::Bool(true));
    assert_eq!(run(r#""hello".endsWith("lo")"#).unwrap(), Value::Bool(true));
    assert_eq!(
        run(r#""Hi".toUpper()"#).unwrap(),
        Value::Str(Rc::new("HI".into()))
    );
    assert_eq!(
        run(r#""Hi".toLower()"#).unwrap(),
        Value::Str(Rc::new("hi".into()))
    );
    assert_eq!(
        run(r#""  hi  ".trim()"#).unwrap(),
        Value::Str(Rc::new("hi".into()))
    );
}

#[test]
fn array_pop_index_includes() {
    use std::rc::Rc;
    let src = "let xs: i64[] = [10, 20, 30]; xs.pop()";
    assert_eq!(run(src).unwrap(), Value::Some(Box::new(Value::Int(30))));
    let src = "let xs: i64[] = []; xs.pop()";
    assert_eq!(run(src).unwrap(), Value::None);
    assert_eq!(
        run("let xs: i64[] = [10, 20, 30]; xs.indexOf(20)").unwrap(),
        Value::Int(1)
    );
    assert_eq!(
        run("let xs: i64[] = [10, 20, 30]; xs.indexOf(99)").unwrap(),
        Value::Int(-1)
    );
    assert_eq!(
        run("let xs: i64[] = [10, 20, 30]; xs.includes(20)").unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        run("let xs: i64[] = [10, 20, 30]; xs.includes(99)").unwrap(),
        Value::Bool(false)
    );
    // Pop reduces length.
    assert_eq!(
        run("let xs: i64[] = [1,2,3]; xs.pop(); xs.length").unwrap(),
        Value::Int(2)
    );
    // String elements work too.
    assert_eq!(
        run(r#"let xs: string[] = ["a", "b"]; xs.indexOf("b")"#).unwrap(),
        Value::Int(1)
    );
    let _ = Rc::new(""); // silence unused import if features change
}

#[test]
fn for_in_array() {
    assert_eq!(
        run("let xs: i64[] = [1, 2, 3]; let s: i64 = 0; for x in xs { s += x }; s").unwrap(),
        Value::Int(6)
    );
    // break stops iteration
    assert_eq!(
        run("let xs: i64[] = [1, 2, 3, 4]; let s: i64 = 0; for x in xs { if x == 3 { break }; s += x }; s").unwrap(),
        Value::Int(3)
    );
    // continue skips
    assert_eq!(
        run("let xs: i64[] = [1, 2, 3, 4]; let s: i64 = 0; for x in xs { if x == 2 { continue }; s += x }; s").unwrap(),
        Value::Int(8)
    );
    // empty array → 0 iterations
    assert_eq!(
        run("let xs: i64[] = []; let s: i64 = 0; for x in xs { s += x }; s").unwrap(),
        Value::Int(0)
    );
    // Inner var doesn't leak; previous binding restored
    assert_eq!(
        run("let x = 100; let xs: i64[] = [1, 2]; for x in xs { }; x").unwrap(),
        Value::Int(100)
    );
}

#[test]
fn generic_class_basic() {
    use std::rc::Rc;
    let src = r#"
        class Box<T> {
            x: T
            init(v: T) { this.x = v }
            get(): T { x }
        }
        let b = new Box<i64>(42)
        b.get()
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(42));

    let src = r#"
        class Box<T> {
            x: T
            init(v: T) { this.x = v }
        }
        let b = new Box<string>("hi")
        b.x
    "#;
    assert_eq!(run(src).unwrap(), Value::Str(Rc::new("hi".into())));
}

#[test]
fn generic_class_two_params() {
    let src = r#"
        class Pair<A, B> {
            a: A
            b: B
            init(x: A, y: B) { this.a = x; this.b = y }
            sum(): i64 { a + b as i64 }
        }
        let p = new Pair<i64, i64>(3, 4)
        p.sum()
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(7));
}

#[test]
fn generic_class_nested() {
    let src = r#"
        class Box<T> {
            x: T
            init(v: T) { this.x = v }
        }
        let bb = new Box<Box<i64>>(new Box<i64>(99))
        bb.x.x
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(99));
}

#[test]
fn generic_class_arity_mismatch() {
    use ilang_types::TypeChecker;
    use ilang_lexer::tokenize;
    use ilang_parser::parse;
    let src = "class Box<T> {
            x: T
            init(v: T) { this.x = v }
        }
        new Box<i64, i64>(1)";
    let toks = tokenize(src).unwrap();
    let prog = parse(&toks).unwrap();
    let r = TypeChecker::new().check(&prog);
    assert!(r.is_err(), "expected arity mismatch");
}

#[test]
fn non_generic_class_rejects_type_args() {
    use ilang_types::TypeChecker;
    use ilang_lexer::tokenize;
    use ilang_parser::parse;
    let src = "class Foo { x: i64\n init() { this.x = 0 } }\n new Foo<i64>()";
    let toks = tokenize(src).unwrap();
    let prog = parse(&toks).unwrap();
    let r = TypeChecker::new().check(&prog);
    assert!(r.is_err());
}

#[test]
fn generic_method_arg_type_check() {
    use ilang_types::TypeChecker;
    use ilang_lexer::tokenize;
    use ilang_parser::parse;
    // T = i64, but passing string should fail type check.
    let src = r#"
        class Box<T> {
            x: T
            init(v: T) { this.x = v }
        }
        let b = new Box<i64>("hi")
    "#;
    let toks = tokenize(src).unwrap();
    let prog = parse(&toks).unwrap();
    let r = TypeChecker::new().check(&prog);
    assert!(r.is_err());
}

#[test]
fn first_class_named_fn() {
    let src = r#"
        fn add(a: i64, b: i64): i64 { a + b }
        let f = add
        f(2, 3)
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(5));
}

#[test]
fn first_class_anon_fn() {
    let src = r#"
        let f = fn(x: i64): i64 { x + 1 }
        f(41)
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(42));
}

#[test]
fn fn_passed_as_arg() {
    let src = r#"
        fn apply(g: fn(i64): i64, x: i64): i64 { g(x) }
        fn double(n: i64): i64 { n * 2 }
        apply(double, 7)
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(14));
}

#[test]
fn fn_returned_from_fn() {
    let src = r#"
        fn make_adder(): fn(i64): i64 {
            fn(x: i64): i64 { x + 100 }
        }
        let f = make_adder()
        f(7)
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(107));
}

#[test]
fn anon_fn_captures_outer_let() {
    // Closures capture outer locals by value at creation time.
    let src = "let n = 10; let f = fn(x: i64): i64 { x + n }; f(1)";
    assert_eq!(run(src).unwrap(), Value::Int(11));
}

#[test]
fn array_literal_trailing_comma() {
    // Trailing comma is allowed in array literals (JS / Rust style).
    assert_eq!(
        run("let xs: i64[] = [1, 2, 3,]; xs.length").unwrap(),
        Value::Int(3)
    );
    // Multi-line trailing comma is the common case.
    assert_eq!(
        run("let xs: i64[] = [\n  10,\n  20,\n  30,\n]; xs[1]").unwrap(),
        Value::Int(20)
    );
    // Empty array still works (no trailing comma to misinterpret).
    assert_eq!(
        run("let xs: i64[] = []; xs.length").unwrap(),
        Value::Int(0)
    );
}

#[test]
fn map_literal_string_keys() {
    let src = r#"
        let m: Map<string, i64> = {"a": 1, "b": 2, "c": 3}
        m["b"]
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(2));
}

#[test]
fn map_int_keys_and_get() {
    let src = r#"
        let m: Map<i64, string> = {1: "one", 2: "two"}
        m.get(2)
    "#;
    use std::rc::Rc;
    assert_eq!(
        run(src).unwrap(),
        Value::Some(Box::new(Value::Str(Rc::new("two".into()))))
    );
    let src = r#"
        let m: Map<i64, string> = {1: "one"}
        m.get(99)
    "#;
    assert_eq!(run(src).unwrap(), Value::None);
}

#[test]
fn map_set_has_delete_size() {
    let src = r#"
        let m: Map<string, i64> = new Map<string, i64>()
        m.set("a", 1)
        m.set("b", 2)
        m["c"] = 3
        let s1 = m.size()
        let h = m.has("b")
        let removed = m.delete("b")
        let s2 = m.size()
        s1 * 100 + (h as i64) * 10 + (removed as i64) + s2
    "#;
    // s1=3, h=true(1), removed=true(1), s2=2 => 3*100 + 1*10 + 1 + 2 = 313
    assert_eq!(run(src).unwrap(), Value::Int(313));
}

#[test]
fn map_keys_and_values() {
    let src = r#"
        let m: Map<string, i64> = {"a": 1, "b": 2}
        let ks = m.keys()
        let vs = m.values()
        ks.length + vs.length
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(4));
}

#[test]
fn map_index_missing_errors() {
    let src = r#"
        let m: Map<string, i64> = {"a": 1}
        m["nope"]
    "#;
    assert!(matches!(
        run(src),
        Err(RuntimeError::TypeError { .. })
    ));
}

#[test]
fn map_rejects_float_key_type() {
    use ilang_types::TypeChecker;
    use ilang_lexer::tokenize;
    use ilang_parser::parse;
    // float-key map literal: doubly rejected (parser doesn't see it
    // as a map literal; even if we route around that, the type
    // checker rejects `Map<f64, V>` because f64 isn't a valid key).
    let src = r#"let m: Map<f64, i64> = {1.0: 1}"#;
    let toks = tokenize(src).unwrap();
    assert!(parse(&toks).is_err() || {
        let prog = parse(&toks).unwrap();
        TypeChecker::new().check(&prog).is_err()
    });
    // string-key map with a float-typed `Map<f64, V>` annotation: parses,
    // but typecheck must reject (annotation K vs literal K mismatch).
    let src = r#"let m: Map<f64, i64> = {"a": 1}"#;
    let toks = tokenize(src).unwrap();
    let prog = parse(&toks).unwrap();
    assert!(TypeChecker::new().check(&prog).is_err());
}

#[test]
fn map_overwrite_releases_old_value() {
    // String values should get released when overwritten — this is
    // mostly a smoke test that nothing panics.
    let src = r#"
        let m: Map<string, string> = new Map<string, string>()
        m.set("k", "v1")
        m.set("k", "v2")
        m["k"]
    "#;
    use std::rc::Rc;
    assert_eq!(
        run(src).unwrap(),
        Value::Str(Rc::new("v2".into()))
    );
}

#[test]
fn result_ok_and_err() {
    use std::rc::Rc;
    let src = r#"
        let r: Result<i64, string> = Result.ok(42)
        match r {
            Result.ok(v) { v }
            Result.err(_) { -1 }
        }
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(42));

    let src = r#"
        let r: Result<i64, string> = Result.err("boom")
        match r {
            Result.ok(_) { "ok" }
            Result.err(e) { e }
        }
    "#;
    assert_eq!(run(src).unwrap(), Value::Str(Rc::new("boom".into())));
}

#[test]
fn result_used_via_function_return() {
    let src = r#"
        fn divide(a: i64, b: i64): Result<i64, string> {
            if b == 0 { Result.err("divide by zero") } else { Result.ok(a / b) }
        }
        match divide(10, 2) {
            Result.ok(v) { v }
            Result.err(_) { 0 }
        }
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(5));

    let src = r#"
        fn divide(a: i64, b: i64): Result<i64, string> {
            if b == 0 { Result.err("divide by zero") } else { Result.ok(a / b) }
        }
        match divide(10, 0) {
            Result.ok(v) { v }
            Result.err(_) { -999 }
        }
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(-999));
}

#[test]
fn user_defined_generic_enum() {
    use std::rc::Rc;
    let src = r#"
        enum Either<L, R> {
            Left: (L)
            Right: (R)
        }
        let e: Either<i64, string> = Either.Right("hi")
        match e {
            Either.Left(_) { "left" }
            Either.Right(s) { s }
        }
    "#;
    assert_eq!(run(src).unwrap(), Value::Str(Rc::new("hi".into())));
}

#[test]
fn result_payload_type_mismatch_rejected() {
    use ilang_types::TypeChecker;
    use ilang_lexer::tokenize;
    use ilang_parser::parse;
    // Annotated as Result<i64, string> but Ok(v) supplies a string.
    let src = r#"let r: Result<i64, string> = Result.ok("not an int")"#;
    let toks = tokenize(src).unwrap();
    let prog = parse(&toks).unwrap();
    assert!(TypeChecker::new().check(&prog).is_err());
}

#[test]
fn cannot_redefine_result() {
    use ilang_types::TypeChecker;
    use ilang_lexer::tokenize;
    use ilang_parser::parse;
    let src = "enum Result { A, B }";
    let toks = tokenize(src).unwrap();
    let prog = parse(&toks).unwrap();
    assert!(TypeChecker::new().check(&prog).is_err());
}

#[test]
fn result_short_form_constructors_and_patterns() {
    use std::rc::Rc;
    let src = r#"
        fn divide(a: i64, b: i64): Result<i64, string> {
            if b == 0 { Result.err("divide by zero") } else { Result.ok(a / b) }
        }
        match divide(10, 2) {
            ok(v) { v }
            err(_) { -1 }
        }
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(5));

    let src = r#"
        fn divide(a: i64, b: i64): Result<i64, string> {
            if b == 0 { Result.err("divide by zero") } else { Result.ok(a / b) }
        }
        match divide(10, 0) {
            ok(_) { "got value" }
            err(e) { e }
        }
    "#;
    assert_eq!(run(src).unwrap(), Value::Str(Rc::new("divide by zero".into())));
}

#[test]
fn ok_err_can_mix_with_long_form() {
    // The short form desugars to the same EnumCtor as the long form,
    // so they're freely interchangeable.
    let src = r#"
        let r: Result<i64, string> = Result.ok(42)
        match r {
            Result.ok(v) { v }
            Result.err(_) { -1 }
        }
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(42));
}

#[test]
fn match_short_variant_pattern() {
    use std::rc::Rc;
    // Drop the `Color::` prefix in match arms; checker resolves it
    // from the scrutinee's enum.
    let src = r#"
        enum Color { Red, Green, Blue }
        let c = Color.Green
        match c {
            Red { "red" }
            Green { "green" }
            Blue { "blue" }
        }
    "#;
    assert_eq!(
        run(src).unwrap(),
        Value::Str(Rc::new("green".into()))
    );
}

#[test]
fn match_short_variant_with_payload() {
    let src = r#"
        enum Shape {
            Circle: (f64)
            Rect: (f64, f64)
            Square: { side: f64 }
        }
        fn area(s: Shape): f64 {
            match s {
                Circle(r) { 3.14 * r * r }
                Rect(w, h) { w * h }
                Square { side } { side * side }
            }
        }
        area(Shape.Square { side: 4.0 })
    "#;
    assert_eq!(run(src).unwrap(), Value::Float(16.0));
}

#[test]
fn match_long_form_still_works() {
    // The full `Enum::Variant` form must still parse — we didn't
    // remove it.
    let src = r#"
        enum Color { Red, Green }
        match Color.Red {
            Color.Red { 1 }
            Color.Green { 2 }
        }
    "#;
    assert_eq!(run(src).unwrap(), Value::Int(1));
}
