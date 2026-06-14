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

/// Boilerplate `args.len() != expected → ArityMismatch` check.
/// Folds the 6-line `if ... { return Err(TypeError::ArityMismatch { ... }) }`
/// pattern that the call-site checks in `checker/expr/calls.rs` repeat
/// 30+ times into a single `check_arity(...)?;` call.
pub(super) fn check_arity(
    actual: usize,
    expected: usize,
    name: Symbol,
    span: Span,
) -> Result<(), TypeError> {
    if actual != expected {
        return Err(TypeError::ArityMismatch {
            name,
            expected,
            got: actual,
            span,
        });
    }
    Ok(())
}

impl TypeChecker {
    /// Cheap literal-only type guess for `let X = expr` cases that
    /// omit the type annotation. Returns `None` for anything that
    /// isn't a primitive literal or a unary on one — letting the
    /// regular type-check path produce a normal error later.
    pub(super) fn infer_literal_type(&self, e: &ilang_ast::Expr) -> Option<Type> {
        use ilang_ast::ExprKind;
        match &e.kind {
            ExprKind::Int(_) => Some(Type::I64),
            ExprKind::Float(_) => Some(Type::F64),
            ExprKind::Bool(_) => Some(Type::Bool),
            ExprKind::Str(_) => Some(Type::Str),
            ExprKind::Unary { expr, .. } => self.infer_literal_type(expr),
            _ => None,
        }
    }

    /// True iff `child` is `parent` or transitively descends from
    /// `parent` via `extends` chains. False if either name is
    /// unknown.
    pub(super) fn is_subclass(&self, child: Symbol, parent: Symbol) -> bool {
        if child == parent {
            return true;
        }
        let mut cur = self.classes.get(&child).and_then(|c| c.parent);
        while let Some(name) = cur {
            if name == parent {
                return true;
            }
            cur = self.classes.get(&name).and_then(|c| c.parent);
        }
        false
    }

    /// Like `is_subclass` but returns the inheritance distance
    /// (0 for the same class, 1 for direct parent, etc.) when
    /// the relation holds. Used by the overload resolver so
    /// `f(B)` outranks `f(A)` when called with a `C: B: A`.
    pub(super) fn subclass_distance(&self, child: Symbol, parent: Symbol) -> Option<u32> {
        if child == parent {
            return Some(0);
        }
        let mut cur = self.classes.get(&child).and_then(|c| c.parent);
        let mut depth: u32 = 1;
        while let Some(name) = cur {
            if name == parent {
                return Some(depth);
            }
            cur = self.classes.get(&name).and_then(|c| c.parent);
            depth += 1;
        }
        None
    }

    /// The nearest common ancestor of two classes — the type used
    /// when joining branches (`if`/`else`, `match` arms) where each
    /// arm produces a different subclass. Returns `None` when there
    /// is no shared ancestor (independent class hierarchies). When
    /// one is a subclass of the other, the result is the parent.
    pub(super) fn common_ancestor(&self, a: Symbol, b: Symbol) -> Option<Symbol> {
        if a == b {
            return Some(a);
        }
        // Walk a's ancestor chain, collect into a set, then walk b's
        // until we find a member.
        let mut a_chain: Vec<Symbol> = vec![a];
        let mut cur = self.classes.get(&a).and_then(|c| c.parent);
        while let Some(name) = cur {
            a_chain.push(name);
            cur = self.classes.get(&name).and_then(|c| c.parent);
        }
        if a_chain.contains(&b) {
            return Some(b);
        }
        let mut cur = self.classes.get(&b).and_then(|c| c.parent);
        while let Some(name) = cur {
            if a_chain.contains(&name) {
                return Some(name);
            }
            cur = self.classes.get(&name).and_then(|c| c.parent);
        }
        None
    }

