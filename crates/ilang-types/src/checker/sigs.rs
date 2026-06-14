//! Extracted from `checker/mod.rs`.

#![allow(unused_imports)]

use std::collections::{HashMap, HashSet};

use ilang_ast::{
    Block, ClassDecl, CtorArgs, EnumDecl, Expr, ExprKind, FieldDecl, FnDecl, Item, Param,
    PatternBindings, PatternKind, Program, Span, Stmt, StmtKind, Symbol, Type, UnOp,
    VariantPayload,
};

use crate::error::TypeError;
use crate::ops::{assignable, bin_result, int_literal_fits};

use super::*;

/// Every FFI marshalling helper now lives in `libs/std/ffi.il` as an
/// `@intrinsic(...) @extern(C)` declaration; the "must be inside
/// @extern(C)" enforcement rides on the C-only-type rule plus the
/// top-level intrinsic check in `checker/decls.rs::check_fn`. This
/// list is kept (empty) so call sites that still reach for it (e.g.
/// out-of-block usage diagnostics) stay structurally consistent.
pub(super) const FFI_HELPERS: &[&str] = &[];

/// Return the first C-only type encountered in `t` (raw pointer,
/// `char`, `void`, `size_t`, `ssize_t`), recursing through composite
/// shapes. `None` if `t` is fully ilang-native.
pub(super) fn first_c_only_type(t: &Type) -> Option<&Type> {
    match t {
        Type::RawPtr { .. } | Type::CVoid | Type::CChar | Type::Size | Type::SSize => Some(t),
        Type::Array { elem, .. } => first_c_only_type(elem),
        Type::Optional(inner) | Type::Weak(inner) => first_c_only_type(inner),
        Type::Generic(g) => g.args.iter().find_map(first_c_only_type),
        Type::Fn(ft) => ft.params
            .iter()
            .find_map(first_c_only_type)
            .or_else(|| first_c_only_type(&ft.ret)),
        Type::Tuple(elems) => elems.iter().find_map(first_c_only_type),
        _ => None,
    }
}

