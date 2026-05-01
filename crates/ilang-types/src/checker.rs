use std::collections::HashMap;

use ilang_ast::{
    Block, ClassDecl, Expr, FnDecl, Item, Param, Program, Stmt, Type, UnOp,
};

use crate::error::TypeError;
use crate::ops::{assignable, bin_result};

#[derive(Debug, Clone)]
struct Signature {
    params: Vec<Type>,
    ret: Type,
}

#[derive(Debug, Clone, Default)]
struct ClassSig {
    fields: HashMap<String, Type>,
    methods: HashMap<String, Signature>,
}

type Vars = HashMap<String, Type>;

#[derive(Debug, Default)]
pub struct TypeChecker {
    fns: HashMap<String, Signature>,
    classes: HashMap<String, ClassSig>,
    /// Persistent top-level variable bindings — needed by the REPL so a `let`
    /// on one line is still in scope on the next.
    vars: HashMap<String, Type>,
}

impl TypeChecker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Type-check a program. Class and fn signatures are registered first so
    /// references can be resolved regardless of declaration order.
    pub fn check(&mut self, prog: &Program) -> Result<Type, TypeError> {
        // Pass 1: register signatures.
        for item in &prog.items {
            match item {
                Item::Fn(f) => {
                    let sig = signature_of(f);
                    self.fns.insert(f.name.clone(), sig);
                }
                Item::Class(c) => {
                    let sig = class_signature(c);
                    self.classes.insert(c.name.clone(), sig);
                }
            }
        }
        // Pass 2: validate field/method types (e.g. `Type::Object(name)`
        // refers to a known class) and check method bodies.
        for item in &prog.items {
            match item {
                Item::Fn(f) => self.check_fn(f, None)?,
                Item::Class(c) => self.check_class(c)?,
            }
        }

        // Top-level let-bindings persist for the REPL.
        let mut env: Vars = self.vars.clone();
        let mut last = Type::Unit;
        for s in &prog.stmts {
            last = self.check_stmt(s, &mut env, None, 0)?;
        }
        if let Some(t) = &prog.tail {
            last = self.check_expr(t, &env, None, 0)?;
        }
        self.vars = env;
        Ok(last)
    }

    fn check_class(&self, c: &ClassDecl) -> Result<(), TypeError> {
        for f in &c.fields {
            self.validate_type(&f.ty)?;
        }
        for m in &c.methods {
            self.check_fn(m, Some(&c.name))?;
        }
        Ok(())
    }

    fn check_fn(&self, f: &FnDecl, in_class: Option<&str>) -> Result<(), TypeError> {
        for p in &f.params {
            self.validate_type(&p.ty)?;
        }
        if let Some(ret) = &f.ret {
            self.validate_type(ret)?;
        }
        let mut env: Vars = HashMap::new();
        for Param { name, ty } in &f.params {
            env.insert(name.clone(), ty.clone());
        }
        // Function body starts a fresh loop-depth scope: a `break` inside a
        // function defined within a loop must NOT escape to the outer loop.
        let body_ty = self.check_block(&f.body, &env, in_class, 0)?;
        let expected = f.ret.clone().unwrap_or(Type::Unit);
        if !assignable(&body_ty, &expected) {
            return Err(TypeError::BadReturn {
                name: f.name.clone(),
                expected,
                got: body_ty,
            });
        }
        Ok(())
    }

    fn validate_type(&self, t: &Type) -> Result<(), TypeError> {
        if let Type::Object(name) = t {
            if !self.classes.contains_key(name) {
                return Err(TypeError::UndefinedClass(name.clone()));
            }
        }
        Ok(())
    }

    fn check_block(
        &self,
        block: &Block,
        outer: &Vars,
        in_class: Option<&str>,
        loop_depth: u32,
    ) -> Result<Type, TypeError> {
        let mut env = outer.clone();
        let mut last = Type::Unit;
        for s in &block.stmts {
            last = self.check_stmt(s, &mut env, in_class, loop_depth)?;
        }
        if let Some(t) = &block.tail {
            last = self.check_expr(t, &env, in_class, loop_depth)?;
        }
        Ok(last)
    }

    fn check_stmt(
        &self,
        stmt: &Stmt,
        env: &mut Vars,
        in_class: Option<&str>,
        loop_depth: u32,
    ) -> Result<Type, TypeError> {
        match stmt {
            Stmt::Let { name, ty, value } => {
                let vt = self.check_expr(value, env, in_class, loop_depth)?;
                let bind = match ty {
                    Some(ann) => {
                        self.validate_type(ann)?;
                        if !assignable(&vt, ann) {
                            return Err(TypeError::Mismatch {
                                expected: ann.clone(),
                                got: vt,
                            });
                        }
                        ann.clone()
                    }
                    None => vt,
                };
                env.insert(name.clone(), bind);
                Ok(Type::Unit)
            }
            Stmt::Expr(e) => self.check_expr(e, env, in_class, loop_depth),
        }
    }

    fn check_expr(
        &self,
        expr: &Expr,
        env: &Vars,
        in_class: Option<&str>,
        loop_depth: u32,
    ) -> Result<Type, TypeError> {
        match expr {
            Expr::Int(_) => Ok(Type::I64),
            Expr::Float(_) => Ok(Type::F64),
            Expr::Bool(_) => Ok(Type::Bool),
            Expr::This => match in_class {
                Some(name) => Ok(Type::Object(name.to_string())),
                None => Err(TypeError::ThisOutsideMethod),
            },
            Expr::Var(n) => {
                // Method-body sugar: an identifier with no local binding
                // resolves against the implicit `this`. Locals always win,
                // so `init(count) { this.count = count }` still works.
                if let Some(t) = env.get(n) {
                    return Ok(t.clone());
                }
                if let Some(class_name) = in_class {
                    if let Some(cls) = self.classes.get(class_name) {
                        if let Some(t) = cls.fields.get(n) {
                            return Ok(t.clone());
                        }
                    }
                }
                Err(TypeError::UndefinedVariable(n.clone()))
            }
            Expr::Unary { op, expr } => {
                let t = self.check_expr(expr, env, in_class, loop_depth)?;
                match (op, &t) {
                    (UnOp::Neg | UnOp::Pos, Type::I64) => Ok(Type::I64),
                    (UnOp::Neg | UnOp::Pos, Type::F64) => Ok(Type::F64),
                    (UnOp::Not, Type::Bool) => Ok(Type::Bool),
                    _ => Err(TypeError::BadUnary(t)),
                }
            }
            Expr::Binary { op, lhs, rhs } => {
                let l = self.check_expr(lhs, env, in_class, loop_depth)?;
                let r = self.check_expr(rhs, env, in_class, loop_depth)?;
                bin_result(*op, &l, &r)
            }
            Expr::Logical { op: _, lhs, rhs } => {
                let l = self.check_expr(lhs, env, in_class, loop_depth)?;
                let r = self.check_expr(rhs, env, in_class, loop_depth)?;
                if l != Type::Bool || r != Type::Bool {
                    return Err(TypeError::BadBinary(l, r));
                }
                Ok(Type::Bool)
            }
            Expr::Call { callee, args } => {
                // Inside a method body, an unqualified call resolves against
                // own methods first (implicit `this.callee(args)`). Falls
                // back to free functions so existing code keeps working.
                if let Some(class_name) = in_class {
                    if let Some(cls) = self.classes.get(class_name) {
                        if let Some(sig) = cls.methods.get(callee).cloned() {
                            self.check_args(callee, &sig, args, env, in_class, loop_depth)?;
                            return Ok(sig.ret);
                        }
                    }
                }
                let sig = self
                    .fns
                    .get(callee)
                    .cloned()
                    .ok_or_else(|| TypeError::UndefinedFunction(callee.clone()))?;
                self.check_args(callee, &sig, args, env, in_class, loop_depth)?;
                Ok(sig.ret)
            }
            Expr::Field { obj, name } => {
                let ot = self.check_expr(obj, env, in_class, loop_depth)?;
                let class_name = expect_object(&ot)?;
                let cls = self
                    .classes
                    .get(class_name)
                    .ok_or_else(|| TypeError::UndefinedClass(class_name.to_string()))?;
                cls.fields.get(name).cloned().ok_or_else(|| {
                    TypeError::UnknownField {
                        class: class_name.to_string(),
                        field: name.clone(),
                    }
                })
            }
            Expr::MethodCall { obj, method, args } => {
                let ot = self.check_expr(obj, env, in_class, loop_depth)?;
                let class_name = expect_object(&ot)?;
                let cls = self
                    .classes
                    .get(class_name)
                    .ok_or_else(|| TypeError::UndefinedClass(class_name.to_string()))?;
                let sig =
                    cls.methods.get(method).cloned().ok_or_else(|| {
                        TypeError::UnknownMethod {
                            class: class_name.to_string(),
                            method: method.clone(),
                        }
                    })?;
                self.check_args(method, &sig, args, env, in_class, loop_depth)?;
                Ok(sig.ret)
            }
            Expr::New { class, args } => {
                let cls = self
                    .classes
                    .get(class)
                    .ok_or_else(|| TypeError::UndefinedClass(class.clone()))?;
                // If the class has an `init` method, treat it as the
                // constructor. Otherwise `new C()` must take zero arguments.
                if let Some(init) = cls.methods.get("init").cloned() {
                    self.check_args(
                        &format!("{class}::init"),
                        &init,
                        args,
                        env,
                        in_class,
                        loop_depth,
                    )?;
                } else if !args.is_empty() {
                    return Err(TypeError::ArityMismatch {
                        name: format!("{class}::init"),
                        expected: 0,
                        got: args.len(),
                    });
                }
                Ok(Type::Object(class.clone()))
            }
            Expr::Block(b) => self.check_block(b, env, in_class, loop_depth),
            Expr::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let c = self.check_expr(cond, env, in_class, loop_depth)?;
                if c != Type::Bool {
                    return Err(TypeError::Mismatch {
                        expected: Type::Bool,
                        got: c,
                    });
                }
                let then_ty = self.check_block(then_branch, env, in_class, loop_depth)?;
                match else_branch {
                    None => {
                        if then_ty != Type::Unit {
                            return Err(TypeError::Mismatch {
                                expected: Type::Unit,
                                got: then_ty,
                            });
                        }
                        Ok(Type::Unit)
                    }
                    Some(else_e) => {
                        let else_ty = self.check_expr(else_e, env, in_class, loop_depth)?;
                        if then_ty == else_ty {
                            Ok(then_ty)
                        } else if assignable(&then_ty, &else_ty) {
                            Ok(else_ty)
                        } else if assignable(&else_ty, &then_ty) {
                            Ok(then_ty)
                        } else {
                            Err(TypeError::Mismatch {
                                expected: then_ty,
                                got: else_ty,
                            })
                        }
                    }
                }
            }
            Expr::While { cond, body } => {
                let c = self.check_expr(cond, env, in_class, loop_depth)?;
                if c != Type::Bool {
                    return Err(TypeError::Mismatch {
                        expected: Type::Bool,
                        got: c,
                    });
                }
                let body_ty = self.check_block(body, env, in_class, loop_depth + 1)?;
                if body_ty != Type::Unit {
                    return Err(TypeError::Mismatch {
                        expected: Type::Unit,
                        got: body_ty,
                    });
                }
                Ok(Type::Unit)
            }
            Expr::Loop { body } => {
                let body_ty = self.check_block(body, env, in_class, loop_depth + 1)?;
                if body_ty != Type::Unit {
                    return Err(TypeError::Mismatch {
                        expected: Type::Unit,
                        got: body_ty,
                    });
                }
                Ok(Type::Unit)
            }
            Expr::Break => {
                if loop_depth == 0 {
                    return Err(TypeError::BreakOutsideLoop);
                }
                Ok(Type::Unit)
            }
            Expr::Continue => {
                if loop_depth == 0 {
                    return Err(TypeError::ContinueOutsideLoop);
                }
                Ok(Type::Unit)
            }
            Expr::Assign { target, value } => {
                // Mirror the read-side fallback: assigning to an unqualified
                // name inside a method body assigns to the field on `this`
                // when no local binding shadows it.
                if let Some(var_ty) = env.get(target).cloned() {
                    let v_ty = self.check_expr(value, env, in_class, loop_depth)?;
                    if !assignable(&v_ty, &var_ty) {
                        return Err(TypeError::Mismatch {
                            expected: var_ty,
                            got: v_ty,
                        });
                    }
                    return Ok(Type::Unit);
                }
                if let Some(class_name) = in_class {
                    if let Some(cls) = self.classes.get(class_name) {
                        if let Some(field_ty) = cls.fields.get(target).cloned() {
                            let v_ty = self.check_expr(value, env, in_class, loop_depth)?;
                            if !assignable(&v_ty, &field_ty) {
                                return Err(TypeError::Mismatch {
                                    expected: field_ty,
                                    got: v_ty,
                                });
                            }
                            return Ok(Type::Unit);
                        }
                    }
                }
                Err(TypeError::UndefinedVariable(target.clone()))
            }
            Expr::AssignField { obj, field, value } => {
                let ot = self.check_expr(obj, env, in_class, loop_depth)?;
                let class_name = expect_object(&ot)?;
                let cls = self
                    .classes
                    .get(class_name)
                    .ok_or_else(|| TypeError::UndefinedClass(class_name.to_string()))?;
                let field_ty = cls.fields.get(field).cloned().ok_or_else(|| {
                    TypeError::UnknownField {
                        class: class_name.to_string(),
                        field: field.clone(),
                    }
                })?;
                let v_ty = self.check_expr(value, env, in_class, loop_depth)?;
                if !assignable(&v_ty, &field_ty) {
                    return Err(TypeError::Mismatch {
                        expected: field_ty,
                        got: v_ty,
                    });
                }
                Ok(Type::Unit)
            }
        }
    }

    fn check_args(
        &self,
        name: &str,
        sig: &Signature,
        args: &[Expr],
        env: &Vars,
        in_class: Option<&str>,
        loop_depth: u32,
    ) -> Result<(), TypeError> {
        if sig.params.len() != args.len() {
            return Err(TypeError::ArityMismatch {
                name: name.to_string(),
                expected: sig.params.len(),
                got: args.len(),
            });
        }
        for (param_ty, arg) in sig.params.iter().zip(args.iter()) {
            let at = self.check_expr(arg, env, in_class, loop_depth)?;
            if !assignable(&at, param_ty) {
                return Err(TypeError::Mismatch {
                    expected: param_ty.clone(),
                    got: at,
                });
            }
        }
        Ok(())
    }
}

fn signature_of(f: &FnDecl) -> Signature {
    Signature {
        params: f.params.iter().map(|p| p.ty.clone()).collect(),
        ret: f.ret.clone().unwrap_or(Type::Unit),
    }
}

fn class_signature(c: &ClassDecl) -> ClassSig {
    let mut fields = HashMap::new();
    for f in &c.fields {
        fields.insert(f.name.clone(), f.ty.clone());
    }
    let mut methods = HashMap::new();
    for m in &c.methods {
        methods.insert(m.name.clone(), signature_of(m));
    }
    ClassSig { fields, methods }
}

fn expect_object(t: &Type) -> Result<&str, TypeError> {
    if let Type::Object(name) = t {
        Ok(name)
    } else {
        Err(TypeError::Mismatch {
            expected: Type::Object("<object>".into()),
            got: t.clone(),
        })
    }
}
