use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use ilang_ast::{
    Block, ClassDecl, CtorArgs, EnumDecl, Expr, ExprKind, FnDecl, Item, LogicalOp,
    PatternBindings, PatternKind, Program, Span, Stmt, StmtKind, VariantPayload,
};

use crate::error::RuntimeError;
use crate::ops::{apply_binary, apply_unary, as_bool, cast_value};
use crate::value::{EnumPayload, ObjectData, ObjectRef, Value};

const MAX_DEPTH: usize = 256;

#[derive(Debug, Default)]
pub struct Interpreter {
    fns: HashMap<String, FnDecl>,
    classes: HashMap<String, ClassDecl>,
    enums: HashMap<String, EnumDecl>,
    vars: HashMap<String, Value>,
    this: Option<ObjectRef>,
    depth: usize,
}

impl Interpreter {
    pub fn new() -> Self {
        let mut i = Self::default();
        i.install_builtins();
        i
    }

    /// Set up the singleton `console` object. Methods on it (currently just
    /// `log`) are dispatched in `call_method` before any user-class lookup,
    /// so no `FnDecl` body is needed.
    fn install_builtins(&mut self) {
        let console: ObjectRef = Rc::new(RefCell::new(ObjectData {
            class: "Console".to_string(),
            fields: HashMap::new(),
        }));
        self.vars
            .insert("console".to_string(), Value::Object(console));
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
                Item::Enum(e) => {
                    self.enums.insert(e.name.clone(), e.clone());
                }
                Item::Use(_) | Item::Const(_) => {}
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
        match &stmt.kind {
            StmtKind::Let { name, ty, value } => {
                let mut v = self.eval_expr(value)?;
                // A type annotation acts as an implicit cast: the runtime
                // representation must match the declared width so later
                // arithmetic dispatches to the right variant.
                if let Some(t) = ty {
                    v = cast_value(v, t);
                }
                self.vars.insert(name.clone(), v);
                Ok(Value::Unit)
            }
            StmtKind::Expr(e) => {
                let v = self.eval_expr(e)?;
                // Top-level expression statement: discard the value
                // and release any heap allocation it owned (matches
                // `eval_block`'s StmtKind::Expr handling).
                self.release(v);
                Ok(Value::Unit)
            }
        }
    }