/// Walk a parametric payload type alongside a concrete arg type and
/// record bindings for each `TypeVar` encountered. Used by the enum
/// constructor checker to infer type arguments from call args.
/// First-found binding wins for any given TypeVar.
pub(super) fn collect_type_var_bindings(
    payload: &Type,
    arg: &Type,
    bindings: &mut HashMap<Symbol, Type>,
) {
    match (payload, arg) {
        (Type::TypeVar(name), other) => {
            // Prefer a concrete binding over a previously-recorded `Any`.
            // An arg like `Result.err("e")` pins `E` but leaves `T = Any`;
            // a later arg that does fix `T` (e.g. `fallback: T` given an
            // `i64`) must win, or the fn instantiates as `<Any>` and the
            // monomorphizer chokes. A first concrete binding still stands
            // against a later one (no silent re-inference of conflicts).
            match bindings.get(name) {
                Some(t) if !matches!(t, Type::Any) => {}
                _ => {
                    bindings.insert(name.clone(), other.clone());
                }
            }
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
        (Type::Generic(pg), Type::Generic(ag)) => {
            for (p, a) in pg.args.iter().zip(ag.args.iter()) {
                collect_type_var_bindings(p, a, bindings);
            }
        }
        (Type::RawPtr { inner: pi, .. }, Type::RawPtr { inner: ai, .. }) => {
            collect_type_var_bindings(pi, ai, bindings);
        }
        (Type::Tuple(pe), Type::Tuple(ae)) => {
            for (p, a) in pe.iter().zip(ae.iter()) {
                collect_type_var_bindings(p, a, bindings);
            }
        }
        (Type::Fn(pf), Type::Fn(af)) => {
            for (p, a) in pf.params.iter().zip(af.params.iter()) {
                collect_type_var_bindings(p, a, bindings);
            }
            collect_type_var_bindings(&pf.ret, &af.ret, bindings);
        }
        _ => {}
    }
}

/// Map keys are constrained to types with stable structural equality.
/// Floats are excluded (NaN), as are arrays. Heap objects are allowed
/// when the class supplies `equals(other: Class): bool` and
/// `hashCode(): i64` — see `class_has_value_equality`. `classes` is
/// optional so callers without a full checker context (e.g. literal
/// type sniffing) still get the conservative primitive-only answer.
pub(super) fn is_valid_map_key_type(
    t: &Type,
    classes: Option<&std::collections::HashMap<Symbol, super::ClassSig>>,
    enums: Option<&std::collections::HashMap<Symbol, super::EnumSig>>,
) -> bool {
    match t {
        Type::Str | Type::Bool
        | Type::I8 | Type::I16 | Type::I32 | Type::I64
        | Type::U8 | Type::U16 | Type::U32 | Type::U64 => true,
        Type::Object(n) => {
            if let Some(em) = enums {
                if em.contains_key(n) {
                    return enum_is_value_keyable(*n, em);
                }
            }
            classes
                .map(|m| class_has_value_equality(*n, m))
                .unwrap_or(false)
        }
        _ => false,
    }
}

/// `Set<MyEnum>` / `Map<MyEnum, _>` are supported when every
/// variant is unit-payload (or the whole enum is `@flags`). Those
/// variants compile to a single i64 tag, so the existing Int store
/// + tag-equality semantics give consistent dedup. Payload-carrying
/// variants would need structural comparison through a hashCode /
/// equals protocol the language doesn't yet offer for enums.
pub(super) fn enum_is_value_keyable(
    enum_name: Symbol,
    enums: &std::collections::HashMap<Symbol, super::EnumSig>,
) -> bool {
    let Some(sig) = enums.get(&enum_name) else {
        return false;
    };
    if sig.flags {
        return true;
    }
    sig.variants
        .iter()
        .all(|v| matches!(v.payload, super::VariantPayloadSig::Unit))
}

/// `Set<T>` / `Map<T, _>` accept primitive `T`s by built-in hashing
/// rules (see `is_valid_set_element_type` / `is_valid_map_key_type`),
/// and `Object(Class)` when the user has supplied the matching
/// equality + hashing protocol on the class. The protocol is:
///
///   pub fn equals(other: Class): bool
///   pub fn hashCode(): i64
///
/// Both must be present (an `equals` without a hash, or vice versa,
/// is rejected so the runtime can rely on consistent dispatch).
/// `@derive(Eq, Hash)` synthesises matching methods in a loader pass
/// — by the time we get here the class already has them.
pub(super) fn class_has_value_equality(
    class_name: Symbol,
    classes: &std::collections::HashMap<Symbol, super::ClassSig>,
) -> bool {
    let Some(sig) = classes.get(&class_name) else {
        return false;
    };
    let has_equals = sig
        .methods
        .get(&Symbol::intern("equals"))
        .map(|sigs| {
            sigs.iter().any(|s| {
                s.params.len() == 1
                    && matches!(&s.params[0], Type::Object(c) if *c == class_name)
                    && matches!(s.ret, Type::Bool)
                    && s.type_params.is_empty()
            })
        })
        .unwrap_or(false);
    let has_hash = sig
        .methods
        .get(&Symbol::intern("hashCode"))
        .map(|sigs| {
            sigs.iter().any(|s| {
                s.params.is_empty()
                    && matches!(s.ret, Type::I64)
                    && s.type_params.is_empty()
            })
        })
        .unwrap_or(false);
    has_equals && has_hash
}

/// `Set<T>` accepts every type Map accepts as a key, plus floats
/// (the runtime hashes them by bit pattern; NaN ≠ NaN follows IEEE
/// semantics). Object element types are accepted when the class
/// satisfies the value-equality protocol — see
/// `class_has_value_equality`. Unit-variant (or `@flags`) enums
/// reuse the primitive i64 store.
pub(super) fn is_valid_set_element_type(
    t: &Type,
    classes: &std::collections::HashMap<Symbol, super::ClassSig>,
    enums: &std::collections::HashMap<Symbol, super::EnumSig>,
) -> bool {
    match t {
        Type::Str | Type::Bool
        | Type::I8 | Type::I16 | Type::I32 | Type::I64
        | Type::U8 | Type::U16 | Type::U32 | Type::U64
        | Type::F32 | Type::F64 => true,
        // Enums share `Type::Object(name)` representation with
        // classes — the loader resolves the name into either the
        // `enums` or `classes` map. Try the enum side first so a
        // unit-variant enum picks the i64-tag store; fall through
        // to the class protocol otherwise.
        Type::Object(n) => {
            if enums.contains_key(n) {
                enum_is_value_keyable(*n, enums)
            } else {
                class_has_value_equality(*n, classes)
            }
        }
        _ => false,
    }
}

pub(super) fn is_reserved_class(name: &str) -> bool {
    matches!(name, "Console" | "Map" | "Promise" | "Result" | "Type" | "TypeKind" | "ObjCBlock")
}

pub(super) fn is_reserved_global(name: &str) -> bool {
    matches!(name, "console" | "typeof")
}

/// Score how well an actual arg type fits a parameter type. `None`
/// means the conversion isn't allowed at all; lower numbers mean a
/// closer match. Used to rank overloads when multiple are viable.
pub(super) fn score_arg<F>(
    arg: &Expr,
    arg_ty: &Type,
    param_ty: &Type,
    is_sub: &F,
) -> Option<u32>
where
    // `is_sub(child, ancestor)` returns the inheritance distance
    // (0 for same class, n for n steps up the parent chain) when
    // the relation holds; `None` when unrelated. Lets the
    // overload-resolver weight closer parents above further ones.
    F: Fn(Symbol, Symbol) -> Option<u32>,
{
    if arg_ty == param_ty {
        return Some(0);
    }
    // `Type::Any` (e.g. inside `console.log(x)` — used elsewhere)
    // matches every concrete type with cost 1 so concrete overloads win.
    if matches!(arg_ty, Type::Any) || matches!(param_ty, Type::Any) {
        return Some(1);
    }
    // Same-sign integer widening / narrowing — implicit per syntax.md §2.
    if arg_ty.is_int() && param_ty.is_int() {
        let same_sign = arg_ty.is_signed_int() == param_ty.is_signed_int();
        if same_sign {
            return Some(1);
        }
        // Differing signs need an explicit `as` cast — not viable here.
        return None;
    }
    // Int → float (also widening between f32 / f64) — implicit.
    if arg_ty.is_int() && param_ty.is_float() {
        return Some(2);
    }
    if matches!((arg_ty, param_ty), (Type::F32, Type::F64) | (Type::F64, Type::F32)) {
        return Some(1);
    }
    // Class-pair conversions (Object → Object subtype upcast,
    // Object → Weak same-class / subclass). Shared rule table
    // lives in `checker::coercion` so the same costs apply at
    // assignment sites (`literal_assignable_with`).
    if let Type::Object(c) = arg_ty {
        if let Some(score) =
            super::coercion::class_pair_coercion(*c, param_ty, is_sub)
        {
            return Some(score);
        }
    }
    // T → T? auto-wrap.
    if let Type::Optional(inner) = param_ty {
        if let Some(inner_score) = score_arg(arg, arg_ty, inner, is_sub) {
            return Some(inner_score + 3);
        }
    }
    // Fall back to literal_assignable: catches int-literal widening
    // into smaller widths (`1` into `i8`) and similar.
    if literal_assignable(arg, arg_ty, param_ty) {
        return Some(2);
    }
    None
}

/// Pick the best matching overload from `sigs`. Returns the index of
/// the chosen signature, or a TypeError if none is viable / multiple
/// tie for best score.
pub(super) fn resolve_overload<F>(
    name: Symbol,
    sigs: &[Signature],
    arg_tys: &[Type],
    args: &[Expr],
    span: Span,
    is_sub: &F,
) -> Result<usize, TypeError>
where
    F: Fn(Symbol, Symbol) -> Option<u32>,
{
    // Variadic built-ins live in this slot too — accept the first that
    // matches arity (which for variadics means "any arg count").
    let mut viable: Vec<(usize, u32)> = Vec::new();
    for (i, sig) in sigs.iter().enumerate() {
        if sig.variadic {
            // Variadic: any arity, no per-arg scoring needed.
            viable.push((i, 0));
            continue;
        }
        if sig.params.len() < arg_tys.len() {
            continue;
        }
        // Default-arg fill: a sig with more params than args is
        // viable iff every unfilled trailing slot has a default.
        // Each filled-by-default slot adds a flat penalty so an
        // exact-arity overload always beats a default-filled one.
        let missing = sig.params.len() - arg_tys.len();
        if missing > 0 {
            let have_defaults = sig
                .defaults
                .iter()
                .skip(arg_tys.len())
                .take(missing)
                .all(|d| d.is_some());
            if !have_defaults {
                continue;
            }
        }
        let mut total = 0u32;
        let mut all_ok = true;
        for ((expected, actual), arg) in sig.params.iter().zip(arg_tys.iter()).zip(args.iter()) {
            match score_arg(arg, actual, expected, is_sub) {
                Some(s) => total += s,
                None => {
                    all_ok = false;
                    break;
                }
            }
        }
        if all_ok {
            // Penalty: each defaulted slot costs 1000, dwarfing any
            // implicit-conversion delta so an exact-arity match wins
            // first.
            total += (missing as u32) * 1000;
            viable.push((i, total));
        }
    }
    if viable.is_empty() {
        return Err(TypeError::Unsupported {
            what: format!(
                "no matching overload for `{name}` with arg types ({})",
                arg_tys.iter().map(|t| format!("{t}")).collect::<Vec<_>>().join(", "),
            ),
            span,
        });
    }
    // Pick lowest score; tie → ambiguous.
    viable.sort_by_key(|(_, s)| *s);
    let best = viable[0].1;
    let tied: Vec<usize> = viable.iter().take_while(|(_, s)| *s == best).map(|(i, _)| *i).collect();
    if tied.len() > 1 {
        return Err(TypeError::Unsupported {
            what: format!(
                "ambiguous call to `{name}` — multiple overloads match equally well \
                 ({} candidates)",
                tied.len()
            ),
            span,
        });
    }
    Ok(tied[0])
}

pub(super) fn signature_of(f: &FnDecl) -> Signature {
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
    // `@extern("...", variadic)` propagates to the signature so the
    // type checker accepts trailing args of any type at call sites.
    let is_variadic = f.attrs.iter().any(|a| {
        a.name == "extern"
            && a.args
                .iter()
                .any(|x| matches!(x, ilang_ast::AttrArg::Path(p) if p.iter().map(|s| s.as_str()).collect::<Vec<_>>() == ["variadic"]))
    });
    // `@deprecated` / `@deprecated("reason")` flows into the
    // Signature so call sites can surface a warning. No-arg form
    // becomes `Some("")`.
    let deprecated = f.attrs.iter().find_map(|a| {
        if a.name.as_str() != "deprecated" {
            return None;
        }
        let reason = a.args.iter().find_map(|x| match x {
            ilang_ast::AttrArg::Str(s) => Some(s.clone()),
            _ => None,
        });
        Some(reason.unwrap_or_default())
    });
    Signature {
        params,
        ret,
        variadic: is_variadic,
        decl_span: f.span,
        type_params: Vec::from(f.type_params.clone()),
        defaults: f.params.iter().map(|p| p.default.clone()).collect(),
        is_pub: f.is_pub,
        deprecated,
        lib_names: Vec::new(),
    }
}

pub(super) fn class_signature(
    c: &ClassDecl,
    parent: Option<&ClassSig>,
    is_subclass: &dyn Fn(Symbol, Symbol) -> bool,
) -> Result<ClassSig, TypeError> {
    // The parser puts the first `:` base into `c.parent`. If our caller
    // passed `parent: None` because that base is actually an interface,
    // pull the interface name out of `c.parent` and join it with the
    // explicit `c.interfaces` list. The resulting parent slot for the
    // ClassSig is `None` in that case.
    let parent_is_interface = c.parent.is_some() && parent.is_none();
    let effective_parent: Option<Symbol> = if parent_is_interface {
        None
    } else {
        c.parent.clone()
    };
    let extra_iface_from_parent: Option<Symbol> = if parent_is_interface {
        c.parent.clone()
    } else {
        None
    };
    // Inheritance restrictions: the parent must not be generic
    // (we don't substitute type params across the boundary), and
    // the child can't add type params either if it inherits.
    if let Some(p) = parent {
        if !p.type_params.is_empty() || !c.type_params.is_empty() {
            return Err(TypeError::Unsupported {
                what: format!(
                    "class {:?}: inheritance with generic classes is not supported",
                    c.name
                ),
                span: c.span,
            });
        }
    }
    // Start from the parent's tables and overlay this class's
    // declarations. Fields and methods are inherited; same-named
    // child decl overrides (must be explicitly marked `override`
    // for methods).
    let mut fields: HashMap<Symbol, Type> = parent
        .map(|p| p.fields.clone())
        .unwrap_or_default();
    let mut field_pub: HashMap<Symbol, bool> = parent
        .map(|p| p.field_pub.clone())
        .unwrap_or_default();
    for f in &c.fields {
        if fields.contains_key(&f.name) {
            return Err(TypeError::Unsupported {
                what: format!(
                    "class {:?}: field {:?} shadows an inherited field of the same name",
                    c.name, f.name
                ),
                span: f.span,
            });
        }
        fields.insert(f.name.clone(), rewrite_type_params(&f.ty, &c.type_params));
        field_pub.insert(f.name.clone(), f.is_pub);
    }
    let mut methods: HashMap<Symbol, Vec<Signature>> = parent
        .map(|p| p.methods.clone())
        .unwrap_or_default();
    let mut method_slots: HashMap<Symbol, usize> = parent
        .map(|p| p.method_slots.clone())
        .unwrap_or_default();
    let mut vtable_len: usize = parent.map(|p| p.vtable_len).unwrap_or(0);
    let has_parent = parent.is_some();
    // Track which init/deinit names this child has declared this
    // pass — needed because `methods` starts with parent entries
    // already populated, so a "first child decl overwrites parent"
    // is legitimate but a second one is a duplicate.
    let mut child_special_seen: HashSet<Symbol> = HashSet::new();
    // Pass 1: handle inheritance interactions (override / hiding / no-overload).
    for m in &c.methods {
        // `init` and `deinit` are per-class — they're NOT inherited
        // in the override sense. Pass 1 just overwrites whatever the
        // parent had (without requiring `override`); pass 2 skips
        // them since `has_parent` is true.
        if m.name == "init" || m.name == "deinit" {
            if has_parent {
                // Inheritance disallows overloading, including for
                // init/deinit. The root-class dup check below only
                // runs when there's no parent, so catch duplicates
                // here.
                if !child_special_seen.insert(m.name.clone()) {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "class {:?} declares `{}` more than once",
                            c.name, m.name
                        ),
                        span: m.span,
                    });
                }
                let mut sig = signature_of(m);
                for p in sig.params.iter_mut() {
                    *p = rewrite_type_params(p, &c.type_params);
                }
                sig.ret = rewrite_type_params(&sig.ret, &c.type_params);
                methods.insert(m.name.clone(), vec![sig]);
            }
            continue;
        }
        let inherited = parent
            .map(|p| p.methods.contains_key(&m.name))
            .unwrap_or(false);
        if m.is_override && !inherited {
            return Err(TypeError::Unsupported {
                what: format!(
                    "method {:?} in class {:?} is `override` but no parent \
                     declares a method by that name",
                    m.name, c.name
                ),
                span: m.span,
            });
        }
        // Generic methods don't compose with virtual dispatch — each
        // specialization is its own concrete function, so a vtable slot
        // would have to pick one specialization per receiver type, which
        // makes no sense. Reject the combination here with a clear
        // message; users who need polymorphism via vtable should use a
        // generic class instead.
        if m.is_override && !m.type_params.is_empty() {
            return Err(TypeError::Unsupported {
                what: format!(
                    "method {:?} in class {:?} cannot be both `override` and \
                     generic — generic methods are monomorphized per call \
                     site, which is incompatible with virtual dispatch",
                    m.name, c.name
                ),
                span: m.span,
            });
        }
        if inherited && !m.is_override {
            return Err(TypeError::Unsupported {
                what: format!(
                    "method {:?} in class {:?} hides a parent method without \
                     the `override` keyword",
                    m.name, c.name
                ),
                span: m.span,
            });
        }
        if inherited {
            // Override: replace parent's entry, reuse parent's slot.
            let parent_sigs = parent.unwrap().methods.get(&m.name).unwrap();
            if parent_sigs.len() != 1 {
                return Err(TypeError::Unsupported {
                    what: format!(
                        "method {:?} in parent of class {:?} is overloaded; \
                         cannot be overridden",
                        m.name, c.name
                    ),
                    span: m.span,
                });
            }
            let parent_sig = &parent_sigs[0];
            let mut sig = signature_of(m);
            for p in sig.params.iter_mut() {
                *p = rewrite_type_params(p, &c.type_params);
            }
            sig.ret = rewrite_type_params(&sig.ret, &c.type_params);
            // Param types must match exactly (contravariant params
            // would relax this, but we keep params invariant — same
            // as Rust's trait-method rule). Return type may be a
            // class subtype of the parent's, matching standard OO
            // covariant-return semantics: callers expecting the
            // parent's return type still get a subtype.
            let ret_compatible = sig.ret == parent_sig.ret
                || match (&sig.ret, &parent_sig.ret) {
                    (Type::Object(child), Type::Object(par)) => {
                        is_subclass(*child, *par)
                    }
                    _ => false,
                };
            if sig.params != parent_sig.params || !ret_compatible {
                return Err(TypeError::Unsupported {
                    what: format!(
                        "override of method {:?} in class {:?} has a different \
                         signature than the parent's declaration",
                        m.name, c.name
                    ),
                    span: m.span,
                });
            }
            methods.insert(m.name.clone(), vec![sig]);
            continue;
        }
        // Not inherited. With a parent, allow overloading among
        // child-declared overloads (the parent has no conflicting
        // signature by construction — `inherited` was false above).
        // The same dup / generic / generic-class checks Pass 2
        // applies to root classes run here, just without the
        // parent-aware vtable slot bookkeeping; slot allocation
        // mirrors Pass 2 (first sig gets a slot, subsequent
        // overloads share it — they can't be overridden anyway).
        if has_parent {
            let mut sig = signature_of(m);
            for p in sig.params.iter_mut() {
                *p = rewrite_type_params(p, &c.type_params);
            }
            sig.ret = rewrite_type_params(&sig.ret, &c.type_params);
            let entry = methods.entry(m.name.clone()).or_default();
            if entry.iter().any(|s| s.params == sig.params) {
                return Err(TypeError::Unsupported {
                    what: format!(
                        "method {:?} in class {:?} has a duplicate overload \
                         (same parameter types as a previous declaration)",
                        m.name, c.name
                    ),
                    span: m.span,
                });
            }
            let any_generic = !sig.type_params.is_empty()
                || entry.iter().any(|s| !s.type_params.is_empty());
            if any_generic && !entry.is_empty() {
                return Err(TypeError::Unsupported {
                    what: format!(
                        "method {:?} in class {:?} mixes a generic declaration with another \
                         overload — generic methods cannot share a name with other methods",
                        m.name, c.name
                    ),
                    span: m.span,
                });
            }
            if !c.type_params.is_empty() && !entry.is_empty() {
                return Err(TypeError::Unsupported {
                    what: format!(
                        "method {:?} in generic class {:?} cannot be overloaded \
                         (generic classes do not support method overloading)",
                        m.name, c.name
                    ),
                    span: m.span,
                });
            }
            entry.push(sig);
            if m.name != "init"
                && m.name != "deinit"
                && !method_slots.contains_key(&m.name)
            {
                method_slots.insert(m.name.clone(), vtable_len);
                vtable_len += 1;
            }
        }
    }
    // Pass 2: legacy overload-aware loop for root classes only.
    for m in &c.methods {
        if has_parent {
            continue;
        }
        let mut sig = signature_of(m);
        for p in sig.params.iter_mut() {
            *p = rewrite_type_params(p, &c.type_params);
        }
        sig.ret = rewrite_type_params(&sig.ret, &c.type_params);
        let entry = methods.entry(m.name.clone()).or_default();
        // `deinit` can't be overloaded — it's always called by the
        // runtime with no args. Reject any second decl.
        if m.name == "deinit" && !entry.is_empty() {
            return Err(TypeError::Unsupported {
                what: format!("class {:?} declares `deinit` more than once", c.name),
                span: m.span,
            });
        }
        // Generic + non-generic same name: forbidden (same rule as
        // top-level fns).
        let any_generic = !sig.type_params.is_empty()
            || entry.iter().any(|s| !s.type_params.is_empty());
        if any_generic && !entry.is_empty() {
            return Err(TypeError::Unsupported {
                what: format!(
                    "method {:?} in class {:?} mixes a generic declaration with another \
                     overload — generic methods cannot share a name with other methods",
                    m.name, c.name
                ),
                span: m.span,
            });
        }
        if entry.iter().any(|s| s.params == sig.params) {
            return Err(TypeError::Unsupported {
                what: format!(
                    "method {:?} in class {:?} has a duplicate overload (same parameter \
                     types as a previous declaration)",
                    m.name, c.name
                ),
                span: m.span,
            });
        }
        // Generic class + method overload: forbidden. Mono and overload
        // resolution paths are kept separate to avoid having to score
        // overloads after type-param substitution per instantiation.
        if !c.type_params.is_empty() && !entry.is_empty() {
            return Err(TypeError::Unsupported {
                what: format!(
                    "method {:?} in generic class {:?} cannot be overloaded \
                     (generic classes do not support method overloading)",
                    m.name, c.name
                ),
                span: m.span,
            });
        }
        entry.push(sig);
        // Slot for the first sig of each method name. Overloaded
        // methods reuse the same slot — but they can't be overridden
        // anyway (forbidden in inheriting classes), so the slot is
        // effectively unused for them. `init` / `deinit` skip slots.
        if m.name != "init"
            && m.name != "deinit"
            && !method_slots.contains_key(&m.name)
        {
            method_slots.insert(m.name.clone(), vtable_len);
            vtable_len += 1;
        }
    }
    // Inherit the parent's properties so a subclass naturally
    // sees `node.position` on every SKNode descendant once
    // SKNode itself declares `position` as a `pub get / pub set`
    // property. Child-declared accessors with the same name
    // overwrite the parent entry (no `override` requirement —
    // ObjC `@property` overrides don't carry one either).
    let mut properties: HashMap<Symbol, PropertySig> = parent
        .map(|p| p.properties.clone())
        .unwrap_or_default();
    for prop in &c.properties {
        // Reject name collisions with fields and methods.
        if fields.contains_key(&prop.name) {
            return Err(TypeError::Unsupported {
                what: format!(
                    "property {:?} in class {:?} collides with a field of the same name",
                    prop.name, c.name
                ),
                span: prop.span,
            });
        }
        if methods.contains_key(&prop.name) {
            return Err(TypeError::Unsupported {
                what: format!(
                    "property {:?} in class {:?} collides with a method of the same name",
                    prop.name, c.name
                ),
                span: prop.span,
            });
        }
        let prop_ty = rewrite_type_params(&prop.ty, &c.type_params);
        // Validate getter / setter signatures match the property type.
        if let Some(g) = &prop.getter {
            let ret = g
                .ret
                .as_ref()
                .map(|t| rewrite_type_params(t, &c.type_params))
                .unwrap_or(Type::Unit);
            if ret != prop_ty {
                return Err(TypeError::Mismatch {
                    expected: prop_ty.clone(),
                    got: ret,
                    span: g.span,
                });
            }
        }
        if let Some(s) = &prop.setter {
            let param = rewrite_type_params(&s.params[0].ty, &c.type_params);
            if param != prop_ty {
                return Err(TypeError::Mismatch {
                    expected: prop_ty.clone(),
                    got: param,
                    span: s.span,
                });
            }
        }
        // Merge the accessor presence with any inherited entry: a
        // subclass that overrides only the getter (or only the setter)
        // keeps the parent's accessor for the other direction, instead
        // of turning the property read-only / write-only.
        let inherited = properties.get(&prop.name);
        let has_get = prop.getter.is_some() || inherited.is_some_and(|p| p.has_get);
        let has_set = prop.setter.is_some() || inherited.is_some_and(|p| p.has_set);
        properties.insert(
            prop.name.clone(),
            PropertySig {
                ty: prop_ty,
                has_get,
                has_set,
                is_pub: prop.is_pub,
                is_static: prop.is_static,
            },
        );
    }
    let mut static_methods: HashMap<Symbol, Vec<Signature>> = HashMap::new();
    if !c.type_params.is_empty()
        && (!c.static_methods.is_empty() || !c.static_fields.is_empty())
    {
        return Err(TypeError::Unsupported {
            what: format!(
                "class {:?}: static members on generic classes are not supported",
                c.name
            ),
            span: c.span,
        });
    }
    for m in &c.static_methods {
        // No name collisions with instance fields / methods / properties.
        if fields.contains_key(&m.name)
            || methods.contains_key(&m.name)
            || properties.contains_key(&m.name)
        {
            return Err(TypeError::Unsupported {
                what: format!(
                    "static method {:?} in class {:?} collides with an instance \
                     field / method / property of the same name",
                    m.name, c.name
                ),
                span: m.span,
            });
        }
        let mut sig = signature_of(m);
        for p in sig.params.iter_mut() {
            *p = rewrite_type_params(p, &c.type_params);
        }
        sig.ret = rewrite_type_params(&sig.ret, &c.type_params);
        static_methods.entry(m.name.clone()).or_default().push(sig);
    }
    // Inherit the parent's static fields so a subclass reaches them by
    // its own name (`Derived.count` == `Base.count` — there is one
    // shared slot). Mirrors how static methods / static properties
    // already inherit; without it `Derived.count` was rejected as an
    // "undefined variable".
    let mut static_fields: HashMap<Symbol, Type> =
        parent.map(|p| p.static_fields.clone()).unwrap_or_default();
    let mut static_field_pub: HashMap<Symbol, bool> =
        parent.map(|p| p.static_field_pub.clone()).unwrap_or_default();
    let mut static_const_fields: HashSet<Symbol> =
        parent.map(|p| p.static_const_fields.clone()).unwrap_or_default();
    for sf in &c.static_fields {
        if static_fields.contains_key(&sf.name)
            || fields.contains_key(&sf.name)
            || methods.contains_key(&sf.name)
            || properties.contains_key(&sf.name)
            || static_methods.contains_key(&sf.name)
        {
            return Err(TypeError::Unsupported {
                what: format!(
                    "static field {:?} in class {:?} collides with a field / \
                     method / property / static method of the same name",
                    sf.name, c.name
                ),
                span: sf.span,
            });
        }
        // Allowed static-field types: numeric primitives (any width),
        // bool, and string — all single-word slot values. A dynamic
        // array was previously accepted here, but the `LoadStatic` /
        // `StoreStatic` codegen only handles single-word values, so
        // reading or reassigning such a field hit "static slot type" at
        // codegen (and an init-then-use path SIGSEGV'd). Reject it at the
        // checker until the slot machinery grows real heap-array support,
        // so the user gets a clean diagnostic instead of a late crash.
        let prim_ok = matches!(
            sf.ty,
            Type::I8 | Type::I16 | Type::I32 | Type::I64
            | Type::U8 | Type::U16 | Type::U32 | Type::U64
            | Type::F32 | Type::F64 | Type::Bool | Type::Str
        );
        if !prim_ok {
            return Err(TypeError::Unsupported {
                what: format!(
                    "static field {:?} in class {:?}: type {} not yet \
                     supported (allowed: numeric primitives, bool, string)",
                    sf.name, c.name, sf.ty
                ),
                span: sf.span,
            });
        }
        static_fields.insert(sf.name.clone(), sf.ty.clone());
        static_field_pub.insert(sf.name.clone(), sf.is_pub);
        if sf.is_const {
            static_const_fields.insert(sf.name.clone());
        }
    }
    let module = module_of_name(c.name.as_str()).to_string();
    let mut implements: Vec<Symbol> = c.interfaces.iter().cloned().collect();
    if let Some(p) = extra_iface_from_parent {
        implements.insert(0, p);
    }
    Ok(ClassSig {
        type_params: Vec::from(c.type_params.clone()),
        fields,
        field_pub,
        methods,
        properties,
        static_methods,
        static_fields,
        static_field_pub,
        static_const_fields,
        implements,
        parent: effective_parent,
        method_slots,
        vtable_len,
        extern_lib: c.extern_lib.clone(),
        is_repr_c: c.is_repr_c,
        is_handle: c.is_handle,
        is_union: c.is_union,
        has_fam: c.is_repr_c
            && c.fields.last().map_or(false, |f| matches!(
                &f.ty, Type::Array { fixed: None, .. }
            )),
        module,
    })
}

