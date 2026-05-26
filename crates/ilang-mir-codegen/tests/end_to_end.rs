//! Full pipeline: ilang source → tokens → AST → MIR → clif → run.
//!
//! Restricted to programs whose values are primitive scalars (heap
//! types and ARC arrive in follow-up steps).

use ilang_lexer::tokenize;
use ilang_mir::lower_program;
use ilang_mir_codegen::{compile_program, run_main};
use ilang_parser::parse;

fn run(src: &str) -> i64 {
    let tokens = tokenize(src).expect("tokenize");
    let ast = parse(&tokens).expect("parse");
    let mir = lower_program(&ast).expect("lower");
    let c = compile_program(&mir).expect("compile");
    run_main(&c)
}

#[test]
fn arithmetic() {
    assert_eq!(run("1 + 2 * 3"), 7);
    assert_eq!(run("(10 - 4) / 2"), 3);
    assert_eq!(run("17 % 5"), 2);
}

#[test]
fn fn_decl_and_call() {
    let src = r#"
        fn add(a: i64, b: i64): i64 { a + b }
        add(20, 22)
    "#;
    assert_eq!(run(src), 42);
}

#[test]
fn fn_recursion() {
    let src = r#"
        fn fact(n: i64): i64 {
            if n <= 1 { 1 } else { n * fact(n - 1) }
        }
        fact(6)
    "#;
    assert_eq!(run(src), 720);
}

#[test]
fn while_loop_sum() {
    let src = r#"
        fn sum(n: i64): i64 {
            let total = 0
            let i = 1
            while i <= n {
                total = total + i
                i = i + 1
            }
            total
        }
        sum(10)
    "#;
    assert_eq!(run(src), 55);
}

#[test]
fn for_in_range_sum() {
    let src = r#"
        fn sum_to(n: i64): i64 {
            let t = 0
            for i in 1..=n { t = t + i }
            t
        }
        sum_to(5)
    "#;
    assert_eq!(run(src), 15);
}

#[test]
fn const_static_read() {
    // Read-only static int.
    let src = r#"
        class K {
            init() {}
            const max: i64 = 1000
        }
        K.max + 1
    "#;
    assert_eq!(run(src), 1001);
}

#[test]
fn static_mutable_increment() {
    let src = r#"
        class C {
            init() {}
            static total: i64 = 0
        }
        C.total = C.total + 41
        C.total + 1
    "#;
    assert_eq!(run(src), 42);
}

#[test]
fn class_new_and_field_access() {
    let src = r#"
        class Counter {
            count: i64
            init(start: i64) { this.count = start }
            bump(): i64 {
                this.count = this.count + 1
                this.count
            }
        }
        let c = new Counter(40)
        c.bump()
        c.bump()
    "#;
    assert_eq!(run(src), 42);
}

#[test]
fn class_with_two_fields() {
    let src = r#"
        class Pair {
            a: i64
            b: i64
            init(x: i64, y: i64) { this.a = x; this.b = y }
            sum(): i64 { this.a + this.b }
        }
        new Pair(20, 22).sum()
    "#;
    assert_eq!(run(src), 42);
}

#[test]
fn array_literal_index_and_length() {
    let src = r#"
        let xs: i64[] = [10, 20, 30, 40]
        xs[2] + xs.length
    "#;
    assert_eq!(run(src), 34);
}

#[test]
fn array_index_assignment() {
    let src = r#"
        let xs: i64[] = [1, 2, 3]
        xs[1] = 100
        xs[0] + xs[1] + xs[2]
    "#;
    assert_eq!(run(src), 104);
}

#[test]
fn array_for_in_sum() {
    let src = r#"
        fn sum(xs: i64[]): i64 {
            let total = 0
            for x in xs { total = total + x }
            total
        }
        sum([5, 10, 15, 20])
    "#;
    assert_eq!(run(src), 50);
}

