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

// ─── ARC Phase B: strings & arrays ─────────────────────────────────────
// No way to peek at refcounts directly, so each test exercises a path
// that would crash (double-free, use-after-free) or leak observably if
// retain/release were misbalanced. Running them under `cargo test` and
// passing is the bar.

#[test]
fn jit_string_concat_in_loop() {
    // Repeated fresh allocations exercise release_string at scope exit.
    let src = r#"
        let i = 0
        let last = ""
        while i < 50 {
            last = "a" + "b"
            i = i + 1
        }
        last
    "#;
    assert_eq!(jit(src), JitValue::Str("ab".into()));
}

#[test]
fn jit_string_param_round_trip() {
    // Aliased `let s` retains the literal (no-op, saturated rc); the
    // function-call retain on its arg matches the callee's exit-release.
    let src = r#"
        fn echo(s: string): string { s }
        let s = "hello"
        echo(s)
    "#;
    assert_eq!(jit(src), JitValue::Str("hello".into()));
}

#[test]
fn jit_string_concat_returned_from_fn() {
    // Fresh-alloc string returned from fn; caller binds it, block-end
    // release frees the concat result.
    let src = r#"
        fn greet(s: string): string { "hi, " + s }
        greet("world")
    "#;
    assert_eq!(jit(src), JitValue::Str("hi, world".into()));
}

#[test]
fn jit_array_in_block_releases() {
    // The inner array is freshly allocated each iteration and goes out
    // of scope at the end of the block. Misbalanced release would
    // double-free (crash) or leak unboundedly (still passes but ARC
    // path is exercised).
    let src = r#"
        let n = 0
        let i = 0
        while i < 100 {
            {
                let xs: i64[] = [1, 2, 3]
                xs.push(4)
                n = n + xs.length
            }
            i = i + 1
        }
        n
    "#;
    assert_eq!(jit(src), JitValue::I64(400));
}

#[test]
fn jit_array_returned_from_fn() {
    // Fresh array allocation crosses a function boundary. Param
    // retain/release for the consumer (none here) plus block-end release
    // on the binding need to balance to rc=1 at observation.
    let src = r#"
        fn make(): i64[] {
            let a: i64[] = [10, 20, 30]
            a
        }
        let a = make()
        a[1]
    "#;
    assert_eq!(jit(src), JitValue::I64(20));
}

#[test]
fn jit_array_param_round_trip() {
    let src = r#"
        fn first(xs: i64[]): i64 { xs[0] }
        let a: i64[] = [7, 8, 9]
        first(a) + first(a)
    "#;
    assert_eq!(jit(src), JitValue::I64(14));
}

#[test]
fn jit_array_push_growth_no_leak_crash() {
    // array_grow_if_full now frees the old buffer; push past initial
    // capacity (4) repeatedly to exercise the realloc path.
    let src = r#"
        let a: i64[] = [1]
        let i = 0
        while i < 50 {
            a.push(i)
            i = i + 1
        }
        a.length
    "#;
    assert_eq!(jit(src), JitValue::I64(51));
}

// ─── ARC Phase C: assignment overwrite + intermediate release ──────────

#[test]
fn jit_field_overwrite_releases_old() {
    // Each Tracked deinit bumps Counter.n. Assigning a new Tracked into
    // the field N times should fire deinit on each replaced value.
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
        class Holder {
            t: Tracked
            init(tt: Tracked) { this.t = tt }
        }
        let counter = new Counter()
        let h = new Holder(new Tracked(counter))
        h.t = new Tracked(counter)
        h.t = new Tracked(counter)
        counter.n
    "#;
    // Two overwrites → two old Tracked instances released → 2 deinits.
    assert_eq!(jit(src), JitValue::I64(2));
}