/// The parser produces `Type::Object(name)` for any user-defined type
/// reference. Inside a generic class body, references that match the
/// Walk every node of a `Type` tree, giving `f` a chance to replace
/// each subtree. `f` returns `Some(replacement)` to take over (the
/// children of `t` are NOT visited in that case), or `None` to let
/// the walker rebuild structurally by recursing into the composite
/// carriers (`Array`, `Optional`, `Weak`, `Generic`, `Tuple`, `Fn`,
/// `RawPtr`) and clone every other leaf as-is.
fn map_type<F>(t: &Type, f: &mut F) -> Type
where
    F: FnMut(&Type) -> Option<Type>,
{
    if let Some(replaced) = f(t) {
        return replaced;
    }
    match t {
        Type::Array { elem, fixed } => Type::Array {
            elem: Box::new(map_type(elem, f)),
            fixed: *fixed,
        },
        Type::Optional(inner) => Type::Optional(Box::new(map_type(inner, f))),
        Type::Weak(inner) => Type::Weak(Box::new(map_type(inner, f))),
        Type::Generic(g) => Type::generic(
            g.base.clone(),
            g.args.iter().map(|a| map_type(a, f)).collect(),
        ),
        Type::Tuple(elems) => Type::Tuple(elems.iter().map(|e| map_type(e, f)).collect()),
        Type::Fn(ft) => Type::func(
            ft.params.iter().map(|p| map_type(p, f)).collect(),
            map_type(&ft.ret, f),
        ),
        Type::RawPtr { is_const, inner } => Type::RawPtr {
            is_const: *is_const,
            inner: Box::new(map_type(inner, f)),
        },
        _ => t.clone(),
    }
}

