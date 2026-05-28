//! Type inference: `infer_expr` (the walker-aware variant that knows
//! about Call / MethodCall return types) plus `resolve_obj_class` for
//! turning an `obj` expression into the class its dotted accesses
//! should look at. Also houses the built-in `Map<K,V>` / `Set<T>`
//! method-return tables and the integer / float primitive `Min` /
//! `Max` constants — small lookup tables the LSP keeps here rather
//! than in `ClassInfo`.

use super::*;

impl<'a> Walker<'a> {
    /// `ClassName.staticMethod()` — parsed as a single dotted callee
    /// rather than `Class.method` MethodCall. Resolve through the
    /// class's `methods` table so chained calls like
    /// `Foo.alloc().init()` can infer past the first hop.
    fn infer_dotted_static_call(&self, callee: &str) -> Option<Type> {
        let (cls, m) = callee.rsplit_once('.')?;
        let info = self.classes.get(&AstSymbol::intern(cls))?;
        info.methods.get(&AstSymbol::intern(m))?.ret_ty.clone()
    }

    /// Walker-aware variant of `infer_expr_type_with_scope` that can
    /// also resolve `Call(callee)` to the callee's declared return
    /// type and `MethodCall` to the resolved method's return type.
    pub(crate) fn infer_expr(&self, e: &Expr, scope: &[Binding]) -> Option<Type> {
        match &e.kind {
            ExprKind::Var(name) => {
                // Locals shadow consts — try scope first, then the
                // module-level const map, then the cross-module
                // returns/consts map (`pub const` imported via
                // `use M { X }` lives there since the loader inlines
                // the literal out of the merged program).
                if let Some(b) = scope.iter().rev().find(|b| b.name == name.as_str())
                {
                    return b.ty.clone();
                }
                self.consts
                    .get(name)
                    .cloned()
                    .or_else(|| self.external_returns.get(name).cloned())
                    // Top-level `let X = expr` bindings (Map / array /
                    // class instance / ...) are recorded in
                    // `var_types` by the pre-pass in `diag.rs`. Without
                    // this fallback an inner method body referencing
                    // `X` couldn't infer its type, so `.get(...)` on a
                    // module-level `Map` wouldn't hit the builtin
                    // hover / ref path.
                    .or_else(|| self.var_types.get(name).cloned())
            }
            ExprKind::Call { callee, args } => {
                let ret = self
                    .fn_returns
                    .get(callee)
                    .or_else(|| self.external_returns.get(callee))
                    .cloned()
                    .or_else(|| {
                        // FFI marshalling helpers (`cstrFromString`,
                        // `readU64`, ...) are pre-registered by the type
                        // checker but never declared in the buffer, so
                        // they don't sit in `fn_returns` or
                        // `external_returns`. Look them up by name so
                        // a binding like `let p = cstrFromString(s)`
                        // hovers with its pointer type.
                        crate::builtins::ffi_helper_return_type(callee.as_str())
                    })
                    .or_else(|| self.infer_dotted_static_call(callee.as_str()))?;
                // Generic intrinsics (`arrayFromCArray<T>(p: *const T,
                // …): T[]`) ship with a TypeVar in the return type.
                // When `external_fn_params` carries the matching param
                // list, infer T from arg types and substitute so
                // hover lands on `u16[]` instead of `T[]`.
                if type_mentions_typevar(&ret) {
                    if let Some(param_tys) = self.external_fn_params.get(callee).cloned() {
                        let mut subst: HashMap<AstSymbol, Type> = HashMap::new();
                        for (p_ty, arg) in param_tys.iter().zip(args.iter()) {
                            if let Some(a_ty) = self.infer_expr(arg, scope) {
                                unify_typevars(p_ty, &a_ty, &mut subst);
                            }
                        }
                        if !subst.is_empty() {
                            return Some(substitute_typevars(&ret, &subst));
                        }
                    }
                }
                Some(ret)
            }
            ExprKind::MethodCall { obj, method, .. } => {
                if let Some(Type::Generic(g)) = self.infer_expr(obj, scope) {
                    if let Some(t) = infer_map_method_type(&g, method.as_str()) {
                        return Some(t);
                    }
                    if let Some(t) = infer_set_method_type(&g, method.as_str()) {
                        return Some(t);
                    }
                }
                let this_class = self.current_this_class.as_deref();
                let class = self.resolve_obj_class(obj, scope, this_class)?;
                let info = self.classes.get(&AstSymbol::intern(&class))?;
                info.methods.get(&AstSymbol::intern(method.as_str()))?.ret_ty.clone()
            }
            ExprKind::Field { obj, name } => {
                // Float primitive associated constants —
                // `f32.NaN` / `f64.MinPositive` etc. Same set the
                // type checker recognises in `check_field`.
                if let ExprKind::Var(recv) = &obj.kind {
                    if let Some(t) = float_prim_const_ty(recv.as_str(), name.as_str()) {
                        return Some(t);
                    }
                    if let Some(t) = int_prim_const_ty(recv.as_str(), name.as_str()) {
                        return Some(t);
                    }
                }
                // `EnumName.Variant` parses as Field too. Try the
                // class path first; if that misses, check whether
                // `obj` names an enum we know about (variant entries
                // live in `external_signatures` under the composite
                // `EnumName.Variant` key) and lift the result to
                // `Type::Object(EnumName)` so a chain like
                // `Flag.a | Flag.b` carries the enum type up.
                //
                // Use `current_this_class` so a `this.field` obj
                // resolves to its class — without it,
                // `this.foo.bar` hover misses the inner `foo`'s
                // type and `bar` falls off the lookup.
                let this_class = self.current_this_class.as_deref();
                if let Some(class) = self.resolve_obj_class(obj, scope, this_class) {
                    if let Some(info) = self.classes.get(&AstSymbol::intern(&class)) {
                        if let Some(t) = info.fields.get(name).and_then(|f| f.ret_ty.clone()) {
                            return Some(t);
                        }
                    }
                }
                // Built-in `.length` on string / array — both return
                // i64. Mirrors the hover ref entry built above in
                // `walk_expr`'s Field arm so chained inference (a
                // `let n = s.length` binding, `let m = (a + s.length)`,
                // etc.) carries the type.
                if name.as_str() == "length" {
                    if let Some(t) = self.infer_expr(obj, scope) {
                        if matches!(t, Type::Str | Type::Array { .. }) {
                            return Some(Type::I64);
                        }
                    }
                }
                let obj_name = enum_obj_name(obj)?;
                let key = AstSymbol::intern(&format!("{obj_name}.{name}"));
                let sig = self.external_signatures.get(&key)?;
                if sig.starts_with("(variant)") {
                    Some(Type::Object(AstSymbol::intern(&obj_name)))
                } else {
                    None
                }
            }
            ExprKind::Index { obj, .. } => match self.infer_expr(obj, scope)? {
                Type::Array { elem, .. } => Some(*elem),
                Type::Str => Some(Type::U8),
                _ => None,
            },
            ExprKind::If { then_branch, else_branch, .. } => {
                let from_then = then_branch
                    .tail
                    .as_ref()
                    .and_then(|t| self.infer_expr(t, scope));
                from_then.or_else(|| {
                    else_branch.as_ref().and_then(|e| self.infer_expr(e, scope))
                })
            }
            ExprKind::IfLet { name, expr, then_branch, else_branch } => {
                let inner_ty = self.infer_expr(expr, scope).and_then(|t| match t {
                    Type::Optional(inner) => Some(*inner),
                    _ => None,
                });
                let mut then_scope = scope.to_vec();
                then_scope.push(Binding {
                    name: name.as_str().to_string(),
                    span: e.span,
                    ty: inner_ty,
                    kind: BindKind::Let,
                    override_signature: None,
                });
                let from_then = then_branch
                    .tail
                    .as_ref()
                    .and_then(|t| self.infer_expr(t, &then_scope));
                from_then.or_else(|| {
                    else_branch.as_ref().and_then(|eb| self.infer_expr(eb, scope))
                })
            }
            ExprKind::Block(b) => b.tail.as_ref().and_then(|t| self.infer_expr(t, scope)),
            // `Foo { f1: v, f2: w }` — typed by its class name.
            // Both `@extern(C) pub struct` and plain `pub struct` use
            // the same StructLit shape, so the hover renders e.g.
            // `let wc: windows.WNDCLASSEXA`.
            ExprKind::StructLit { class, .. } => {
                Some(Type::Object(class.clone()))
            }
            // `expr as T` — the binding takes the cast's target
            // type. `let device = raw as ID3D12Device` then resolves
            // method calls (`device.CreateCommandQueue(...)`)
            // through the @com interface registered under
            // `ID3D12Device`.
            ExprKind::Cast { ty, .. } => Some(ty.clone()),
            // `loop { ... break v ... }` — the value of the loop is the
            // first `break v` we find. Bare `break` (no value) yields
            // Unit; absence of any break we treat as no info.
            ExprKind::Loop { body } => {
                let mut found: Option<Type> = None;
                find_break_type(body, scope, self, &mut found);
                found
            }
            ExprKind::Match { arms, .. } => arms.iter().find_map(|a| {
                // Pattern-bound vars (e.g. `Foo(x)` => x) must be in
                // scope when we infer the arm body, otherwise hover on
                // such a binding inside the body returns nothing.
                let mut arm_scope = scope.to_vec();
                bind_pattern(&a.pattern, &mut arm_scope);
                self.infer_expr(&a.body, &arm_scope)
            }),
            ExprKind::Binary { op, lhs, rhs } => {
                use ilang_ast::BinOp;
                if matches!(
                    op,
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
                ) {
                    return Some(Type::Bool);
                }
                let lt = self.infer_expr(lhs, scope);
                let rt = self.infer_expr(rhs, scope);
                match (lt, rt) {
                    (Some(l), Some(r)) => Some(promote_pair(&l, &r, lhs, rhs)),
                    (Some(t), None) | (None, Some(t)) => Some(t),
                    (None, None) => None,
                }
            }
            ExprKind::Unary { op, expr } => match op {
                ilang_ast::UnOp::Not => Some(Type::Bool),
                _ => self.infer_expr(expr, scope),
            },
            // `EnumName.Variant` (with or without payload args). The
            // type checker treats enums as nominal types reachable
            // through `Type::Object(EnumName)`; match that so a
            // bitwise OR chain of `@flags` variants infers cleanly.
            ExprKind::EnumCtor { enum_name, .. } => {
                Some(Type::Object(enum_name.clone()))
            }
            // `{k: v, ...}` map literal — read K from the first key
            // and V from the first value, mirroring the type checker's
            // first-entry adoption rule. An empty literal can't be
            // typed here (it needs an annotation on the `let` binding
            // to resolve K / V), so fall through to the generic path.
            ExprKind::MapLit(entries) if !entries.is_empty() => {
                let (k0, v0) = &entries[0];
                let k_ty = self.infer_expr(k0, scope)?;
                let v_ty = self.infer_expr(v0, scope)?;
                Some(Type::generic("Map", vec![k_ty, v_ty]))
            }
            // Fall back to the scope-aware inferer for everything else.
            _ => infer_expr_type_with_scope(e, scope),
        }
    }

