use ilang_ast::Type;
use ilang_lexer::tokenize;
use ilang_parser::parse;
use ilang_types::{check, TypeChecker, TypeError};

fn ty(src: &str) -> Result<Type, TypeError> {
    let toks = tokenize(src).unwrap();
    let prog = parse(&toks).unwrap();
    check(&prog)
}

#[test]
fn literals() {
    assert_eq!(ty("1").unwrap(), Type::I64);
    assert_eq!(ty("1.0").unwrap(), Type::F64);
}

#[test]
fn promotion_in_binary() {
    assert_eq!(ty("1 + 2.0").unwrap(), Type::F64);
    assert_eq!(ty("1 + 2").unwrap(), Type::I64);
}

#[test]
fn let_inference_and_use() {
    assert_eq!(ty("let x = 1; x + 2").unwrap(), Type::I64);
    assert_eq!(ty("let x = 1.0; x + 2").unwrap(), Type::F64);
}

#[test]
fn let_annotation_ok() {
    assert!(ty("let x: f64 = 1;").is_ok());
    assert!(ty("let x: i64 = 1;").is_ok());
}

#[test]
fn let_annotation_mismatch() {
    assert!(matches!(
        ty("let x: i64 = 1.0;"),
        Err(TypeError::Mismatch { .. })
    ));
}

#[test]
fn fn_signature_checks() {
    assert_eq!(
        ty("fn add(a: i64, b: i64): i64 { a + b } add(1, 2)").unwrap(),
        Type::I64
    );
}

#[test]
fn fn_arg_promotion() {
    assert_eq!(ty("fn id(x: f64): f64 { x } id(5)").unwrap(), Type::F64);
}

#[test]
fn fn_arg_type_error() {
    assert!(matches!(
        ty("fn need_int(x: i64): i64 { x } need_int(1.5)"),
        Err(TypeError::Mismatch { .. })
    ));
}

#[test]
fn arity_error() {
    assert!(matches!(
        ty("fn id(x: i64): i64 { x } id(1, 2)"),
        Err(TypeError::ArityMismatch { .. })
    ));
}

#[test]
fn return_type_mismatch() {
    assert!(matches!(
        ty("fn bad(): i64 { 1.0 }"),
        Err(TypeError::BadReturn { .. })
    ));
}

#[test]
fn undefined_variable() {
    assert!(matches!(
        ty("x + 1"),
        Err(TypeError::UndefinedVariable { .. })
    ));
}

#[test]
fn undefined_function() {
    assert!(matches!(
        ty("foo(1)"),
        Err(TypeError::UndefinedFunction { .. })
    ));
}

#[test]
fn attribute_does_not_affect_typing() {
    assert_eq!(
        ty("#[requires(net)] fn f(x: i64): i64 { x } f(1)").unwrap(),
        Type::I64
    );
}

#[test]
fn repl_persistence() {
    let mut tc = TypeChecker::new();
    let toks = tokenize("let x = 1.0;").unwrap();
    let p = parse(&toks).unwrap();
    assert_eq!(tc.check(&p).unwrap(), Type::Unit);

    let toks = tokenize("x + 2").unwrap();
    let p = parse(&toks).unwrap();
    assert_eq!(tc.check(&p).unwrap(), Type::F64);
}

#[test]
fn bool_and_comparison() {
    assert_eq!(ty("true").unwrap(), Type::Bool);
    assert_eq!(ty("1 < 2").unwrap(), Type::Bool);
    assert_eq!(ty("1.0 == 2").unwrap(), Type::Bool);
    assert!(matches!(
        ty("true < false"),
        Err(TypeError::BadBinary { .. })
    ));
}

#[test]
fn logical_and_not() {
    assert_eq!(ty("true && false || !true").unwrap(), Type::Bool);
    assert!(matches!(ty("true && 1"), Err(TypeError::BadBinary { .. })));
    assert!(matches!(ty("!1"), Err(TypeError::BadUnary { .. })));
}

#[test]
fn if_expression_branches_match() {
    assert_eq!(ty("if true { 1 } else { 2 }").unwrap(), Type::I64);
    assert_eq!(ty("if true { 1 } else { 2.0 }").unwrap(), Type::F64);
    assert!(matches!(
        ty("if true { 1 } else { true }"),
        Err(TypeError::Mismatch { .. })
    ));
}

#[test]
fn if_without_else_must_be_unit() {
    assert_eq!(ty("if true { let _x = 1; }").unwrap(), Type::Unit);
    assert!(matches!(
        ty("if true { 1 }"),
        Err(TypeError::Mismatch { .. })
    ));
}