/// class's type-parameter names are actually type variables — convert
/// them to `Type::TypeVar` so the checker can substitute later.
pub(super) fn rewrite_type_params(t: &Type, params: &[Symbol]) -> Type {
    map_type(t, &mut |ty| match ty {
        Type::Object(name) if params.iter().any(|p| p == name) => {
            Some(Type::TypeVar(name.clone()))
        }
        _ => None,
    })
}

/// Substitute concrete types for type variables. Used when a generic
/// class is instantiated: each `TypeVar(P)` is replaced with the i-th
/// concrete arg from the matching position in `params`. A TypeVar
/// whose name is not in `params` keeps its original form (map_type's
/// leaf clone).
pub(super) fn subst_type(t: &Type, params: &[Symbol], args: &[Type]) -> Type {
    map_type(t, &mut |ty| match ty {
        Type::TypeVar(name) => params
            .iter()
            .position(|p| p == name)
            .and_then(|i| args.get(i).cloned()),
        _ => None,
    })
}

/// When two arms each produced a `Type::Generic` with the same base
/// but different concrete args (commonly with `Any` placeholders left
/// over from constructor-type inference, e.g. `Result<i64, Any>` on
/// one side and `Result<Any, string>` on the other), merge them by
/// taking the non-`Any` side at each position. Returns `None` if the
/// bases differ, the arities differ, or any position has two
/// incompatible non-`Any` types.
pub(super) fn merge_generic_with_holes(a: &Type, b: &Type) -> Option<Type> {
    let (Type::Generic(ga), Type::Generic(gb)) = (a, b) else {
        return None;
    };
    if ga.base != gb.base || ga.args.len() != gb.args.len() {
        return None;
    }
    let mut merged = Vec::with_capacity(ga.args.len());
    for (x, y) in ga.args.iter().zip(gb.args.iter()) {
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
    Some(Type::generic(ga.base.clone(), merged))
}

pub(super) fn enum_signature(e: &EnumDecl) -> EnumSig {
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
        type_params: Vec::from(e.type_params.clone()),
        variants,
        flags: e.flags,
        repr: e.repr_ty.clone(),
    }
}

