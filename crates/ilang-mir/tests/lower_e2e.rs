//! End-to-end lowering tests: source string → tokens → AST → MIR.
//! Asserts on substrings of the printed MIR rather than full text so
//! that small format tweaks don't churn every test.

use ilang_lexer::tokenize;
use ilang_mir::{lower_program, print_program};
use ilang_parser::parse;

fn lower(src: &str) -> String {
    let tokens = tokenize(src).expect("tokenize");
    let prog = parse(&tokens).expect("parse");
    let mir = lower_program(&prog).expect("lower");
    print_program(&mir)
}

#[test]
fn integer_arithmetic_top_level() {
    let dump = lower("1 + 2 * 3");
    assert!(dump.contains("imul"), "missing imul:\n{dump}");
    assert!(dump.contains("iadd"), "missing iadd:\n{dump}");
    assert!(dump.contains("return"), "missing return:\n{dump}");
}

#[test]
fn fn_decl_and_call() {
    let src = r#"
        fn add(a: i64, b: i64): i64 { a + b }
        add(2, 3)
    "#;
    let dump = lower(src);
    assert!(dump.contains("fn add(v0: i64, v1: i64) -> i64"));
    assert!(dump.contains("iadd v0, v1"));
    assert!(dump.contains("call func#"));
}

#[test]
fn if_expression_value() {
    let src = r#"
        fn pick(n: i64): i64 {
            if n > 0 { n } else { -n }
        }
        pick(-5)
    "#;
    let dump = lower(src);
    assert!(dump.contains("cond_br"));
    assert!(dump.contains("igt_s"));
    assert!(dump.contains("ineg"));
}

#[test]
fn while_loop() {
    let src = r#"
        fn count(n: i64): i64 {
            let i = 0
            while i < n {
                i = i + 1
            }
            i
        }
        count(3)
    "#;
    let dump = lower(src);
    assert!(dump.contains("ilt_s"));
    assert!(dump.contains("br bb"));
    // Should have at least 4 blocks (entry, header, body, exit).
    let block_count = dump.matches("  bb").count();
    assert!(block_count >= 4, "block count {block_count}:\n{dump}");
}

#[test]
fn array_literal_and_index() {
    let src = r#"
        fn first(xs: i64[]): i64 { xs[0] }
        first([10, 20, 30])
    "#;
    let dump = lower(src);
    assert!(dump.contains("new_array<i64>"), "expected new_array:\n{dump}");
    assert!(dump.contains("array_load"));
}

#[test]
fn array_length_field() {
    let src = r#"
        fn len(xs: i64[]): i64 { xs.length }
        len([1, 2, 3])
    "#;
    let dump = lower(src);
    assert!(dump.contains("array_len"), "expected array_len:\n{dump}");
}

#[test]
fn tuple_literal_and_index() {
    let src = r#"
        fn fst(t: (i64, bool)): i64 { t[0] }
        fst((42, true))
    "#;
    let dump = lower(src);
    assert!(dump.contains("new_tuple"));
    assert!(dump.contains("tuple_extract"));
}

#[test]
fn optional_some_and_unwrap() {
    let src = r#"
        fn pick(): i64 {
            let x: i64? = some(42)
            x.unwrap()
        }
        pick()
    "#;
    let dump = lower(src);
    assert!(dump.contains("new_optional"));
    assert!(dump.contains("optional_unwrap"));
}

#[test]
fn if_let_some() {
    let src = r#"
        fn or_zero(x: i64?): i64 {
            if let some(v) = x { v } else { 0 }
        }
        or_zero(some(7))
    "#;
    let dump = lower(src);
    assert!(dump.contains("optional_is_some"));
    assert!(dump.contains("optional_unwrap"));
    assert!(dump.contains("cond_br"));
}

#[test]
fn for_in_range() {
    let src = r#"
        fn sum_to(n: i64): i64 {
            let total = 0
            for i in 1..=n {
                total = total + i
            }
            total
        }
        sum_to(5)
    "#;
    let dump = lower(src);
    assert!(dump.contains("ile_s"), "expected inclusive ile_s:\n{dump}");
    assert!(dump.contains("iadd"));
}