#[test]
fn jit_local_var_overwrite_releases_old_object() {
    // Reassigning a local Object var releases the previous binding's
    // referent.
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
        let t = new Tracked(counter)
        t = new Tracked(counter)
        t = new Tracked(counter)
        counter.n
    "#;
    // Two overwrites → 2 deinits during the program. A third deinit
    // fires at the top-level `t` release after `counter.n` is read,
    // but counter.n captured 2 before that. Actually the read happens
    // last expression; releases follow. So we observe 2.
    assert_eq!(jit(src), JitValue::I64(2));
}

#[test]
fn jit_discarded_fresh_object_releases() {
    // `new X()` as a discarded statement should not leak — release
    // fires at the statement boundary so deinit runs.
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
        new Tracked(counter)
        new Tracked(counter)
        counter.n
    "#;
    assert_eq!(jit(src), JitValue::I64(2));
}

#[test]
fn jit_string_concat_chain_no_crash() {
    // ("a"+"b") + ("c"+"d") used to leak both inner concats. Now
    // they're released as fresh operands. Loop hard so any double-free
    // would be detected.
    let src = r#"
        let i = 0
        let last = ""
        while i < 100 {
            last = ("a" + "b") + ("c" + "d")
            i = i + 1
        }
        last
    "#;
    assert_eq!(jit(src), JitValue::Str("abcd".into()));
}

#[test]
fn jit_string_field_overwrite_no_crash() {
    let src = r#"
        class Holder {
            s: string
            init(x: string) { this.s = x }
        }
        let h = new Holder("hi")
        h.s = "world"
        h.s = "a" + "b"
        h.s
    "#;
    assert_eq!(jit(src), JitValue::Str("ab".into()));
}

// ─── ARC Phase D: recursive field/element release ──────────────────────

#[test]
fn jit_object_field_recursive_release() {
    // Holder has no deinit, only a Tracked field. When Holder is
    // released, its drop wrapper must release the field, firing
    // Tracked's deinit.
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
        class Holder {
            t: Tracked
            init(tt: Tracked) { this.t = tt }
        }
        let counter = new Counter()
        {
            let h = new Holder(new Tracked(counter))
        }
        counter.n
    "#;
    assert_eq!(jit(src), JitValue::I64(1));
}

#[test]
fn jit_nested_object_chain_release() {
    // Outer → Mid → Tracked. Releasing Outer should chain.
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
        class Mid {
            t: Tracked
            init(tt: Tracked) { this.t = tt }
        }
        class Outer {
            m: Mid
            init(mm: Mid) { this.m = mm }
        }
        let counter = new Counter()
        {
            let o = new Outer(new Mid(new Tracked(counter)))
        }
        counter.n
    "#;
    assert_eq!(jit(src), JitValue::I64(1));
}

#[test]
fn jit_array_of_objects_recursive_release() {
    // Array of Tracked: when the array is released, each element's
    // deinit must fire.
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
            let xs: Tracked[] = [new Tracked(counter), new Tracked(counter), new Tracked(counter)]
        }
        counter.n
    "#;
    assert_eq!(jit(src), JitValue::I64(3));
}

#[test]
fn jit_array_of_strings_no_crash() {
    // Strings inside an array: no crash on array release. (No deinit
    // counter since strings have none — just exercise the path.)
    let src = r#"
        let i = 0
        while i < 50 {
            {
                let xs: string[] = ["a" + "b", "c" + "d", "e" + "f"]
            }
            i = i + 1
        }
        i
    "#;
    assert_eq!(jit(src), JitValue::I64(50));
}

#[test]
fn jit_string_field_release_on_drop() {
    // Holder.s = fresh concat. When Holder drops, the string is
    // released. Loop hard so any double-free shows up.
    let src = r#"
        class Holder {
            s: string
            init(x: string) { this.s = x }
        }
        let i = 0
        while i < 50 {
            {
                let h = new Holder("a" + "b")
            }
            i = i + 1
        }
        i
    "#;
    assert_eq!(jit(src), JitValue::I64(50));
}

