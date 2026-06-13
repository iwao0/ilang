//! Extracted from `checker/mod.rs`.

#![allow(unused_imports)]

use std::collections::{HashMap, HashSet};

use ilang_ast::{
    Block, ClassDecl, CtorArgs, DiscriminantLit, EnumDecl, Expr, ExprKind, FieldDecl, FnDecl,
    Item, Param, PatternBindings, PatternKind, Program, Span, Stmt, StmtKind, Symbol, Type, UnOp,
    VariantPayload,
};

use crate::error::TypeError;
use crate::ops::{assignable, bin_result, int_literal_fits};

use super::*;

impl TypeChecker {
    pub(super) fn check_enum(&self, e: &EnumDecl) -> Result<(), TypeError> {
        // Repr / `@flags` / discriminant well-formedness. These mirror
        // the MIR lowerer's `register_enum` checks, but run here so they
        // fire for EVERY declared enum: the lowerer only sees enums that
        // survive AST dead-code elimination (`ast_dce`, which runs after
        // type-checking), so an unused-but-malformed enum would slip
        // through with no diagnostic. Keep the messages byte-for-byte in
        // sync with `register_enum` so either layer reports identically.
        let is_str_repr = matches!(e.repr_ty, Some(Type::Str));
        if is_str_repr && e.flags {
            return Err(TypeError::Unsupported {
                what: "@flags is not allowed on `: string`-repr enums (bitwise ops are int-only)"
                    .into(),
                span: e.span,
            });
        }
        for v in &e.variants {
            match (&v.discriminant, is_str_repr) {
                (Some(DiscriminantLit::Int(_)), true) => {
                    return Err(TypeError::Unsupported {
                        what: "integer discriminant used on a `: string` repr enum".into(),
                        span: v.span,
                    });
                }
                (Some(DiscriminantLit::Str(_)), false) => {
                    return Err(TypeError::Unsupported {
                        what: "string discriminant used on a non-string-repr enum".into(),
                        span: v.span,
                    });
                }
                (None, true) => {
                    return Err(TypeError::Unsupported {
                        what: "enum with `: string` repr requires an explicit `= \"…\"` \
                               discriminant on every variant"
                            .into(),
                        span: v.span,
                    });
                }
                _ => {}
            }
        }
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
                        // Fixed-length heap-element arrays are
                        // allowed as payload cells: the ctor stores
                        // a value copy and the release cascade
                        // decodes the packed payload tag.
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

    /// Verify every "no safe default" field of `c` is assigned in
    /// each of its `init` overloads. Fields that have a usable
    /// runtime default (`0` / `false` / `none` / `""` / `[]` /
    /// empty `Map` / dead `weak`) skip the check; everything else
    /// (Object / Fn / Tuple — types whose zero bytes are unsafe
    /// to read) needs an explicit `this.field = ...` along the
    /// path.
    pub(super) fn check_init_field_coverage(&self, c: &ClassDecl) -> Result<(), TypeError> {
        let required: Vec<&FieldDecl> = c
            .fields
            .iter()
            .filter(|f| !type_has_safe_default(&f.ty))
            .collect();
        if required.is_empty() {
            return Ok(());
        }
        let inits: Vec<&FnDecl> =
            c.methods.iter().filter(|m| m.name == "init").collect();
        if inits.is_empty() {
            // No init at all — readable shorthand for the user-
            // facing error: point at the first uncovered field.
            let f = required[0];
            return Err(TypeError::Unsupported {
                what: format!(
                    "class {:?} field {:?} of type {} has no safe default — \
                     declare an `init` that assigns `this.{}` (or use `{}?`)",
                    c.name, f.name, f.ty, f.name, f.ty
                ),
                span: f.span,
            });
        }
        for init in &inits {
            let mut assigned: HashSet<Symbol> = HashSet::new();
            collect_this_field_assignments(&init.body, &mut assigned);
            for f in &required {
                if !assigned.contains(&f.name) {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "class {:?} init: field `{}` of type {} has no \
                             safe default and is not assigned by this init \
                             (set `this.{} = ...` or wrap the field as `{}?`)",
                            c.name, f.name, f.ty, f.name, f.ty
                        ),
                        span: init.span,
                    });
                }
            }
        }
        Ok(())
    }

    /// Reject access to a non-`pub` class member when the access
    /// site lives in a different module from the class's
    /// declaration. Same-module access is unrestricted (matches
    /// the top-level pub rule). The error is structured as
    /// `Unsupported` so the existing diagnostic plumbing handles
    /// it.
    pub(super) fn require_visible(
        &self,
        class_name: &str,
        class_module: &str,
        member_kind: &str,
        member_name: &str,
        is_pub: bool,
        span: Span,
    ) -> Result<(), TypeError> {
        if self.skip_visibility {
            return Ok(());
        }
        if is_pub {
            return Ok(());
        }
        let cur = self.current_module.borrow();
        if cur.as_str() == class_module {
            return Ok(());
        }
        let class_disp = class_name;
        let cur_disp = if cur.is_empty() { "<entry>" } else { cur.as_str() };
        let mod_disp = if class_module.is_empty() { "<entry>" } else { class_module };
        Err(TypeError::Unsupported {
            what: format!(
                "{member_kind} `{class_disp}.{member_name}` is module-private (defined in module `{mod_disp}`) — not reachable from module `{cur_disp}`. Mark it `pub` to expose it"
            ),
            span,
        })
    }

    pub(super) fn check_class(&self, c: &ClassDecl) -> Result<(), TypeError> {
        // Reclassify the parsed `parent` slot if it actually names an
        // interface (the parser can't tell which is which). Combined
        // with the explicit `interfaces` list, validate that every
        // interface entry exists and that the class implements every
        // method the interface requires with a matching signature.
        let mut declared_ifaces: Vec<Symbol> = Vec::new();
        if let Some(p) = &c.parent {
            if self.interfaces.contains_key(p) {
                declared_ifaces.push(p.clone());
            }
        }
        for ifn in c.interfaces.iter() {
            declared_ifaces.push(ifn.clone());
        }
        for ifn in declared_ifaces.iter() {
            let Some(isig) = self.interfaces.get(ifn) else {
                return Err(TypeError::Unsupported {
                    what: format!("class {:?}: `{ifn}` is not a known interface", c.name),
                    span: c.span,
                });
            };
            let cls_methods = self
                .classes
                .get(&c.name)
                .map(|s| s.methods.clone())
                .unwrap_or_default();
            for im in isig.methods.iter() {
                let sigs = cls_methods.get(&im.name).cloned().unwrap_or_default();
                let mut matched = false;
                for s in sigs.iter() {
                    if s.params == im.params && s.ret == im.ret {
                        matched = true;
                        break;
                    }
                }
                if !matched && !im.is_optional {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "class {:?} does not implement {:?}.{:?} (expected fn(...) matching the interface signature)",
                            c.name, ifn, im.name
                        ),
                        span: c.span,
                    });
                }
            }
        }
        for FieldDecl { ty, span, .. } in &c.fields {
            self.validate_type(ty, *span, &c.type_params)?;
        }
        // `@extern(C) struct`es must have C-compatible field types so
        // the in-memory bytes line up with what native code expects.
        // Allowed: numeric primitives, bool, and other `@extern(C) struct`
        // classes (which embed inline). Reject ARC types, regular
        // classes (heap-managed), arrays, optional, etc.
        if !c.is_repr_c {
            for f in &c.fields {
                if f.bits.is_some() {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "@bits on field {:?} of class {:?}: bitfields are \
                             only supported inside `@extern(C) struct`es",
                            f.name, c.name
                        ),
                        span: f.span,
                    });
                }
            }
        }
        if c.is_repr_c {
            // `@extern(C) union` extra restrictions: every field
            // shares offset 0 so writing one overwrites the others.
            // Heap fields (string / object / array) would leak or
            // dangle when the storage is reused, so reject them.
            // FAM / bitfields don't make sense for unions.
            if c.is_union {
                if c.fields.is_empty() {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "@extern(C) union {:?}: union must have at \
                             least one field",
                            c.name
                        ),
                        span: c.span,
                    });
                }
                for f in &c.fields {
                    if f.bits.is_some() {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "@bits on union field {:?}: bitfields aren't \
                                 supported inside `@extern(C) union` classes",
                                f.name
                            ),
                            span: f.span,
                        });
                    }
                    let union_ok = matches!(
                        &f.ty,
                        Type::I8 | Type::I16 | Type::I32 | Type::I64
                        | Type::U8 | Type::U16 | Type::U32 | Type::U64
                        | Type::F32 | Type::F64
                        | Type::Bool
                    ) || matches!(&f.ty, Type::Array { elem, fixed: Some(_) }
                        if matches!(elem.as_ref(),
                            Type::I8 | Type::I16 | Type::I32 | Type::I64
                            | Type::U8 | Type::U16 | Type::U32 | Type::U64
                            | Type::F32 | Type::F64 | Type::Bool));
                    if !union_ok {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "@extern(C) union {:?} field {:?}: type {} \
                                 not supported (allowed inside a union: numeric \
                                 primitives / bool / fixed-length numeric array \
                                 `T[N]`. Heap types and nested aggregates aren't \
                                 safe under shared storage)",
                                c.name, f.name, f.ty
                            ),
                            span: f.span,
                        });
                    }
                }
            }
            for (i, f) in c.fields.iter().enumerate() {
                let is_last = i + 1 == c.fields.len();
                let primitive_ok = |t: &Type| {
                    matches!(
                        t,
                        Type::I8 | Type::I16 | Type::I32 | Type::I64
                        | Type::U8 | Type::U16 | Type::U32 | Type::U64
                        | Type::F32 | Type::F64
                        | Type::Bool
                    )
                };
                let ok = match &f.ty {
                    t if primitive_ok(t) => true,
                    Type::Object(name) => self
                        .classes
                        .get(name)
                        .map(|cs| cs.is_repr_c)
                        .unwrap_or(false),
                    // Fixed-length numeric arrays — `u8[64]`,
                    // `i32[4]` etc — are laid out inline (no
                    // heap allocation, no ARC).
                    Type::Array { elem, fixed: Some(_) } if primitive_ok(elem) => true,
                    // C99 flexible array member: `T[]` (no length) as
                    // the **last** field. Allocation size is set by
                    // `new ClassName(n)`. Bounds checks are skipped
                    // (the user maintains the count, just like in C).
                    Type::Array { elem, fixed: None } if is_last && primitive_ok(elem) => true,
                    // Owned C-string slot (`char *`) — class manages
                    // the malloc'd buffer on assign / drop.
                    Type::Str => true,
                    // Raw C pointer — `*T` / `*const T`. 8 bytes on
                    // 64-bit targets. Used for handle / opaque-struct
                    // / byte-buffer fields that mirror C structs.
                    Type::RawPtr { .. } => true,
                    // Bare C function pointer — `fn(T1, ...): R`.
                    // Stored as the raw 8-byte code address (no
                    // closure box / ARC). The StructLit lowering
                    // emits `func_addr` instead of `MakeClosure`
                    // when the destination field has this shape.
                    Type::Fn(_) => true,
                    _ => false,
                };
                if let Some(bits) = f.bits {
                    let max = match &f.ty {
                        Type::U8 => 8u32,
                        Type::U16 => 16,
                        Type::U32 => 32,
                        Type::U64 => 64,
                        _ => {
                            return Err(TypeError::Unsupported {
                                what: format!(
                                    "@bits on field {:?} of class {:?}: bitfields are \
                                     only supported on unsigned integer types \
                                     (u8/u16/u32/u64), got {}",
                                    f.name, c.name, f.ty
                                ),
                                span: f.span,
                            });
                        }
                    };
                    if bits == 0 || bits > max {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "@bits({}) on field {:?} of class {:?}: width must be \
                                 in 1..={} for {}",
                                bits, f.name, c.name, max, f.ty
                            ),
                            span: f.span,
                        });
                    }
                }
                if !ok {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "@extern(C) struct {:?} field {:?}: type {} not supported \
                             (allowed: numeric primitives / bool / str (owned C-string) / \
                             other @extern(C) struct / fixed-length primitive array `T[N]`)",
                            c.name, f.name, f.ty
                        ),
                        span: f.span,
                    });
                }
            }
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
            // `pub deinit` makes no sense — the destructor is invoked by
            // the ARC runtime when the rc reaches zero, never by user
            // code. Marking it `pub` would imply external callability
            // that doesn't exist; reject at the type-check layer.
            if m.name == "deinit" && m.is_pub {
                return Err(TypeError::Unsupported {
                    what: "`deinit` cannot be marked `pub` — it is invoked by the ARC runtime, never by external callers".into(),
                    span: m.span,
                });
            }
            self.check_fn(m, Some(c.name))?;
        }
        // Init coverage: every field whose type has no safe runtime
        // default (Object / Fn / Tuple — anything not auto-zeroable
        // to a usable value) must be assigned by the class's own
        // `init`. Inherited fields are the parent's responsibility
        // and are reached via `super(...)`. Skipped for `@extern(C)`
        // / opaque-extern classes which don't go through ilang init.
        if c.extern_lib.is_none() && !c.is_repr_c {
            self.check_init_field_coverage(c)?;
        }
        for prop in &c.properties {
            self.validate_type(&prop.ty, prop.span, &c.type_params)?;
            if let Some(g) = &prop.getter {
                self.check_fn(g, Some(c.name))?;
            }
            if let Some(s) = &prop.setter {
                self.check_fn(s, Some(c.name))?;
            }
        }
        // Static methods don't have `this` — pass `in_class=None` so
        // their bodies fail to resolve `this` / implicit field refs.
        for m in &c.static_methods {
            self.check_fn(m, None)?;
        }
        // Static field initializers were already folded to literals
        // by the loader. Just verify each one's type matches.
        let env: Vars = HashMap::new();
        for sf in &c.static_fields {
            self.validate_type(&sf.ty, sf.span, &c.type_params)?;
            let vt = self.check_expr(&sf.value, &env, None, None, 0)?;
            if !self.value_assignable(&sf.value, &vt, &sf.ty) {
                return Err(TypeError::Mismatch {
                    expected: sf.ty.clone(),
                    got: vt,
                    span: sf.value.span,
                });
            }
        }
        Ok(())
    }

    pub(super) fn check_fn(&self, f: &FnDecl, in_class: Option<Symbol>) -> Result<(), TypeError> {
        // `@intrinsic("...")` fns are runtime-provided — they carry
        // an empty body and bind to a `$<runtime>` symbol. The body
        // has nothing to check, but the signature still needs to
        // pass the "no C-only types at top level" rule (forces
        // `@intrinsic` on ptr / size_t signatures to live inside an
        // `@extern(C) { ... }` block instead).
        if f.intrinsic_name.is_some() {
            for p in f.params.iter() {
                if let Some(bad) = super::sigs::first_c_only_type(&p.ty) {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "`@intrinsic` fn `{}` parameter has C-only type `{}` — wrap the declaration in an `@extern(C) {{ ... }}` block",
                            f.name, bad,
                        ),
                        span: p.span,
                    });
                }
            }
            if let Some(ret) = &f.ret {
                if let Some(bad) = super::sigs::first_c_only_type(ret) {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "`@intrinsic` fn `{}` return type `{}` is C-only — wrap the declaration in an `@extern(C) {{ ... }}` block",
                            f.name, bad,
                        ),
                        span: f.span,
                    });
                }
            }
            return Ok(());
        }
        if f.is_async {
            return Err(TypeError::Unsupported {
                what: format!(
                    "`async fn {}` body has a shape the current \
                     state-machine lowering can't handle (multi-state \
                     poll-fn synthesis is the next phase)",
                    f.name.as_str()
                ),
                span: f.span,
            });
        }
        // Closure wrappers lifted out of a class method body get
        // their lexical class restored here so that `super.method(...)`
        // inside the wrapper still resolves against the original
        // enclosing class.
        let in_class = in_class.or_else(|| self.closure_wrapper_class.get(&f.name).copied());
        // Type parameters in scope: the class's (if we're inside a
        // generic class) plus the fn's own `<T, U>`.
        let mut params_in_scope: Vec<Symbol> = in_class
            .and_then(|n| self.classes.get(&n))
            .map(|c| c.type_params.clone())
            .unwrap_or_default();
        params_in_scope.extend(f.type_params.iter().cloned());
        let class_params = params_in_scope;
        // Make these visible to body-level type annotations
        // (`let y: T[] = ...` references the fn's own `<T>`).
        // The guard restores on every exit path — important
        // because validate_type (called below) returns errors via
        // `?`, and the next fn check shouldn't see stale params.
        struct TpsGuard<'a> {
            slot: &'a std::cell::RefCell<Vec<Symbol>>,
            saved: Vec<Symbol>,
        }
        impl<'a> Drop for TpsGuard<'a> {
            fn drop(&mut self) {
                *self.slot.borrow_mut() = std::mem::take(&mut self.saved);
            }
        }
        let saved_tps = std::mem::replace(
            &mut *self.current_type_params.borrow_mut(),
            class_params.clone(),
        );
        // Reset the per-fn const-name set on entry, restore on
        // exit. Bindings inside this fn don't leak out and
        // outer-scope consts shouldn't leak in.
        struct ConstGuard<'a> {
            slot: &'a std::cell::RefCell<HashSet<Symbol>>,
            saved: HashSet<Symbol>,
        }
        impl<'a> Drop for ConstGuard<'a> {
            fn drop(&mut self) {
                *self.slot.borrow_mut() = std::mem::take(&mut self.saved);
            }
        }
        let saved_consts = std::mem::take(&mut *self.const_names.borrow_mut());
        let _const_guard = ConstGuard {
            slot: &self.const_names,
            saved: saved_consts,
        };
        let _tps_guard = TpsGuard {
            slot: &self.current_type_params,
            saved: saved_tps,
        };
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
        // Seed the body env with module-level globals (the built-in
        // `console` singleton plus any `static` declarations from
        // `@extern(C) {}` blocks). Top-level `let` bindings are NOT
        // here yet at fn-body-check time — they get checked after
        // all fn bodies, matching most module systems.
        let mut env: Vars = self.vars.clone();
        // Closure wrapper: the body's "free" vars actually resolve
        // to captured values. Pre-populate the env with their
        // declared types so the body type-checks. Used by the
        // JIT's post-hoist re-typecheck.
        if let Some(captures) = self.closure_wrapper_captures.get(&f.name) {
            for (n, t) in captures {
                env.insert(n.clone(), t.clone());
            }
        }
        for Param { name, ty, .. } in &f.params {
            // Rewrite Object(T) → TypeVar(T) so the body checker treats
            // references to T as the type variable (not an unknown class).
            env.insert(name.clone(), rewrite_type_params(ty, &class_params));
        }
        let expected = rewrite_type_params(
            &f.ret.clone().unwrap_or(Type::Unit),
            &class_params,
        );
        // Function bodies start a fresh loop-stack: a `break` inside a
        // closure / nested fn body never refers to an outer loop.
        let saved_loops = std::mem::take(&mut *self.loop_stack.borrow_mut());
        let body_res = self.check_block(&f.body, &env, Some(&expected), in_class, 0);
        *self.loop_stack.borrow_mut() = saved_loops;
        let mut body_ty = body_res?;
        // A generic fn call in tail position whose type param is fixed
        // only by the return type (`fn f(): i64[] { makeArr() }`) infers
        // `Any` from its args alone — solve it from the declared return
        // type so the tail check passes and the stashed type-args become
        // concrete.
        if let Some(tail) = f.body.tail.as_deref() {
            if let Some(corrected) = self.refine_fn_call_type_args(tail, &expected) {
                body_ty = corrected;
            }
        }
        // Prefer the tail-expression check via `value_assignable`
        // when the body has a tail expression — that path catches
        // literal-int-doesn't-fit-target ergonomically (`fn f():
        // i8 { 200 }` rejects, `{ 100 }` accepts) and threads
        // class-subtype upcast through composite literals. The
        // plain `assignable` / `assignable_obj` covers
        // non-literal narrowings (e.g. `let x: i64 = ...; x` into
        // an `i8` return) and Object-vs-Object upcasts where
        // there's no tail expr to inspect.
        // Unit-return fns silently discard any trailing expression
        // value — same ergonomics as a top-level expression statement.
        // The MIR `finalise_return` already drops the tail when the
        // declared return is `Unit`, so the type checker just has to
        // stop complaining.
        let ok = if matches!(expected, Type::Unit) {
            true
        } else {
            let tail_check = f
                .body
                .tail
                .as_deref()
                .map(|t| self.value_assignable(t, &body_ty, &expected));
            match tail_check {
                Some(true) => true,
                Some(false) => false,
                None => {
                    assignable(&body_ty, &expected)
                        || self.assignable_obj(&body_ty, &expected)
                }
            }
        };
        if !ok {
            return Err(TypeError::BadReturn {
                name: f.name.clone(),
                expected,
                got: body_ty,
                span: f.span,
            });
        }
        // Refine enum-ctor entries in tail / Return sites against the
        // declared return type. See the equivalent in `check_stmt`'s
        // Let arm.
        self.refine_enum_ctor_args_in_block(&f.body, &expected);
        Ok(())
    }

}