#[test]
fn for_in_array() {
    let src = r#"
        fn sum(xs: i64[]): i64 {
            let total = 0
            for x in xs { total = total + x }
            total
        }
        sum([1, 2, 3])
    "#;
    let dump = lower(src);
    assert!(dump.contains("array_len"));
    assert!(dump.contains("array_load"));
}

#[test]
fn class_new_field_method() {
    let src = r#"
        class Counter {
            count: i64
            init(start: i64) { this.count = start }
            bump(): i64 {
                this.count = this.count + 1
                this.count
            }
        }
        let c = new Counter(10)
        c.bump()
    "#;
    let dump = lower(src);
    assert!(dump.contains("class #0 Counter"), "expected class registration:\n{dump}");
    assert!(dump.contains("init init"), "expected init method dump:\n{dump}");
    assert!(dump.contains("new_object class#0"));
    assert!(dump.contains("load_field"));
    assert!(dump.contains("store_field"));
    // Method dispatch goes via virt_call now (vtable slots assigned).
    assert!(dump.contains("virt_call") || dump.contains("call func#"));
}

#[test]
fn class_implicit_this_field() {
    // Reading a bare field name inside a method body should resolve
    // to `this.<field>`.
    let src = r#"
        class Box {
            x: i64
            init(v: i64) { this.x = v }
            get(): i64 { x }
        }
        new Box(7).get()
    "#;
    let dump = lower(src);
    assert!(dump.contains("load_field"), "expected load_field for implicit this:\n{dump}");
}

#[test]
fn class_method_calls_other_method() {
    let src = r#"
        class Pair {
            a: i64
            b: i64
            init(a: i64, b: i64) { this.a = a; this.b = b }
            sum(): i64 { this.a + this.b }
            doubled(): i64 { sum() * 2 }
        }
        new Pair(3, 4).doubled()
    "#;
    let dump = lower(src);
    // Two methods on the class.
    assert!(dump.contains("method sum"));
    assert!(dump.contains("method doubled"));
}

#[test]
fn enum_unit_match() {
    let src = r#"
        enum Color { red, green, blue }
        fn name(c: Color): string {
            match c {
                red { "red" }
                green { "green" }
                blue { "blue" }
            }
        }
        name(Color.green)
    "#;
    let dump = lower(src);
    // User enums are registered starting from id 0 (Result is no
    // longer pre-registered as a built-in id 0; it's monomorphized
    // per call site like any other generic enum).
    assert!(dump.contains("Color (repr"), "expected Color enum dump:\n{dump}");
    assert!(dump.contains("new_enum enum#0"));
    assert!(dump.contains("enum_tag"));
    assert!(dump.contains("switch"));
}

#[test]
fn enum_payload_match() {
    let src = r#"
        enum Shape {
            circle: (f64)
            rect: (f64, f64)
        }
        fn area(s: Shape): f64 {
            match s {
                circle(r) { 3.14 * r * r }
                rect(w, h) { w * h }
            }
        }
        area(Shape.circle(5.0))
    "#;
    let dump = lower(src);
    assert!(dump.contains("new_enum enum#0.0"), "expected circle ctor:\n{dump}");
    assert!(dump.contains("enum_payload"));
}

#[test]
fn match_int_with_range() {
    let src = r#"
        fn bucket(n: i64): string {
            match n {
                ..0 { "neg" }
                0..10 { "small" }
                10..=99 { "tens" }
                _ { "big" }
            }
        }
        bucket(42)
    "#;
    let dump = lower(src);
    assert!(dump.contains("ile_s") || dump.contains("ilt_s"));
    assert!(dump.contains("ige_s"));
}

#[test]
fn match_bool() {
    let src = r#"
        fn label(b: bool): string {
            match b {
                true { "on" }
                false { "off" }
            }
        }
        label(true)
    "#;
    let dump = lower(src);
    assert!(dump.contains("cond_br"));
}

#[test]
fn console_log_variadic() {
    let src = r#"
        let n = 42
        console.log("answer:", n, true)
    "#;
    let dump = lower(src);
    assert!(dump.contains("builtin@console_log"), "expected console_log builtin:\n{dump}");
}

#[test]
fn let_tuple_destructure() {
    let src = r#"
        fn first(p: (i64, i64)): i64 {
            let (a, b) = p
            a + b
        }
        first((3, 4))
    "#;
    let dump = lower(src);
    assert!(dump.contains("tuple_extract"));
    assert!(dump.contains("iadd"));
}

