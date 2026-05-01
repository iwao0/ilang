use std::collections::HashMap;

use ilang_ast::{BinOp, Block, Expr, FnDecl, Item, Program, Stmt, UnOp};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Unit,
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Float(x) => {
                if x.is_finite() && x.fract() == 0.0 {
                    write!(f, "{x:.1}")
                } else {
                    write!(f, "{x}")
                }
            }
            Value::Unit => write!(f, "()"),
        }
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum RuntimeError {
    #[error("integer division by zero")]
    DivisionByZero,
    #[error("integer overflow")]
    Overflow,
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
    #[error("recursion depth exceeded")]
    StackOverflow,
}

const MAX_DEPTH: usize = 256;

/// Persistent interpreter state — used by the REPL across input lines.
/// In file mode you can use [`run_program`] which builds a fresh `Interpreter`.
#[derive(Debug, Default)]
pub struct Interpreter {
    fns: HashMap<String, FnDecl>,
    vars: HashMap<String, Value>,
    depth: usize,
}

impl Interpreter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Execute a program. Items (fn declarations) are registered into the
    /// function table; statements update variable bindings; the trailing
    /// expression's value (or `Unit` if absent) is returned.
    pub fn run(&mut self, prog: &Program) -> Result<Value, RuntimeError> {
        for item in &prog.items {
            match item {
                Item::Fn(f) => {
                    self.fns.insert(f.name.clone(), f.clone());
                }
            }
        }
        let mut last = Value::Unit;
        for s in &prog.stmts {
            last = self.exec_stmt(s)?;
        }
        if let Some(tail) = &prog.tail {
            last = self.eval_expr(tail)?;
        }
        Ok(last)
    }

    fn exec_stmt(&mut self, stmt: &Stmt) -> Result<Value, RuntimeError> {
        match stmt {
            Stmt::Let { name, ty: _, value } => {
                let v = self.eval_expr(value)?;
                self.vars.insert(name.clone(), v);
                Ok(Value::Unit)
            }
            Stmt::Expr(e) => self.eval_expr(e),
        }
    }

    fn eval_expr(&mut self, expr: &Expr) -> Result<Value, RuntimeError> {
        match expr {
            Expr::Int(n) => Ok(Value::Int(*n)),
            Expr::Float(f) => Ok(Value::Float(*f)),
            Expr::Var(name) => self
                .vars
                .get(name)
                .copied()
                .ok_or_else(|| RuntimeError::UndefinedVariable(name.clone())),
            Expr::Unary { op, expr } => {
                let v = self.eval_expr(expr)?;
                apply_unary(*op, v)
            }
            Expr::Binary { op, lhs, rhs } => {
                let l = self.eval_expr(lhs)?;
                let r = self.eval_expr(rhs)?;
                apply_binary(*op, l, r)
            }
            Expr::Call { callee, args } => self.call(callee, args),
            Expr::Block(b) => self.eval_block(b),
        }
    }

    fn eval_block(&mut self, block: &Block) -> Result<Value, RuntimeError> {
        // Block introduces a fresh scope. We snapshot/restore the variable map
        // so let-bindings inside the block don't leak out (Rust-style scoping).
        let saved = self.vars.clone();
        let mut last = Value::Unit;
        for s in &block.stmts {
            last = self.exec_stmt(s)?;
        }
        if let Some(tail) = &block.tail {
            last = self.eval_expr(tail)?;
        }
        self.vars = saved;
        Ok(last)
    }

    fn call(&mut self, name: &str, args: &[Expr]) -> Result<Value, RuntimeError> {
        let evaluated: Vec<Value> = args
            .iter()
            .map(|a| self.eval_expr(a))
            .collect::<Result<_, _>>()?;
        let decl = self
            .fns
            .get(name)
            .cloned()
            .ok_or_else(|| RuntimeError::UndefinedFunction(name.to_string()))?;
        if decl.params.len() != evaluated.len() {
            return Err(RuntimeError::ArityMismatch {
                name: name.to_string(),
                expected: decl.params.len(),
                got: evaluated.len(),
            });
        }
        if self.depth >= MAX_DEPTH {
            return Err(RuntimeError::StackOverflow);
        }
        self.depth += 1;
        let saved_vars = std::mem::take(&mut self.vars);
        for (p, v) in decl.params.iter().zip(evaluated.into_iter()) {
            self.vars.insert(p.name.clone(), v);
        }
        let result = self.eval_block(&decl.body);
        self.vars = saved_vars;
        self.depth -= 1;
        result
    }
}

/// Convenience for one-shot evaluation (file mode).
pub fn run_program(prog: &Program) -> Result<Value, RuntimeError> {
    Interpreter::new().run(prog)
}

fn apply_unary(op: UnOp, v: Value) -> Result<Value, RuntimeError> {
    match (op, v) {
        (UnOp::Pos, v) => Ok(v),
        (UnOp::Neg, Value::Int(n)) => n.checked_neg().map(Value::Int).ok_or(RuntimeError::Overflow),
        (UnOp::Neg, Value::Float(f)) => Ok(Value::Float(-f)),
        (UnOp::Neg, Value::Unit) => Err(RuntimeError::UndefinedVariable("()".into())),
    }
}