    /// Best-effort: figure out which class an `obj` expression refers
    /// to, so `obj.field` / `obj.method()` can resolve. Handles `this`,
    /// known-typed locals, and `new ClassName(...)`.
    pub(crate) fn resolve_obj_class(
        &self,
        obj: &Expr,
        scope: &[Binding],
        this_class: Option<&str>,
    ) -> Option<String> {
        match &obj.kind {
            ExprKind::This => this_class.map(|s| s.to_string()),
            ExprKind::Var(name) => {
                if let Some(b) = scope.iter().rev().find(|b| b.name.as_str() == name.as_str()) {
                    type_to_class(b.ty.as_ref()?)
                } else if self.classes.contains_key(name) {
                    // Bare `ClassName.field/method` — static access on
                    // the class itself.
                    Some(name.as_str().to_string())
                } else if name == "console" {
                    // Built-in singleton: maps to the `Console` class.
                    Some("Console".to_string())
                } else if let Some(t) = self.var_types.get(name) {
                    // Top-level `let` whose type was inferred during
                    // the diag pre-pass — not in the per-method
                    // scope, but the class info still applies for
                    // `topLevelLet.method()` lookups inside item
                    // bodies.
                    type_to_class(t)
                } else {
                    None
                }
            }
            ExprKind::New { class, .. } => Some(class.as_str().to_string()),
            // Chained calls — `a.b().c()` needs the inner call's
            // return type resolved to a class so `.c()` knows where
            // to look. Defer to `infer_expr` (which handles Call /
            // MethodCall / Field already) and then class-ify it.
            ExprKind::Call { .. }
            | ExprKind::MethodCall { .. }
            | ExprKind::Field { .. } => {
                self.infer_expr(obj, scope).as_ref().and_then(type_to_class)
            }
            _ => None,
        }
    }
}

