//! Phase 2 minimal type checker.
//!
//! Supports `i64`, `f64`, and `()` (unit). Mixed `i64`/`f64` arithmetic is
//! allowed and promoted to `f64` (matching the runtime). Function signatures
//! and `let` annotations are checked. `#[requires(...)]` attributes are not
//! enforced — that arrives in a later phase along with the capability system.

use std::collections::HashMap;

use ilang_ast::{Block, Expr, FnDecl, Item, Param, Program, Stmt, Type};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum TypeError {
    #[error("type mismatch: expected {expected}, got {got}")]
    Mismatch { expected: Type, got: Type },
    #[error("undefined variable {0:?}")]
    UndefinedVariable(String),
    #[error("undefined function {0:?}")]
    UndefinedFunction(String),
    #[error("function {name:?} expects {expected} arguments but got {got}")]
    ArityMismatch {
        name: String,
        expected: usize,
        got: usize,
    },
    #[error("cannot apply unary op to {0}")]
    BadUnary(Type),
    #[error("cannot apply binary op between {0} and {1}")]
    BadBinary(Type, Type),
    #[error("function {name:?} declared to return {expected} but body produces {got}")]
    BadReturn {
        name: String,
        expected: Type,
        got: Type,
    },
}

#[derive(Debug, Clone)]
struct Signature {
    params: Vec<Type>,
    ret: Type,
}

#[derive(Debug, Default)]
pub struct TypeChecker {
    fns: HashMap<String, Signature>,
    /// Persistent top-level variable bindings — needed by the REPL so a `let`
    /// on one line is still in scope on the next.
    vars: HashMap<String, Type>,
}

impl TypeChecker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Type-check a program. Top-level `fn` items are registered first so
    /// functions can reference each other regardless of declaration order.
    /// Returns the type of the program's result (tail expression, last stmt,
    /// or `Unit`).
    pub fn check(&mut self, prog: &Program) -> Result<Type, TypeError> {
        for item in &prog.items {
            match item {
                Item::Fn(f) => {
                    let sig = Signature {
                        params: f.params.iter().map(|p| p.ty).collect(),
                        ret: f.ret.unwrap_or(Type::Unit),
                    };
                    self.fns.insert(f.name.clone(), sig);
                }
            }
        }
        for item in &prog.items {
            match item {
                Item::Fn(f) => self.check_fn(f)?,
            }
        }

        // Top-level `let`s persist across calls (REPL); a temp env starts
        // from the persistent vars and the additions are merged back at the
        // end iff the whole check succeeded.
        let mut env: Vars = self.vars.clone();
        let mut last = Type::Unit;
        for s in &prog.stmts {
            last = self.check_stmt(s, &mut env)?;
        }
        if let Some(t) = &prog.tail {
            last = self.check_expr(t, &env)?;
        }
        self.vars = env;
        Ok(last)
    }

    fn check_fn(&self, f: &FnDecl) -> Result<(), TypeError> {
        let mut env: Vars = HashMap::new();
        for Param { name, ty } in &f.params {
            env.insert(name.clone(), *ty);
        }
        let body_ty = self.check_block(&f.body, &env)?;
        let expected = f.ret.unwrap_or(Type::Unit);
        if !assignable(body_ty, expected) {
            return Err(TypeError::BadReturn {
                name: f.name.clone(),
                expected,
                got: body_ty,
            });
        }
        Ok(())
    }

    fn check_block(&self, block: &Block, outer: &Vars) -> Result<Type, TypeError> {
        let mut env = outer.clone();
        let mut last = Type::Unit;
        for s in &block.stmts {
            last = self.check_stmt(s, &mut env)?;
        }
        if let Some(t) = &block.tail {
            last = self.check_expr(t, &env)?;
        }
        Ok(last)
    }

    fn check_stmt(&self, stmt: &Stmt, env: &mut Vars) -> Result<Type, TypeError> {
        match stmt {
            Stmt::Let { name, ty, value } => {
                let vt = self.check_expr(value, env)?;
                let bind = match ty {
                    Some(ann) => {
                        if !assignable(vt, *ann) {
                            return Err(TypeError::Mismatch {
                                expected: *ann,
                                got: vt,
                            });
                        }
                        *ann
                    }
                    None => vt,
                };
                env.insert(name.clone(), bind);
                Ok(Type::Unit)
            }
            Stmt::Expr(e) => self.check_expr(e, env),
        }
    }

    fn check_expr(&self, expr: &Expr, env: &Vars) -> Result<Type, TypeError> {
        match expr {
            Expr::Int(_) => Ok(Type::I64),
            Expr::Float(_) => Ok(Type::F64),
            Expr::Var(n) => env
                .get(n)
                .copied()
                .ok_or_else(|| TypeError::UndefinedVariable(n.clone())),
            Expr::Unary { op: _, expr } => match self.check_expr(expr, env)? {
                t @ (Type::I64 | Type::F64) => Ok(t),
                other => Err(TypeError::BadUnary(other)),
            },
            Expr::Binary { op, lhs, rhs } => {
                let _ = op;
                let l = self.check_expr(lhs, env)?;
                let r = self.check_expr(rhs, env)?;
                bin_result(l, r)
            }
            Expr::Call { callee, args } => {
                let sig = self
                    .fns
                    .get(callee)
                    .cloned()
                    .ok_or_else(|| TypeError::UndefinedFunction(callee.clone()))?;
                if sig.params.len() != args.len() {
                    return Err(TypeError::ArityMismatch {
                        name: callee.clone(),
                        expected: sig.params.len(),
                        got: args.len(),
                    });
                }
                for (param_ty, arg) in sig.params.iter().zip(args.iter()) {
                    let at = self.check_expr(arg, env)?;
                    if !assignable(at, *param_ty) {
                        return Err(TypeError::Mismatch {
                            expected: *param_ty,
                            got: at,
                        });
                    }
                }
                Ok(sig.ret)
            }
            Expr::Block(b) => self.check_block(b, env),
        }
    }
}

type Vars = HashMap<String, Type>;

/// `from` can be assigned to a binding of type `to`. Numeric widening from
/// `i64` to `f64` is allowed (matches the runtime's promotion rule).
fn assignable(from: Type, to: Type) -> bool {
    if from == to {
        return true;
    }
    matches!((from, to), (Type::I64, Type::F64))
}

fn bin_result(l: Type, r: Type) -> Result<Type, TypeError> {
    match (l, r) {
        (Type::I64, Type::I64) => Ok(Type::I64),
        (Type::F64, Type::F64) => Ok(Type::F64),
        (Type::I64, Type::F64) | (Type::F64, Type::I64) => Ok(Type::F64),
        (a, b) => Err(TypeError::BadBinary(a, b)),
    }
}

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
        assert!(ty("let x: f64 = 1;").is_ok()); // i64 widens to f64
        assert!(ty("let x: i64 = 1;").is_ok());
    }

    #[test]
    fn let_annotation_mismatch() {
        // f64 cannot narrow to i64
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
        // i64 can be passed where f64 is expected
        assert_eq!(
            ty("fn id(x: f64) -> f64 { x } id(5)").unwrap(),
            Type::F64
        );
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
    fn block_scope_in_types() {
        // x rebinding inside a block doesn't leak out, but the program's
        // result is the tail expression `x` of the outer let.
        assert_eq!(
            ty("let x = 1; { let x = 2.0; x }").unwrap(),
            Type::F64
        );
    }
}