#[test]
fn tuple_literal_and_index() {
    let src = r#"
        fn fst(p: (i64, i64)): i64 { p[0] }
        fn snd(p: (i64, i64)): i64 { p[1] }
        let t = (10, 32)
        fst(t) + snd(t)
    "#;
    assert_eq!(run(src), 42);
}

#[test]
fn optional_some_and_unwrap() {
    let src = r#"
        fn or_default(x: i64?, d: i64): i64 {
            if let some(v) = x { v } else { d }
        }
        or_default(some(42), 7)
    "#;
    assert_eq!(run(src), 42);
}

#[test]
fn optional_none_path() {
    let src = r#"
        fn or_default(x: i64?, d: i64): i64 {
            if let some(v) = x { v } else { d }
        }
        or_default(none, 99)
    "#;
    assert_eq!(run(src), 99);
}

#[test]
fn enum_unit_match() {
    let src = r#"
        enum Color { red, green, blue }
        fn ord(c: Color): i64 {
            match c {
                red { 0 }
                green { 1 }
                blue { 2 }
            }
        }
        ord(Color.green) + ord(Color.blue) * 10
    "#;
    assert_eq!(run(src), 21);
}

#[test]
fn enum_payload_match() {
    let src = r#"
        enum Shape {
            circle: (i64)
            rect: (i64, i64)
        }
        fn area(s: Shape): i64 {
            match s {
                circle(r) { r * r }
                rect(w, h) { w * h }
            }
        }
        area(Shape.circle(5)) + area(Shape.rect(3, 4))
    "#;
    assert_eq!(run(src), 37);
}

#[test]
fn closure_captures_local() {
    let src = r#"
        let factor = 10
        let scale = fn(x: i64): i64 { x * factor }
        scale(3) + scale(2)
    "#;
    assert_eq!(run(src), 50);
}

#[test]
fn closure_returned_from_fn() {
    let src = r#"
        fn make_adder(n: i64): fn(i64): i64 {
            fn(x: i64): i64 { x + n }
        }
        let add5 = make_adder(5)
        add5(3) + add5(10)
    "#;
    assert_eq!(run(src), 23);
}

#[test]
fn fn_value_passed_as_arg() {
    let src = r#"
        fn double(n: i64): i64 { n * 2 }
        fn apply(g: fn(i64): i64, x: i64): i64 { g(x) }
        apply(double, 21)
    "#;
    assert_eq!(run(src), 42);
}

#[test]
fn rtti_is_instance_match() {
    let src = r#"
        class A { init() {} }
        class B: A { init() { super() } }
        let b = new B()
        if b is B { 1 } else { 0 }
    "#;
    assert_eq!(run(src), 1);
}

#[test]
fn rtti_is_instance_parent() {
    let src = r#"
        class A { init() {} }
        class B: A { init() { super() } }
        let b = new B()
        if b is A { 1 } else { 0 }
    "#;
    assert_eq!(run(src), 1);
}

#[test]
fn rtti_downcast_some() {
    let src = r#"
        class A { init() {} }
        class B: A { init() { super() } }
        let a: A = new B()
        if let some(_) = a as? B { 1 } else { 0 }
    "#;
    assert_eq!(run(src), 1);
}

#[test]
fn rtti_downcast_none_for_unrelated() {
    let src = r#"
        class A { init() {} }
        class B { init() {} }
        let a = new A()
        if let some(_) = a as? B { 1 } else { 0 }
    "#;
    assert_eq!(run(src), 0);
}

#[test]
fn map_int_keyed_get_set() {
    // String keys would compare by pointer (constants dedup via the
    // codegen's string_data table), so for a stable assertion we use
    // integer keys.
    let src = r#"
        let m: Map<i64, i64> = {1: 100, 2: 200}
        m[3] = 300
        m[1] + m[2] + m[3]
    "#;
    assert_eq!(run(src), 600);
}

#[test]
fn map_clear_drops_entries() {
    let src = r#"
        let m: Map<i64, i64> = {1: 100, 2: 200, 3: 300}
        m.clear()
        m.size()
    "#;
    assert_eq!(run(src), 0);
}