/// True iff a runtime can synthesize a usable blank value for `t`
/// without user-supplied initializer. Numerics / `bool` zero-init,
/// `T?` is `none`, `string` is `""`, `T[]` is `[]`, `Map<K, V>` is
/// empty, `T.weak` starts dead, fixed-length arrays default per
/// element. `Object` / `Fn` / `Tuple` / `T[N]`-of-Objects have no
/// safe shape and need explicit init.
pub(super) fn type_has_safe_default(t: &Type) -> bool {
    use Type as T;
    match t {
        T::I8 | T::I16 | T::I32 | T::I64
        | T::U8 | T::U16 | T::U32 | T::U64
        | T::F32 | T::F64
        | T::Bool
        | T::Str
        | T::Unit
        | T::Optional(_)
        | T::Weak(_)
        | T::Size | T::SSize
        | T::CChar | T::CVoid
        | T::RawPtr { .. } => true,
        // Dynamic `T[]` always defaults to an empty array — the
        // element type only matters at access time, not at
        // construction.
        T::Array { fixed: None, .. } => true,
        // Fixed-length `T[N]` zero-fills element-by-element, so
        // every element must itself have a safe default.
        T::Array { elem, fixed: Some(_) } => type_has_safe_default(elem),
        // Object / Generic class / Map / Fn / Tuple / Enum: no
        // safe blank in v1. Wrap in `T?` to opt in to `none`.
        _ => false,
    }
}