/// Return-type for the built-in `Map<K, V>` methods (`get`, `has`,
/// `delete`, `size`, `keys`, `values`). The LSP doesn't carry a
/// `ClassInfo` for `Map` (it's only registered in the type checker),
/// so the type-inference path resolves them off the receiver's
/// generic args directly. Returns `None` for `(base, args)` pairs
/// that aren't `Map<K, V>` or for unknown method names.
fn infer_map_method_type(g: &GenericTy, method: &str) -> Option<Type> {
    if g.base.as_str() != "Map" || g.args.len() != 2 {
        return None;
    }
    let k = g.args[0].clone();
    let v = g.args[1].clone();
    match method {
        "get" => Some(Type::Optional(Box::new(v))),
        "has" | "delete" => Some(Type::Bool),
        "size" => Some(Type::I64),
        "keys" => Some(Type::Array { elem: Box::new(k), fixed: None }),
        "values" => Some(Type::Array { elem: Box::new(v), fixed: None }),
        _ => None,
    }
}

/// `f32.NaN` / `f64.MinPositive` and friends. Receivers limited
/// to `f32` / `f64`; names mirror Rust's `f32::*` constants in
/// CamelCase. Returns the constant's static type, which is the
/// receiver type itself.
fn float_prim_const_ty(receiver: &str, name: &str) -> Option<Type> {
    let is_const = matches!(
        name,
        "NaN" | "Infinity" | "NegInfinity"
            | "Min" | "Max" | "MinPositive" | "Epsilon"
    );
    if !is_const {
        return None;
    }
    match receiver {
        "f32" => Some(Type::F32),
        "f64" => Some(Type::F64),
        _ => None,
    }
}