    /// Join two object types for an `if` / `match` branch result. First
    /// tries a common class ancestor (`common_ancestor`); if the two
    /// classes share no class ancestor, falls back to a common
    /// INTERFACE that both implement. So `if c { new Circle() } else {
    /// new Square() }` — where `Circle` and `Square` both implement
    /// `Shape` but share no parent class — joins to `Shape`, mirroring
    /// how subclasses of a common parent already join.
    ///
    /// A unique common interface is required: when the two classes share
    /// more than one interface the join is ambiguous (no expected type
    /// is available at this point to pick), so this returns `None` and
    /// the caller reports a mismatch — annotate or restructure.
    pub(super) fn common_object_join(&self, a: Symbol, b: Symbol) -> Option<Symbol> {
        if let Some(anc) = self.common_ancestor(a, b) {
            return Some(anc);
        }
        let mut shared: Vec<Symbol> = self
            .interfaces
            .keys()
            .copied()
            .filter(|i| self.class_implements(a, *i) && self.class_implements(b, *i))
            .collect();
        match shared.len() {
            1 => shared.pop(),
            _ => None,
        }
    }

    /// Object-aware extension of `assignable`: returns true if the
    /// plain assignable check passes OR `from` is an object whose
    /// class is a (transitive) subclass of `to`'s class, OR `to`
    /// is an interface that `from`'s class implements.
    pub(super) fn assignable_obj(&self, from: &Type, to: &Type) -> bool {
        if assignable(from, to) {
            return true;
        }
        if let (Type::Object(c), Type::Object(p)) = (from, to) {
            if self.is_subclass(*c, *p) {
                return true;
            }
            // Interface upcast: any class implementing `p` (or whose
            // ancestor implements it) satisfies `Type::Object(p)`.
            if self.interfaces.contains_key(p) && self.class_implements(*c, *p) {
                return true;
            }
        }
        false
    }

    /// Walk the parent chain of `class_name` looking for a declared
    /// implementation of `iface`.
    pub(super) fn class_implements(&self, class_name: Symbol, iface: Symbol) -> bool {
        let mut cur = Some(class_name);
        while let Some(name) = cur {
            let Some(cs) = self.classes.get(&name) else {
                return false;
            };
            if cs.implements.contains(&iface) {
                return true;
            }
            cur = cs.parent;
        }
        false
    }

    /// `literal_assignable` with class-subtype awareness threaded
    /// through the recursive composite (Array / Tuple / Optional)
    /// cases. Used by call sites that previously paired the free
    /// `literal_assignable` with `assignable_obj` at the top level —
    /// this new helper additionally accepts e.g. `[new Child()]`
    /// flowing into a `Parent[]` slot, `(new Child(),)` into
    /// `(Parent,)`, and `some(new Child())` into `Parent?`.
    pub(super) fn value_assignable(&self, value: &Expr, vt: &Type, target: &Type) -> bool {
        // `subclass_distance` answers the class chain; interface
        // implementations report distance 0 so they fold into the
        // same path without re-shaping the call.
        let is_sub = |c: Symbol, p: Symbol| -> Option<u32> {
            self.subclass_distance(c, p).or_else(|| {
                if self.class_implements(c, p) {
                    Some(0)
                } else {
                    None
                }
            })
        };
        literal_assignable_with(value, vt, target, &is_sub)
            || self.enum_repr_assignable(vt, target)
            || self.handle_void_ptr_assignable(vt, target)
            || empty_block_as_map(value, target)
    }

    /// `pub enum E: T { ... }` flows into a slot typed `T`
    /// implicitly. The same value already flowed through an
    /// explicit `as T` cast at every call site (Win32 message
    /// matches, `pDesc.Type: i32 = D3D12CommandListType.direct
    /// as i32`, etc.); since the cast was a no-op beyond reading
    /// the tag, drop the boilerplate. Restricted to the declared
    /// repr type to avoid surprising widening.
    fn enum_repr_assignable(&self, vt: &Type, target: &Type) -> bool {
        let Type::Object(name) = vt else { return false };
        let Some(sig) = self.enums.get(name) else { return false };
        let Some(repr) = &sig.repr else { return false };
        repr == target
    }