fn apply_binary(op: BinOp, l: Value, r: Value) -> Result<Value, RuntimeError> {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => int_op(op, a, b),
        (Value::Unit, _) | (_, Value::Unit) => Err(RuntimeError::UndefinedVariable("()".into())),
        (a, b) => Ok(Value::Float(float_op(op, to_f64(a), to_f64(b)))),
    }
}

fn to_f64(v: Value) -> f64 {
    match v {
        Value::Int(n) => n as f64,
        Value::Float(f) => f,
        Value::Unit => 0.0, // unreachable in practice — apply_binary guards this
    }
}

fn int_op(op: BinOp, a: i64, b: i64) -> Result<Value, RuntimeError> {
    let r = match op {
        BinOp::Add => a.checked_add(b),
        BinOp::Sub => a.checked_sub(b),
        BinOp::Mul => a.checked_mul(b),
        BinOp::Div => {
            if b == 0 {
                return Err(RuntimeError::DivisionByZero);
            }
            a.checked_div(b)
        }
        BinOp::Rem => {
            if b == 0 {
                return Err(RuntimeError::DivisionByZero);
            }
            a.checked_rem(b)
        }
    };
    r.map(Value::Int).ok_or(RuntimeError::Overflow)
}

fn float_op(op: BinOp, a: f64, b: f64) -> f64 {
    match op {
        BinOp::Add => a + b,
        BinOp::Sub => a - b,
        BinOp::Mul => a * b,
        BinOp::Div => a / b,
        BinOp::Rem => a % b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ilang_lexer::tokenize;
    use ilang_parser::parse;

    fn run(src: &str) -> Result<Value, RuntimeError> {
        let toks = tokenize(src).unwrap();
        let prog = parse(&toks).unwrap();
        run_program(&prog)
    }

    #[test]
    fn int_arithmetic() {
        assert_eq!(run("1 + 2 * 3").unwrap(), Value::Int(7));
        assert_eq!(run("(1 + 2) * 3").unwrap(), Value::Int(9));
        assert_eq!(run("7 / 2").unwrap(), Value::Int(3));
    }

    #[test]
    fn float_promotion() {
        assert_eq!(run("7.0 / 2").unwrap(), Value::Float(3.5));
        assert_eq!(run("1 + 2.0").unwrap(), Value::Float(3.0));
    }

    #[test]
    fn let_and_use() {
        assert_eq!(run("let x = 1 + 2; x * 3").unwrap(), Value::Int(9));
        assert_eq!(
            run("let x = 1; let y = 2; x + y").unwrap(),
            Value::Int(3)
        );
    }

    #[test]
    fn undefined_variable() {
        assert_eq!(
            run("x + 1"),
            Err(RuntimeError::UndefinedVariable("x".into()))
        );
    }

    #[test]
    fn fn_call_basic() {
        let src = "fn add(a: i64, b: i64) -> i64 { a + b } add(2, 3)";
        assert_eq!(run(src).unwrap(), Value::Int(5));
    }

    #[test]
    fn fn_recursive() {
        // factorial via let-bound branch isn't possible without if; use a simpler recursion stop with subtraction depth not supported.
        // Instead test mutual reference: call from inside another fn.
        let src = "fn double(x: i64) -> i64 { x * 2 } fn quad(x: i64) -> i64 { double(double(x)) } quad(3)";
        assert_eq!(run(src).unwrap(), Value::Int(12));
    }

    #[test]
    fn block_scoping() {
        let src = "let x = 1; { let x = 99; x }";
        assert_eq!(run(src).unwrap(), Value::Int(99));
        let src = "let x = 1; { let y = 2; }; x";
        assert_eq!(run(src).unwrap(), Value::Int(1));
    }

    #[test]
    fn arity_mismatch() {
        let src = "fn id(x: i64) -> i64 { x } id(1, 2)";
        assert!(matches!(
            run(src),
            Err(RuntimeError::ArityMismatch { .. })
        ));
    }

    #[test]
    fn attribute_parses_but_does_not_enforce() {
        // #[requires(net)] is parsed and ignored at runtime in phase 2.
        let src = "#[requires(net)] fn f(x: i64) -> i64 { x + 1 } f(41)";
        assert_eq!(run(src).unwrap(), Value::Int(42));
    }

    #[test]
    fn division_by_zero_int() {
        assert_eq!(run("1 / 0"), Err(RuntimeError::DivisionByZero));
    }

    #[test]
    fn overflow_detected() {
        let src = format!("{} + 1", i64::MAX);
        assert_eq!(run(&src), Err(RuntimeError::Overflow));
    }

    #[test]
    fn repl_persistence() {
        let mut interp = Interpreter::new();
        let toks = tokenize("let x = 10;").unwrap();
        let p = parse(&toks).unwrap();
        assert_eq!(interp.run(&p).unwrap(), Value::Unit);

        let toks = tokenize("x + 5").unwrap();
        let p = parse(&toks).unwrap();
        assert_eq!(interp.run(&p).unwrap(), Value::Int(15));
    }
}
