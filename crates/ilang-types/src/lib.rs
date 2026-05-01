//! Phase 2 minimal type checker.
//!
//! Supports `i64`, `f64`, `bool`, and `()` (unit). Mixed `i64`/`f64`
//! arithmetic is allowed and promoted to `f64` (matching the runtime).
//! Function signatures and `let` annotations are checked.
//! `#[requires(...)]` attributes are not enforced — that arrives in a later
//! phase along with the capability system.

pub mod checker;
pub mod error;
mod ops;

use ilang_ast::{Program, Type};

pub use checker::TypeChecker;
pub use error::TypeError;

/// One-shot type check for callers that don't need to keep state.
pub fn check(prog: &Program) -> Result<Type, TypeError> {
    TypeChecker::new().check(prog)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ilang_lexer::tokenize;
    use ilang_parser::parse;

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
            ty("fn add(a: i64, b: i64) -> i64 { a + b } add(1, 2)").unwrap(),
            Type::I64
        );
    }

    #[test]
    fn fn_arg_promotion() {
        assert_eq!(ty("fn id(x: f64) -> f64 { x } id(5)").unwrap(), Type::F64);
    }

    #[test]
    fn fn_arg_type_error() {
        assert!(matches!(
            ty("fn need_int(x: i64) -> i64 { x } need_int(1.5)"),
            Err(TypeError::Mismatch { .. })
        ));
    }

    #[test]
    fn arity_error() {
        assert!(matches!(
            ty("fn id(x: i64) -> i64 { x } id(1, 2)"),
            Err(TypeError::ArityMismatch { .. })
        ));
    }

    #[test]
    fn return_type_mismatch() {
        assert!(matches!(
            ty("fn bad() -> i64 { 1.0 }"),
            Err(TypeError::BadReturn { .. })
        ));
    }

    #[test]
    fn undefined_variable() {
        assert!(matches!(
            ty("x + 1"),
            Err(TypeError::UndefinedVariable(_))
        ));
    }

    #[test]
    fn undefined_function() {
        assert!(matches!(
            ty("foo(1)"),
            Err(TypeError::UndefinedFunction(_))
        ));
    }

    #[test]
    fn attribute_does_not_affect_typing() {
        assert_eq!(
            ty("#[requires(net)] fn f(x: i64) -> i64 { x } f(1)").unwrap(),
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
            Err(TypeError::BadBinary(_, _))
        ));
    }

    #[test]
    fn logical_and_not() {
        assert_eq!(ty("true && false || !true").unwrap(), Type::Bool);
        assert!(matches!(ty("true && 1"), Err(TypeError::BadBinary(_, _))));
        assert!(matches!(ty("!1"), Err(TypeError::BadUnary(_))));
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
            Err(TypeError::UndefinedVariable(_))
        ));
        assert!(matches!(
            ty("let x: i64 = 0; x = 1.5;"),
            Err(TypeError::Mismatch { .. })
        ));
    }

    #[test]
    fn block_scope_in_types() {
        assert_eq!(
            ty("let x = 1; { let x = 2.0; x }").unwrap(),
            Type::F64
        );
    }
}