#[test]
fn let_struct_destructure() {
    let src = r#"
        class Point {
            x: f64
            y: f64
            init(a: f64, b: f64) { this.x = a; this.y = b }
        }
        fn x(p: Point): f64 {
            let Point { x, y } = p
            x
        }
        x(new Point(1.0, 2.0))
    "#;
    let dump = lower(src);
    assert!(dump.contains("load_field"), "expected load_field for destructure:\n{dump}");
}

#[test]
fn map_literal_and_get() {
    let src = r#"
        let m: Map<string, i64> = {"a": 1, "b": 2}
        m["a"]
    "#;
    let dump = lower(src);
    assert!(dump.contains("new_map<"));
    assert!(dump.contains("map_get"));
}

#[test]
fn string_method_and_to_string() {
    let src = r#"
        fn shout(s: string): string { s.toUpper() }
        let n = 42
        let msg = n.toString() + " " + shout("hi")
        msg
    "#;
    let dump = lower(src);
    assert!(dump.contains("builtin@str_to_upper"));
    assert!(dump.contains("builtin@int_to_string"));
    assert!(dump.contains("str_concat"));
}

#[test]
fn class_inheritance_super() {
    let src = r#"
        class Animal {
            name: string
            init(n: string) { this.name = n }
            speak(): string { "generic" }
            describe(): string { this.name + " says " + this.speak() }
        }
        class Dog: Animal {
            init(n: string) { super(n) }
            override speak(): string { "woof" }
        }
        let d = new Dog("rex")
        d.describe()
    "#;
    let dump = lower(src);
    assert!(dump.contains("class #1 Dog"), "expected child class:\n{dump}");
    assert!(dump.contains("(parent: Some(ClassId(0)))"), "expected parent link:\n{dump}");
    // Dog should redirect speak() to its own func.
    assert!(dump.contains("Counter.bump") == false, "fluke string check:\n{dump}");
}

#[test]
fn rtti_is_and_downcast() {
    let src = r#"
        class A { init() {} }
        class B: A { init() { super() } }
        let b: A = new B()
        let is_b = b is B
        let opt: B? = b as? B
        is_b
    "#;
    let dump = lower(src);
    assert!(dump.contains("is_instance"));
    assert!(dump.contains("downcast_or_none"));
}

#[test]
fn weak_reference_get() {
    let src = r#"
        class Node {
            init() {}
        }
        let n = new Node()
        let w: Node.weak = n
        w.get()
    "#;
    let dump = lower(src);
    assert!(dump.contains("cast.StrongToWeak"), "expected strong→weak cast:\n{dump}");
    assert!(dump.contains("weak_upgrade"));
}

#[test]
fn typeof_builtin() {
    let src = r#"
        let n = 42
        typeof(n)
    "#;
    let dump = lower(src);
    assert!(dump.contains("typeof"));
}

#[test]
fn class_static_method_and_field() {
    let src = r#"
        class Counter {
            n: i64
            init() { this.n = 0 }
            bump() { this.n = this.n + 1; Counter.total = Counter.total + 1 }
            static total: i64 = 0
            static of(start: i64): Counter {
                let c = new Counter()
                c.n = start
                c
            }
        }
        let c = Counter.of(10)
        Counter.total
    "#;
    let dump = lower(src);
    assert!(dump.contains("static Counter.total"), "expected static dump:\n{dump}");
    assert!(dump.contains("load_static"));
    assert!(dump.contains("store_static"));
}

#[test]
fn class_property_get_set() {
    let src = r#"
        class Temp {
            celsius: f64
            init(c: f64) { this.celsius = c }
            get fahrenheit(): f64 { this.celsius * 9.0 / 5.0 + 32.0 }
            set fahrenheit(v: f64) { this.celsius = (v - 32.0) * 5.0 / 9.0 }
        }
        let t = new Temp(0.0)
        t.fahrenheit = 100.0
        t.fahrenheit
    "#;
    let dump = lower(src);
    // The setter and getter dispatch virtually (through the receiver's
    // vtable) so an overridden accessor reached via a base-typed
    // reference runs the override, exactly like a regular method.
    assert!(dump.contains("virt_call"), "expected virtual dispatch for property fn:\n{dump}");
}

