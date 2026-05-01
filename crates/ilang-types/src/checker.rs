use std::collections::HashMap;

use ilang_ast::{
    Block, ClassDecl, Expr, ExprKind, FieldDecl, FnDecl, Item, Param, Program, Span, Stmt,
    StmtKind, Type, UnOp,
};

use crate::error::TypeError;
use crate::ops::{assignable, bin_result, int_literal_fits};

/// Check whether a value expression can be assigned to a binding of type
/// `target`. In addition to the normal `assignable` rule, an integer
/// literal (or its unary negation) infers into any integer type whose
/// range it fits — this is what lets `let x: u8 = 5` work even though
/// the literal's natural type is i64.
fn literal_assignable(value: &Expr, vt: &Type, target: &Type) -> bool {
    if assignable(vt, target) {
        return true;
    }
    // Array literal → array type. Lets `let a: i32[] = [1, 2, 3]` work
    // even though the literal's natural element type is i64, and lets
    // `let a: i32[3] = [1, 2, 3]` match a fixed-length annotation.
    if let (
        ExprKind::Array(elements),
        Type::Array {
            elem: target_elem,
            fixed: target_fixed,
        },
    ) = (&value.kind, target)
    {
        if let Some(n) = target_fixed {
            if elements.len() != *n {
                return false;
            }
        }
        // Empty literal: element type doesn't matter, accept whatever the
        // annotation asks for (subject to the length check above).
        if elements.is_empty() {
            return true;
        }
        let vt_elem = match vt {
            Type::Array { elem, .. } => elem.clone(),
            _ => return false,
        };
        return elements
            .iter()
            .all(|e| literal_assignable(e, &vt_elem, target_elem));
    }
    if let ExprKind::Int(n) = &value.kind {
        if target.is_int() {
            return int_literal_fits(*n, target);
        }
        if target.is_float() {
            return true;
        }
    }
    if let ExprKind::Unary { op: ilang_ast::UnOp::Neg, expr: inner } = &value.kind {
        if let ExprKind::Int(n) = &inner.kind {
            if target.is_int() {
                return n.checked_neg().is_some_and(|v| int_literal_fits(v, target));
            }
            if target.is_float() {
                return true;
            }
        }
    }
    if let ExprKind::Float(_) = &value.kind {
        if target.is_float() {
            return true;
        }
    }
    false
}

#[derive(Debug, Clone)]
struct Signature {
    params: Vec<Type>,
    ret: Type,
    /// `true` for built-ins like `console.log` that accept any number of
    /// arguments (each typed as `Any`). User-defined variadics are not
    /// yet supported (parser doesn't accept `...args`).
    variadic: bool,
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
    vars: HashMap<String, Type>,
}

impl TypeChecker {
    pub fn new() -> Self {
        let mut tc = Self::default();
        tc.install_builtins();
        tc
    }

    /// Pre-register the built-in `Console` class and the `console`
    /// singleton so `console.log(x)` type-checks for any `x`. Kept in one
    /// place so it's easy to grow with `console.error`, `console.warn`, etc.
    fn install_builtins(&mut self) {
        let mut methods = HashMap::new();
        methods.insert(
            "log".to_string(),
            Signature {
                // The `params` slot is unused for variadics; left as a single
                // `Any` so any introspection still has something to print.
                params: vec![Type::Any],
                ret: Type::Unit,
                variadic: true,
            },
        );
        self.classes.insert(
            "Console".to_string(),
            ClassSig {
                fields: HashMap::new(),
                methods,
            },
        );
        self.vars
            .insert("console".to_string(), Type::Object("Console".to_string()));
    }

