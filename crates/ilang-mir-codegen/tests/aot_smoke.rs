//! Sanity check that compile_program_to_object emits non-empty bytes
//! for the M0 subset and rejects programs outside it.

use ilang_lexer::tokenize;
use ilang_mir::lower_program;
use ilang_mir_codegen::{compile_program_to_object, AotError};
use ilang_parser::parse;

fn mir(src: &str) -> ilang_mir::Program {
    let tokens = tokenize(src).expect("tokenize");
    let ast = parse(&tokens).expect("parse");
    lower_program(&ast).expect("lower")
}

#[test]
fn emits_object_for_int_tail() {
    let bytes = compile_program_to_object(&mir("42")).expect("compile");
    assert!(bytes.len() > 64, "object too small: {} bytes", bytes.len());
}

#[test]
fn rejects_arithmetic_in_m0() {
    let err = compile_program_to_object(&mir("1 + 2")).unwrap_err();
    assert!(matches!(err, AotError::Unsupported(_)), "got {err:?}");
}

#[test]
fn rejects_classes_in_m0() {
    let src = r#"
        class P { x: i64 }
        let p = P { x: 1 }
        p.x
    "#;
    let err = compile_program_to_object(&mir(src)).unwrap_err();
    assert!(matches!(err, AotError::Unsupported(_)), "got {err:?}");
}