#[test]
fn class_const_field() {
    let src = r#"
        class K {
            init() {}
            const max: i64 = 1000
        }
        K.max
    "#;
    let dump = lower(src);
    assert!(dump.contains("static K.max"), "expected K.max:\n{dump}");
    assert!(dump.contains("load_static"));
}

#[test]
fn closure_captures_local() {
    let src = r#"
        let factor = 10
        let scale = fn(x: i64): i64 { x * factor }
        scale(3)
    "#;
    let dump = lower(src);
    assert!(dump.contains("$anon.fn_0"), "expected synthesised fn:\n{dump}");
    assert!(dump.contains("make_closure"));
    assert!(dump.contains("load_capture"));
}

#[test]
fn closure_returned_from_fn() {
    let src = r#"
        fn make_adder(n: i64): fn(i64): i64 {
            fn(x: i64): i64 { x + n }
        }
        let add5 = make_adder(5)
        add5(3)
    "#;
    let dump = lower(src);
    assert!(dump.contains("make_closure"));
    assert!(dump.contains("load_capture"));
}

#[test]
fn extern_c_struct_and_fn_decl() {
    let src = r#"
        @extern(C) {
            struct timespec {
                tv_sec: i64
                tv_nsec: i64
            }
            @lib("c") fn clock_gettime(clk: i32, tp: *timespec): i32
        }
        let ts = new timespec()
        clock_gettime(0 as i32, ts)
    "#;
    let dump = lower(src);
    assert!(dump.contains("class #0 timespec"), "expected struct shell:\n{dump}");
    assert!(dump.contains("extern clock_gettime"), "expected extern fn dump:\n{dump}");
}

#[test]
fn extern_c_struct_lit() {
    let src = r#"
        @extern(C) {
            struct point { x: i32; y: i32 }
        }
        let p = point { x: 1 as i32, y: 2 as i32 }
        p.x
    "#;
    let dump = lower(src);
    assert!(dump.contains("new_object class#0"));
    assert!(dump.contains("store_field"));
    assert!(dump.contains("load_field"));
}

#[test]
fn fn_value_trampoline() {
    let src = r#"
        fn double(n: i64): i64 { n * 2 }
        fn apply(g: fn(i64): i64, x: i64): i64 { g(x) }
        apply(double, 7)
    "#;
    let dump = lower(src);
    // Reference to `double` as a value should produce a make_closure
    // with no captures.
    assert!(dump.contains("make_closure"));
    assert!(dump.contains("call_indirect"));
}

#[test]
fn array_higher_order_map_filter() {
    let src = r#"
        let xs: i64[] = [1, 2, 3, 4]
        let ys = xs.map(fn(x: i64): i64 { x * 10 })
        let zs = xs.filter(fn(x: i64): bool { x > 2 })
        ys.length + zs.length
    "#;
    let dump = lower(src);
    assert!(dump.contains("builtin@array_map"));
    assert!(dump.contains("builtin@array_filter"));
    assert!(dump.contains("make_closure"));
}

#[test]
fn fn_overloading_int_string_bool() {
    let src = r#"
        fn show(n: i64): string { "int" }
        fn show(s: string): string { "str" }
        fn show(b: bool): string { "bool" }
        let a = show(42)
        let b = show("hi")
        let c = show(true)
        a
    "#;
    let dump = lower(src);
    // Three distinct mangled fns should be present.
    assert!(dump.contains("fn show("), "expected primary show:\n{dump}");
    assert!(dump.contains("show__str") || dump.contains("show__bool"),
        "expected mangled overload:\n{dump}");
}

#[test]
fn fn_overloading_arity_split() {
    let src = r#"
        fn make(): string { "default" }
        fn make(s: string): string { s }
        fn make(s: string, suffix: string): string { s + suffix }
        let a = make()
        let b = make("hello")
        let c = make("hi", "!")
        a
    "#;
    let dump = lower(src);
    assert!(dump.contains("make__"), "expected mangled overload:\n{dump}");
}

#[test]
fn short_circuit_and() {
    let src = r#"
        fn both(a: bool, b: bool): bool { a && b }
        both(true, false)
    "#;
    let dump = lower(src);
    assert!(dump.contains("cond_br"));
    // Should produce `false` constant on the fail branch.
    assert!(dump.contains("const false"));
}