    pub fn check(&mut self, prog: &Program) -> Result<Type, TypeError> {
        // Pass 0: refuse to redefine built-in names. Otherwise a user
        // `class Console { ... }` would silently overwrite the built-in
        // signature and `console.log` would call the user code.
        for item in &prog.items {
            if let Item::Class(c) = item {
                if is_reserved_class(&c.name) {
                    return Err(TypeError::ReservedName {
                        name: c.name.clone(),
                        span: c.span,
                    });
                }
            }
        }
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
        for item in &prog.items {
            match item {
                Item::Fn(f) => self.check_fn(f, None)?,
                Item::Class(c) => self.check_class(c)?,
            }
        }

        let mut env: Vars = self.vars.clone();
        let mut last = Type::Unit;
        for s in &prog.stmts {
            // Refuse to redefine built-in globals at top level so a
            // wayward `let console = ...` cannot disable `console.log`.
            // Inner-scope shadowing is still allowed.
            if let StmtKind::Let { name, .. } = &s.kind {
                if is_reserved_global(name) {
                    return Err(TypeError::ReservedName {
                        name: name.clone(),
                        span: s.span,
                    });
                }
            }
            last = self.check_stmt(s, &mut env, None, 0)?;
        }
        if let Some(t) = &prog.tail {
            last = self.check_expr(t, &env, None, 0)?;
        }
        self.vars = env;
        Ok(last)
    }

    fn check_class(&self, c: &ClassDecl) -> Result<(), TypeError> {
        for FieldDecl { ty, span, .. } in &c.fields {
            self.validate_type(ty, *span)?;
        }
        for m in &c.methods {
            // `deinit` is the destructor: zero params, no return value (or
            // explicit Unit). Anything else would be a footgun since the
            // runtime calls it with no arguments and discards the result.
            if m.name == "deinit"
                && (!m.params.is_empty()
                    || matches!(&m.ret, Some(t) if *t != Type::Unit))
            {
                return Err(TypeError::BadDeinitSignature { span: m.span });
            }
            self.check_fn(m, Some(&c.name))?;
        }
        Ok(())
    }

    fn check_fn(&self, f: &FnDecl, in_class: Option<&str>) -> Result<(), TypeError> {
        for Param { ty, span, .. } in &f.params {
            self.validate_type(ty, *span)?;
        }
        if let Some(ret) = &f.ret {
            self.validate_type(ret, f.span)?;
        }
        let mut env: Vars = HashMap::new();
        for Param { name, ty, .. } in &f.params {
            env.insert(name.clone(), ty.clone());
        }
        let body_ty = self.check_block(&f.body, &env, in_class, 0)?;
        let expected = f.ret.clone().unwrap_or(Type::Unit);
        if !assignable(&body_ty, &expected) {
            return Err(TypeError::BadReturn {
                name: f.name.clone(),
                expected,
                got: body_ty,
                span: f.span,
            });
        }
        Ok(())
    }

