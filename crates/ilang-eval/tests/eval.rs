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
    let src = "#[requires(net)] fn f(x: i64): i64 { x + 1 } f(41)";
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