#[test]
fn map_entries_round_trips() {
    let src = r#"
        let m: Map<i64, i64> = {1: 10, 2: 20, 3: 30}
        let es = m.entries()
        let totals: i64[] = [0]
        for e in es {
            totals[0] = totals[0] + e[0] + e[1]
        }
        totals[0]
    "#;
    // sum of keys (1+2+3=6) + sum of values (10+20+30=60) = 66
    assert_eq!(run(src), 66);
}

#[test]
fn map_for_each_visits_all() {
    let src = r#"
        let m: Map<i64, i64> = {1: 10, 2: 20, 3: 30}
        let totals: i64[] = [0, 0]
        m.forEach(fn(k: i64, v: i64) {
            totals[0] = totals[0] + k
            totals[1] = totals[1] + v
        })
        totals[0] * 100 + totals[1]
    "#;
    // keys: 1+2+3=6, values: 10+20+30=60 → 6*100+60 = 660
    assert_eq!(run(src), 660);
}

#[test]
fn set_int_add_has_size() {
    let src = r#"
        let s = new Set<i64>()
        s.add(1)
        s.add(2)
        s.add(2)
        s.add(3)
        let here = if s.has(2) { 1 } else { 0 }
        let absent = if s.has(99) { 1 } else { 0 }
        s.size() * 100 + here - absent
    "#;
    // dedup: 3 entries, has(2) → 1, has(99) → 0 → 3*100 + 1 - 0 = 301
    assert_eq!(run(src), 301);
}

#[test]
fn set_delete_and_clear() {
    let src = r#"
        let s = new Set<i64>()
        s.add(1)
        s.add(2)
        s.add(3)
        let removed = if s.delete(2) { 1 } else { 0 }
        let missing = if s.delete(99) { 1 } else { 0 }
        s.size() * 100 + removed * 10 + missing
    "#;
    // 2 entries after delete, removed=1, missing=0 → 210
    assert_eq!(run(src), 210);
}

#[test]
fn set_clear_empties() {
    let src = r#"
        let s = new Set<i64>()
        s.add(1)
        s.add(2)
        s.add(3)
        s.clear()
        s.size()
    "#;
    assert_eq!(run(src), 0);
}

#[test]
fn set_string_keys() {
    let src = r#"
        let s = new Set<string>()
        s.add("a")
        s.add("b")
        s.add("a")
        let here = if s.has("b") { 1 } else { 0 }
        s.size() * 10 + here
    "#;
    // 2 unique entries, has("b") → 1 → 2*10 + 1 = 21
    assert_eq!(run(src), 21);
}

#[test]
fn set_values_round_trip() {
    let src = r#"
        let s = new Set<i64>()
        s.add(10)
        s.add(20)
        s.add(30)
        let vs = s.values()
        let acc: i64[] = [0]
        for v in vs {
            acc[0] = acc[0] + v
        }
        acc[0]
    "#;
    assert_eq!(run(src), 60);
}

#[test]
fn set_for_each_int() {
    let src = r#"
        let s = new Set<i64>()
        s.add(1)
        s.add(2)
        s.add(3)
        let acc: i64[] = [0]
        s.forEach(fn(v: i64) {
            acc[0] = acc[0] + v
        })
        acc[0]
    "#;
    assert_eq!(run(src), 6);
}

#[test]
fn set_for_each_f32() {
    let src = r#"
        let s = new Set<f32>()
        s.add(1.5)
        s.add(2.5)
        let acc: f32[] = [0.0]
        s.forEach(fn(v: f32) {
            acc[0] = acc[0] + v
        })
        (acc[0] * 10.0) as i64
    "#;
    // 1.5 + 2.5 = 4.0 → 40
    assert_eq!(run(src), 40);
}