pub(super) fn is_result_type(t: &Type) -> bool {
    // Matches both the pre-monomorphization names (`Result` /
    // `Result<T, E>`) and the post-monomorphization mangled object
    // names like `Result<i64, string>` that the JIT emits.
    let name = match t {
        Type::Object(name) => *name,
        Type::Generic(g) => g.base,
        _ => return false,
    };
    let s = name.as_str();
    s == "Result" || s.starts_with("Result<")
}

pub(super) fn expect_object(t: &Type, span: Span) -> Result<Symbol, TypeError> {
    match t {
        Type::Object(name) => Ok(*name),
        Type::Generic(g) => Ok(g.base),
        _ => Err(TypeError::Mismatch {
            expected: Type::Object("<object>".into()),
            got: t.clone(),
            span,
        }),
    }
}

/// Extract the concrete type arguments from an object-typed value, if
/// any. Non-generic objects return an empty slice.
pub(super) fn type_args_of(t: &Type) -> &[Type] {
    if let Type::Generic(g) = t {
        &g.args
    } else {
        &[]
    }
}

/// Helper for `bin_result`-style spanless errors (the ops module returns
/// `BadBinary`/`BadUnary` without knowing the source position; we attach
/// the surrounding expression's span here).
pub(super) fn attach_span(e: TypeError, span: Span) -> TypeError {
    match e {
        TypeError::BadBinary { lhs, rhs, .. } => TypeError::BadBinary { lhs, rhs, span },
        TypeError::BadUnary { ty, .. } => TypeError::BadUnary { ty, span },
        TypeError::MixedSignedness { lhs, rhs, .. } => {
            TypeError::MixedSignedness { lhs, rhs, span }
        }
        other => other,
    }
}