    fn validate_type(&self, t: &Type, span: Span) -> Result<(), TypeError> {
        match t {
            Type::Object(name) => {
                if !self.classes.contains_key(name) {
                    return Err(TypeError::UndefinedClass {
                        name: name.clone(),
                        span,
                    });
                }
            }
            Type::Array { elem, .. } => {
                self.validate_type(elem, span)?;
            }
            _ => {}
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
        match &stmt.kind {
            StmtKind::Let { name, ty, value } => {
                // `let a = []` cannot pick an element type. Force the
                // user to annotate before we lose the chance to do so.
                if ty.is_none() {
                    if let ExprKind::Array(elements) = &value.kind {
                        if elements.is_empty() {
                            return Err(TypeError::EmptyArrayNeedsAnnotation {
                                span: value.span,
                            });
                        }
                    }
                }
                let vt = self.check_expr(value, env, in_class, loop_depth)?;
                let bind = match ty {
                    Some(ann) => {
                        self.validate_type(ann, stmt.span)?;
                        if !literal_assignable(value, &vt, ann) {
                            return Err(TypeError::Mismatch {
                                expected: ann.clone(),
                                got: vt,
                                span: value.span,
                            });
                        }
                        ann.clone()
                    }
                    None => vt,
                };
                env.insert(name.clone(), bind);
                Ok(Type::Unit)
            }
            StmtKind::Expr(e) => self.check_expr(e, env, in_class, loop_depth),
        }
    }

    fn check_expr(
        &self,
        expr: &Expr,
        env: &Vars,
        in_class: Option<&str>,
        loop_depth: u32,
    ) -> Result<Type, TypeError> {
        let span = expr.span;
        match &expr.kind {
            ExprKind::Int(_) => Ok(Type::I64),
            ExprKind::Float(_) => Ok(Type::F64),
            ExprKind::Bool(_) => Ok(Type::Bool),
            ExprKind::Str(_) => Ok(Type::Str),
            ExprKind::This => match in_class {
                Some(name) => Ok(Type::Object(name.to_string())),
                None => Err(TypeError::ThisOutsideMethod { span }),
            },
            ExprKind::Var(n) => {
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
                Err(TypeError::UndefinedVariable {
                    name: n.clone(),
                    span,
                })
            }
            ExprKind::Unary { op, expr: inner } => {
                let t = self.check_expr(inner, env, in_class, loop_depth)?;
                match op {
                    // Unary `-` is only meaningful on signed numerics.
                    UnOp::Neg if t.is_signed_int() || t.is_float() => Ok(t),
                    // Unary `+` is identity on any numeric.
                    UnOp::Pos if t.is_numeric() => Ok(t),
                    UnOp::Not if t == Type::Bool => Ok(t),
                    // Bit-not on any int (signed or unsigned).
                    UnOp::BitNot if t.is_int() => Ok(t),
                    _ => Err(TypeError::BadUnary { ty: t, span }),
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let l = self.check_expr(lhs, env, in_class, loop_depth)?;
                let r = self.check_expr(rhs, env, in_class, loop_depth)?;
                bin_result(*op, &l, &r).map_err(|e| attach_span(e, span))
            }
            ExprKind::Logical { op: _, lhs, rhs } => {
                let l = self.check_expr(lhs, env, in_class, loop_depth)?;
                let r = self.check_expr(rhs, env, in_class, loop_depth)?;
                if l != Type::Bool || r != Type::Bool {
                    return Err(TypeError::BadBinary {
                        lhs: l,
                        rhs: r,
                        span,
                    });
                }
                Ok(Type::Bool)
            }
            ExprKind::Call { callee, args } => {
                if callee == "deinit" {
                    return Err(TypeError::CannotCallDeinit { span });
                }
                if let Some(class_name) = in_class {
                    if let Some(cls) = self.classes.get(class_name) {
                        if let Some(sig) = cls.methods.get(callee).cloned() {
                            self.check_args(callee, &sig, args, env, in_class, loop_depth, span)?;
                            return Ok(sig.ret);
                        }
                    }
                }
                let sig = self.fns.get(callee).cloned().ok_or_else(|| {
                    TypeError::UndefinedFunction {
                        name: callee.clone(),
                        span,
                    }
                })?;
                self.check_args(callee, &sig, args, env, in_class, loop_depth, span)?;
                Ok(sig.ret)
            }
            ExprKind::Field { obj, name } => {
                let ot = self.check_expr(obj, env, in_class, loop_depth)?;
                // Built-in property: every array exposes `length: i64`.
                if matches!(ot, Type::Array { .. }) && name == "length" {
                    return Ok(Type::I64);
                }
                let class_name = expect_object(&ot, span)?;
                let cls = self.classes.get(class_name).ok_or_else(|| {
                    TypeError::UndefinedClass {
                        name: class_name.to_string(),
                        span,
                    }
                })?;
                cls.fields.get(name).cloned().ok_or_else(|| {
                    TypeError::UnknownField {
                        class: class_name.to_string(),
                        field: name.clone(),
                        span,
                    }
                })
            }
            ExprKind::MethodCall { obj, method, args } => {
                if method == "deinit" {
                    return Err(TypeError::CannotCallDeinit { span });
                }
                let ot = self.check_expr(obj, env, in_class, loop_depth)?;
                // Built-in array methods.
                if let Type::Array { elem, fixed } = &ot {
                    if method == "push" {
                        if fixed.is_some() {
                            return Err(TypeError::Mismatch {
                                expected: Type::Array {
                                    elem: elem.clone(),
                                    fixed: None,
                                },
                                got: ot.clone(),
                                span,
                            });
                        }
                        if args.len() != 1 {
                            return Err(TypeError::ArityMismatch {
                                name: "push".into(),
                                expected: 1,
                                got: args.len(),
                                span,
                            });
                        }
                        let at = self.check_expr(&args[0], env, in_class, loop_depth)?;
                        if !literal_assignable(&args[0], &at, elem) {
                            return Err(TypeError::Mismatch {
                                expected: (**elem).clone(),
                                got: at,
                                span: args[0].span,
                            });
                        }
                        return Ok(Type::Unit);
                    }
                    return Err(TypeError::UnknownMethod {
                        class: format!("{ot}"),
                        method: method.clone(),
                        span,
                    });
                }
                let class_name = expect_object(&ot, span)?;
                let cls = self.classes.get(class_name).ok_or_else(|| {
                    TypeError::UndefinedClass {
                        name: class_name.to_string(),
                        span,
                    }
                })?;
                let sig = cls.methods.get(method).cloned().ok_or_else(|| {
                    TypeError::UnknownMethod {
                        class: class_name.to_string(),
                        method: method.clone(),
                        span,
                    }
                })?;
                self.check_args(method, &sig, args, env, in_class, loop_depth, span)?;
                Ok(sig.ret)
            }
            ExprKind::New { class, args } => {
                let cls = self.classes.get(class).ok_or_else(|| TypeError::UndefinedClass {
                    name: class.clone(),
                    span,
                })?;
                if let Some(init) = cls.methods.get("init").cloned() {
                    self.check_args(
                        &format!("{class}::init"),
                        &init,
                        args,
                        env,
                        in_class,
                        loop_depth,
                        span,
                    )?;
                } else if !args.is_empty() {
                    return Err(TypeError::ArityMismatch {
                        name: format!("{class}::init"),
                        expected: 0,
                        got: args.len(),
                        span,
                    });
                }
                Ok(Type::Object(class.clone()))
            }
            ExprKind::Block(b) => self.check_block(b, env, in_class, loop_depth),
            ExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let c = self.check_expr(cond, env, in_class, loop_depth)?;
                if c != Type::Bool {
                    return Err(TypeError::Mismatch {
                        expected: Type::Bool,
                        got: c,
                        span: cond.span,
                    });
                }
                let then_ty = self.check_block(then_branch, env, in_class, loop_depth)?;
                match else_branch {
                    None => {
                        if then_ty != Type::Unit {
                            return Err(TypeError::Mismatch {
                                expected: Type::Unit,
                                got: then_ty,
                                span,
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
                                span: else_e.span,
                            })
                        }
                    }
                }
            }
            ExprKind::While { cond, body } => {
                let c = self.check_expr(cond, env, in_class, loop_depth)?;
                if c != Type::Bool {
                    return Err(TypeError::Mismatch {
                        expected: Type::Bool,
                        got: c,
                        span: cond.span,
                    });
                }
                let body_ty = self.check_block(body, env, in_class, loop_depth + 1)?;
                if body_ty != Type::Unit {
                    return Err(TypeError::Mismatch {
                        expected: Type::Unit,
                        got: body_ty,
                        span,
                    });
                }
                Ok(Type::Unit)
            }
            ExprKind::Loop { body } => {
                let body_ty = self.check_block(body, env, in_class, loop_depth + 1)?;
                if body_ty != Type::Unit {
                    return Err(TypeError::Mismatch {
                        expected: Type::Unit,
                        got: body_ty,
                        span,
                    });
                }
                Ok(Type::Unit)
            }
            ExprKind::Break => {
                if loop_depth == 0 {
                    return Err(TypeError::BreakOutsideLoop { span });
                }
                Ok(Type::Unit)
            }
            ExprKind::Continue => {
                if loop_depth == 0 {
                    return Err(TypeError::ContinueOutsideLoop { span });
                }
                Ok(Type::Unit)
            }
            ExprKind::Assign { target, value } => {
                if let Some(var_ty) = env.get(target).cloned() {
                    let v_ty = self.check_expr(value, env, in_class, loop_depth)?;
                    if !literal_assignable(value, &v_ty, &var_ty) {
                        return Err(TypeError::Mismatch {
                            expected: var_ty,
                            got: v_ty,
                            span: value.span,
                        });
                    }
                    return Ok(Type::Unit);
                }
                if let Some(class_name) = in_class {
                    if let Some(cls) = self.classes.get(class_name) {
                        if let Some(field_ty) = cls.fields.get(target).cloned() {
                            let v_ty = self.check_expr(value, env, in_class, loop_depth)?;
                            if !literal_assignable(value, &v_ty, &field_ty) {
                                return Err(TypeError::Mismatch {
                                    expected: field_ty,
                                    got: v_ty,
                                    span: value.span,
                                });
                            }
                            return Ok(Type::Unit);
                        }
                    }
                }
                Err(TypeError::UndefinedVariable {
                    name: target.clone(),
                    span,
                })
            }
            ExprKind::Array(elements) => {
                if elements.is_empty() {
                    // Element type is unknown; surface a marker
                    // (`Any`-element array) and let `literal_assignable`
                    // accept it when the let / param annotation pins the
                    // type. Bare `let a = []` falls through to the
                    // EmptyArrayNeedsAnnotation error in `check_stmt`.
                    return Ok(Type::Array {
                        elem: Box::new(Type::Any),
                        fixed: Some(0),
                    });
                }
                let first_ty = self.check_expr(&elements[0], env, in_class, loop_depth)?;
                for e in &elements[1..] {
                    let et = self.check_expr(e, env, in_class, loop_depth)?;
                    if !literal_assignable(e, &et, &first_ty) {
                        return Err(TypeError::Mismatch {
                            expected: first_ty.clone(),
                            got: et,
                            span: e.span,
                        });
                    }
                }
                Ok(Type::Array {
                    elem: Box::new(first_ty),
                    fixed: Some(elements.len()),
                })
            }
            ExprKind::Index { obj, index } => {
                let ot = self.check_expr(obj, env, in_class, loop_depth)?;
                let it = self.check_expr(index, env, in_class, loop_depth)?;
                if !it.is_int() {
                    return Err(TypeError::Mismatch {
                        expected: Type::I64,
                        got: it,
                        span: index.span,
                    });
                }
                match ot {
                    Type::Array { elem, .. } => Ok((*elem).clone()),
                    other => Err(TypeError::Mismatch {
                        expected: Type::Array {
                            elem: Box::new(Type::Any),
                            fixed: None,
                        },
                        got: other,
                        span: obj.span,
                    }),
                }
            }
            ExprKind::AssignIndex { obj, index, value } => {
                let ot = self.check_expr(obj, env, in_class, loop_depth)?;
                let it = self.check_expr(index, env, in_class, loop_depth)?;
                if !it.is_int() {
                    return Err(TypeError::Mismatch {
                        expected: Type::I64,
                        got: it,
                        span: index.span,
                    });
                }
                let elem_ty = match &ot {
                    Type::Array { elem, .. } => (**elem).clone(),
                    other => {
                        return Err(TypeError::Mismatch {
                            expected: Type::Array {
                                elem: Box::new(Type::Any),
                                fixed: None,
                            },
                            got: other.clone(),
                            span: obj.span,
                        });
                    }
                };
                let vt = self.check_expr(value, env, in_class, loop_depth)?;
                if !literal_assignable(value, &vt, &elem_ty) {
                    return Err(TypeError::Mismatch {
                        expected: elem_ty,
                        got: vt,
                        span: value.span,
                    });
                }
                Ok(Type::Unit)
            }
            ExprKind::Cast { expr: inner, ty } => {
                let from = self.check_expr(inner, env, in_class, loop_depth)?;
                self.validate_type(ty, span)?;
                // Permit any numeric → numeric cast plus `bool → int` for
                // 0/1 conversion. Other casts (e.g. object → numeric) are
                // a type error.
                let from_ok = from.is_numeric() || from == Type::Bool;
                let to_ok = ty.is_numeric();
                if !from_ok || !to_ok {
                    return Err(TypeError::Mismatch {
                        expected: ty.clone(),
                        got: from,
                        span,
                    });
                }
                Ok(ty.clone())
            }
            ExprKind::AssignField { obj, field, value } => {
                let ot = self.check_expr(obj, env, in_class, loop_depth)?;
                let class_name = expect_object(&ot, obj.span)?;
                let cls = self.classes.get(class_name).ok_or_else(|| {
                    TypeError::UndefinedClass {
                        name: class_name.to_string(),
                        span: obj.span,
                    }
                })?;
                let field_ty = cls.fields.get(field).cloned().ok_or_else(|| {
                    TypeError::UnknownField {
                        class: class_name.to_string(),
                        field: field.clone(),
                        span,
                    }
                })?;
                let v_ty = self.check_expr(value, env, in_class, loop_depth)?;
                if !literal_assignable(value, &v_ty, &field_ty) {
                    return Err(TypeError::Mismatch {
                        expected: field_ty,
                        got: v_ty,
                        span: value.span,
                    });
                }
                Ok(Type::Unit)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn check_args(
        &self,
        name: &str,
        sig: &Signature,
        args: &[Expr],
        env: &Vars,
        in_class: Option<&str>,
        loop_depth: u32,
        call_span: Span,
    ) -> Result<(), TypeError> {
        if sig.variadic {
            // Variadic: any arity, every arg type-checks but acts as `Any`.
            for arg in args {
                self.check_expr(arg, env, in_class, loop_depth)?;
            }
            return Ok(());
        }
        if sig.params.len() != args.len() {
            return Err(TypeError::ArityMismatch {
                name: name.to_string(),
                expected: sig.params.len(),
                got: args.len(),
                span: call_span,
            });
        }
        for (param_ty, arg) in sig.params.iter().zip(args.iter()) {
            let at = self.check_expr(arg, env, in_class, loop_depth)?;
            if !literal_assignable(arg, &at, param_ty) {
                return Err(TypeError::Mismatch {
                    expected: param_ty.clone(),
                    got: at,
                    span: arg.span,
                });
            }
        }
        Ok(())
    }
}

fn is_reserved_class(name: &str) -> bool {
    matches!(name, "Console")
}

fn is_reserved_global(name: &str) -> bool {
    matches!(name, "console")
}

fn signature_of(f: &FnDecl) -> Signature {
    Signature {
        params: f.params.iter().map(|p| p.ty.clone()).collect(),
        ret: f.ret.clone().unwrap_or(Type::Unit),
        variadic: false,
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

fn expect_object(t: &Type, span: Span) -> Result<&str, TypeError> {
    if let Type::Object(name) = t {
        Ok(name)
    } else {
        Err(TypeError::Mismatch {
            expected: Type::Object("<object>".into()),
            got: t.clone(),
            span,
        })
    }
}

/// Helper for `bin_result`-style spanless errors (the ops module returns
/// `BadBinary`/`BadUnary` without knowing the source position; we attach
/// the surrounding expression's span here).
fn attach_span(e: TypeError, span: Span) -> TypeError {
    match e {
        TypeError::BadBinary { lhs, rhs, .. } => TypeError::BadBinary { lhs, rhs, span },
        TypeError::BadUnary { ty, .. } => TypeError::BadUnary { ty, span },
        other => other,
    }
}