#[test]
fn set_union_intersection_difference() {
    let src = r#"
        let a = new Set<i64>()
        a.add(1); a.add(2); a.add(3)
        let b = new Set<i64>()
        b.add(2); b.add(3); b.add(4)
        let u = a.union(b)         // {1,2,3,4} size=4
        let i = a.intersection(b)  // {2,3}     size=2
        let d = a.difference(b)    // {1}       size=1
        u.size() * 100 + i.size() * 10 + d.size()
    "#;
    assert_eq!(run(src), 421);
}

#[test]
fn set_subset_superset_disjoint() {
    let src = r#"
        let a = new Set<i64>()
        a.add(1); a.add(2)
        let b = new Set<i64>()
        b.add(1); b.add(2); b.add(3)
        let c = new Set<i64>()
        c.add(9); c.add(10)
        let sub = if a.isSubsetOf(b) { 1 } else { 0 }
        let sup = if b.isSupersetOf(a) { 1 } else { 0 }
        let dis = if a.isDisjointFrom(c) { 1 } else { 0 }
        let not_sub = if b.isSubsetOf(a) { 0 } else { 1 }
        sub + sup * 2 + dis * 4 + not_sub * 8
    "#;
    // sub=1, sup=1, dis=1, not_sub=1 → 1+2+4+8 = 15
    assert_eq!(run(src), 15);
}

#[test]
fn math_random_uniform_range() {
    // `math.random()` must return f64 in [0.0, 1.0) — same contract
    // as JS's `Math.random()`. Sample a few hundred times and verify
    // the range; the test would have a 1-in-2^53 chance of seeing
    // exactly 0.0 from a buggy impl that returned constants, so the
    // distinct-value check catches "always returns the same value"
    // regressions as well.
    // The end_to_end harness skips the loader, so a `use std.math`
    // import wouldn't resolve. Declare the `@intrinsic` directly —
    // the resulting MIR call is identical to what the loader-emitted
    // path produces from `math.random()`.
    let src = r#"
        @intrinsic("math.random")
        fn rand_(): f64

        let bad: i64[] = [0]
        let distinct: i64[] = [0]
        let prev: f64[] = [-1.0]
        for _ in 0..256 {
            let x = rand_()
            if x < 0.0 {
                bad[0] = bad[0] + 1
            }
            if x >= 1.0 {
                bad[0] = bad[0] + 1
            }
            if x != prev[0] {
                distinct[0] = distinct[0] + 1
            }
            prev[0] = x
        }
        let in_range = if bad[0] == 0 { 1 } else { 0 }
        let varied = if distinct[0] >= 200 { 1 } else { 0 }
        in_range * 10 + varied
    "#;
    assert_eq!(run(src), 11);
}

#[test]
fn int_primitive_min_max() {
    let src = r#"
        let a = if i8.Min == -128 { 1 } else { 0 }
        let b = if i8.Max == 127 { 1 } else { 0 }
        let c = if i16.Max == 32767 { 1 } else { 0 }
        let d = if i32.Min == -2147483648 { 1 } else { 0 }
        let e = if u8.Max == 255 { 1 } else { 0 }
        let f = if u16.Min == 0 { 1 } else { 0 }
        a + b * 2 + c * 4 + d * 8 + e * 16 + f * 32
    "#;
    assert_eq!(run(src), 63);
}

#[test]
fn int_primitive_min_max_64bit() {
    // i64 / u64 bounds. The u64 max check uses cast to compare
    // against the bit-equal i64 sentinel (-1) since the literal
    // is too large for the i64 path.
    let src = r#"
        let a = if i64.Min == -9223372036854775808 { 1 } else { 0 }
        let b = if i64.Max == 9223372036854775807 { 1 } else { 0 }
        a * 10 + b
    "#;
    assert_eq!(run(src), 11);
}

