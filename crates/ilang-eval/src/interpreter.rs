use std::collections::HashMap;

use ilang_ast::{Block, Expr, FnDecl, Item, LogicalOp, Program, Stmt};

use crate::error::RuntimeError;
use crate::ops::{apply_binary, apply_unary, as_bool};
use crate::value::Value;

const MAX_DEPTH: usize = 256;

/// Persistent interpreter state — used by the REPL across input lines.
/// In file mode you can use [`crate::run_program`] which builds a fresh `Interpreter`.
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
            Expr::Bool(b) => Ok(Value::Bool(*b)),
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
            Expr::Logical { op, lhs, rhs } => {
                let l = self.eval_expr(lhs)?;
                let lb = as_bool(l)?;
                match op {
                    LogicalOp::And => {
                        if !lb {
                            Ok(Value::Bool(false))
                        } else {
                            let r = self.eval_expr(rhs)?;
                            Ok(Value::Bool(as_bool(r)?))
                        }
                    }
                    LogicalOp::Or => {
                        if lb {
                            Ok(Value::Bool(true))
                        } else {
                            let r = self.eval_expr(rhs)?;
                            Ok(Value::Bool(as_bool(r)?))
                        }
                    }
                }
            }
            Expr::Call { callee, args } => self.call(callee, args),
            Expr::Block(b) => self.eval_block(b),
            Expr::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let c = self.eval_expr(cond)?;
                if as_bool(c)? {
                    self.eval_block(then_branch)
                } else if let Some(eb) = else_branch {
                    self.eval_expr(eb)
                } else {
                    Ok(Value::Unit)
                }
            }
            Expr::While { cond, body } => loop {
                let c = self.eval_expr(cond)?;
                if !as_bool(c)? {
                    break Ok(Value::Unit);
                }
                self.eval_block(body)?;
            },
            Expr::Assign { target, value } => {
                let v = self.eval_expr(value)?;
                if !self.vars.contains_key(target) {
                    return Err(RuntimeError::UndefinedVariable(target.clone()));
                }
                self.vars.insert(target.clone(), v);
                Ok(Value::Unit)
            }
        }
    }

    fn eval_block(&mut self, block: &Block) -> Result<Value, RuntimeError> {
        // Track let-bindings introduced in this block so they can be undone
        // on exit (Rust-style lexical scoping for `let`). Assignments to
        // *outer* variables, in contrast, must persist after the block ends —
        // that's how `while x < n { x = x + 1; }` updates `x` in the outer
        // scope. Recording previous values lets us restore shadowed bindings
        // without dropping concurrent assignments.
        let mut shadows: Vec<(String, Option<Value>)> = Vec::new();
        let mut last = Value::Unit;
        for s in &block.stmts {
            match s {
                Stmt::Let { name, ty: _, value } => {
                    let v = self.eval_expr(value)?;
                    let prev = self.vars.insert(name.clone(), v);
                    shadows.push((name.clone(), prev));
                    last = Value::Unit;
                }
                Stmt::Expr(e) => {
                    last = self.eval_expr(e)?;
                }
            }
        }
        if let Some(tail) = &block.tail {
            last = self.eval_expr(tail)?;
        }
        while let Some((name, prev)) = shadows.pop() {
            match prev {
                Some(v) => {
                    self.vars.insert(name, v);
                }
                None => {
                    self.vars.remove(&name);
                }
            }
        }
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
