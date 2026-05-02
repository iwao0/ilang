use std::collections::HashMap;

use ilang_ast::{
    Block, ClassDecl, CtorArgs, EnumDecl, Expr, ExprKind, FieldDecl, FnDecl, Item, Param,
    PatternBindings, PatternKind, Program, Span, Stmt, StmtKind, Type, UnOp, VariantPayload,
};

use crate::error::TypeError;
use crate::ops::{assignable, bin_result, int_literal_fits};

/// Check whether a value expression can be assigned to a binding of type
/// `target`. In addition to the normal `assignable` rule, an integer
/// literal (or its unary negation) infers into any integer type whose
/// range it fits — this is what lets `let x: u8 = 5` work even though
/// the literal's natural type is i64.
/// `if` の枝合流専用の判定: 値式が **素の数値リテラル** (整数/浮動小数、
/// 任意で単項 `-`) で、`target` 型に収まるかどうか。`assignable` を経由
/// しないので i64 値→f64 のような暗黙拡張は通さない。
fn numeric_literal_fits(value: &Expr, target: &Type) -> bool {
    match &value.kind {
        ExprKind::Int(n) => {
            if target.is_int() {
                int_literal_fits(*n, target)
            } else {
                target.is_float()
            }
        }
        ExprKind::Unary { op: ilang_ast::UnOp::Neg, expr: inner } => {
            if let ExprKind::Int(n) = &inner.kind {
                if target.is_int() {
                    n.checked_neg().is_some_and(|v| int_literal_fits(v, target))
                } else {
                    target.is_float()
                }
            } else if matches!(inner.kind, ExprKind::Float(_)) {
                target.is_float()
            } else {
                false
            }
        }
        ExprKind::Float(_) => target.is_float(),
        _ => false,
    }
}