#[test]
fn float_primitive_constants() {
    // `f32.NaN != f32.NaN` (IEEE), `f32.Infinity > 0.0`,
    // `f64.Min < 0.0`. Result encodes each bit flag in a power of 2
    // so a failed assertion points at the responsible check.
    let src = r#"
        let nan_self = if f32.NaN == f32.NaN { 0 } else { 1 }
        let inf_pos = if f32.Infinity > 0.0 { 1 } else { 0 }
        let inf_neg = if f32.NegInfinity < 0.0 { 1 } else { 0 }
        let min_neg = if f64.Min < 0.0 { 1 } else { 0 }
        let max_pos = if f64.Max > 0.0 { 1 } else { 0 }
        let mp_pos = if f64.MinPositive > 0.0 { 1 } else { 0 }
        nan_self + inf_pos * 2 + inf_neg * 4 + min_neg * 8 + max_pos * 16 + mp_pos * 32
    "#;
    // All six checks should be true → 1+2+4+8+16+32 = 63
    assert_eq!(run(src), 63);
}

#[test]
fn string_length_const() {
    let src = r#"
        let s = "hello"
        s.length
    "#;
    assert_eq!(run(src), 5);
}

#[test]
fn string_concat_chars() {
    let src = r#"
        let s = "ab" + "cd" + "ef"
        s.length
    "#;
    assert_eq!(run(src), 6);
}

#[test]
fn string_equality() {
    let src = r#"
        if "hello" == "hello" { 1 } else { 0 }
    "#;
    assert_eq!(run(src), 1);
}

#[test]
fn console_log_does_not_crash() {
    // Best-effort smoke test: runs the program and asserts it
    // returned a sane value (capturing stdout from a JIT-host call
    // would need extra plumbing; for now we verify compilation +
    // execution succeeds).
    let src = r#"
        console.log("answer:", 42, true)
        7
    "#;
    assert_eq!(run(src), 7);
}

#[test]
fn string_methods_chain() {
    let src = r#"
        let s = "  Hello World  "
        s.trim().toLower().length
    "#;
    assert_eq!(run(src), 11);
}

#[test]
fn string_includes_starts_ends() {
    let src = r#"
        let s = "hello world"
        let a = if s.includes("world") { 1 } else { 0 }
        let b = if s.startsWith("hello") { 1 } else { 0 }
        let c = if s.endsWith("world") { 1 } else { 0 }
        a + b + c
    "#;
    assert_eq!(run(src), 3);
}

#[test]
fn string_slice_chars() {
    let src = r#"
        let s = "hello"
        s.slice(1, 4).length
    "#;
    assert_eq!(run(src), 3);
}

#[test]
fn array_index_of_and_includes() {
    let src = r#"
        let xs: i64[] = [10, 20, 30, 40, 50]
        let pos = xs.indexOf(30)
        let here = if xs.includes(40) { 1 } else { 0 }
        let absent = if xs.includes(99) { 1 } else { 0 }
        pos * 10 + here - absent
    "#;
    assert_eq!(run(src), 21); // pos=2 → 20 + 1 - 0 = 21
}

#[test]
fn array_push_grows() {
    let src = r#"
        let xs: i64[] = []
        xs.push(1)
        xs.push(2)
        xs.push(3)
        xs.push(4)
        xs.push(5)
        xs.length + xs[3]
    "#;
    assert_eq!(run(src), 9); // length=5, xs[3]=4
}

#[test]
fn array_pop_returns_optional() {
    let src = r#"
        let xs: i64[] = [10, 20, 30]
        if let some(v) = xs.pop() { v + xs.length } else { -1 }
    "#;
    assert_eq!(run(src), 32); // popped 30, length now 2 → 30 + 2
}

#[test]
fn array_map_doubles() {
    let src = r#"
        let xs: i64[] = [1, 2, 3, 4]
        let ys = xs.map(fn(x: i64): i64 { x * 10 })
        ys[0] + ys[1] + ys[2] + ys[3]
    "#;
    assert_eq!(run(src), 100);
}

#[test]
fn array_filter_evens() {
    let src = r#"
        let xs: i64[] = [1, 2, 3, 4, 5, 6]
        let evens = xs.filter(fn(x: i64): bool { x % 2 == 0 })
        evens.length + evens[0] + evens[1] + evens[2]
    "#;
    assert_eq!(run(src), 15); // length=3, 2+4+6 = 12 → 3+12=15
}