#[test]
fn jit_returning_fresh_object_balances() {
    // `fn f(): Foo { new Foo() }` previously over-retained tail.
    // Caller binds the result and the scope-end release should free it.
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
        fn make(c: Counter): Tracked { new Tracked(c) }
        let counter = new Counter()
        {
            let t = make(counter)
        }
        counter.n
    "#;
    // t goes out of scope inside the inner block → 1 deinit.
    assert_eq!(jit(src), JitValue::I64(1));
}

// ─── Phase E-1b: Optional in JIT ──────────────────────────────────────

#[test]
fn jit_optional_string_some_some() {
    let src = r#"
        let x: string? = some("hello")
        if let some(s) = x {
            s
        } else {
            "missing"
        }
    "#;
    assert_eq!(jit(src), JitValue::Str("hello".into()));
}

#[test]
fn jit_optional_string_none_takes_else() {
    let src = r#"
        let x: string? = none
        if let some(s) = x {
            s
        } else {
            "missing"
        }
    "#;
    assert_eq!(jit(src), JitValue::Str("missing".into()));
}

#[test]
fn jit_optional_predicates() {
    let src = r#"
        let a: string? = some("yes")
        let b: string? = none
        let r1 = if a.is_some() { 1 } else { 0 }
        let r2 = if b.is_none() { 10 } else { 0 }
        r1 + r2
    "#;
    assert_eq!(jit(src), JitValue::I64(11));
}

#[test]
fn jit_optional_field_recursive_release() {
    // Holder.t is a Tracked? field. When Holder drops, the field's
    // release fires, which (when Some) bumps the counter via deinit.
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
        class Holder {
            t: Tracked?
            init() { this.t = none }
        }
        let counter = new Counter()
        {
            let h = new Holder()
            h.t = some(new Tracked(counter))
        }
        counter.n
    "#;
    assert_eq!(jit(src), JitValue::I64(1));
}

#[test]
fn jit_optional_field_none_no_crash() {
    let src = r#"
        class Holder {
            s: string?
            init() { this.s = none }
        }
        let h = new Holder()
        if h.s.is_none() { 42 } else { -1 }
    "#;
    assert_eq!(jit(src), JitValue::I64(42));
}

// ─── Phase E-2b: Weak references in JIT ──────────────────────────────

#[test]
fn jit_weak_get_some_when_alive() {
    let src = r#"
        class Counter {
            n: i64
            init() { this.n = 7 }
        }
        let c = new Counter()
        let w: Counter.weak = c
        if let some(s) = w.get() {
            s.n
        } else {
            -1
        }
    "#;
    assert_eq!(jit(src), JitValue::I64(7));
}

#[test]
fn jit_weak_get_none_after_strong_dropped() {
    // The Counter is allocated and only weakly referenced; with no
    // strong binding, its strong_rc reaches 0 immediately and the
    // weak's get() returns none.
    let src = r#"
        class Counter {
            n: i64
            init() { this.n = 0 }
        }
        let r = 0
        let w: Counter.weak = new Counter()
        if w.get().is_none() {
            r = 42
        }
        r
    "#;
    assert_eq!(jit(src), JitValue::I64(42));
}