    /// `@handle pub struct H {}` is C-style "pointer-sized opaque"
    /// — values flow freely between `H` and `*void` / `*const void`
    /// (in either direction), mirroring how a C function returning
    /// `HWND` can be stored in `void *` and back. Two distinct
    /// `@handle` types stay nominally distinct under the equal-name
    /// check elsewhere.
    fn handle_void_ptr_assignable(&self, a: &Type, b: &Type) -> bool {
        let is_void_ptr = |t: &Type| {
            matches!(t, Type::RawPtr { inner, .. } if matches!(**inner, Type::CVoid))
        };
        let is_handle_obj = |t: &Type| {
            matches!(
                t,
                Type::Object(name) if self.classes.get(name).map(|s| s.is_handle).unwrap_or(false)
            )
        };
        (is_handle_obj(a) && is_void_ptr(b)) || (is_void_ptr(a) && is_handle_obj(b))
    }

    /// When an EnumCtor's inferred type-args contain `Type::Any` (because
    /// only some of T/E were resolvable from the args alone), use the
    /// surrounding context's expected type to fill in the holes. This
    /// runs at let / return / tail positions so the JIT monomorphizer
    /// sees a fully concrete instantiation.
    pub(super) fn refine_enum_ctor_args(&self, expr: &Expr, target: &Type) {
        let target_args = match target {
            Type::Generic(g) => Some((g.base.clone(), g.args.to_vec())),
            _ => None,
        };
        match &expr.kind {
            ExprKind::EnumCtor { enum_name, variant, args } => {
                if let Some((tbase, targs)) = &target_args {
                    if tbase == enum_name {
                        // Refine the ctor's own recorded type args: a slot
                        // is replaced by the target's corresponding arg
                        // when it (a) mentions `Any` (bare or nested like
                        // `Maybe<Any>`), or (b) is an Object subtype of it
                        // (literal covariance — `Result.ok(new Dog())` into
                        // `Result<Animal,_>` records the enum as
                        // `Result<Animal,string>`, storing the Dog upcast).
                        let current: Option<Vec<Type>> = self
                            .enum_ctor_type_args
                            .borrow()
                            .get(&expr.span)
                            .map(|(_, r)| r.clone());
                        let refined_args: Option<Vec<Type>> = current.map(|mut recorded| {
                            for (i, slot) in recorded.iter_mut().enumerate() {
                                if let Some(t) = targs.get(i) {
                                    let any_fill =
                                        type_contains_any(slot) && !type_contains_any(t);
                                    if any_fill || self.is_covariant_upcast(slot, t) {
                                        *slot = t.clone();
                                    }
                                }
                            }
                            if let Some(entry) =
                                self.enum_ctor_type_args.borrow_mut().get_mut(&expr.span)
                            {
                                entry.1 = recorded.clone();
                            }
                            recorded
                        });
                        // Recurse into the payload args against the now-
                        // concrete payload types, so a nested ctor like
                        // `Result.ok(Maybe.nope)` gets its own `Maybe<T>`
                        // refined from the declared `Result<Maybe<Box>,_>`.
                        if let Some(refined_args) = refined_args {
                            if let Some((tparams, payload)) = self.enums.get(enum_name).and_then(|s| {
                                s.variants
                                    .iter()
                                    .find(|v| v.name == *variant)
                                    .map(|v| (s.type_params.clone(), v.payload.clone()))
                            }) {
                                match (&payload, args) {
                                    (VariantPayloadSig::Tuple(tys), CtorArgs::Tuple(elems)) => {
                                        for (e, t) in elems.iter().zip(tys.iter()) {
                                            let pt = subst_type(t, &tparams, &refined_args);
                                            self.refine_enum_ctor_args(e, &pt);
                                        }
                                    }
                                    (VariantPayloadSig::Struct(fields), CtorArgs::Struct(provided)) => {
                                        for (fname, fty) in fields {
                                            if let Some((_, e)) =
                                                provided.iter().find(|(n, _)| n == fname)
                                            {
                                                let pt = subst_type(fty, &tparams, &refined_args);
                                                self.refine_enum_ctor_args(e, &pt);
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
            ExprKind::If { then_branch, else_branch, .. } => {
                self.refine_enum_ctor_args_in_block(then_branch, target);
                if let Some(e) = else_branch {
                    self.refine_enum_ctor_args(e, target);
                }
            }
            ExprKind::IfLet { then_branch, else_branch, .. } => {
                self.refine_enum_ctor_args_in_block(then_branch, target);
                if let Some(e) = else_branch {
                    self.refine_enum_ctor_args(e, target);
                }
            }
            ExprKind::Block(b) => self.refine_enum_ctor_args_in_block(b, target),
            ExprKind::Match { arms, .. } => {
                for arm in arms {
                    self.refine_enum_ctor_args(&arm.body, target);
                }
            }
            ExprKind::Return(Some(inner)) => self.refine_enum_ctor_args(inner, target),
            // Recurse into the composite-literal shapes so an enum ctor
            // nested in `some(..)` / a tuple / an array literal is refined
            // from the corresponding slot of the declared type, e.g.
            // `some(Result.err("e"))` against `Result<i64,string>?` or
            // `(Result.err("e"), 5)` against `(Result<i64,string>, i64)`.
            ExprKind::Some(inner) => {
                if let Type::Optional(it) = target {
                    self.refine_enum_ctor_args(inner, it);
                }
            }
            ExprKind::Tuple(elems) => {
                if let Type::Tuple(tys) = target {
                    for (e, t) in elems.iter().zip(tys.iter()) {
                        self.refine_enum_ctor_args(e, t);
                    }
                }
            }
            ExprKind::Array(elems) => {
                if let Type::Array { elem, .. } = target {
                    for e in elems.iter() {
                        self.refine_enum_ctor_args(e, elem);
                    }
                }
            }
            _ => {}
        }
    }

    /// When a generic fn call couldn't pin all its type params from the
    /// arguments — a param that appears only in the return type, e.g.
    /// `fn makeArr<T>(): T[]` or `fn wrapErr<T>(): Result<T, string>` —
    /// solve the leftover params by unifying the declared return type
    /// against the expected type from context (a `let` annotation, a
    /// return position, or a call argument's param type). Updates the
    /// stashed `fn_call_type_args` and returns the corrected (fully
    /// substituted) return type, or `None` if nothing was newly solved.
    pub(super) fn refine_fn_call_type_args(&self, expr: &Expr, target: &Type) -> Option<Type> {
        let ExprKind::Call { callee, .. } = &expr.kind else {
            return None;
        };
        let sigs = self.fns.get(callee)?;
        if sigs.len() != 1 || sigs[0].type_params.is_empty() {
            return None;
        }
        let sig = &sigs[0];
        let mut tbl = self.fn_call_type_args.borrow_mut();
        let (cn, cur_args) = tbl.get(&expr.span)?;
        if cn != callee || cur_args.len() != sig.type_params.len() {
            return None;
        }
        // Only act when a param is still unresolved (`Any`).
        if !cur_args.iter().any(|t| matches!(t, Type::Any)) {
            return None;
        }
        let cur_args = cur_args.clone();
        // Seed with what the args already pinned, then solve the rest by
        // unifying the declared return type against the expected type.
        let mut bindings: HashMap<Symbol, Type> = HashMap::new();
        for (p, a) in sig.type_params.iter().zip(cur_args.iter()) {
            if !matches!(a, Type::Any) {
                bindings.insert(p.clone(), a.clone());
            }
        }
        collect_type_var_bindings(&sig.ret, target, &mut bindings);
        let new_args: Vec<Type> = sig
            .type_params
            .iter()
            .map(|p| bindings.get(p).cloned().unwrap_or(Type::Any))
            .collect();
        if new_args == cur_args {
            return None;
        }
        let ret = subst_type(&sig.ret, &sig.type_params, &new_args);
        let type_params = sig.type_params.clone();
        let params = sig.params.clone();
        tbl.insert(expr.span, (callee.clone(), new_args.clone()));
        drop(tbl);
        // An inline enum-ctor argument shares the fn's type params: a call
        // `f(Result.err("e"))` against `fn f<T>(r: Result<T,string>)`
        // stashed the arg's `Result.err` as `[Any, string]` (checked while
        // the param was still `Result<Any,string>`). Now that `T` is
        // solved from context, re-refine each arg against the concrete
        // param type so the arg's own type args are filled too.
        if let ExprKind::Call { args, .. } = &expr.kind {
            for (param, arg) in params.iter().zip(args.iter()) {
                let concrete = subst_type(param, &type_params, &new_args);
                self.refine_enum_ctor_args(arg, &concrete);
            }
        }
        Some(ret)
    }

    /// Is `from` an Object subtype (subclass or interface impl) of `to`?
    /// Used to upcast a covariant enum-ctor type-arg slot to the declared
    /// parent type (`Dog` -> `Animal`).
    fn is_covariant_upcast(&self, from: &Type, to: &Type) -> bool {
        match (from, to) {
            (Type::Object(c), Type::Object(p)) if c != p => {
                self.subclass_distance(*c, *p).is_some() || self.class_implements(*c, *p)
            }
            _ => false,
        }
    }

    pub(super) fn refine_enum_ctor_args_in_block(&self, b: &ilang_ast::Block, target: &Type) {
        // Tail produces the block's value — refine against target.
        if let Some(t) = &b.tail {
            self.refine_enum_ctor_args(t, target);
            // A tail expression can also EMBED `return` statements — a
            // `?` nested inside a call argument (`Result.ok(take(g()?))`)
            // desugars to a block whose err arm does `return
            // Result.err(e)`, buried below the tail's own value. Walk the
            // tail for those nested returns too (statements get the same
            // walk in the loop below).
            refine_returns(self, t, target);
        }
        // Return statements anywhere in the block also produce the
        // function's return value — refine those too. Walk every
        // statement shape that can carry an expression, not just
        // bare `StmtKind::Expr` — `let v = match ... { err(e) {
        // return Result.err(e) } }` puts the return inside a
        // `Let` value, and skipping it leaves the `Result.err`
        // call with `T = Any` (the monomorphizer then chokes on the
        // unresolved type argument).
        for s in &b.stmts {
            match &s.kind {
                StmtKind::Expr(e)
                | StmtKind::Let { value: e, .. }
                | StmtKind::LetTuple { value: e, .. }
                | StmtKind::LetStruct { value: e, .. } => {
                    refine_returns(self, e, target);
                }
            }
        }
    }

    /// Join two branch result types. Equality / generic-hole merge
    /// first, then `assignable` either way (numeric / nominal
    /// covariance), then class-subtype upcast (so two arms
    /// returning different subclasses of a common parent unify to
    /// the parent). The subclass step is Object↔Object only —
    /// numeric widening is intentionally NOT applied here so e.g.
    /// `i64`-arm and `f64`-arm still reject like in `if/else`.
    pub(super) fn unify_branch_obj(
        &self,
        a: Type,
        b: Type,
        allow_generic_join: bool,
        span: Span,
    ) -> Result<Type, TypeError> {
        if a == b {
            return Ok(a);
        }
        if let Some(merged) = merge_generic_with_holes(&a, &b) {
            return Ok(merged);
        }
        if assignable(&a, &b) {
            return Ok(b);
        }
        if assignable(&b, &a) {
            return Ok(a);
        }
        if let (Type::Object(ca), Type::Object(cb)) = (&a, &b) {
            if let Some(anc) = self.common_object_join(*ca, *cb) {
                return Ok(Type::Object(anc));
            }
        }
        // Same-base generic instantiations at different subclass args
        // (`Box<Dog>` ⊔ `Box<Cat>`) join to the common-ancestor
        // instantiation (`Box<Animal>`). Gated on `allow_generic_join`:
        // generic-enum covariance is LITERAL-only (a `Box<Dog>` ctor
        // literal covaries, an aliased `Box<Dog>` variable does not — see
        // `generic_enum_literal_covariant_alias_error.il`), so the caller
        // only allows it when every arm is a ctor literal.
        if allow_generic_join {
            if let Some(joined) = self.common_generic_join(&a, &b) {
                return Ok(joined);
            }
        }
        Err(TypeError::Mismatch {
            expected: a,
            got: b,
            span,
        })
    }

    /// Is `e` a generic-enum ctor LITERAL (or an `if`/`match` whose every
    /// arm tail is one)? Such a value covaries at its binding site, so two
    /// arms building the same generic enum at different subclasses can be
    /// joined to the common-ancestor instantiation. An aliased variable of
    /// the same type does NOT covary (it could be mutated / shared), so it
    /// must not enable the covariant join.
    pub(super) fn is_covariant_join_literal(&self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::EnumCtor { .. } => true,
            // A fresh array / map / tuple literal covaries in its element /
            // value type; an aliased container variable does not.
            // `some(lit)` covaries when its inner does (the Optional shell
            // is immutable).
            ExprKind::Array(_) | ExprKind::MapLit(_) | ExprKind::Tuple(_) => true,
            ExprKind::Some(inner) => self.is_covariant_join_literal(inner),
            ExprKind::Block(b) => b
                .tail
                .as_ref()
                .is_some_and(|t| self.is_covariant_join_literal(t)),
            ExprKind::If { then_branch, else_branch, .. } => {
                let Some(else_e) = else_branch else { return false };
                then_branch
                    .tail
                    .as_ref()
                    .is_some_and(|t| self.is_covariant_join_literal(t))
                    && self.is_covariant_join_literal(else_e)
            }
            ExprKind::Match { arms, .. } => {
                arms.iter().all(|a| self.is_covariant_join_literal(&a.body))
            }
            _ => false,
        }
    }

    /// Class / generic / composite covariant WIDENING — `Child` into
    /// `Parent`, `Box<Dog>` into `Box<Animal>`, the same under `?` / `[]`.
    /// Excludes numeric narrowing. Callers gate this on the value being a
    /// covariant LITERAL (`is_covariant_join_literal`) so an aliased
    /// generic value can't covary — it's the boundary analogue of the
    /// `common_generic_join` arm-merge, used when an if/match of ctor
    /// literals all produced the SAME subclass (`Box<Dog>`) and so was
    /// never widened by the join itself.
    pub(super) fn covariant_widening(&self, from: &Type, to: &Type) -> bool {
        if from == to {
            return true;
        }
        match (from, to) {
            (Type::Object(c), Type::Object(p)) => {
                self.is_subclass(*c, *p)
                    || (self.interfaces.contains_key(p) && self.class_implements(*c, *p))
            }
            (Type::Generic(ga), Type::Generic(gb)) => {
                ga.base == gb.base
                    && ga.args.len() == gb.args.len()
                    && ga
                        .args
                        .iter()
                        .zip(gb.args.iter())
                        .all(|(x, y)| self.covariant_widening(x, y))
            }
            (Type::Optional(a), Type::Optional(b)) => self.covariant_widening(a, b),
            (Type::Array { elem: a, fixed: fa }, Type::Array { elem: b, fixed: fb }) => {
                fa == fb && self.covariant_widening(a, b)
            }
            (Type::Tuple(ea), Type::Tuple(eb)) => {
                ea.len() == eb.len()
                    && ea.iter().zip(eb.iter()).all(|(a, b)| self.covariant_widening(a, b))
            }
            _ => false,
        }
    }

    /// Covariant join of two same-base generic instantiations: join each
    /// type argument to its common supertype (objects → nearest common
    /// ancestor, nested generics recurse, an `Any` hole yields to the
    /// concrete side). `Box<Dog>` ⊔ `Box<Cat>` = `Box<Animal>`. This
    /// mirrors `assignable`, which already accepts a generic whose args
    /// are pairwise-assignable (so `Box<Dog>` is assignable to
    /// `Box<Animal>`); the join just lets an `if`/`match` build that
    /// supertype bottom-up, so two arms constructing the same generic
    /// enum at different subclass instantiations unify the way a single
    /// covariant arm already does. Returns `None` when the bases differ
    /// or an arg pair has no common supertype.
    pub(super) fn common_generic_join(&self, a: &Type, b: &Type) -> Option<Type> {
        match (a, b) {
            (Type::Generic(ga), Type::Generic(gb))
                if ga.base == gb.base && ga.args.len() == gb.args.len() =>
            {
                let mut merged = Vec::with_capacity(ga.args.len());
                for (x, y) in ga.args.iter().zip(gb.args.iter()) {
                    merged.push(self.join_type_arg(x, y)?);
                }
                Some(Type::generic(ga.base.clone(), merged))
            }
            // `Dog[]` ⊔ `Cat[]` = `Animal[]` (same length kind). Sound for
            // the same reason as `Box<Dog>` ⊔ `Box<Cat>`: the caller gates
            // on both arms being fresh literals, so no aliased array can be
            // mutated through the widened element face.
            (
                Type::Array { elem: e1, fixed: f1 },
                Type::Array { elem: e2, fixed: f2 },
            ) if f1 == f2 => self
                .join_type_arg(e1, e2)
                .map(|e| Type::Array { elem: Box::new(e), fixed: *f1 }),
            // `Box<Dog>?` ⊔ `Box<Cat>?` = `Box<Animal>?` — the join through a
            // `some(..)`-wrapped covariant literal.
            (Type::Optional(i1), Type::Optional(i2)) => self
                .join_type_arg(i1, i2)
                .map(|i| Type::Optional(Box::new(i))),
            // `(Dog, i64)` ⊔ `(Cat, i64)` = `(Animal, i64)` — covary each
            // element of two same-arity tuple literals.
            (Type::Tuple(e1), Type::Tuple(e2)) if e1.len() == e2.len() => {
                let mut merged = Vec::with_capacity(e1.len());
                for (x, y) in e1.iter().zip(e2.iter()) {
                    merged.push(self.join_type_arg(x, y)?);
                }
                Some(Type::Tuple(merged.into()))
            }
            _ => None,
        }
    }

    fn join_type_arg(&self, x: &Type, y: &Type) -> Option<Type> {
        if x == y {
            return Some(x.clone());
        }
        if matches!(x, Type::Any) {
            return Some(y.clone());
        }
        if matches!(y, Type::Any) {
            return Some(x.clone());
        }
        if let (Type::Object(ca), Type::Object(cb)) = (x, y) {
            return self.common_object_join(*ca, *cb).map(Type::Object);
        }
        self.common_generic_join(x, y)
    }

}

/// `{}` parses as an empty block (value `()`), but when a `Map<K, V>` is
/// the expected type it reads as an empty map — the JS-style shorthand
/// alongside `new Map<K, V>()`. A non-empty `{ k: v }` is a `MapLit` and
/// never reaches here. Restricted to a literal empty block so a block that
/// merely happens to evaluate to unit isn't silently turned into a map.
/// Does `t` mention `Type::Any` anywhere (top-level or nested)? Used to
/// decide whether a recorded enum-ctor type arg like `Maybe<Any>` still
/// needs refining from the authoritative target type.
fn type_contains_any(t: &Type) -> bool {
    match t {
        Type::Any => true,
        Type::Generic(g) => g.args.iter().any(type_contains_any),
        Type::Array { elem, .. } => type_contains_any(elem),
        Type::Optional(i) | Type::Weak(i) => type_contains_any(i),
        Type::Tuple(es) => es.iter().any(type_contains_any),
        Type::RawPtr { inner, .. } => type_contains_any(inner),
        Type::Fn(f) => f.params.iter().any(type_contains_any) || type_contains_any(&f.ret),
        _ => false,
    }
}

fn empty_block_as_map(value: &Expr, target: &Type) -> bool {
    matches!(target, Type::Generic(g) if g.base == "Map" && g.args.len() == 2)
        && matches!(
            &value.kind,
            ExprKind::Block(b) if b.stmts.is_empty() && b.tail.is_none()
        )
}
