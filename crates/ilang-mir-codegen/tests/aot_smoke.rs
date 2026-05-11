//! Sanity check that compile_program_to_object emits non-empty bytes
//! for the supported subset and rejects programs outside it.

use ilang_lexer::tokenize;
use ilang_mir::lower_program;
use ilang_mir_codegen::{compile_program_to_object, AotError};
use ilang_parser::parse;

fn mir(src: &str) -> ilang_mir::Program {
    let tokens = tokenize(src).expect("tokenize");
    let ast = parse(&tokens).expect("parse");
    lower_program(&ast).expect("lower")
}

fn expect_object(src: &str) {
    let bytes = compile_program_to_object(&mir(src)).expect("compile");
    assert!(bytes.len() > 64, "object too small: {} bytes", bytes.len());
}

#[test]
fn emits_object_for_int_tail() {
    expect_object("42");
}

#[test]
fn emits_object_for_let_then_int() {
    expect_object("let x = 42\nx");
}

#[test]
fn emits_object_for_arithmetic() {
    expect_object("1 + 2 * 3");
}

#[test]
fn emits_object_for_let_with_arithmetic() {
    expect_object("let a = 10\nlet b = 3\na - b");
}

#[test]
fn emits_object_for_if_else() {
    expect_object(
        r#"
        let x = 7
        if x > 5 { 100 } else { 0 }
    "#,
    );
}

#[test]
fn emits_object_for_while_loop() {
    expect_object(
        r#"
        let total = 0
        let i = 1
        while i <= 10 {
          total = total + i
          i = i + 1
        }
        total
    "#,
    );
}

#[test]
fn emits_object_for_user_fn_call() {
    expect_object(
        r#"
        fn add(a: i64, b: i64): i64 { a + b }
        add(20, 22)
    "#,
    );
}

#[test]
fn emits_object_for_recursive_fn() {
    expect_object(
        r#"
        fn fact(n: i64): i64 {
          if n <= 1 { 1 } else { n * fact(n - 1) }
        }
        fact(5)
    "#,
    );
}

#[test]
fn emits_object_for_console_log() {
    expect_object("console.log(42)");
}

#[test]
fn emits_object_for_console_log_multi_arg() {
    expect_object(
        r#"
        console.log(1, 2, 3)
        console.log(true, false)
    "#,
    );
}

#[test]
fn emits_object_for_console_log_with_string() {
    expect_object(
        r#"
        console.log("hello")
        console.log("answer:", 42)
    "#,
    );
}

#[test]
fn emits_object_for_dynamic_array() {
    expect_object(
        r#"
        let xs: i64[] = [10, 20, 30]
        xs.push(40)
        console.log(xs.length)
        console.log(xs[3])
    "#,
    );
}

#[test]
fn emits_object_for_string_keyed_map() {
    expect_object(
        r#"
        let m = new Map<string, i64>()
        m["a"] = 1
        m["b"] = 2
        console.log(m["a"], m["b"])
        console.log(m.has("a"), m.has("zzz"))
    "#,
    );
}

#[test]
fn emits_object_for_string_array() {
    expect_object(
        r#"
        let names: string[] = ["alice", "bob"]
        console.log(names[0], names[1])
    "#,
    );
}

#[test]
fn emits_object_for_string_operations() {
    expect_object(
        r#"
        let a = "hello"
        let b = " world"
        let c = a + b
        console.log(c)
        console.log(c.length)
        console.log(c.toUpper())
        console.log("foo,bar".replace(",", "-"))
    "#,
    );
}

#[test]
fn emits_object_for_integer_division() {
    // Division compiles even though div-by-zero panics at runtime —
    // we just verify the AOT pass emits something.
    expect_object("20 / 4");
}

#[test]
fn emits_object_for_class_with_method() {
    expect_object(
        r#"
        class Rect {
          w: i64
          h: i64
          init(w: i64, h: i64) { this.w = w; this.h = h }
          area(): i64 { this.w * this.h }
        }
        let r = new Rect(3, 4)
        console.log(r.area())
        console.log(r.w, r.h)
    "#,
    );
}

#[test]
fn emits_object_for_virtual_dispatch() {
    expect_object(
        r#"
        class Shape {
          area(): i64 { 0 }
        }
        class Box extends Shape {
          w: i64
          h: i64
          init(w: i64, h: i64) { this.w = w; this.h = h }
          override area(): i64 { this.w * this.h }
        }
        let b = new Box(5, 6)
        let s: Shape = b
        console.log(s.area())
    "#,
    );
}
