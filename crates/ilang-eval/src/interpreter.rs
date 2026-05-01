use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use ilang_ast::{Block, ClassDecl, Expr, FnDecl, Item, LogicalOp, Program, Stmt};

use crate::error::RuntimeError;
use crate::ops::{apply_binary, apply_unary, as_bool};
use crate::value::{ObjectData, ObjectRef, Value};

const MAX_DEPTH: usize = 256;

/// Persistent interpreter state — used by the REPL across input lines.
#[derive(Debug, Default)]
pub struct Interpreter {
    fns: HashMap<String, FnDecl>,
    classes: HashMap<String, ClassDecl>,
    vars: HashMap<String, Value>,
    /// Current `this` binding (`Some` while executing a method body).
    this: Option<ObjectRef>,
    depth: usize,
}

impl Interpreter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn run(&mut self, prog: &Program) -> Result<Value, RuntimeError> {
        for item in &prog.items {
            match item {
                Item::Fn(f) => {
                    self.fns.insert(f.name.clone(), f.clone());
                }
                Item::Class(c) => {
                    self.classes.insert(c.name.clone(), c.clone());
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
            Expr::This => match &self.this {
                Some(o) => Ok(Value::Object(o.clone())),
                None => Err(RuntimeError::ThisOutsideMethod),
            },
            Expr::Var(name) => {
                if let Some(v) = self.vars.get(name) {
                    return Ok(v.clone());
                }
                if let Some(this) = &self.this {
                    let this = this.borrow();
                    if let Some(v) = this.fields.get(name) {
                        return Ok(v.clone());
                    }
                }
                Err(RuntimeError::UndefinedVariable(name.clone()))
            }
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
            Expr::Call { callee, args } => {
                if let Some(this) = self.this.clone() {
                    let class_name = this.borrow().class.clone();
                    if let Some(class) = self.classes.get(&class_name) {
                        if class.methods.iter().any(|m| m.name == *callee) {
                            return self.call_method(this, callee, args);
                        }
                    }
                }
                self.call_fn(callee, args)
            }
            Expr::Field { obj, name } => {
                let v = self.eval_expr(obj)?;
                let o = expect_object(v)?;
                let o = o.borrow();
                o.fields.get(name).cloned().ok_or_else(|| {
                    RuntimeError::UnknownField {
                        class: o.class.clone(),
                        field: name.clone(),
                    }
                })
            }
            Expr::MethodCall { obj, method, args } => {
                let v = self.eval_expr(obj)?;
                let o = expect_object(v)?;
                self.call_method(o, method, args)
            }
            Expr::New { class, args } => self.new_object(class, args),
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
                match self.eval_block(body) {
                    Ok(_) => {}
                    Err(RuntimeError::Break) => break Ok(Value::Unit),
                    Err(RuntimeError::Continue) => {}
                    Err(e) => break Err(e),
                }
            },
            Expr::Loop { body } => loop {
                match self.eval_block(body) {
                    Ok(_) => {}
                    Err(RuntimeError::Break) => break Ok(Value::Unit),
                    Err(RuntimeError::Continue) => {}
                    Err(e) => break Err(e),
                }
            },
            Expr::Break => Err(RuntimeError::Break),
            Expr::Continue => Err(RuntimeError::Continue),
            Expr::Assign { target, value } => {
                let v = self.eval_expr(value)?;
                if self.vars.contains_key(target) {
                    self.vars.insert(target.clone(), v);
                    return Ok(Value::Unit);
                }
                if let Some(this) = self.this.clone() {
                    // Symmetric with the read path: an unqualified assignment
                    // inside a method targets the field on `this` when no
                    // local shadows it. The type checker has already
                    // validated that the field exists, so we just write.
                    this.borrow_mut().fields.insert(target.clone(), v);
                    return Ok(Value::Unit);
                }
                Err(RuntimeError::UndefinedVariable(target.clone()))
            }
            Expr::AssignField { obj, field, value } => {
                // Existence of the field is validated by the type checker;
                // the runtime just stores the value (init creates the field
                // entry on first assignment).
                let v = self.eval_expr(value)?;
                let target = self.eval_expr(obj)?;
                let target = expect_object(target)?;
                target.borrow_mut().fields.insert(field.clone(), v);
                Ok(Value::Unit)
            }
        }
    }

