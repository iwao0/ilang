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
fn rejects_classes_in_subset() {
    let src = r#"
        class P { x: i64 }
        let p = P { x: 1 }
        p.x
    "#;
    let err = compile_program_to_object(&mir(src)).unwrap_err();
    assert!(matches!(err, AotError::Unsupported(_)), "got {err:?}");
}