    fn eval_expr(&mut self, expr: &Expr) -> Result<Value, RuntimeError> {
        let span = expr.span;
        match &expr.kind {
            ExprKind::Int(n) => Ok(Value::Int(*n)),
            ExprKind::Float(f) => Ok(Value::Float(*f)),
            ExprKind::Bool(b) => Ok(Value::Bool(*b)),
            ExprKind::Str(s) => Ok(Value::Str(Rc::new(s.clone()))),
            ExprKind::This => match &self.this {
                Some(o) => Ok(Value::Object(o.clone())),
                None => Err(RuntimeError::ThisOutsideMethod { span }),
            },
            ExprKind::Var(name) => {
                if let Some(v) = self.vars.get(name) {
                    return Ok(v.clone());
                }
                if let Some(this) = &self.this {
                    let this = this.borrow();
                    if let Some(v) = this.fields.get(name) {
                        return Ok(v.clone());
                    }
                }
                // First-class function: bare reference to a top-level
                // `fn` becomes a function value.
                if let Some(decl) = self.fns.get(name) {
                    return Ok(Value::Fn(Rc::new(decl.clone())));
                }
                Err(RuntimeError::UndefinedVariable {
                    name: name.clone(),
                    span,
                })
            }
            ExprKind::Unary { op, expr: inner } => {
                let v = self.eval_expr(inner)?;
                apply_unary(*op, v).map_err(|e| e.with_span(span))
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let l = self.eval_expr(lhs)?;
                let r = self.eval_expr(rhs)?;
                apply_binary(*op, l, r).map_err(|e| e.with_span(span))
            }
            ExprKind::Logical { op, lhs, rhs } => {
                let l = self.eval_expr(lhs)?;
                let lb = as_bool(l).map_err(|e| e.with_span(lhs.span))?;
                match op {
                    LogicalOp::And => {
                        if !lb {
                            Ok(Value::Bool(false))
                        } else {
                            let r = self.eval_expr(rhs)?;
                            Ok(Value::Bool(as_bool(r).map_err(|e| e.with_span(rhs.span))?))
                        }
                    }
                    LogicalOp::Or => {
                        if lb {
                            Ok(Value::Bool(true))
                        } else {
                            let r = self.eval_expr(rhs)?;
                            Ok(Value::Bool(as_bool(r).map_err(|e| e.with_span(rhs.span))?))
                        }
                    }
                }
            }
            ExprKind::Call { callee, args } => {
                // Indirect call through a function-typed local first
                // (matches the type checker's lookup order — locals
                // shadow methods and top-level fns).
                if let Some(Value::Fn(f)) = self.vars.get(callee).cloned() {
                    return self.invoke_fn_value(&f, args, span);
                }
                if let Some(this) = self.this.clone() {
                    let class_name = this.borrow().class.clone();
                    if let Some(class) = self.classes.get(&class_name) {
                        if class.methods.iter().any(|m| m.name == *callee) {
                            return self.call_method(this, callee, args, span);
                        }
                    }
                }
                self.call_fn(callee, args, span)
            }
            ExprKind::Field { obj, name } => {
                let v = self.eval_expr(obj)?;
                if let Value::Array(arr) = &v {
                    if name == "length" {
                        return Ok(Value::Int(arr.borrow().len() as i64));
                    }
                }
                if let Value::Str(s) = &v {
                    if name == "length" {
                        return Ok(Value::Int(s.chars().count() as i64));
                    }
                }
                let o = expect_object(v, obj.span)?;
                // Property getter: dispatch through the synthetic FnDecl.
                let class_name = o.borrow().class.clone();
                if let Some(getter) = self
                    .classes
                    .get(&class_name)
                    .and_then(|c| c.properties.iter().find(|p| &p.name == name))
                    .and_then(|p| p.getter.clone())
                {
                    return self.invoke(name, &getter, vec![], Some(o.clone()), span);
                }
                let o = o.borrow();
                o.fields.get(name).cloned().ok_or_else(|| {
                    RuntimeError::UnknownField {
                        class: o.class.clone(),
                        field: name.clone(),
                        span,
                    }
                })
            }
            ExprKind::MethodCall { obj, method, args } => {
                let v = self.eval_expr(obj)?;
                // Weak.get(): try to upgrade to a strong Object ref;
                // returns Optional<T>.
                if let Value::Weak(w) = &v {
                    if method == "get" {
                        return Ok(match w.upgrade() {
                            Some(obj) => Value::Some(Box::new(Value::Object(obj))),
                            std::option::Option::None => Value::None,
                        });
                    }
                    return Err(RuntimeError::UnknownMethod {
                        class: "weak".into(),
                        method: method.clone(),
                        span,
                    });
                }
                // Built-in Optional methods. The type checker has
                // verified arity (0 args).
                if matches!(v, Value::None | Value::Some(_)) {
                    match method.as_str() {
                        "isSome" => return Ok(Value::Bool(matches!(v, Value::Some(_)))),
                        "isNone" => return Ok(Value::Bool(matches!(v, Value::None))),
                        "unwrap" => {
                            return match v {
                                Value::Some(inner) => Ok(*inner),
                                Value::None => Err(RuntimeError::TypeError {
                                    msg: "unwrap on `none`".into(),
                                    span,
                                }),
                                _ => unreachable!(),
                            };
                        }
                        _ => {
                            return Err(RuntimeError::UnknownMethod {
                                class: "optional".into(),
                                method: method.clone(),
                                span,
                            });
                        }
                    }
                }
                if let Value::Array(arr) = &v {
                    if method == "push" {
                        // Type checker enforced arity 1 and dynamic-only.
                        let val = self.eval_expr(&args[0])?;
                        arr.borrow_mut().push(val);
                        return Ok(Value::Unit);
                    }
                    if method == "pop" {
                        let popped = arr.borrow_mut().pop();
                        return Ok(match popped {
                            Some(v) => Value::Some(Box::new(v)),
                            std::option::Option::None => Value::None,
                        });
                    }
                    if method == "indexOf" || method == "includes" {
                        let needle = self.eval_expr(&args[0])?;
                        let pos = arr
                            .borrow()
                            .iter()
                            .position(|x| x == &needle);
                        return Ok(if method == "indexOf" {
                            Value::Int(pos.map(|i| i as i64).unwrap_or(-1))
                        } else {
                            Value::Bool(pos.is_some())
                        });
                    }
                    if method == "slice" {
                        // slice(start, end) — JS-style: start inclusive,
                        // end exclusive; clamps to [0, len].
                        let start = match self.eval_expr(&args[0])? {
                            Value::Int(n) => n,
                            other => return Err(RuntimeError::TypeError {
                                msg: format!("slice start must be i64, got {other:?}"),
                                span: args[0].span,
                            }),
                        };
                        let end = match self.eval_expr(&args[1])? {
                            Value::Int(n) => n,
                            other => return Err(RuntimeError::TypeError {
                                msg: format!("slice end must be i64, got {other:?}"),
                                span: args[1].span,
                            }),
                        };
                        let inner = arr.borrow();
                        let len = inner.len() as i64;
                        let s = start.max(0).min(len) as usize;
                        let e_ = end.max(0).min(len) as usize;
                        let s = s.min(e_);
                        let out: Vec<Value> = inner[s..e_].to_vec();
                        return Ok(Value::Array(Rc::new(RefCell::new(out))));
                    }
                    if method == "map" || method == "filter" || method == "forEach" {
                        let f = self.eval_expr(&args[0])?;
                        let decl = match &f {
                            Value::Fn(d) => d.clone(),
                            other => return Err(RuntimeError::TypeError {
                                msg: format!("{method} expects a function, got {other:?}"),
                                span: args[0].span,
                            }),
                        };
                        let snapshot: Vec<Value> = arr.borrow().clone();
                        match method.as_str() {
                            "map" => {
                                let mut out = Vec::with_capacity(snapshot.len());
                                for x in snapshot {
                                    let r = self.invoke(&decl.name, &decl, vec![x], None, span)?;
                                    out.push(r);
                                }
                                return Ok(Value::Array(Rc::new(RefCell::new(out))));
                            }
                            "filter" => {
                                let mut out = Vec::new();
                                for x in snapshot {
                                    let r = self.invoke(&decl.name, &decl, vec![x.clone()], None, span)?;
                                    match r {
                                        Value::Bool(true) => out.push(x),
                                        Value::Bool(false) => {}
                                        other => return Err(RuntimeError::TypeError {
                                            msg: format!("filter predicate must return bool, got {other:?}"),
                                            span,
                                        }),
                                    }
                                }
                                return Ok(Value::Array(Rc::new(RefCell::new(out))));
                            }
                            "forEach" => {
                                for x in snapshot {
                                    self.invoke(&decl.name, &decl, vec![x], None, span)?;
                                }
                                return Ok(Value::Unit);
                            }
                            _ => unreachable!(),
                        }
                    }
                    return Err(RuntimeError::UnknownMethod {
                        class: "array".into(),
                        method: method.clone(),
                        span,
                    });
                }
                if let Value::Map(m) = &v {
                    let m = m.clone();
                    match method.as_str() {
                        "get" => {
                            let kv = self.eval_expr(&args[0])?;
                            let key = crate::value::MapKey::from_value(&kv).ok_or_else(|| {
                                RuntimeError::TypeError {
                                    msg: format!("invalid map key value {kv:?}"),
                                    span: args[0].span,
                                }
                            })?;
                            return Ok(match m.borrow().get(&key) {
                                Some(v) => Value::Some(Box::new(v.clone())),
                                None => Value::None,
                            });
                        }
                        "set" => {
                            let kv = self.eval_expr(&args[0])?;
                            let vv = self.eval_expr(&args[1])?;
                            let key = crate::value::MapKey::from_value(&kv).ok_or_else(|| {
                                RuntimeError::TypeError {
                                    msg: format!("invalid map key value {kv:?}"),
                                    span: args[0].span,
                                }
                            })?;
                            if let Some(old) = m.borrow_mut().insert(key, vv) {
                                self.release(old);
                            }
                            return Ok(Value::Unit);
                        }
                        "has" => {
                            let kv = self.eval_expr(&args[0])?;
                            let key = crate::value::MapKey::from_value(&kv).ok_or_else(|| {
                                RuntimeError::TypeError {
                                    msg: format!("invalid map key value {kv:?}"),
                                    span: args[0].span,
                                }
                            })?;
                            return Ok(Value::Bool(m.borrow().contains_key(&key)));
                        }
                        "delete" => {
                            let kv = self.eval_expr(&args[0])?;
                            let key = crate::value::MapKey::from_value(&kv).ok_or_else(|| {
                                RuntimeError::TypeError {
                                    msg: format!("invalid map key value {kv:?}"),
                                    span: args[0].span,
                                }
                            })?;
                            let removed = m.borrow_mut().remove(&key);
                            let was_present = removed.is_some();
                            if let Some(old) = removed {
                                self.release(old);
                            }
                            return Ok(Value::Bool(was_present));
                        }
                        "size" => {
                            return Ok(Value::Int(m.borrow().len() as i64));
                        }
                        "keys" => {
                            let ks: Vec<Value> = m
                                .borrow()
                                .keys()
                                .cloned()
                                .map(|k| k.into_value())
                                .collect();
                            return Ok(Value::Array(Rc::new(RefCell::new(ks))));
                        }
                        "values" => {
                            let vs: Vec<Value> = m.borrow().values().cloned().collect();
                            return Ok(Value::Array(Rc::new(RefCell::new(vs))));
                        }
                        _ => {
                            return Err(RuntimeError::UnknownMethod {
                                class: "Map".into(),
                                method: method.clone(),
                                span,
                            });
                        }
                    }
                }
                if let Value::Str(s) = &v {
                    let s = s.clone();
                    match method.as_str() {
                        "charAt" => {
                            let idx = match self.eval_expr(&args[0])? {
                                Value::Int(n) => n,
                                other => {
                                    return Err(RuntimeError::TypeError {
                                        msg: format!("charAt expects int, got {other:?}"),
                                        span,
                                    });
                                }
                            };
                            let out = if idx < 0 {
                                String::new()
                            } else {
                                s.chars().nth(idx as usize).map(|c| c.to_string()).unwrap_or_default()
                            };
                            return Ok(Value::Str(Rc::new(out)));
                        }
                        "includes" | "startsWith" | "endsWith" => {
                            let arg = match self.eval_expr(&args[0])? {
                                Value::Str(t) => t,
                                other => {
                                    return Err(RuntimeError::TypeError {
                                        msg: format!("{method} expects string, got {other:?}"),
                                        span,
                                    });
                                }
                            };
                            let r = match method.as_str() {
                                "includes" => s.contains(arg.as_str()),
                                "startsWith" => s.starts_with(arg.as_str()),
                                "endsWith" => s.ends_with(arg.as_str()),
                                _ => unreachable!(),
                            };
                            return Ok(Value::Bool(r));
                        }
                        "toUpper" => return Ok(Value::Str(Rc::new(s.to_uppercase()))),
                        "toLower" => return Ok(Value::Str(Rc::new(s.to_lowercase()))),
                        "trim" => return Ok(Value::Str(Rc::new(s.trim().to_string()))),
                        "replace" => {
                            // Replace ALL occurrences (Rust-style). JS's
                            // single-occurrence replace is reachable via
                            // future `replaceFirst` if needed.
                            let needle = match self.eval_expr(&args[0])? {
                                Value::Str(s) => s,
                                other => return Err(RuntimeError::TypeError {
                                    msg: format!("replace needs string, got {other:?}"),
                                    span,
                                }),
                            };
                            let repl = match self.eval_expr(&args[1])? {
                                Value::Str(s) => s,
                                other => return Err(RuntimeError::TypeError {
                                    msg: format!("replace needs string, got {other:?}"),
                                    span,
                                }),
                            };
                            return Ok(Value::Str(Rc::new(s.replace(needle.as_str(), repl.as_str()))));
                        }
                        "split" => {
                            let sep = match self.eval_expr(&args[0])? {
                                Value::Str(s) => s,
                                other => return Err(RuntimeError::TypeError {
                                    msg: format!("split needs string, got {other:?}"),
                                    span,
                                }),
                            };
                            let parts: Vec<Value> = if sep.is_empty() {
                                // Empty separator: split into individual
                                // chars (mirrors JS behavior).
                                s.chars().map(|c| Value::Str(Rc::new(c.to_string()))).collect()
                            } else {
                                s.split(sep.as_str())
                                    .map(|p| Value::Str(Rc::new(p.to_string())))
                                    .collect()
                            };
                            return Ok(Value::Array(Rc::new(RefCell::new(parts))));
                        }
                        "slice" => {
                            // start / end can be any int width — coerced
                            // to i64 here. Indices are clamped to
                            // [0, len_chars] and operate on Unicode code
                            // points (mirrors `.length` / `charAt`).
                            let to_i64 = |v: Value| -> Result<i64, RuntimeError> {
                                match v {
                                    Value::Int(n) => Ok(n),
                                    Value::Int8(n) => Ok(n as i64),
                                    Value::Int16(n) => Ok(n as i64),
                                    Value::Int32(n) => Ok(n as i64),
                                    Value::UInt8(n) => Ok(n as i64),
                                    Value::UInt16(n) => Ok(n as i64),
                                    Value::UInt32(n) => Ok(n as i64),
                                    Value::UInt64(n) => Ok(n as i64),
                                    other => Err(RuntimeError::TypeError {
                                        msg: format!("slice index must be int, got {other:?}"),
                                        span,
                                    }),
                                }
                            };
                            let start = to_i64(self.eval_expr(&args[0])?)?;
                            let end = to_i64(self.eval_expr(&args[1])?)?;
                            let chars: Vec<char> = s.chars().collect();
                            let len = chars.len() as i64;
                            let s_idx = start.max(0).min(len) as usize;
                            let e_idx = end.max(0).min(len) as usize;
                            let s_idx = s_idx.min(e_idx);
                            let out: String = chars[s_idx..e_idx].iter().collect();
                            return Ok(Value::Str(Rc::new(out)));
                        }
                        _ => {
                            return Err(RuntimeError::UnknownMethod {
                                class: "string".into(),
                                method: method.clone(),
                                span,
                            });
                        }
                    }
                }
                let o = expect_object(v, obj.span)?;
                self.call_method(o, method, args, span)
            }
            ExprKind::New { class, type_args: _, args, init_method } => {
                self.new_object(class, args, init_method.as_deref(), span)
            }
            ExprKind::Block(b) => self.eval_block(b),
            ExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let c = self.eval_expr(cond)?;
                if as_bool(c).map_err(|e| e.with_span(cond.span))? {
                    self.eval_block(then_branch)
                } else if let Some(eb) = else_branch {
                    self.eval_expr(eb)
                } else {
                    Ok(Value::Unit)
                }
            }
            ExprKind::While { cond, body } => loop {
                let c = self.eval_expr(cond)?;
                if !as_bool(c).map_err(|e| e.with_span(cond.span))? {
                    break Ok(Value::Unit);
                }
                match self.eval_block(body) {
                    Ok(_) => {}
                    Err(RuntimeError::Break(_)) => break Ok(Value::Unit),
                    Err(RuntimeError::Continue) => {}
                    Err(e) => break Err(e),
                }
            },
            ExprKind::Loop { body } => loop {
                match self.eval_block(body) {
                    Ok(_) => {}
                    Err(RuntimeError::Break(v)) => break Ok(v),
                    Err(RuntimeError::Continue) => {}
                    Err(e) => break Err(e),
                }
            },
            ExprKind::ForIn { var, iter, body } => {
                // Range iter is special-cased: don't eval it as a value
                // (Range has no Value representation by design).
                if let ExprKind::Range { start, end, inclusive } = &iter.kind {
                    let s = match self.eval_expr(start)? {
                        Value::Int(n) => n,
                        other => {
                            return Err(RuntimeError::TypeError {
                                msg: format!("range start must be integer, got {other:?}"),
                                span: start.span,
                            });
                        }
                    };
                    let e = match self.eval_expr(end)? {
                        Value::Int(n) => n,
                        other => {
                            return Err(RuntimeError::TypeError {
                                msg: format!("range end must be integer, got {other:?}"),
                                span: end.span,
                            });
                        }
                    };
                    let prev = self.vars.remove(var);
                    let mut result: Result<Value, RuntimeError> = Ok(Value::Unit);
                    let mut i = s;
                    let limit_open = if *inclusive { e + 1 } else { e };
                    while i < limit_open {
                        self.vars.insert(var.clone(), Value::Int(i));
                        match self.eval_block(body) {
                            Ok(_) => {}
                            Err(RuntimeError::Break(_)) => break,
                            Err(RuntimeError::Continue) => {}
                            Err(err) => {
                                result = Err(err);
                                break;
                            }
                        }
                        i += 1;
                    }
                    self.vars.remove(var);
                    if let Some(p) = prev {
                        self.vars.insert(var.clone(), p);
                    }
                    return result;
                }
                let it = self.eval_expr(iter)?;
                let arr = match it {
                    Value::Array(a) => a,
                    other => {
                        return Err(RuntimeError::TypeError {
                            msg: format!("for-in expects array, got {other:?}"),
                            span: iter.span,
                        });
                    }
                };
                let prev = self.vars.remove(var);
                let len = arr.borrow().len();
                let mut result: Result<Value, RuntimeError> = Ok(Value::Unit);
                for i in 0..len {
                    let v = arr.borrow()[i].clone();
                    self.vars.insert(var.clone(), v);
                    match self.eval_block(body) {
                        Ok(_) => {}
                        Err(RuntimeError::Break(_)) => break,
                        Err(RuntimeError::Continue) => {}
                        Err(e) => {
                            result = Err(e);
                            break;
                        }
                    }
                }
                self.vars.remove(var);
                if let Some(p) = prev {
                    self.vars.insert(var.clone(), p);
                }
                result
            }
            ExprKind::Range { .. } => Err(RuntimeError::TypeError {
                msg: "range expression is only valid as a `for-in` iterator".into(),
                span,
            }),
            ExprKind::Break(opt) => {
                let v = match opt {
                    Some(e) => self.eval_expr(e)?,
                    None => Value::Unit,
                };
                Err(RuntimeError::Break(v))
            }
            ExprKind::Continue => Err(RuntimeError::Continue),
            ExprKind::Return(value) => {
                let v = match value {
                    Some(e) => self.eval_expr(e)?,
                    None => Value::Unit,
                };
                Err(RuntimeError::Return(v))
            }
            ExprKind::Assign { target, value } => {
                let v = self.eval_expr(value)?;
                if self.vars.contains_key(target) {
                    let old = self.vars.insert(target.clone(), v);
                    if let Some(o) = old {
                        self.release(o);
                    }
                    return Ok(Value::Unit);
                }
                if let Some(this) = self.this.clone() {
                    let old = this.borrow_mut().fields.insert(target.clone(), v);
                    if let Some(o) = old {
                        self.release(o);
                    }
                    return Ok(Value::Unit);
                }
                Err(RuntimeError::UndefinedVariable {
                    name: target.clone(),
                    span,
                })
            }
            ExprKind::Cast { expr: inner, ty } => {
                let v = self.eval_expr(inner)?;
                Ok(cast_value(v, ty))
            }
            ExprKind::FnExpr { params, ret, body } => {
                // Build a synthetic FnDecl on the fly — empty name +
                // no captures. Cheap because the body is moved into
                // an Rc once.
                let decl = ilang_ast::FnDecl {
                    attrs: Vec::new(),
                    name: String::new(),
                    type_params: Vec::new(),
                    params: params.clone(),
                    ret: ret.clone(),
                    body: body.clone(),
                    span,
                };
                Ok(Value::Fn(Rc::new(decl)))
            }
            ExprKind::Array(elements) => {
                let mut vals = Vec::with_capacity(elements.len());
                for e in elements {
                    vals.push(self.eval_expr(e)?);
                }
                Ok(Value::Array(Rc::new(RefCell::new(vals))))
            }
            ExprKind::MapLit(entries) => {
                let mut m: std::collections::HashMap<crate::value::MapKey, Value> =
                    std::collections::HashMap::with_capacity(entries.len());
                for (k_expr, v_expr) in entries {
                    let kv = self.eval_expr(k_expr)?;
                    let vv = self.eval_expr(v_expr)?;
                    let key = crate::value::MapKey::from_value(&kv).ok_or_else(|| {
                        RuntimeError::TypeError {
                            msg: format!("invalid map key value {kv:?}"),
                            span: k_expr.span,
                        }
                    })?;
                    m.insert(key, vv);
                }
                Ok(Value::Map(Rc::new(RefCell::new(m))))
            }
            ExprKind::Index { obj, index } => {
                let target = self.eval_expr(obj)?;
                let idx = self.eval_expr(index)?;
                if let Value::Map(m) = target {
                    let key = crate::value::MapKey::from_value(&idx).ok_or_else(|| {
                        RuntimeError::TypeError {
                            msg: format!("invalid map key value {idx:?}"),
                            span: index.span,
                        }
                    })?;
                    return m.borrow().get(&key).cloned().ok_or_else(|| {
                        RuntimeError::TypeError {
                            msg: "map key not found".into(),
                            span,
                        }
                    });
                }
                let i = index_to_usize(idx, index.span)?;
                let arr = expect_array(target, obj.span)?;
                let arr = arr.borrow();
                arr.get(i)
                    .cloned()
                    .ok_or_else(|| RuntimeError::IndexOutOfBounds {
                        index: i as i64,
                        len: arr.len() as i64,
                        span,
                    })
            }
            ExprKind::AssignIndex { obj, index, value } => {
                let target = self.eval_expr(obj)?;
                let idx = self.eval_expr(index)?;
                let v = self.eval_expr(value)?;
                if let Value::Map(m) = target {
                    let key = crate::value::MapKey::from_value(&idx).ok_or_else(|| {
                        RuntimeError::TypeError {
                            msg: format!("invalid map key value {idx:?}"),
                            span: index.span,
                        }
                    })?;
                    if let Some(old) = m.borrow_mut().insert(key, v) {
                        self.release(old);
                    }
                    return Ok(Value::Unit);
                }
                let i = index_to_usize(idx, index.span)?;
                let arr = expect_array(target, obj.span)?;
                let mut arr = arr.borrow_mut();
                if i >= arr.len() {
                    return Err(RuntimeError::IndexOutOfBounds {
                        index: i as i64,
                        len: arr.len() as i64,
                        span,
                    });
                }
                let old = std::mem::replace(&mut arr[i], v);
                drop(arr);
                self.release(old);
                Ok(Value::Unit)
            }
            ExprKind::AssignField { obj, field, value } => {
                let v = self.eval_expr(value)?;
                let target = self.eval_expr(obj)?;
                let target = expect_object(target, obj.span)?;
                let class_name = target.borrow().class.clone();
                // Property setter: dispatch to the synthetic FnDecl
                // before falling back to direct field write.
                if let Some(setter) = self
                    .classes
                    .get(&class_name)
                    .and_then(|c| c.properties.iter().find(|p| &p.name == field))
                    .and_then(|p| p.setter.clone())
                {
                    // Cast incoming value to the setter's param type so
                    // Optional auto-wrap / Weak auto-downgrade rules
                    // match field-write behavior.
                    let v = cast_value(v, &setter.params[0].ty);
                    self.invoke(field, &setter, vec![v], Some(target.clone()), span)?;
                    return Ok(Value::Unit);
                }
                // Apply the field's declared type as an implicit cast,
                // mirroring `let x: T = ...`. This covers auto-wrap to
                // Optional and auto-downgrade Object → Weak.
                let field_ty = self
                    .classes
                    .get(&class_name)
                    .and_then(|c| c.fields.iter().find(|f| f.name == *field))
                    .map(|f| f.ty.clone());
                let v = match field_ty {
                    Some(t) => cast_value(v, &t),
                    None => v,
                };
                let old = target.borrow_mut().fields.insert(field.clone(), v);
                if let Some(o) = old {
                    self.release(o);
                }
                Ok(Value::Unit)
            }
            ExprKind::None => Ok(Value::None),
            ExprKind::Some(inner) => {
                let v = self.eval_expr(inner)?;
                Ok(Value::Some(Box::new(v)))
            }
            ExprKind::IfLet {
                name,
                expr,
                then_branch,
                else_branch,
            } => {
                let scrut = self.eval_expr(expr)?;
                match scrut {
                    Value::Some(inner) => {
                        // Bind `name` for the then-branch only, then
                        // restore the prior binding (mirrors the shadow
                        // discipline of `let` inside `eval_block`).
                        let prev = self.vars.insert(name.clone(), *inner);
                        let result = self.eval_block(then_branch);
                        let outgoing = match prev {
                            Some(v) => self.vars.insert(name.clone(), v),
                            None => self.vars.remove(name),
                        };
                        if let Some(v) = outgoing {
                            self.release(v);
                        }
                        result
                    }
                    Value::None => match else_branch {
                        Some(e) => self.eval_expr(e),
                        None => Ok(Value::Unit),
                    },
                    other => Err(RuntimeError::TypeError {
                        msg: format!("if-let on non-optional value {other}"),
                        span,
                    }),
                }
            }
            ExprKind::EnumCtor {
                enum_name,
                variant,
                args,
            } => {
                let payload = match args {
                    CtorArgs::Unit => EnumPayload::Unit,
                    CtorArgs::Tuple(elems) => {
                        let mut vs = Vec::with_capacity(elems.len());
                        for e in elems {
                            vs.push(self.eval_expr(e)?);
                        }
                        // Cast to declared payload types so the runtime
                        // representation matches (e.g. f64 → f32 narrows
                        // when the variant declared f32).
                        if let Some(decl) = self.enums.get(enum_name).cloned() {
                            if let Some(v) = decl.variants.iter().find(|v| v.name == *variant) {
                                if let VariantPayload::Tuple(tys) = &v.payload {
                                    for (i, t) in tys.iter().enumerate() {
                                        if let Some(slot) = vs.get_mut(i) {
                                            *slot = cast_value(slot.clone(), t);
                                        }
                                    }
                                }
                            }
                        }
                        EnumPayload::Tuple(vs)
                    }
                    CtorArgs::Struct(pairs) => {
                        let decl_payload = self
                            .enums
                            .get(enum_name)
                            .and_then(|d| {
                                d.variants
                                    .iter()
                                    .find(|v| v.name == *variant)
                                    .map(|v| v.payload.clone())
                            });
                        let mut fs = HashMap::new();
                        for (name, e) in pairs {
                            let mut v = self.eval_expr(e)?;
                            if let Some(VariantPayload::Struct(decl_fields)) = &decl_payload {
                                if let Some(fty) =
                                    decl_fields.iter().find(|f| f.name == *name).map(|f| f.ty.clone())
                                {
                                    v = cast_value(v, &fty);
                                }
                            }
                            fs.insert(name.clone(), v);
                        }
                        EnumPayload::Struct(fs)
                    }
                };
                Ok(Value::Enum {
                    ty: enum_name.clone(),
                    variant: variant.clone(),
                    payload,
                })
            }
            ExprKind::Match { scrutinee, arms } => {
                let v = self.eval_expr(scrutinee)?;
                let (sv_ty, sv_var, sv_payload) = match v {
                    Value::Enum { ty, variant, payload } => (ty, variant, payload),
                    other => {
                        return Err(RuntimeError::TypeError {
                            msg: format!("match on non-enum value {other}"),
                            span,
                        });
                    }
                };
                for arm in arms {
                    match &arm.pattern.kind {
                        PatternKind::Wildcard => {
                            return self.eval_expr(&arm.body);
                        }
                        PatternKind::Variant {
                            enum_name: _,
                            variant,
                            bindings,
                        } if *variant == sv_var => {
                            // Bind payload, run body, restore env.
                            let mut shadows: Vec<(String, Option<Value>)> = Vec::new();
                            match (bindings, &sv_payload) {
                                (PatternBindings::Unit, EnumPayload::Unit) => {}
                                (
                                    PatternBindings::Tuple(names),
                                    EnumPayload::Tuple(values),
                                ) => {
                                    for (n, val) in names.iter().zip(values.iter()) {
                                        if n != "_" {
                                            let prev =
                                                self.vars.insert(n.clone(), val.clone());
                                            shadows.push((n.clone(), prev));
                                        }
                                    }
                                }
                                (
                                    PatternBindings::Struct(pairs),
                                    EnumPayload::Struct(values),
                                ) => {
                                    for (fname, bname) in pairs {
                                        if bname != "_" {
                                            if let Some(val) = values.get(fname) {
                                                let prev = self
                                                    .vars
                                                    .insert(bname.clone(), val.clone());
                                                shadows.push((bname.clone(), prev));
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                            let result = self.eval_expr(&arm.body);
                            while let Some((n, prev)) = shadows.pop() {
                                let outgoing = match prev {
                                    Some(p) => self.vars.insert(n, p),
                                    None => self.vars.remove(&n),
                                };
                                if let Some(o) = outgoing {
                                    self.release(o);
                                }
                            }
                            return result;
                        }
                        _ => continue,
                    }
                }
                Err(RuntimeError::TypeError {
                    msg: format!("match: no arm matched variant {sv_ty}::{sv_var}"),
                    span,
                })
            }
        }
    }

    fn eval_block(&mut self, block: &Block) -> Result<Value, RuntimeError> {
        let mut shadows: Vec<(String, Option<Value>)> = Vec::new();
        // Run the body, capturing any control-flow Err so we can run the
        // scope-end shadow cleanup before propagating it up. Otherwise an
        // early `return`/`break`/`continue` would skip releasing this
        // block's bindings and their deinits would never fire.
        let result: Result<Value, RuntimeError> = (|| {
            let mut last = Value::Unit;
            for s in &block.stmts {
                match &s.kind {
                    StmtKind::Let { name, ty, value } => {
                        let mut v = self.eval_expr(value)?;
                        if let Some(t) = ty {
                            v = cast_value(v, t);
                        }
                        let prev = self.vars.insert(name.clone(), v);
                        shadows.push((name.clone(), prev));
                        last = Value::Unit;
                    }
                    StmtKind::Expr(e) => {
                        let v = self.eval_expr(e)?;
                        // Statement value is discarded. If it was a
                        // fresh heap allocation (e.g. `new C(...)` as
                        // its own expression statement), release it
                        // now so deinit fires — matches the JIT, which
                        // emits a release for non-aliased heap stmts.
                        // Aliased reads (rc>1) and primitives are
                        // no-ops inside `release`.
                        self.release(v);
                        last = Value::Unit;
                    }
                }
            }
            if let Some(tail) = &block.tail {
                last = self.eval_expr(tail)?;
            }
            Ok(last)
        })();
        while let Some((name, prev)) = shadows.pop() {
            // Restore the prior binding (or remove it). The displaced value
            // — the one this `let` introduced into scope — is then released
            // so its `deinit` runs if no other binding still points to it.
            let outgoing = match prev {
                Some(v) => self.vars.insert(name, v),
                None => self.vars.remove(&name),
            };
            if let Some(v) = outgoing {
                self.release(v);
            }
        }
        result
    }

    /// Drop a value that is leaving scope. The release path is recursive:
    ///
    /// - For an `Object` whose only remaining strong reference is the
    ///   binding being removed, the class's `deinit` (if any) runs while
    ///   fields are still live, and then each field is released in turn.
    ///   Errors inside `deinit` are reported to stderr and swallowed —
    ///   destructors must not surface failures up the stack.
    /// - For an `Array` similarly: when uniquely owned, every element is
    ///   released, so e.g. `let xs: Foo[] = [...]` going out of scope
    ///   fires `deinit` on each `Foo`.
    /// - Other variants need no cleanup.
    ///
    /// Cyclic references are not yet collected (no weak refs); they leak.
    fn release(&mut self, v: Value) {
        match v {
            Value::Object(obj) => {
                if Rc::strong_count(&obj) != 1 {
                    return;
                }
                let class_name = obj.borrow().class.clone();
                if let Some(cls) = self.classes.get(&class_name).cloned() {
                    if let Some(deinit) =
                        cls.methods.iter().find(|m| m.name == "deinit").cloned()
                    {
                        if let Err(e) = self.invoke(
                            "deinit",
                            &deinit,
                            vec![],
                            Some(obj.clone()),
                            deinit.span,
                        ) {
                            eprintln!("error in deinit for {class_name}: {e}");
                        }
                    }
                }
                // Release fields after `deinit` ran. Take the map out so
                // we never hold a borrow while recursing.
                let fields = std::mem::take(&mut obj.borrow_mut().fields);
                for (_, v) in fields {
                    self.release(v);
                }
            }
            Value::Array(arr) => {
                if Rc::strong_count(&arr) != 1 {
                    return;
                }
                let elements = std::mem::take(&mut *arr.borrow_mut());
                for v in elements {
                    self.release(v);
                }
            }
            Value::Enum { payload, .. } => match payload {
                EnumPayload::Unit => {}
                EnumPayload::Tuple(items) => {
                    for v in items {
                        self.release(v);
                    }
                }
                EnumPayload::Struct(fields) => {
                    for (_, v) in fields {
                        self.release(v);
                    }
                }
            },
            Value::Some(boxed) => self.release(*boxed),
            _ => {}
        }
    }

    fn call_fn(&mut self, name: &str, args: &[Expr], span: Span) -> Result<Value, RuntimeError> {
        let evaluated = self.eval_args(args)?;
        let decl = self
            .fns
            .get(name)
            .cloned()
            .ok_or_else(|| RuntimeError::UndefinedFunction {
                name: name.to_string(),
                span,
            })?;
        self.invoke(name, &decl, evaluated, None, span)
    }

    /// Invoke a `Value::Fn` with the given arg expressions. Used for
    /// indirect calls (locals bound to a function value or anonymous
    /// `fn(...) { ... }` expressions).
    fn invoke_fn_value(
        &mut self,
        decl: &ilang_ast::FnDecl,
        args: &[Expr],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let evaluated = self.eval_args(args)?;
        let name = if decl.name.is_empty() {
            "<closure>"
        } else {
            decl.name.as_str()
        };
        self.invoke(name, decl, evaluated, None, span)
    }

    fn call_method(
        &mut self,
        receiver: ObjectRef,
        method: &str,
        args: &[Expr],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let evaluated = self.eval_args(args)?;
        let class_name = receiver.borrow().class.clone();
        if class_name == "Console" && method == "log" {
            // Variadic: print every argument separated by a single space,
            // matching the JS `console.log(...)` convention. Zero args
            // prints just a newline.
            let parts: Vec<String> = evaluated.iter().map(|v| format!("{v}")).collect();
            println!("{}", parts.join(" "));
            return Ok(Value::Unit);
        }
        let class = self
            .classes
            .get(&class_name)
            .cloned()
            .ok_or_else(|| RuntimeError::UndefinedClass {
                name: class_name.clone(),
                span,
            })?;
        let decl = class
            .methods
            .iter()
            .find(|m| m.name == method)
            .cloned()
            .ok_or_else(|| RuntimeError::UnknownMethod {
                class: class_name,
                method: method.to_string(),
                span,
            })?;
        self.invoke(method, &decl, evaluated, Some(receiver), span)
    }

    fn new_object(
        &mut self,
        class: &str,
        args: &[Expr],
        init_method: Option<&str>,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        // Built-in `new Map<K, V>()` — returns an empty Value::Map.
        // The type checker has already verified the arity (no args)
        // and that K is a valid key type.
        if class == "Map" {
            if !args.is_empty() {
                return Err(RuntimeError::ArityMismatch {
                    name: "Map::init".into(),
                    expected: 0,
                    got: args.len(),
                    span,
                });
            }
            return Ok(Value::Map(Rc::new(RefCell::new(
                std::collections::HashMap::new(),
            ))));
        }
        let evaluated = self.eval_args(args)?;
        let decl = self
            .classes
            .get(class)
            .cloned()
            .ok_or_else(|| RuntimeError::UndefinedClass {
                name: class.to_string(),
                span,
            })?;
        // Default-initialize every declared field. Mirrors the JIT's
        // `alloc_zeroed`: primitive fields get 0/false/"" defaults so
        // a class without an explicit init is still safe to read from.
        // The `init` (if any) overwrites these immediately.
        let mut fields = HashMap::new();
        for f in &decl.fields {
            fields.insert(f.name.clone(), default_value(&f.ty));
        }
        let obj: ObjectRef = Rc::new(RefCell::new(ObjectData {
            class: class.to_string(),
            fields,
        }));
        // Pick the init to run. The mangler may have set
        // `init_method` to a specific overload mangle (e.g.
        // `init__i64`); fall back to plain `"init"` for the common
        // non-overloaded case.
        let init_lookup = init_method.unwrap_or("init");
        if let Some(init) = decl.methods.iter().find(|m| m.name == init_lookup).cloned() {
            self.invoke(init_lookup, &init, evaluated, Some(obj.clone()), span)?;
        } else if !evaluated.is_empty() {
            return Err(RuntimeError::ArityMismatch {
                name: format!("{class}::init"),
                expected: 0,
                got: evaluated.len(),
                span,
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
        call_span: Span,
    ) -> Result<Value, RuntimeError> {
        if decl.params.len() != evaluated.len() {
            return Err(RuntimeError::ArityMismatch {
                name: name.to_string(),
                expected: decl.params.len(),
                got: evaluated.len(),
                span: call_span,
            });
        }
        // `@extern` fns dispatch to a host-side function in the
        // built-in registry (e.g. `math.sin` → `f64::sin`).
        if decl.attrs.iter().any(|a| a.name == "extern") {
            return crate::externs::invoke_extern(&decl.name, &evaluated)
                .ok_or_else(|| RuntimeError::TypeError {
                    msg: format!("no extern handler registered for {:?}", decl.name),
                    span: call_span,
                });
        }
        if self.depth >= MAX_DEPTH {
            return Err(RuntimeError::StackOverflow { span: call_span });
        }
        self.depth += 1;
        let saved_vars = std::mem::take(&mut self.vars);
        let saved_this = std::mem::replace(&mut self.this, receiver);
        for (p, v) in decl.params.iter().zip(evaluated.into_iter()) {
            // Coerce arguments to the parameter's declared type so the
            // body sees the right runtime variant (i32 vs i64, etc.).
            self.vars.insert(p.name.clone(), cast_value(v, &p.ty));
        }
        let result = self.eval_block(&decl.body);
        self.vars = saved_vars;
        self.this = saved_this;
        self.depth -= 1;
        match result {
            Err(RuntimeError::Break(_)) => Err(RuntimeError::TypeError {
                msg: "`break` escaped function body".into(),
                span: call_span,
            }),
            Err(RuntimeError::Continue) => Err(RuntimeError::TypeError {
                msg: "`continue` escaped function body".into(),
                span: call_span,
            }),
            Err(RuntimeError::Return(v)) => Ok(v),
            other => other,
        }
    }
}

fn expect_object(v: Value, span: Span) -> Result<ObjectRef, RuntimeError> {
    match v {
        Value::Object(o) => Ok(o),
        other => Err(RuntimeError::NotAnObject {
            actual: format!("{other}"),
            span,
        }),
    }
}

fn expect_array(
    v: Value,
    span: Span,
) -> Result<Rc<RefCell<Vec<Value>>>, RuntimeError> {
    match v {
        Value::Array(a) => Ok(a),
        other => Err(RuntimeError::TypeError {
            msg: format!("expected an array, got {other}"),
            span,
        }),
    }
}

/// Coerce any int-shaped `Value` into a `usize` for indexing. Negative
/// indices are rejected (we don't yet do Python-style wrap-around).
fn index_to_usize(v: Value, span: Span) -> Result<usize, RuntimeError> {
    let n: i128 = match v {
        Value::Int8(n) => n as i128,
        Value::Int16(n) => n as i128,
        Value::Int32(n) => n as i128,
        Value::Int(n) => n as i128,
        Value::UInt8(n) => n as i128,
        Value::UInt16(n) => n as i128,
        Value::UInt32(n) => n as i128,
        Value::UInt64(n) => n as i128,
        other => {
            return Err(RuntimeError::TypeError {
                msg: format!("array index must be an integer, got {other}"),
                span,
            });
        }
    };
    if n < 0 {
        return Err(RuntimeError::TypeError {
            msg: format!("negative array index: {n}"),
            span,
        });
    }
    Ok(n as usize)
}

/// Default value for a field type when a class has no `init`. Mirrors
/// the JIT's `alloc_zeroed`. Heap-reference fields (Object / Map /
/// Weak) get a Unit placeholder — accessing them before assignment
/// would be a runtime error in practice, but readability-wise this
/// matches the JIT's "null pointer" semantics for those slots.
fn default_value(t: &ilang_ast::Type) -> Value {
    use ilang_ast::Type as T;
    match t {
        T::I8 => Value::Int8(0),
        T::I16 => Value::Int16(0),
        T::I32 => Value::Int32(0),
        T::I64 => Value::Int(0),
        T::U8 => Value::UInt8(0),
        T::U16 => Value::UInt16(0),
        T::U32 => Value::UInt32(0),
        T::U64 => Value::UInt64(0),
        T::F32 => Value::Float32(0.0),
        T::F64 => Value::Float(0.0),
        T::Bool => Value::Bool(false),
        T::Str => Value::Str(Rc::new(String::new())),
        T::Optional(_) => Value::None,
        T::Array { .. } => Value::Array(Rc::new(RefCell::new(Vec::new()))),
        // Heap reference / Map / Weak / Object / Enum: no safe blank
        // value the interpreter can synthesize — leave as Unit so a
        // field read before assignment fails loudly elsewhere.
        _ => Value::Unit,
    }
}
