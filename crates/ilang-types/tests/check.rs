use ilang_ast::Type;
use ilang_lexer::tokenize;
use ilang_parser::parse;
use ilang_types::{check, TypeChecker, TypeError};

fn ty(src: &str) -> Result<Type, TypeError> {
    let (t, errs) = errors_and_ty(src);
    match errs.into_iter().next() {
        Some(e) => Err(e),
        None => Ok(t),
    }
}

fn errors_and_ty(src: &str) -> (Type, Vec<TypeError>) {
    let toks = tokenize(src).unwrap();
    let prog = parse(&toks).unwrap();
    check(&prog)
}

// Full error list from a single check pass. `ty()` collapses to the
// first error for backward compat with the older single-error tests;
// tests that need to assert on multiple errors call `errors` directly.
fn errors(src: &str) -> Vec<TypeError> {
    errors_and_ty(src).1
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
        ty("@requires(net) fn f(x: i64): i64 { x } f(1)").unwrap(),
        Type::I64
    );
}

#[test]
fn repl_persistence() {
    let mut tc = TypeChecker::new();
    let toks = tokenize("let x = 1.0;").unwrap();
    let p = parse(&toks).unwrap();
    let (t, errs) = tc.check(&p);
    assert!(errs.is_empty(), "unexpected errors: {errs:?}");
    assert_eq!(t, Type::Unit);

    let toks = tokenize("x + 2").unwrap();
    let p = parse(&toks).unwrap();
    let (t, errs) = tc.check(&p);
    assert!(errs.is_empty(), "unexpected errors: {errs:?}");
    assert_eq!(t, Type::F64);
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
fn if_without_else_evaluates_to_unit() {
    // No else: the expression is always () regardless of the
    // then-branch's type. Lets `if cond { return X }` compile in a
    // non-Unit-returning function — and matches the JS-style intent
    // that a value-less `if` discards whatever the body produced.
    assert_eq!(ty("if true { let _x = 1; }").unwrap(), Type::Unit);
    assert_eq!(ty("if true { 1 }").unwrap(), Type::Unit);
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
    let (_, errs) = check(&prog);
    let err = errs.into_iter().next().expect("expected an error");
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

#[test]
fn console_log_accepts_any_value() {
    assert_eq!(ty("console.log(1)").unwrap(), Type::Unit);
    assert_eq!(ty("console.log(1.5)").unwrap(), Type::Unit);
    assert_eq!(ty("console.log(true)").unwrap(), Type::Unit);
    let src = "class P { x: i64; init(a: i64) { this.x = a } } console.log(new P(1))";
    assert_eq!(ty(src).unwrap(), Type::Unit);
}

#[test]
fn console_log_is_variadic() {
    // Any arity (0+) and any mix of types is accepted.
    assert_eq!(ty("console.log()").unwrap(), Type::Unit);
    assert_eq!(ty("console.log(1)").unwrap(), Type::Unit);
    assert_eq!(ty("console.log(1, 2, 3)").unwrap(), Type::Unit);
    assert_eq!(ty("console.log(1, true, 3.14)").unwrap(), Type::Unit);
}

#[test]
fn bitwise_requires_i64() {
    assert_eq!(ty("1 & 2").unwrap(), Type::I64);
    assert!(matches!(ty("1.0 & 2"), Err(TypeError::BadBinary { .. })));
    assert!(matches!(ty("true & false"), Err(TypeError::BadBinary { .. })));
    assert!(matches!(ty("~true"), Err(TypeError::BadUnary { .. })));
}

#[test]
fn cast_typechecks() {
    assert_eq!(ty("1 as i32").unwrap(), Type::I32);
    assert_eq!(ty("1.5 as f32").unwrap(), Type::F32);
    assert_eq!(ty("true as i64").unwrap(), Type::I64);
    // Cast from object → numeric is rejected.
    let src = "class P { init() {} } new P() as i32";
    assert!(matches!(ty(src), Err(TypeError::Mismatch { .. })));
}

#[test]
fn implicit_int_to_float_ok() {
    // i64 → f64 implicit, no `as` required.
    assert!(ty("let x: f64 = 5;").is_ok());
    // f64 → i64 implicit is *not* allowed.
    assert!(matches!(
        ty("let x: i64 = 1.5;"),
        Err(TypeError::Mismatch { .. })
    ));
}

#[test]
fn signed_unsigned_mix_rejected() {
    // i32 + u32 (without literal) requires explicit `as`. The error
    // points the user at the cast rather than the generic BadBinary.
    let src = "let a: i32 = 1; let b: u32 = 2; a + b";
    assert!(matches!(ty(src), Err(TypeError::MixedSignedness { .. })));
    // Same for comparisons.
    let src = "let a: i32 = 1; let b: u32 = 2; a == b";
    assert!(matches!(ty(src), Err(TypeError::MixedSignedness { .. })));
}

#[test]
fn signed_unsigned_widening_within_signedness_ok() {
    assert!(ty("let a: i8 = 1; let b: i32 = 2; a + b").is_ok());
    assert!(ty("let a: u8 = 1; let b: u32 = 2; a + b").is_ok());
}

#[test]
fn negation_on_unsigned_rejected() {
    // -x where x is u8 is a type error (no signed semantics).
    let src = "let x: u8 = 5; -x";
    assert!(matches!(ty(src), Err(TypeError::BadUnary { .. })));
}

#[test]
fn integer_literal_too_large_for_unsigned() {
    let src = "let x: u8 = 300";
    assert!(matches!(ty(src), Err(TypeError::Mismatch { .. })));
}

#[test]
fn string_typechecks() {
    assert_eq!(ty(r#""hello""#).unwrap(), Type::Str);
    assert_eq!(ty(r#""a" + "b""#).unwrap(), Type::Str);
    assert_eq!(ty(r#""a" == "b""#).unwrap(), Type::Bool);
    // Cannot mix string and number.
    assert!(matches!(
        ty(r#""a" + 1"#),
        Err(TypeError::BadBinary { .. })
    ));
    // Ordering is rejected.
    assert!(matches!(
        ty(r#""a" < "b""#),
        Err(TypeError::BadBinary { .. })
    ));
}

#[test]
fn array_typechecks() {
    assert!(matches!(
        ty("[1, 2, 3]").unwrap(),
        Type::Array { .. }
    ));
    assert!(ty("let a: i32[] = [1, 2, 3]; a").is_ok());
    assert!(ty("let a: i32[3] = [1, 2, 3]; a").is_ok());
}

#[test]
fn array_length_mismatch_rejected() {
    assert!(matches!(
        ty("let a: i32[5] = [1, 2, 3]"),
        Err(TypeError::Mismatch { .. })
    ));
}

#[test]
fn array_heterogeneous_rejected() {
    assert!(matches!(
        ty(r#"[1, "hello"]"#),
        Err(TypeError::Mismatch { .. })
    ));
}

#[test]
fn push_on_fixed_array_rejected() {
    let src = "let a: i32[3] = [1, 2, 3]; a.push(4)";
    assert!(matches!(ty(src), Err(TypeError::Mismatch { .. })));
}

#[test]
fn array_index_must_be_int() {
    assert!(matches!(
        ty("let a: i32[] = [1]; a[1.5]"),
        Err(TypeError::Mismatch { .. })
    ));
}

#[test]
fn array_equality_rejected() {
    // Arrays don't support `==` at the type level (no deep equality yet).
    let src = "let a: i32[] = [1]; let b: i32[] = [1]; a == b";
    assert!(matches!(ty(src), Err(TypeError::BadBinary { .. })));
}

#[test]
fn object_equality_still_works() {
    let src = "class P { init() {} } let a = new P(); let b = a; a == b";
    assert_eq!(ty(src).unwrap(), Type::Bool);
}

#[test]
fn console_class_redefinition_rejected() {
    let src = "class Console { init() {} }";
    assert!(matches!(ty(src), Err(TypeError::ReservedName { .. })));
}

#[test]
fn console_global_redefinition_rejected() {
    let src = "let console = 1";
    assert!(matches!(ty(src), Err(TypeError::ReservedName { .. })));
}

#[test]
fn empty_array_needs_annotation() {
    assert!(matches!(
        ty("let a = []"),
        Err(TypeError::EmptyArrayNeedsAnnotation { .. })
    ));
    // Annotated form is fine and produces an empty dynamic array.
    assert!(ty("let a: i32[] = []").is_ok());
}

// ───── previously-uncovered TypeError variants ────────────────────
// Guards that each of these variants is still produced after the
// multi-error rewrite (each was previously absent from the test suite,
// so a silent loss of detection would have gone unnoticed).

#[test]
fn undefined_class_in_annotation_rejected() {
    assert!(matches!(
        ty("let x: NoSuch = 0"),
        Err(TypeError::UndefinedClass { .. })
    ));
}

#[test]
fn unknown_field_access_rejected() {
    let src = "class P { init() {} } new P().missing";
    assert!(matches!(ty(src), Err(TypeError::UnknownField { .. })));
}

#[test]
fn unknown_method_call_rejected() {
    let src = "class P { init() {} } new P().nope()";
    assert!(matches!(ty(src), Err(TypeError::UnknownMethod { .. })));
}

#[test]
fn this_outside_method_rejected() {
    assert!(matches!(
        ty("this"),
        Err(TypeError::ThisOutsideMethod { .. })
    ));
    assert!(matches!(
        ty("fn f() { this }"),
        Err(TypeError::ThisOutsideMethod { .. })
    ));
}

#[test]
fn tuple_destructure_arity_mismatch_unsupported() {
    // Representative `Unsupported` trigger: tuple-destructure slot
    // count mismatch.
    assert!(matches!(
        ty("let (a, b) = (1, 2, 3)"),
        Err(TypeError::Unsupported { .. })
    ));
}

#[test]
fn deprecated_method_call_emits_warning() {
    let src = r#"
        class C {
            init() {}
            @deprecated("use bar") foo() {}
            bar() {}
        }
        new C().foo()
    "#;
    let toks = tokenize(src).unwrap();
    let prog = parse(&toks).unwrap();
    let mut tc = TypeChecker::new();
    let (_, errs) = tc.check(&prog);
    assert!(errs.is_empty(), "unexpected errors: {errs:?}");
    let ws = tc.warnings();
    assert_eq!(ws.len(), 1, "expected one deprecation warning, got {ws:?}");
    assert!(
        ws[0].message.contains("deprecated"),
        "warning message lacks 'deprecated': {:?}",
        ws[0].message
    );
}

// ───── cascade suppression ────────────────────────────────────────
// Programs whose first error makes subsequent expressions trivially
// ill-typed must still report exactly one error — the placeholder
// `Type::Error` propagation in `ops::assignable` & friends absorbs
// the follow-up so the diagnostic list stays focused on the root
// cause.

#[test]
fn cascade_undefined_then_binary_stays_single() {
    // Only the `undef` UndefinedVariable surfaces — no follow-up
    // BadBinary from `x + 1`.
    assert!(matches!(
        ty("let x = undef; x + 1"),
        Err(TypeError::UndefinedVariable { .. })
    ));
}

#[test]
fn cascade_mismatch_then_use_stays_single() {
    assert!(matches!(
        ty("let x: i64 = \"s\"; x + 1"),
        Err(TypeError::Mismatch { .. })
    ));
}

#[test]
fn cascade_unknown_method_on_undefined_var_stays_single() {
    // Only the `noSuch` UndefinedVariable surfaces — no follow-up
    // UnknownMethod from the `.method()` call against it.
    assert!(matches!(
        ty("noSuch.method()"),
        Err(TypeError::UndefinedVariable { .. })
    ));
}

// ───── @lib call must be inside @extern(...) ─────────────────────
// Calling a dlsym'd `@lib pub fn ...` declaration from ordinary code
// fails at JIT time with `can't resolve symbol X`. The type checker
// rejects the call up front; the only exception is `@lib("objc")`
// (the ObjC runtime primitives the cocoa bindings expose through
// wrapper fns).

#[test]
fn lib_kernel32_fn_called_outside_extern_rejected() {
    let src = r#"
        @extern(C, "kernel32") {
            @lib pub fn GetProcessId(p: i64): u32
        }
        GetProcessId(0)
    "#;
    let errs = errors(src);
    assert!(
        errs.iter().any(|e| matches!(e, TypeError::Unsupported { what, .. } if what.contains("@lib"))),
        "expected @lib-outside-extern error, got {errs:?}"
    );
}

#[test]
fn lib_kernel32_fn_called_inside_another_extern_block_ok() {
    let src = r#"
        @extern(C, "kernel32") {
            @lib pub fn GetProcessId(p: i64): u32
        }
        @extern(C) {
            pub fn wrap(p: i64): u32 { GetProcessId(p) }
        }
    "#;
    let errs = errors(src);
    assert!(errs.is_empty(), "calling from @extern(C) must type-check: {errs:?}");
}

#[test]
fn intrinsic_fn_called_outside_extern_ok() {
    // `@intrinsic("ns.name")` desugars to an `@extern(C) { fn ... }`
    // declaration with empty `libs` (the runtime symbol table
    // resolves it via the `$ns.name` sigil), so it falls through
    // the @lib gate and is callable anywhere — including from a
    // plain `pub fn` wrapper outside any extern block. This is the
    // pattern cocoa's `_objc_err_slot_ptr` etc. now use.
    let src = r#"
        @intrinsic("objc.err_slot_ptr") fn _slot(): i64
        pub fn slot(): i64 { _slot() }
        let _ = slot()
    "#;
    let errs = errors(src);
    assert!(
        errs.is_empty(),
        "@intrinsic must be callable anywhere: {errs:?}"
    );
}

#[test]
fn lib_objc_fn_called_outside_extern_also_rejected() {
    // `@lib("objc")` used to be exempt to keep cocoa's `objcRetain`
    // wrapper compiling, but the wrapper is now declared INSIDE the
    // bindings' `@extern(ObjC) { ... }` block — so the exemption is
    // no longer needed and `@lib("objc")` is gated like every other
    // dlsym'd library.
    let src = r#"
        @extern(C) {
            @lib("objc") fn _objc_retain(h: i64): i64
        }
        _objc_retain(0)
    "#;
    let errs = errors(src);
    assert!(
        errs.iter().any(|e| matches!(e, TypeError::Unsupported { what, .. } if what.contains("@lib"))),
        "@lib(\"objc\") outside @extern must error: {errs:?}"
    );
}

// ───── multi-error headline ───────────────────────────────────────
// Locks in that one pass collects every independent error.

#[test]
fn console_log_reports_each_undefined_arg() {
    let errs = errors("console.log(aa, bb)");
    assert_eq!(
        errs.len(),
        2,
        "expected one error per bad arg, got {errs:?}"
    );
    assert!(matches!(errs[0], TypeError::UndefinedVariable { .. }));
    assert!(matches!(errs[1], TypeError::UndefinedVariable { .. }));
}

#[test]
fn separate_statements_each_report_their_error() {
    // Distinct errors on distinct stmts are both collected.
    let errs = errors("let x = undef1\nlet y = undef2");
    assert_eq!(
        errs.len(),
        2,
        "expected one error per bad let, got {errs:?}"
    );
}

#[test]
fn if_else_no_implicit_numeric_widening() {
    // i64 と f64 をぶつけたらエラー (Rust と同じ挙動)
    assert!(matches!(
        ty("let b = if 1 == 0 { 1 as i64 } else { 2 as f64 }"),
        Err(TypeError::Mismatch { .. })
    ));
    // 同じ型同士は OK
    assert!(ty("let b = if 1 == 0 { 1 as i64 } else { 2 as i64 }").is_ok());
    // 片方が素の整数リテラルで他方の型に収まるなら literal coercion 可
    assert!(ty("let b = if 1 == 0 { 1 } else { 2 as i32 }").is_ok());
    assert!(ty("let b = if 1 == 0 { 1.0 } else { 2 as f64 }").is_ok());
    // i64 値 (リテラルでない) を f64 アームに混ぜたら拒否
    assert!(matches!(
        ty("fn f(x: i64): f64 { if 1 == 0 { x } else { 2.0 } }"),
        Err(TypeError::Mismatch { .. })
    ));
}