#[test]
fn while_requires_bool_cond_and_unit_body() {
    assert!(ty("let n = 0; while n < 10 { n = n + 1; }").is_ok());
    assert!(matches!(
        ty("while 1 { }"),
        Err(TypeError::Mismatch { .. })
    ));
}

#[test]
fn assign_requires_existing_var_and_compat_type() {
    assert!(ty("let x = 1; x = 2;").is_ok());
    assert!(ty("let x: f64 = 0.0; x = 1;").is_ok());
    assert!(matches!(
        ty("y = 1;"),
        Err(TypeError::UndefinedVariable { .. })
    ));
    assert!(matches!(
        ty("let x: i64 = 0; x = 1.5;"),
        Err(TypeError::Mismatch { .. })
    ));
}

#[test]
fn block_scope_in_types() {
    assert_eq!(ty("let x = 1; { let x = 2.0; x }").unwrap(), Type::F64);
}

#[test]
fn loop_break_continue_are_unit() {
    assert_eq!(ty("loop { break }").unwrap(), Type::Unit);
    assert_eq!(
        ty("let i = 0\nloop { i = i + 1; if i > 3 { break } else { continue } }").unwrap(),
        Type::Unit
    );
}

#[test]
fn break_outside_loop_rejected() {
    assert!(matches!(ty("break"), Err(TypeError::BreakOutsideLoop { .. })));
}

#[test]
fn continue_outside_loop_rejected() {
    assert!(matches!(
        ty("continue"),
        Err(TypeError::ContinueOutsideLoop { .. })
    ));
}

#[test]
fn break_does_not_cross_function_boundary() {
    // The `break` is inside a function defined inside a loop, but it does
    // not refer to that outer loop.
    let src = r#"
        fn helper() { break }
        loop { helper(); break }
    "#;
    assert!(matches!(ty(src), Err(TypeError::BreakOutsideLoop { .. })));
}

#[test]
fn break_inside_while_ok() {
    assert_eq!(
        ty("let i = 0\nwhile i < 10 { i = i + 1; if i == 5 { break } }").unwrap(),
        Type::Unit
    );
}

#[test]
fn implicit_this_field_typechecks() {
    let src = r#"
        class P {
            x: i64
            init(x: i64) { this.x = x }
            get(): i64 { x }
        }
        new P(1).get()
    "#;
    assert_eq!(ty(src).unwrap(), Type::I64);
}

#[test]
fn implicit_method_call_typechecks() {
    let src = r#"
        class M {
            init() {}
            a(): i64 { b() }
            b(): i64 { 7 }
        }
        new M().a()
    "#;
    assert_eq!(ty(src).unwrap(), Type::I64);
}

#[test]
fn implicit_this_assign_typechecks() {
    let src = r#"
        class A {
            n: i64
            init() { this.n = 0 }
            inc() { n = n + 1 }
        }
        new A().inc()
    "#;
    assert_eq!(ty(src).unwrap(), Type::Unit);
}

#[test]
fn unknown_implicit_field_still_errors() {
    let src = r#"
        class B {
            init() {}
            bad(): i64 { missing }
        }
        new B().bad()
    "#;
    assert!(matches!(ty(src), Err(TypeError::UndefinedVariable { .. })));
}

#[test]
fn type_error_carries_span() {
    // The undefined variable `z` is at line 1, column 9 (1-based).
    let toks = tokenize("let x = z").unwrap();
    let prog = parse(&toks).unwrap();
    let err = check(&prog).unwrap_err();
    let span = err.span();
    assert_eq!((span.line, span.col), (1, 9));
    let s = format!("{err}");
    assert!(s.starts_with("[1:9]:"), "got: {s}");
}

#[test]
fn deinit_explicit_call_rejected() {
    let src = "class T { init() {} deinit() {} } new T().deinit()";
    assert!(matches!(ty(src), Err(TypeError::CannotCallDeinit { .. })));
}

#[test]
fn deinit_with_params_rejected() {
    let src = "class T { init() {} deinit(x: i64) {} } new T()";
    assert!(matches!(
        ty(src),
        Err(TypeError::BadDeinitSignature { .. })
    ));
}

#[test]
fn deinit_with_return_rejected() {
    let src = "class T { init() {} deinit(): i64 { 1 } } new T()";
    assert!(matches!(
        ty(src),
        Err(TypeError::BadDeinitSignature { .. })
    ));
}