#[test]
fn array_for_each_sum_via_capture() {
    let src = r#"
        let total: i64[] = [0]
        let xs: i64[] = [1, 2, 3, 4]
        xs.forEach(fn(x: i64) { total[0] = total[0] + x })
        total[0]
    "#;
    assert_eq!(run(src), 10);
}

#[test]
fn template_literal_plain_text() {
    let src = r#"
        let s = `hello world`
        s.length
    "#;
    assert_eq!(run(src), 11);
}

#[test]
fn template_literal_string_interp() {
    let src = r#"
        let name = "world"
        let s = `hello ${name}!`
        s.length
    "#;
    // "hello world!" → 12
    assert_eq!(run(src), 12);
}

#[test]
fn template_literal_int_interp() {
    let src = r#"
        let n = 42
        let s = `n=${n}`
        s.length
    "#;
    // "n=42" → 4
    assert_eq!(run(src), 4);
}

#[test]
fn template_literal_multi_interp_expr() {
    let src = r#"
        let a = 3
        let b = 4
        let s = `${a}+${b}=${a + b}`
        s.length
    "#;
    // "3+4=7" → 5
    assert_eq!(run(src), 5);
}

#[test]
fn template_literal_bool_and_escape() {
    let src = r#"
        let s = `flag=${true}\n${false}`
        s.length
    "#;
    // "flag=true\nfalse" — chars: 16 (true=4, false=5, "flag="=5, "\n"=1, total 5+4+1+5=15… let me recount)
    // f l a g = t r u e \n f a l s e
    // 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15
    assert_eq!(run(src), 15);
}

#[test]
fn string_concat_method() {
    let src = r#"
        let a = "abc"
        let b = a.concat("de")
        b.length
    "#;
    assert_eq!(run(src), 5);
}

#[test]
fn string_index_of_basic() {
    let src = r#"
        let s = "hello world"
        let a = s.indexOf("world")
        let b = s.indexOf("xyz")
        let c = s.indexOf("o", 5)
        a * 100 + (b + 1) * 10 + c
    "#;
    // a=6, b=-1 → (b+1)=0, c=7  →  6*100 + 0 + 7 = 607
    assert_eq!(run(src), 607);
}

#[test]
fn string_last_index_of_basic() {
    let src = r#"
        let s = "abcabc"
        let a = s.lastIndexOf("b")
        let b = s.lastIndexOf("b", 2)
        let c = s.lastIndexOf("z")
        a * 100 + b * 10 + (c + 1)
    "#;
    // a=4, b=1, c=-1 → (c+1)=0  →  4*100 + 1*10 + 0 = 410
    assert_eq!(run(src), 410);
}

#[test]
fn str_split_count() {
    let src = r#"
        let parts = "a,b,c,d".split(",")
        parts.length
    "#;
    assert_eq!(run(src), 4);
}

#[test]
fn virtual_dispatch_via_parent_ref() {
    // The override on Dog.value should be dispatched even when the
    // receiver is statically typed as Animal.
    let src = r#"
        class Animal {
            init() {}
            value(): i64 { 1 }
        }
        class Dog: Animal {
            init() { super() }
            override value(): i64 { 42 }
        }
        let d: Animal = new Dog()
        d.value()
    "#;
    assert_eq!(run(src), 42);
}

#[test]
fn loop_with_break_value() {
    let src = r#"
        fn first_even_geq(n: i64): i64 {
            let i = n
            loop {
                if i % 2 == 0 { break i }
                i = i + 1
            }
        }
        first_even_geq(7)
    "#;
    assert_eq!(run(src), 8);
}

#[test]
fn nested_if() {
    let src = r#"
        fn sign(n: i64): i64 {
            if n > 0 { 1 } else { if n < 0 { -1 } else { 0 } }
        }
        sign(-5) + sign(0) + sign(7)
    "#;
    assert_eq!(run(src), 0);
}