fn literal_assignable(value: &Expr, vt: &Type, target: &Type) -> bool {
    if assignable(vt, target) {
        return true;
    }
    // `let x: T? = literal` — auto-wrap. The literal is assignable to T?
    // iff it's assignable to the inner T (with literal coercions).
    if let Type::Optional(inner) = target {
        // `none` itself: vt = Optional<Any>, handled by `assignable`. For
        // `some(x)`, vt = Optional<U>, also handled there. The remaining
        // case is a bare literal that should coerce to the inner.
        if matches!(vt, Type::Optional(_)) {
            return false; // already handled above
        }
        return literal_assignable(value, vt, inner);
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
    /// Generic type parameters declared on the fn (e.g. `<T, U>`).
    /// Empty for non-generic fns. `params` / `ret` may reference these
    /// as `Type::TypeVar(name)`; concrete types are inferred from the
    /// arg expression types at each call site.
    type_params: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct ClassSig {
    /// Names of generic type parameters on the class. Empty for
    /// non-generic classes. Field/method types may reference these as
    /// `Type::TypeVar(name)`; instantiation substitutes them.
    type_params: Vec<String>,
    fields: HashMap<String, Type>,
    methods: HashMap<String, Signature>,
}

/// Type-checker view of an enum. Variants preserve declaration order so
/// the JIT can use the same indices as ordinal tags.
#[derive(Debug, Clone)]
struct EnumSig {
    /// Generic type parameters declared on the enum (mirrors
    /// `ClassSig.type_params`). Empty for non-generic enums.
    /// Variant payloads may reference these as `Type::TypeVar`.
    type_params: Vec<String>,
    variants: Vec<EnumVariantSig>,
}

#[derive(Debug, Clone)]
struct EnumVariantSig {
    name: String,
    payload: VariantPayloadSig,
}

#[derive(Debug, Clone)]
enum VariantPayloadSig {
    Unit,
    Tuple(Vec<Type>),
    Struct(Vec<(String, Type)>),
}

type Vars = HashMap<String, Type>;

#[derive(Debug, Default)]
pub struct TypeChecker {
    fns: HashMap<String, Signature>,
    classes: HashMap<String, ClassSig>,
    enums: HashMap<String, EnumSig>,
    vars: HashMap<String, Type>,
    /// Inferred type-argument vector for each generic-fn call site,
    /// keyed by the call expression's span. Populated during checking;
    /// consumed by the JIT's monomorphization pass. Values may contain
    /// `Type::TypeVar` when the call sits inside another generic
    /// context — the monomorphizer substitutes those at expansion time.
    /// Wrapped in `RefCell` because `check_expr` takes `&self`.
    fn_call_type_args: std::cell::RefCell<HashMap<Span, (String, Vec<Type>)>>,
}

impl TypeChecker {
    pub fn new() -> Self {
        let mut tc = Self::default();
        tc.install_builtins();
        tc
    }

    /// Map of generic-fn call site → (callee name, inferred type args).
    /// Filled in during `check`; consumed by the JIT monomorphizer.
    pub fn fn_call_type_args(&self) -> HashMap<Span, (String, Vec<Type>)> {
        self.fn_call_type_args.borrow().clone()
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
                variadic: true, type_params: Vec::new(),
            },
        );
        self.classes.insert(
            "Console".to_string(),
            ClassSig {
                type_params: Vec::new(),
                fields: HashMap::new(),
                methods,
            },
        );
        self.vars
            .insert("console".to_string(), Type::Object("Console".to_string()));

        // Built-in `Map<K, V>` — generic class with no fields. Methods
        // are intercepted in the interpreter; the signatures here are
        // what the type checker enforces. Indexing (`m[k]` / `m[k] = v`)
        // is handled in the Index/AssignIndex arms by recognizing
        // `Type::Generic { Map, [K, V] }` receivers.
        let k = || Type::TypeVar("K".into());
        let v = || Type::TypeVar("V".into());
        let mut map_methods = HashMap::new();
        map_methods.insert(
            "init".into(),
            Signature { params: vec![], ret: Type::Unit, variadic: false, type_params: Vec::new() },
        );
        map_methods.insert(
            "get".into(),
            Signature {
                params: vec![k()],
                ret: Type::Optional(Box::new(v())),
                variadic: false, type_params: Vec::new(),
            },
        );
        map_methods.insert(
            "set".into(),
            Signature { params: vec![k(), v()], ret: Type::Unit, variadic: false, type_params: Vec::new() },
        );
        map_methods.insert(
            "has".into(),
            Signature { params: vec![k()], ret: Type::Bool, variadic: false, type_params: Vec::new() },
        );
        map_methods.insert(
            "delete".into(),
            Signature { params: vec![k()], ret: Type::Bool, variadic: false, type_params: Vec::new() },
        );
        map_methods.insert(
            "size".into(),
            Signature { params: vec![], ret: Type::I64, variadic: false, type_params: Vec::new() },
        );
        map_methods.insert(
            "keys".into(),
            Signature {
                params: vec![],
                ret: Type::Array { elem: Box::new(k()), fixed: None },
                variadic: false, type_params: Vec::new(),
            },
        );
        map_methods.insert(
            "values".into(),
            Signature {
                params: vec![],
                ret: Type::Array { elem: Box::new(v()), fixed: None },
                variadic: false, type_params: Vec::new(),
            },
        );
        self.classes.insert(
            "Map".into(),
            ClassSig {
                type_params: vec!["K".into(), "V".into()],
                fields: HashMap::new(),
                methods: map_methods,
            },
        );

        // Built-in `Result<T, E>` — generic enum with `Ok(T)` and
        // `Err(E)` variants. Constructed via `Result::Ok(v)` /
        // `Result::Err(e)` and matched like any other enum.
        self.enums.insert(
            "Result".into(),
            EnumSig {
                type_params: vec!["T".into(), "E".into()],
                variants: vec![
                    EnumVariantSig {
                        name: "ok".into(),
                        payload: VariantPayloadSig::Tuple(vec![Type::TypeVar("T".into())]),
                    },
                    EnumVariantSig {
                        name: "err".into(),
                        payload: VariantPayloadSig::Tuple(vec![Type::TypeVar("E".into())]),
                    },
                ],
            },
        );
    }

    pub fn check(&mut self, prog: &Program) -> Result<Type, TypeError> {
        // Pass 0: refuse to redefine built-in names. Otherwise a user
        // `class Console { ... }` would silently overwrite the built-in
        // signature and `console.log` would call the user code.
        for item in &prog.items {
            match item {
                Item::Class(c) if is_reserved_class(&c.name) => {
                    return Err(TypeError::ReservedName {
                        name: c.name.clone(),
                        span: c.span,
                    });
                }
                Item::Enum(e) if is_reserved_class(&e.name) => {
                    return Err(TypeError::ReservedName {
                        name: e.name.clone(),
                        span: e.span,
                    });
                }
                _ => {}
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
                Item::Enum(e) => {
                    let sig = enum_signature(e);
                    self.enums.insert(e.name.clone(), sig);
                }
                // The loader replaces Use items with their resolved
                // contents before type checking; any Use that survives
                // here was emitted by something that bypassed the
                // loader, and silently ignoring it is fine — there's
                // nothing to check.
                Item::Use(_) => {}
                // Const items are inlined by the loader's substitution
                // pass — they shouldn't appear here in the normal
                // pipeline. Skip if any survives.
                Item::Const(_) => {}
            }
        }
        for item in &prog.items {
            match item {
                Item::Fn(f) => self.check_fn(f, None)?,
                Item::Class(c) => self.check_class(c)?,
                Item::Enum(e) => self.check_enum(e)?,
                Item::Use(_) | Item::Const(_) => {}
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
            last = self.check_stmt(s, &mut env, None, None, 0)?;
        }
        if let Some(t) = &prog.tail {
            last = self.check_expr(t, &env, None, None, 0)?;
        }
        self.vars = env;
        Ok(last)
    }

    fn check_enum(&self, e: &EnumDecl) -> Result<(), TypeError> {
        // Validate every payload type now that all class/enum names are
        // known. Duplicate variant names are rejected — they'd make
        // pattern matching ambiguous.
        let mut seen = std::collections::HashSet::new();
        for v in &e.variants {
            if !seen.insert(v.name.clone()) {
                return Err(TypeError::Unsupported {
                    what: format!("duplicate variant {:?} in enum {:?}", v.name, e.name),
                    span: v.span,
                });
            }
            match &v.payload {
                VariantPayload::Unit => {}
                VariantPayload::Tuple(tys) => {
                    for t in tys {
                        self.validate_type(t, v.span, &e.type_params)?;
                    }
                }
                VariantPayload::Struct(fields) => {
                    let mut fseen = std::collections::HashSet::new();
                    for f in fields {
                        if !fseen.insert(f.name.clone()) {
                            return Err(TypeError::Unsupported {
                                what: format!(
                                    "duplicate field {:?} in {}::{}",
                                    f.name, e.name, v.name
                                ),
                                span: f.span,
                            });
                        }
                        self.validate_type(&f.ty, f.span, &e.type_params)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn check_class(&self, c: &ClassDecl) -> Result<(), TypeError> {
        for FieldDecl { ty, span, .. } in &c.fields {
            self.validate_type(ty, *span, &c.type_params)?;
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
        // Type parameters in scope: the class's (if we're inside a
        // generic class) plus the fn's own `<T, U>`.
        let mut params_in_scope: Vec<String> = in_class
            .and_then(|n| self.classes.get(n))
            .map(|c| c.type_params.clone())
            .unwrap_or_default();
        params_in_scope.extend(f.type_params.iter().cloned());
        let class_params = params_in_scope;
        for Param { ty, span, .. } in &f.params {
            self.validate_type(ty, *span, &class_params)?;
        }
        if let Some(ret) = &f.ret {
            self.validate_type(ret, f.span, &class_params)?;
        }
        // `@extern` fns have no body — the runtime supplies the
        // implementation. Skip the body check; the signature is the
        // contract and the runtime is responsible for honoring it.
        if f.attrs.iter().any(|a| a.name == "extern") {
            return Ok(());
        }
        let mut env: Vars = HashMap::new();
        for Param { name, ty, .. } in &f.params {
            // Rewrite Object(T) → TypeVar(T) so the body checker treats
            // references to T as the type variable (not an unknown class).
            env.insert(name.clone(), rewrite_type_params(ty, &class_params));
        }
        let expected = rewrite_type_params(
            &f.ret.clone().unwrap_or(Type::Unit),
            &class_params,
        );
        let body_ty = self.check_block(&f.body, &env, Some(&expected), in_class, 0)?;
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

    fn validate_type(
        &self,
        t: &Type,
        span: Span,
        type_params_in_scope: &[String],
    ) -> Result<(), TypeError> {
        match t {
            Type::Object(name) => {
                // An identifier may refer to either a class, an enum,
                // or — when checking a generic class body — one of the
                // class's type parameters. `Type::Enum` only exists
                // when the checker resolved it explicitly (currently
                // unused — the parser produces `Object(name)` for both
                // classes and enums).
                if self.classes.contains_key(name)
                    || self.enums.contains_key(name)
                    || type_params_in_scope.iter().any(|p| p == name)
                {
                    // ok
                } else {
                    return Err(TypeError::UndefinedClass {
                        name: name.clone(),
                        span,
                    });
                }
            }
            Type::Enum(name) => {
                if !self.enums.contains_key(name) {
                    return Err(TypeError::UndefinedClass {
                        name: name.clone(),
                        span,
                    });
                }
            }
            Type::Array { elem, .. } => {
                self.validate_type(elem, span, type_params_in_scope)?;
            }
            Type::Optional(inner) => {
                self.validate_type(inner, span, type_params_in_scope)?;
            }
            Type::Weak(inner) => {
                // Weak is meaningful only for class instances. Reject
                // `string.weak`, `i64.weak`, etc. up front.
                if !matches!(inner.as_ref(), Type::Object(_)) {
                    return Err(TypeError::Unsupported {
                        what: format!("weak reference of {inner} (only class types allowed)"),
                        span,
                    });
                }
                self.validate_type(inner, span, type_params_in_scope)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn check_block(
        &self,
        block: &Block,
        outer: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<&str>,
        loop_depth: u32,
    ) -> Result<Type, TypeError> {
        let mut env = outer.clone();
        let mut last = Type::Unit;
        for s in &block.stmts {
            last = self.check_stmt(s, &mut env, ret_ty, in_class, loop_depth)?;
        }
        if let Some(t) = &block.tail {
            last = self.check_expr(t, &env, ret_ty, in_class, loop_depth)?;
        }
        Ok(last)
    }

    fn check_stmt(
        &self,
        stmt: &Stmt,
        env: &mut Vars,
        ret_ty: Option<&Type>,
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
                let vt = self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
                let bind = match ty {
                    Some(ann) => {
                        self.validate_type(ann, stmt.span, &[])?;
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
            StmtKind::Expr(e) => self.check_expr(e, env, ret_ty, in_class, loop_depth),
        }
    }

    fn check_expr(
        &self,
        expr: &Expr,
        env: &Vars,
        ret_ty: Option<&Type>,
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
                // First-class function: a bare reference to a top-level
                // `fn` becomes a function value (`Type::Fn(...)`).
                if let Some(sig) = self.fns.get(n) {
                    return Ok(Type::Fn {
                        params: sig.params.clone(),
                        ret: Box::new(sig.ret.clone()),
                    });
                }
                Err(TypeError::UndefinedVariable {
                    name: n.clone(),
                    span,
                })
            }
            ExprKind::Unary { op, expr: inner } => {
                let t = self.check_expr(inner, env, ret_ty, in_class, loop_depth)?;
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
                let l = self.check_expr(lhs, env, ret_ty, in_class, loop_depth)?;
                let r = self.check_expr(rhs, env, ret_ty, in_class, loop_depth)?;
                bin_result(*op, &l, &r).map_err(|e| attach_span(e, span))
            }
            ExprKind::Logical { op: _, lhs, rhs } => {
                let l = self.check_expr(lhs, env, ret_ty, in_class, loop_depth)?;
                let r = self.check_expr(rhs, env, ret_ty, in_class, loop_depth)?;
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
                // Indirect call through a function-typed local: shadows
                // both methods and top-level fns, mirroring how a local
                // `let print = ...` shadows an outer name.
                if let Some(Type::Fn { params, ret }) = env.get(callee).cloned() {
                    let sig = Signature {
                        params,
                        ret: (*ret).clone(),
                        variadic: false, type_params: Vec::new(),
                    };
                    self.check_args(callee, &sig, args, env, ret_ty, in_class, loop_depth, span)?;
                    return Ok(sig.ret);
                }
                if let Some(class_name) = in_class {
                    if let Some(cls) = self.classes.get(class_name) {
                        if let Some(sig) = cls.methods.get(callee).cloned() {
                            self.check_args(callee, &sig, args, env, ret_ty, in_class, loop_depth, span)?;
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
                if sig.type_params.is_empty() {
                    self.check_args(callee, &sig, args, env, ret_ty, in_class, loop_depth, span)?;
                    return Ok(sig.ret);
                }
                // Generic fn — see below; we also stash the inferred
                // type-args vector keyed by call span so the JIT's
                // monomorphization pass can find it later.
                // Generic fn: infer type-arg bindings from the (parametric
                // param type, arg type) pairs, then validate arg-by-arg
                // against the substituted param types and return the
                // substituted return type. Mirrors enum-ctor inference.
                if sig.params.len() != args.len() {
                    return Err(TypeError::ArityMismatch {
                        name: callee.clone(),
                        expected: sig.params.len(),
                        got: args.len(),
                        span,
                    });
                }
                let mut bindings: HashMap<String, Type> = HashMap::new();
                let mut arg_tys: Vec<Type> = Vec::with_capacity(args.len());
                for (param_ty, arg) in sig.params.iter().zip(args.iter()) {
                    let at = self.check_expr(arg, env, ret_ty, in_class, loop_depth)?;
                    collect_type_var_bindings(param_ty, &at, &mut bindings);
                    arg_tys.push(at);
                }
                let inferred_args: Vec<Type> = sig
                    .type_params
                    .iter()
                    .map(|p| bindings.get(p).cloned().unwrap_or(Type::Any))
                    .collect();
                // Stash for the JIT monomorphizer. Args may still
                // contain TypeVars when the call is inside another
                // generic context — that's resolved at expansion time.
                self.fn_call_type_args
                    .borrow_mut()
                    .insert(span, (callee.clone(), inferred_args.clone()));
                for ((param_ty, arg), at) in sig.params.iter().zip(args.iter()).zip(arg_tys.iter()) {
                    let actual = subst_type(param_ty, &sig.type_params, &inferred_args);
                    if !literal_assignable(arg, at, &actual) {
                        return Err(TypeError::Mismatch {
                            expected: actual,
                            got: at.clone(),
                            span: arg.span,
                        });
                    }
                }
                Ok(subst_type(&sig.ret, &sig.type_params, &inferred_args))
            }
            ExprKind::Field { obj, name } => {
                let ot = self.check_expr(obj, env, ret_ty, in_class, loop_depth)?;
                // Built-in property: every array exposes `length: i64`.
                if matches!(ot, Type::Array { .. }) && name == "length" {
                    return Ok(Type::I64);
                }
                // Built-in property: strings expose `length: i64` (Unicode
                // code-point count, JS-style).
                if matches!(ot, Type::Str) && name == "length" {
                    return Ok(Type::I64);
                }
                let class_name = expect_object(&ot, span)?;
                let cls = self.classes.get(class_name).ok_or_else(|| {
                    TypeError::UndefinedClass {
                        name: class_name.to_string(),
                        span,
                    }
                })?;
                let raw = cls.fields.get(name).cloned().ok_or_else(|| {
                    TypeError::UnknownField {
                        class: class_name.to_string(),
                        field: name.clone(),
                        span,
                    }
                })?;
                Ok(subst_type(&raw, &cls.type_params, type_args_of(&ot)))
            }
            ExprKind::MethodCall { obj, method, args } => {
                if method == "deinit" {
                    return Err(TypeError::CannotCallDeinit { span });
                }
                let ot = self.check_expr(obj, env, ret_ty, in_class, loop_depth)?;
                // Built-in Weak method: get(): T?.
                if let Type::Weak(inner) = &ot {
                    if method == "get" {
                        if !args.is_empty() {
                            return Err(TypeError::ArityMismatch {
                                name: "get".into(),
                                expected: 0,
                                got: args.len(),
                                span,
                            });
                        }
                        return Ok(Type::Optional(inner.clone()));
                    }
                    return Err(TypeError::UnknownMethod {
                        class: format!("{ot}"),
                        method: method.clone(),
                        span,
                    });
                }
                // Built-in Optional methods: is_some / is_none / unwrap.
                if let Type::Optional(inner) = &ot {
                    match method.as_str() {
                        "isSome" | "isNone" => {
                            if !args.is_empty() {
                                return Err(TypeError::ArityMismatch {
                                    name: method.clone(),
                                    expected: 0,
                                    got: args.len(),
                                    span,
                                });
                            }
                            return Ok(Type::Bool);
                        }
                        "unwrap" => {
                            if !args.is_empty() {
                                return Err(TypeError::ArityMismatch {
                                    name: method.clone(),
                                    expected: 0,
                                    got: args.len(),
                                    span,
                                });
                            }
                            return Ok((**inner).clone());
                        }
                        _ => {
                            return Err(TypeError::UnknownMethod {
                                class: format!("{ot}"),
                                method: method.clone(),
                                span,
                            });
                        }
                    }
                }
                // Built-in string methods (JS-style camelCase).
                if matches!(ot, Type::Str) {
                    let arity_check = |expected: usize| -> Result<(), TypeError> {
                        if args.len() != expected {
                            Err(TypeError::ArityMismatch {
                                name: method.clone(),
                                expected,
                                got: args.len(),
                                span,
                            })
                        } else {
                            Ok(())
                        }
                    };
                    match method.as_str() {
                        "charAt" => {
                            arity_check(1)?;
                            let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                            if !matches!(at, Type::I64 | Type::I32 | Type::I16 | Type::I8 | Type::U64 | Type::U32 | Type::U16 | Type::U8) {
                                return Err(TypeError::Mismatch {
                                    expected: Type::I64,
                                    got: at,
                                    span: args[0].span,
                                });
                            }
                            return Ok(Type::Str);
                        }
                        "includes" | "startsWith" | "endsWith" => {
                            arity_check(1)?;
                            let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                            if !matches!(at, Type::Str) {
                                return Err(TypeError::Mismatch {
                                    expected: Type::Str,
                                    got: at,
                                    span: args[0].span,
                                });
                            }
                            return Ok(Type::Bool);
                        }
                        "toUpperCase" | "toLowerCase" | "trim" => {
                            arity_check(0)?;
                            return Ok(Type::Str);
                        }
                        _ => {
                            return Err(TypeError::UnknownMethod {
                                class: "string".into(),
                                method: method.clone(),
                                span,
                            });
                        }
                    }
                }
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
                        let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                        if !literal_assignable(&args[0], &at, elem) {
                            return Err(TypeError::Mismatch {
                                expected: (**elem).clone(),
                                got: at,
                                span: args[0].span,
                            });
                        }
                        return Ok(Type::Unit);
                    }
                    if method == "pop" {
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
                        if !args.is_empty() {
                            return Err(TypeError::ArityMismatch {
                                name: "pop".into(),
                                expected: 0,
                                got: args.len(),
                                span,
                            });
                        }
                        return Ok(Type::Optional(elem.clone()));
                    }
                    if method == "indexOf" || method == "includes" {
                        if args.len() != 1 {
                            return Err(TypeError::ArityMismatch {
                                name: method.clone(),
                                expected: 1,
                                got: args.len(),
                                span,
                            });
                        }
                        let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                        if !literal_assignable(&args[0], &at, elem) {
                            return Err(TypeError::Mismatch {
                                expected: (**elem).clone(),
                                got: at,
                                span: args[0].span,
                            });
                        }
                        return Ok(if method == "indexOf" {
                            Type::I64
                        } else {
                            Type::Bool
                        });
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
                let raw_sig = cls.methods.get(method).cloned().ok_or_else(|| {
                    TypeError::UnknownMethod {
                        class: class_name.to_string(),
                        method: method.clone(),
                        span,
                    }
                })?;
                let class_params = cls.type_params.clone();
                let inst_args: Vec<Type> = type_args_of(&ot).to_vec();
                let sig = Signature {
                    params: raw_sig
                        .params
                        .iter()
                        .map(|t| subst_type(t, &class_params, &inst_args))
                        .collect(),
                    ret: subst_type(&raw_sig.ret, &class_params, &inst_args),
                    variadic: raw_sig.variadic,
                    type_params: Vec::new(),
                };
                self.check_args(method, &sig, args, env, ret_ty, in_class, loop_depth, span)?;
                Ok(sig.ret)
            }
            ExprKind::New { class, type_args, args } => {
                let cls = self.classes.get(class).ok_or_else(|| TypeError::UndefinedClass {
                    name: class.clone(),
                    span,
                })?;
                let class_params = cls.type_params.clone();
                let init_raw = cls.methods.get("init").cloned();
                // Generic instantiation: arity check on type args.
                if !class_params.is_empty() && type_args.len() != class_params.len() {
                    return Err(TypeError::ArityMismatch {
                        name: format!("{class}::<type args>"),
                        expected: class_params.len(),
                        got: type_args.len(),
                        span,
                    });
                }
                // Non-generic class with explicit `<...>` args is an error.
                if class_params.is_empty() && !type_args.is_empty() {
                    return Err(TypeError::ArityMismatch {
                        name: format!("{class}::<type args>"),
                        expected: 0,
                        got: type_args.len(),
                        span,
                    });
                }
                let inst_args: Vec<Type> = type_args.clone();
                if let Some(init) = init_raw {
                    let sig = Signature {
                        params: init
                            .params
                            .iter()
                            .map(|t| subst_type(t, &class_params, &inst_args))
                            .collect(),
                        ret: subst_type(&init.ret, &class_params, &inst_args),
                        variadic: init.variadic,
                        type_params: Vec::new(),
                    };
                    self.check_args(
                        &format!("{class}::init"),
                        &sig,
                        args,
                        env,
                        ret_ty,
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
                Ok(if class_params.is_empty() {
                    Type::Object(class.clone())
                } else {
                    Type::Generic {
                        base: class.clone(),
                        args: inst_args,
                    }
                })
            }
            ExprKind::Block(b) => self.check_block(b, env, ret_ty, in_class, loop_depth),
            ExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let c = self.check_expr(cond, env, ret_ty, in_class, loop_depth)?;
                if c != Type::Bool {
                    return Err(TypeError::Mismatch {
                        expected: Type::Bool,
                        got: c,
                        span: cond.span,
                    });
                }
                let then_ty = self.check_block(then_branch, env, ret_ty, in_class, loop_depth)?;
                match else_branch {
                    None => {
                        // No else: the expression evaluates to () regardless
                        // of the then-branch's type (any value would be
                        // discarded). Mirrors `if let some(...)` and matches
                        // the JS-style intent of "do this conditionally".
                        Ok(Type::Unit)
                    }
                    Some(else_e) => {
                        let else_ty = self.check_expr(else_e, env, ret_ty, in_class, loop_depth)?;
                        if then_ty == else_ty {
                            return Ok(then_ty);
                        }
                        // Generic types (e.g. `Result<T, E>`) where each
                        // arm fixed a different type parameter need to
                        // be merged into the more specific shape — e.g.
                        // `Result<i64, Any>` and `Result<Any, string>`
                        // unify to `Result<i64, string>`.
                        if let Some(merged) = merge_generic_with_holes(&then_ty, &else_ty) {
                            return Ok(merged);
                        }
                        // Rust 流: 暗黙の数値拡張は禁止 (i64 と f64 を
                        // ぶつけたらエラー)。例外として、片方のアームの末尾式
                        // が「素の数値リテラル」 (整数/浮動小数、単項マイナス
                        // 込み) で、もう一方の型に収まるときだけ受け入れる。
                        let then_tail = then_branch.tail.as_deref();
                        if let Some(t) = then_tail {
                            if numeric_literal_fits(t, &else_ty) {
                                return Ok(else_ty);
                            }
                        }
                        if numeric_literal_fits(else_e, &then_ty) {
                            return Ok(then_ty);
                        }
                        Err(TypeError::Mismatch {
                            expected: then_ty,
                            got: else_ty,
                            span: else_e.span,
                        })
                    }
                }
            }
            ExprKind::While { cond, body } => {
                let c = self.check_expr(cond, env, ret_ty, in_class, loop_depth)?;
                if c != Type::Bool {
                    return Err(TypeError::Mismatch {
                        expected: Type::Bool,
                        got: c,
                        span: cond.span,
                    });
                }
                let body_ty = self.check_block(body, env, ret_ty, in_class, loop_depth + 1)?;
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
                let body_ty = self.check_block(body, env, ret_ty, in_class, loop_depth + 1)?;
                if body_ty != Type::Unit {
                    return Err(TypeError::Mismatch {
                        expected: Type::Unit,
                        got: body_ty,
                        span,
                    });
                }
                Ok(Type::Unit)
            }
            ExprKind::ForIn { var, iter, body } => {
                let it = self.check_expr(iter, env, ret_ty, in_class, loop_depth)?;
                let elem = match &it {
                    Type::Array { elem, .. } => (**elem).clone(),
                    other => {
                        return Err(TypeError::Mismatch {
                            expected: Type::Array {
                                elem: Box::new(Type::Any),
                                fixed: None,
                            },
                            got: other.clone(),
                            span: iter.span,
                        });
                    }
                };
                let mut inner = env.clone();
                inner.insert(var.clone(), elem);
                let body_ty =
                    self.check_block(body, &inner, ret_ty, in_class, loop_depth + 1)?;
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
            ExprKind::Return(value) => {
                let expected = match ret_ty {
                    Some(t) => t.clone(),
                    None => {
                        return Err(TypeError::Unsupported {
                            what: "`return` outside of a function body".into(),
                            span,
                        });
                    }
                };
                match value {
                    Some(v) => {
                        let vt = self.check_expr(v, env, ret_ty, in_class, loop_depth)?;
                        if !literal_assignable(v, &vt, &expected) {
                            return Err(TypeError::Mismatch {
                                expected,
                                got: vt,
                                span: v.span,
                            });
                        }
                    }
                    None => {
                        if !matches!(expected, Type::Unit) {
                            return Err(TypeError::Mismatch {
                                expected,
                                got: Type::Unit,
                                span,
                            });
                        }
                    }
                }
                // `return` diverges — control never continues past it.
                // We pretend the expression has the function's return
                // type so a body ending in `return X` and a non-else
                // `if cond { return X }` both type-check without
                // needing a separate Never type.
                Ok(expected)
            }
            ExprKind::Assign { target, value } => {
                if let Some(var_ty) = env.get(target).cloned() {
                    let v_ty = self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
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
                            let v_ty = self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
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
                let first_ty = self.check_expr(&elements[0], env, ret_ty, in_class, loop_depth)?;
                for e in &elements[1..] {
                    let et = self.check_expr(e, env, ret_ty, in_class, loop_depth)?;
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
            ExprKind::MapLit(entries) => {
                // The parser only ever emits MapLit when there's at least
                // one `key: value` entry; `{}` parses as an empty block.
                let (k0, v0) = &entries[0];
                let k_ty = self.check_expr(k0, env, ret_ty, in_class, loop_depth)?;
                if !is_valid_map_key_type(&k_ty) {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "map key type {k_ty} (only string / int / bool keys are supported)"
                        ),
                        span: k0.span,
                    });
                }
                let v_ty = self.check_expr(v0, env, ret_ty, in_class, loop_depth)?;
                for (k, v) in &entries[1..] {
                    let kt = self.check_expr(k, env, ret_ty, in_class, loop_depth)?;
                    if !literal_assignable(k, &kt, &k_ty) {
                        return Err(TypeError::Mismatch {
                            expected: k_ty.clone(),
                            got: kt,
                            span: k.span,
                        });
                    }
                    let vt = self.check_expr(v, env, ret_ty, in_class, loop_depth)?;
                    if !literal_assignable(v, &vt, &v_ty) {
                        return Err(TypeError::Mismatch {
                            expected: v_ty.clone(),
                            got: vt,
                            span: v.span,
                        });
                    }
                }
                Ok(Type::Generic {
                    base: "Map".into(),
                    args: vec![k_ty, v_ty],
                })
            }
            ExprKind::Index { obj, index } => {
                let ot = self.check_expr(obj, env, ret_ty, in_class, loop_depth)?;
                let it = self.check_expr(index, env, ret_ty, in_class, loop_depth)?;
                // Map<K, V> indexing: `m[k]` returns V (panics at runtime
                // if missing — use `.get(k)` for `V?`).
                if let Type::Generic { base, args } = &ot {
                    if base == "Map" && args.len() == 2 {
                        if !literal_assignable(index, &it, &args[0]) {
                            return Err(TypeError::Mismatch {
                                expected: args[0].clone(),
                                got: it,
                                span: index.span,
                            });
                        }
                        return Ok(args[1].clone());
                    }
                }
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
                let ot = self.check_expr(obj, env, ret_ty, in_class, loop_depth)?;
                let it = self.check_expr(index, env, ret_ty, in_class, loop_depth)?;
                // Map<K, V>: `m[k] = v` desugars to `set(k, v)`.
                if let Type::Generic { base, args } = &ot {
                    if base == "Map" && args.len() == 2 {
                        if !literal_assignable(index, &it, &args[0]) {
                            return Err(TypeError::Mismatch {
                                expected: args[0].clone(),
                                got: it,
                                span: index.span,
                            });
                        }
                        let vt = self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
                        if !literal_assignable(value, &vt, &args[1]) {
                            return Err(TypeError::Mismatch {
                                expected: args[1].clone(),
                                got: vt,
                                span: value.span,
                            });
                        }
                        return Ok(Type::Unit);
                    }
                }
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
                let vt = self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
                if !literal_assignable(value, &vt, &elem_ty) {
                    return Err(TypeError::Mismatch {
                        expected: elem_ty,
                        got: vt,
                        span: value.span,
                    });
                }
                Ok(Type::Unit)
            }
            ExprKind::FnExpr { params, ret, body } => {
                for Param { ty, span: pspan, .. } in params {
                    self.validate_type(ty, *pspan, &[])?;
                }
                if let Some(r) = ret {
                    self.validate_type(r, span, &[])?;
                }
                // Closures aren't supported yet — the anon body sees
                // only its own parameters (plus top-level fns/classes
                // resolved through `self.*` tables), not outer locals.
                let mut inner: Vars = HashMap::new();
                for Param { name, ty, .. } in params {
                    inner.insert(name.clone(), ty.clone());
                }
                let expected = ret.clone().unwrap_or(Type::Unit);
                let body_ty =
                    self.check_block(body, &inner, Some(&expected), in_class, 0)?;
                if !assignable(&body_ty, &expected) {
                    return Err(TypeError::BadReturn {
                        name: "<closure>".into(),
                        expected,
                        got: body_ty,
                        span,
                    });
                }
                Ok(Type::Fn {
                    params: params.iter().map(|p| p.ty.clone()).collect(),
                    ret: Box::new(ret.clone().unwrap_or(Type::Unit)),
                })
            }
            ExprKind::Cast { expr: inner, ty } => {
                let from = self.check_expr(inner, env, ret_ty, in_class, loop_depth)?;
                self.validate_type(ty, span, &[])?;
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
                let ot = self.check_expr(obj, env, ret_ty, in_class, loop_depth)?;
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
                let v_ty = self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
                if !literal_assignable(value, &v_ty, &field_ty) {
                    return Err(TypeError::Mismatch {
                        expected: field_ty,
                        got: v_ty,
                        span: value.span,
                    });
                }
                Ok(Type::Unit)
            }
            ExprKind::None => Ok(Type::Optional(Box::new(Type::Any))),
            ExprKind::Some(inner) => {
                let it = self.check_expr(inner, env, ret_ty, in_class, loop_depth)?;
                Ok(Type::Optional(Box::new(it)))
            }
            ExprKind::IfLet {
                name,
                expr,
                then_branch,
                else_branch,
            } => {
                let scrut_ty = self.check_expr(expr, env, ret_ty, in_class, loop_depth)?;
                let inner = match &scrut_ty {
                    Type::Optional(t) => (**t).clone(),
                    _ => {
                        return Err(TypeError::Mismatch {
                            expected: Type::Optional(Box::new(Type::Any)),
                            got: scrut_ty,
                            span: expr.span,
                        });
                    }
                };
                // Inner must be concrete for the binding to be useful;
                // we reject `if let some(x) = none` because the type of
                // x would be `Any`.
                if matches!(inner, Type::Any) {
                    return Err(TypeError::Mismatch {
                        expected: Type::Optional(Box::new(Type::Any)),
                        got: scrut_ty,
                        span: expr.span,
                    });
                }
                let mut then_env = env.clone();
                then_env.insert(name.clone(), inner);
                let then_ty = self.check_block(then_branch, &then_env, ret_ty, in_class, loop_depth)?;
                if let Some(eb) = else_branch {
                    let else_ty = self.check_expr(eb, env, ret_ty, in_class, loop_depth)?;
                    // Pick the unifying type: if either branch is Unit, the
                    // overall expr is Unit (statement-style); otherwise the
                    // two branches must agree.
                    if matches!(then_ty, Type::Unit) || matches!(else_ty, Type::Unit) {
                        Ok(Type::Unit)
                    } else if assignable(&else_ty, &then_ty) {
                        Ok(then_ty)
                    } else if assignable(&then_ty, &else_ty) {
                        Ok(else_ty)
                    } else {
                        Err(TypeError::Mismatch {
                            expected: then_ty,
                            got: else_ty,
                            span,
                        })
                    }
                } else {
                    // No else: the result is Unit even if then has a value.
                    Ok(Type::Unit)
                }
            }
            ExprKind::EnumCtor {
                enum_name,
                variant,
                args,
            } => {
                let sig = self.enums.get(enum_name).cloned().ok_or_else(|| {
                    TypeError::UndefinedClass {
                        name: enum_name.clone(),
                        span,
                    }
                })?;
                let v = sig.variants.iter().find(|v| v.name == *variant).ok_or_else(|| {
                    TypeError::Unsupported {
                        what: format!("enum {enum_name:?} has no variant {variant:?}"),
                        span,
                    }
                })?;
                let type_params = sig.type_params.clone();
                // First pass: gather arg types, infer type-parameter
                // bindings from the (parametric payload type, arg type)
                // pairs. Bindings absent here stay as `Any`, to be
                // refined by an outer annotation.
                let mut bindings: HashMap<String, Type> = HashMap::new();
                let mut arg_tys_tuple: Vec<Type> = Vec::new();
                let mut arg_tys_struct: Vec<(String, Type)> = Vec::new();
                match (&v.payload, args) {
                    (VariantPayloadSig::Unit, CtorArgs::Unit) => {}
                    (VariantPayloadSig::Tuple(tys), CtorArgs::Tuple(elems)) => {
                        if tys.len() != elems.len() {
                            return Err(TypeError::ArityMismatch {
                                name: format!("{enum_name}::{variant}"),
                                expected: tys.len(),
                                got: elems.len(),
                                span,
                            });
                        }
                        for (e, t) in elems.iter().zip(tys.iter()) {
                            let et = self.check_expr(e, env, ret_ty, in_class, loop_depth)?;
                            collect_type_var_bindings(t, &et, &mut bindings);
                            arg_tys_tuple.push(et);
                        }
                    }
                    (VariantPayloadSig::Struct(fields), CtorArgs::Struct(provided)) => {
                        if provided.len() != fields.len() {
                            return Err(TypeError::ArityMismatch {
                                name: format!("{enum_name}::{variant}"),
                                expected: fields.len(),
                                got: provided.len(),
                                span,
                            });
                        }
                        for (fname, fty) in fields {
                            let supplied = provided.iter().find(|(n, _)| n == fname).ok_or_else(
                                || TypeError::UnknownField {
                                    class: format!("{enum_name}::{variant}"),
                                    field: fname.clone(),
                                    span,
                                },
                            )?;
                            let st = self.check_expr(
                                &supplied.1,
                                env,
                                ret_ty,
                                in_class,
                                loop_depth,
                            )?;
                            collect_type_var_bindings(fty, &st, &mut bindings);
                            arg_tys_struct.push((fname.clone(), st));
                        }
                    }
                    _ => {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "constructor shape for {enum_name}::{variant} doesn't match its declaration"
                            ),
                            span,
                        });
                    }
                }
                // Build inferred type-arg vector (Any for unsolved).
                let inferred_args: Vec<Type> = type_params
                    .iter()
                    .map(|p| bindings.get(p).cloned().unwrap_or(Type::Any))
                    .collect();
                // Validate each arg against the substituted payload type.
                match (&v.payload, args) {
                    (VariantPayloadSig::Unit, _) => {}
                    (VariantPayloadSig::Tuple(tys), CtorArgs::Tuple(elems)) => {
                        for ((e, t), et) in elems.iter().zip(tys.iter()).zip(arg_tys_tuple.iter()) {
                            let actual = subst_type(t, &type_params, &inferred_args);
                            if !literal_assignable(e, et, &actual) {
                                return Err(TypeError::Mismatch {
                                    expected: actual,
                                    got: et.clone(),
                                    span: e.span,
                                });
                            }
                        }
                    }
                    (VariantPayloadSig::Struct(fields), CtorArgs::Struct(provided)) => {
                        for (fname, fty) in fields {
                            let supplied = provided.iter().find(|(n, _)| n == fname).unwrap();
                            let st = arg_tys_struct
                                .iter()
                                .find(|(n, _)| n == fname)
                                .map(|(_, t)| t.clone())
                                .unwrap();
                            let actual = subst_type(fty, &type_params, &inferred_args);
                            if !literal_assignable(&supplied.1, &st, &actual) {
                                return Err(TypeError::Mismatch {
                                    expected: actual,
                                    got: st,
                                    span: supplied.1.span,
                                });
                            }
                        }
                    }
                    _ => {}
                }
                Ok(if type_params.is_empty() {
                    Type::Object(enum_name.clone())
                } else {
                    Type::Generic {
                        base: enum_name.clone(),
                        args: inferred_args,
                    }
                })
            }
            ExprKind::Match { scrutinee, arms } => {
                let st = self.check_expr(scrutinee, env, ret_ty, in_class, loop_depth)?;
                let (enum_name, scrut_args) = match &st {
                    Type::Object(name) if self.enums.contains_key(name) => {
                        (name.clone(), Vec::<Type>::new())
                    }
                    Type::Generic { base, args } if self.enums.contains_key(base) => {
                        (base.clone(), args.clone())
                    }
                    _ => {
                        return Err(TypeError::Mismatch {
                            expected: Type::Object("<enum>".into()),
                            got: st,
                            span: scrutinee.span,
                        });
                    }
                };
                let sig = self.enums[&enum_name].clone();
                let enum_params = sig.type_params.clone();
                let mut covered: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                let mut has_wildcard = false;
                let mut result_ty: Option<Type> = None;
                for arm in arms {
                    if has_wildcard {
                        return Err(TypeError::Unsupported {
                            what: "match arm after wildcard `_` is unreachable".into(),
                            span: arm.span,
                        });
                    }
                    let mut arm_env = env.clone();
                    let arm_kind_span = arm.pattern.span;
                    match &arm.pattern.kind {
                        PatternKind::Wildcard => {
                            has_wildcard = true;
                        }
                        PatternKind::Variant {
                            enum_name: pat_enum,
                            variant,
                            bindings,
                        } => {
                            // Short form (`Variant ...` without `Enum::`)
                            // borrows the scrutinee's enum name. Long
                            // form must match it exactly.
                            if let Some(pe) = pat_enum {
                                if pe != &enum_name {
                                    return Err(TypeError::Mismatch {
                                        expected: Type::Object(enum_name.clone()),
                                        got: Type::Object(pe.clone()),
                                        span: arm_kind_span,
                                    });
                                }
                            }
                            let v = sig
                                .variants
                                .iter()
                                .find(|v| v.name == *variant)
                                .ok_or_else(|| TypeError::Unsupported {
                                    what: format!(
                                        "enum {enum_name:?} has no variant {variant:?}"
                                    ),
                                    span: arm_kind_span,
                                })?;
                            if !covered.insert(variant.clone()) {
                                return Err(TypeError::Unsupported {
                                    what: format!("duplicate match arm for {variant:?}"),
                                    span: arm_kind_span,
                                });
                            }
                            // Check binding shape matches and add bindings.
                            // Generic enums: substitute the scrutinee's
                            // concrete type args into each parametric
                            // payload type before binding the name.
                            match (&v.payload, bindings) {
                                (VariantPayloadSig::Unit, PatternBindings::Unit) => {}
                                (
                                    VariantPayloadSig::Tuple(tys),
                                    PatternBindings::Tuple(names),
                                ) => {
                                    if tys.len() != names.len() {
                                        return Err(TypeError::ArityMismatch {
                                            name: format!("{enum_name}::{variant}"),
                                            expected: tys.len(),
                                            got: names.len(),
                                            span: arm_kind_span,
                                        });
                                    }
                                    for (n, t) in names.iter().zip(tys.iter()) {
                                        if n != "_" {
                                            let bind_ty =
                                                subst_type(t, &enum_params, &scrut_args);
                                            arm_env.insert(n.clone(), bind_ty);
                                        }
                                    }
                                }
                                (
                                    VariantPayloadSig::Struct(fields),
                                    PatternBindings::Struct(pairs),
                                ) => {
                                    for (fname, bname) in pairs {
                                        let fty = fields
                                            .iter()
                                            .find(|(n, _)| n == fname)
                                            .map(|(_, t)| t.clone())
                                            .ok_or_else(|| TypeError::UnknownField {
                                                class: format!("{enum_name}::{variant}"),
                                                field: fname.clone(),
                                                span: arm_kind_span,
                                            })?;
                                        if bname != "_" {
                                            let bind_ty =
                                                subst_type(&fty, &enum_params, &scrut_args);
                                            arm_env.insert(bname.clone(), bind_ty);
                                        }
                                    }
                                }
                                _ => {
                                    return Err(TypeError::Unsupported {
                                        what: format!(
                                            "pattern shape for {enum_name}::{variant} doesn't match its declaration"
                                        ),
                                        span: arm_kind_span,
                                    });
                                }
                            }
                        }
                    }
                    let bt = self.check_expr(&arm.body, &arm_env, ret_ty, in_class, loop_depth)?;
                    result_ty = Some(match result_ty {
                        None => bt,
                        Some(prev) => unify_branch(prev, bt, arm.body.span)?,
                    });
                }
                if !has_wildcard {
                    let total = sig.variants.len();
                    if covered.len() != total {
                        let missing: Vec<_> = sig
                            .variants
                            .iter()
                            .filter(|v| !covered.contains(&v.name))
                            .map(|v| v.name.clone())
                            .collect();
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "non-exhaustive match on {enum_name}: missing {}",
                                missing.join(", ")
                            ),
                            span,
                        });
                    }
                }
                Ok(result_ty.unwrap_or(Type::Unit))
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
        ret_ty: Option<&Type>,
        in_class: Option<&str>,
        loop_depth: u32,
        call_span: Span,
    ) -> Result<(), TypeError> {
        if sig.variadic {
            // Variadic: any arity, every arg type-checks but acts as `Any`.
            for arg in args {
                self.check_expr(arg, env, ret_ty, in_class, loop_depth)?;
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
            let at = self.check_expr(arg, env, ret_ty, in_class, loop_depth)?;
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

/// Walk a parametric payload type alongside a concrete arg type and
/// record bindings for each `TypeVar` encountered. Used by the enum
/// constructor checker to infer type arguments from call args.
/// First-found binding wins for any given TypeVar.
fn collect_type_var_bindings(
    payload: &Type,
    arg: &Type,
    bindings: &mut HashMap<String, Type>,
) {
    match (payload, arg) {
        (Type::TypeVar(name), other) => {
            bindings.entry(name.clone()).or_insert_with(|| other.clone());
        }
        (Type::Array { elem: pe, .. }, Type::Array { elem: ae, .. }) => {
            collect_type_var_bindings(pe, ae, bindings);
        }
        (Type::Optional(p), Type::Optional(a)) => {
            collect_type_var_bindings(p, a, bindings);
        }
        (Type::Weak(p), Type::Weak(a)) => {
            collect_type_var_bindings(p, a, bindings);
        }
        (Type::Generic { args: pa, .. }, Type::Generic { args: aa, .. }) => {
            for (p, a) in pa.iter().zip(aa.iter()) {
                collect_type_var_bindings(p, a, bindings);
            }
        }
        _ => {}
    }
}

/// Map keys are constrained to types with stable structural equality.
/// Floats are excluded (NaN), as are heap objects and arrays.
fn is_valid_map_key_type(t: &Type) -> bool {
    matches!(
        t,
        Type::Str | Type::Bool
            | Type::I8 | Type::I16 | Type::I32 | Type::I64
            | Type::U8 | Type::U16 | Type::U32 | Type::U64
    )
}

fn is_reserved_class(name: &str) -> bool {
    matches!(name, "Console" | "Map" | "Result")
}

fn is_reserved_global(name: &str) -> bool {
    matches!(name, "console")
}

fn signature_of(f: &FnDecl) -> Signature {
    // Rewrite the fn's own `<T, U>` type parameters from `Object(T)` to
    // `TypeVar(T)` so call-site inference (which substitutes for
    // `TypeVar`) fires. Methods rewrite the *class's* type params on top
    // of this in `class_signature`.
    let params: Vec<Type> = f
        .params
        .iter()
        .map(|p| rewrite_type_params(&p.ty, &f.type_params))
        .collect();
    let ret = rewrite_type_params(
        &f.ret.clone().unwrap_or(Type::Unit),
        &f.type_params,
    );
    Signature {
        params,
        ret,
        variadic: false,
        type_params: f.type_params.clone(),
    }
}

fn class_signature(c: &ClassDecl) -> ClassSig {
    let mut fields = HashMap::new();
    for f in &c.fields {
        fields.insert(f.name.clone(), rewrite_type_params(&f.ty, &c.type_params));
    }
    let mut methods = HashMap::new();
    for m in &c.methods {
        let mut sig = signature_of(m);
        for p in sig.params.iter_mut() {
            *p = rewrite_type_params(p, &c.type_params);
        }
        sig.ret = rewrite_type_params(&sig.ret, &c.type_params);
        methods.insert(m.name.clone(), sig);
    }
    ClassSig {
        type_params: c.type_params.clone(),
        fields,
        methods,
    }
}

/// The parser produces `Type::Object(name)` for any user-defined type
/// reference. Inside a generic class body, references that match the
/// class's type-parameter names are actually type variables — convert
/// them to `Type::TypeVar` so the checker can substitute later.
fn rewrite_type_params(t: &Type, params: &[String]) -> Type {
    match t {
        Type::Object(name) if params.iter().any(|p| p == name) => {
            Type::TypeVar(name.clone())
        }
        Type::Array { elem, fixed } => Type::Array {
            elem: Box::new(rewrite_type_params(elem, params)),
            fixed: *fixed,
        },
        Type::Optional(inner) => {
            Type::Optional(Box::new(rewrite_type_params(inner, params)))
        }
        Type::Weak(inner) => Type::Weak(Box::new(rewrite_type_params(inner, params))),
        Type::Generic { base, args } => Type::Generic {
            base: base.clone(),
            args: args
                .iter()
                .map(|a| rewrite_type_params(a, params))
                .collect(),
        },
        _ => t.clone(),
    }
}

/// Substitute concrete types for type variables. Used when a generic
/// class is instantiated: each `TypeVar(P)` is replaced with the i-th
/// concrete arg from the matching position in `params`.
fn subst_type(t: &Type, params: &[String], args: &[Type]) -> Type {
    match t {
        Type::TypeVar(name) => params
            .iter()
            .position(|p| p == name)
            .and_then(|i| args.get(i).cloned())
            .unwrap_or_else(|| t.clone()),
        Type::Array { elem, fixed } => Type::Array {
            elem: Box::new(subst_type(elem, params, args)),
            fixed: *fixed,
        },
        Type::Optional(inner) => Type::Optional(Box::new(subst_type(inner, params, args))),
        Type::Weak(inner) => Type::Weak(Box::new(subst_type(inner, params, args))),
        Type::Generic { base, args: targs } => Type::Generic {
            base: base.clone(),
            args: targs.iter().map(|a| subst_type(a, params, args)).collect(),
        },
        _ => t.clone(),
    }
}

/// Pick the unifying type between two branches of an `if`/`match`. If
/// either side is assignable to the other, the wider one wins; otherwise
/// surface a type-mismatch.
fn unify_branch(a: Type, b: Type, span: Span) -> Result<Type, TypeError> {
    if a == b {
        return Ok(a);
    }
    // Generic with `Any` placeholders (from enum-ctor inference) — try
    // to merge the two sides into the more specific type.
    if let Some(merged) = merge_generic_with_holes(&a, &b) {
        return Ok(merged);
    }
    if assignable(&a, &b) {
        return Ok(b);
    }
    if assignable(&b, &a) {
        return Ok(a);
    }
    Err(TypeError::Mismatch {
        expected: a,
        got: b,
        span,
    })
}

/// When two arms each produced a `Type::Generic` with the same base
/// but different concrete args (commonly with `Any` placeholders left
/// over from constructor-type inference, e.g. `Result<i64, Any>` on
/// one side and `Result<Any, string>` on the other), merge them by
/// taking the non-`Any` side at each position. Returns `None` if the
/// bases differ, the arities differ, or any position has two
/// incompatible non-`Any` types.
fn merge_generic_with_holes(a: &Type, b: &Type) -> Option<Type> {
    let (Type::Generic { base: ba, args: aa }, Type::Generic { base: bb, args: ab }) =
        (a, b)
    else {
        return None;
    };
    if ba != bb || aa.len() != ab.len() {
        return None;
    }
    let mut merged = Vec::with_capacity(aa.len());
    for (x, y) in aa.iter().zip(ab.iter()) {
        if x == y {
            merged.push(x.clone());
        } else if matches!(x, Type::Any) {
            merged.push(y.clone());
        } else if matches!(y, Type::Any) {
            merged.push(x.clone());
        } else if let Some(inner) = merge_generic_with_holes(x, y) {
            merged.push(inner);
        } else {
            return None;
        }
    }
    Some(Type::Generic {
        base: ba.clone(),
        args: merged,
    })
}

fn enum_signature(e: &EnumDecl) -> EnumSig {
    let params = &e.type_params;
    let variants = e
        .variants
        .iter()
        .map(|v| EnumVariantSig {
            name: v.name.clone(),
            payload: match &v.payload {
                VariantPayload::Unit => VariantPayloadSig::Unit,
                VariantPayload::Tuple(tys) => VariantPayloadSig::Tuple(
                    tys.iter().map(|t| rewrite_type_params(t, params)).collect(),
                ),
                VariantPayload::Struct(fs) => VariantPayloadSig::Struct(
                    fs.iter()
                        .map(|f| (f.name.clone(), rewrite_type_params(&f.ty, params)))
                        .collect(),
                ),
            },
        })
        .collect();
    EnumSig {
        type_params: e.type_params.clone(),
        variants,
    }
}

fn expect_object(t: &Type, span: Span) -> Result<&str, TypeError> {
    match t {
        Type::Object(name) => Ok(name),
        Type::Generic { base, .. } => Ok(base),
        _ => Err(TypeError::Mismatch {
            expected: Type::Object("<object>".into()),
            got: t.clone(),
            span,
        }),
    }
}

/// Extract the concrete type arguments from an object-typed value, if
/// any. Non-generic objects return an empty slice.
fn type_args_of(t: &Type) -> &[Type] {
    if let Type::Generic { args, .. } = t {
        args
    } else {
        &[]
    }
}

/// Helper for `bin_result`-style spanless errors (the ops module returns
/// `BadBinary`/`BadUnary` without knowing the source position; we attach
/// the surrounding expression's span here).
fn attach_span(e: TypeError, span: Span) -> TypeError {
    match e {
        TypeError::BadBinary { lhs, rhs, .. } => TypeError::BadBinary { lhs, rhs, span },
        TypeError::BadUnary { ty, .. } => TypeError::BadUnary { ty, span },
        TypeError::MixedSignedness { lhs, rhs, .. } => {
            TypeError::MixedSignedness { lhs, rhs, span }
        }
        other => other,
    }
}