#[test]
fn jit_weak_breaks_cycle() {
    let src = r#"
        class Counter {
            n: i64
            init() { this.n = 0 }
            inc() { n = n + 1 }
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
    assert_eq!(jit(src), JitValue::I64(1));
}

#[test]
fn jit_console_log_array_object_optional_weak() {
    // Just verify these compile + run without error. Output is not
    // captured by the test harness; the body's tail evaluates to a
    // plain integer for the assertion.
    let src = r#"
        class Foo {
            x: i64
            init() { this.x = 7 }
        }
        let xs: i32[] = [1, 2, 3]
        let s: string[] = ["a", "b"]
        let opt: string? = some("yes")
        let nope: string? = none
        let f = new Foo()
        let w: Foo.weak = f
        console.log("xs:", xs, xs.length)
        console.log("strs:", s)
        console.log("opt:", opt, "none:", nope)
        console.log("obj:", f, "weak:", w)
        42
    "#;
    assert_eq!(jit(src), JitValue::I64(42));
}

// ─── return statement ────────────────────────────────────────────────

#[test]
fn jit_return_early_from_fn() {
    let src = r#"
        fn abs(n: i64): i64 {
            if n < 0 { return -n }
            n
        }
        abs(-7) + abs(3)
    "#;
    assert_eq!(jit(src), JitValue::I64(10));
}

#[test]
fn jit_return_unit_fn() {
    // `return` with no value in a Unit-returning function. We use a
    // shared Counter instead of console.log because globals like
    // `console` aren't visible inside fn bodies (a separate
    // pre-existing limitation).
    let src = r#"
        class Counter {
            n: i64
            init() { this.n = 0 }
            inc() { n = n + 1 }
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
    assert_eq!(jit(src), JitValue::I64(2));
}

#[test]
fn jit_return_from_method_runs_deinit() {
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
        fn run_once(c: Counter): i64 {
            let _t = new Tracked(c)
            return 99
        }
        let counter = new Counter()
        run_once(counter)
        counter.n
    "#;
    assert_eq!(jit(src), JitValue::I64(1));
}

#[test]
fn jit_return_aliased_object() {
    // Returning a borrowed object: the function-exit path retains the
    // value to give the caller +1, then releases all params/bindings.
    let src = r#"
        class Counter {
            n: i64
            init() { this.n = 7 }
        }
        fn pick(c: Counter): Counter {
            return c
        }
        let c = new Counter()
        pick(c).n
    "#;
    assert_eq!(jit(src), JitValue::I64(7));
}

// ─── enum + match (Phase 1) ───────────────────────────────────────────

#[test]
fn jit_enum_unit_construct_and_match() {
    let src = r#"
        enum Color { Red, Green, Blue }
        let c = Color::Green
        match c {
            Color::Red => 1
            Color::Green => 2
            Color::Blue => 3
        }
    "#;
    assert_eq!(jit(src), JitValue::I64(2));
}

#[test]
fn jit_enum_match_wildcard() {
    let src = r#"
        enum Day { Mon, Tue, Wed, Thu, Fri, Sat, Sun }
        let d = Day::Sat
        match d {
            Day::Sat => 1
            Day::Sun => 1
            _ => 0
        }
    "#;
    assert_eq!(jit(src), JitValue::I64(1));
}

#[test]
fn jit_enum_returned_as_value() {
    let src = r#"
        enum Color { Red, Green, Blue }
        Color::Blue
    "#;
    assert_eq!(
        jit(src),
        JitValue::Enum {
            ty: "Color".into(),
            variant: "Blue".into(),
            payload: ilang_codegen::JitEnumPayload::Unit,
        }
    );
}

// ─── enum payloads (Phase 2) ─────────────────────────────────────────

#[test]
fn jit_enum_tuple_payload() {
    let src = r#"
        enum Shape {
            Circle(f64)
            Rect(f64, f64)
        }
        fn area(s: Shape): f64 {
            match s {
                Shape::Circle(r) => 3.14 * r * r
                Shape::Rect(w, h) => w * h
            }
        }
        area(Shape::Rect(3.0, 4.0))
    "#;
    assert_eq!(jit(src), JitValue::F64(12.0));
}

#[test]
fn jit_enum_struct_payload() {
    let src = r#"
        enum Pt {
            Origin
            At { x: i64, y: i64 }
        }
        fn sumxy(p: Pt): i64 {
            match p {
                Pt::Origin => 0
                Pt::At { x, y } => x + y
            }
        }
        sumxy(Pt::At { x: 3, y: 4 })
    "#;
    assert_eq!(jit(src), JitValue::I64(7));
}

#[test]
fn jit_enum_payload_runs_deinit() {
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
        enum Wrap { Has(Tracked), Empty }
        let counter = new Counter()
        {
            let _w = Wrap::Has(new Tracked(counter))
        }
        counter.n
    "#;
    assert_eq!(jit(src), JitValue::I64(1));
}