/// `i32.Min` / `u8.Max` etc. — the per-integer bounds. Hover
/// renders as the receiver's own type.
fn int_prim_const_ty(receiver: &str, name: &str) -> Option<Type> {
    if !matches!(name, "Min" | "Max") {
        return None;
    }
    Some(match receiver {
        "i8" => Type::I8,
        "i16" => Type::I16,
        "i32" => Type::I32,
        "i64" => Type::I64,
        "u8" => Type::U8,
        "u16" => Type::U16,
        "u32" => Type::U32,
        "u64" => Type::U64,
        _ => return None,
    })
}

/// Built-in `Set<T>` method-type inference. Symmetric with
/// `infer_map_method_type` — the LSP keeps Set's signatures here
/// rather than in `ClassInfo` so module imports don't have to do a
/// generic-class lookup just to hover `s.add(x)`.
fn infer_set_method_type(g: &GenericTy, method: &str) -> Option<Type> {
    if g.base.as_str() != "Set" || g.args.len() != 1 {
        return None;
    }
    let t = g.args[0].clone();
    match method {
        "add" | "clear" | "forEach" => Some(Type::Unit),
        "has" | "delete" | "isSubsetOf" | "isSupersetOf" | "isDisjointFrom" => Some(Type::Bool),
        "size" => Some(Type::I64),
        "values" => Some(Type::Array { elem: Box::new(t), fixed: None }),
        "union" | "intersection" | "difference" => {
            Some(Type::generic("Set", vec![t]))
        }
        _ => None,
    }
}