    fn eval_block(&mut self, block: &Block) -> Result<Value, RuntimeError> {
        // Track let-bindings introduced in this block so they can be undone
        // on exit (Rust-style lexical scoping for `let`). Assignments to
        // outer variables persist after the block ends.
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

    fn call_fn(&mut self, name: &str, args: &[Expr]) -> Result<Value, RuntimeError> {
        let evaluated = self.eval_args(args)?;
        let decl = self
            .fns
            .get(name)
            .cloned()
            .ok_or_else(|| RuntimeError::UndefinedFunction(name.to_string()))?;
        self.invoke(name, &decl, evaluated, None)
    }

    fn call_method(
        &mut self,
        receiver: ObjectRef,
        method: &str,
        args: &[Expr],
    ) -> Result<Value, RuntimeError> {
        let evaluated = self.eval_args(args)?;
        let class_name = receiver.borrow().class.clone();
        let class = self
            .classes
            .get(&class_name)
            .cloned()
            .ok_or_else(|| RuntimeError::UndefinedClass(class_name.clone()))?;
        let decl = class
            .methods
            .iter()
            .find(|m| m.name == method)
            .cloned()
            .ok_or_else(|| RuntimeError::UnknownMethod {
                class: class_name,
                method: method.to_string(),
            })?;
        self.invoke(method, &decl, evaluated, Some(receiver))
    }

    fn new_object(&mut self, class: &str, args: &[Expr]) -> Result<Value, RuntimeError> {
        let evaluated = self.eval_args(args)?;
        let decl = self
            .classes
            .get(class)
            .cloned()
            .ok_or_else(|| RuntimeError::UndefinedClass(class.to_string()))?;
        let obj: ObjectRef = Rc::new(RefCell::new(ObjectData {
            class: class.to_string(),
            fields: HashMap::new(),
        }));
        if let Some(init) = decl.methods.iter().find(|m| m.name == "init").cloned() {
            self.invoke("init", &init, evaluated, Some(obj.clone()))?;
        } else if !evaluated.is_empty() {
            return Err(RuntimeError::ArityMismatch {
                name: format!("{class}::init"),
                expected: 0,
                got: evaluated.len(),
            });
        }
        Ok(Value::Object(obj))
    }

    fn eval_args(&mut self, args: &[Expr]) -> Result<Vec<Value>, RuntimeError> {
        args.iter().map(|a| self.eval_expr(a)).collect()
    }

    fn invoke(
        &mut self,
        name: &str,
        decl: &FnDecl,
        evaluated: Vec<Value>,
        receiver: Option<ObjectRef>,
    ) -> Result<Value, RuntimeError> {
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
        let saved_this = std::mem::replace(&mut self.this, receiver);
        for (p, v) in decl.params.iter().zip(evaluated.into_iter()) {
            self.vars.insert(p.name.clone(), v);
        }
        let result = self.eval_block(&decl.body);
        self.vars = saved_vars;
        self.this = saved_this;
        self.depth -= 1;
        // Defense in depth: the type checker rejects break/continue that
        // would escape the function, but if a malformed AST slips through we
        // surface it as a TypeError rather than letting the signal bubble up
        // and silently affect an outer loop in the caller.
        match result {
            Err(RuntimeError::Break) => {
                Err(RuntimeError::TypeError("`break` escaped function body".into()))
            }
            Err(RuntimeError::Continue) => Err(RuntimeError::TypeError(
                "`continue` escaped function body".into(),
            )),
            other => other,
        }
    }
}

fn expect_object(v: Value) -> Result<ObjectRef, RuntimeError> {
    match v {
        Value::Object(o) => Ok(o),
        other => Err(RuntimeError::NotAnObject(format!("{other}"))),
    }
}
