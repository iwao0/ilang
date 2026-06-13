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
            ExprKind::EnumCtor { enum_name, .. } => {
                if let Some((tbase, targs)) = &target_args {
                    if tbase == enum_name {
                        let mut tbl = self.enum_ctor_type_args.borrow_mut();
                        if let Some((_, recorded)) = tbl.get_mut(&expr.span) {
                            for (i, slot) in recorded.iter_mut().enumerate() {
                                if matches!(slot, Type::Any) {
                                    if let Some(t) = targs.get(i) {
                                        *slot = t.clone();
                                    }
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
        Err(TypeError::Mismatch {
            expected: a,
            got: b,
            span,
        })
    }

}

/// `{}` parses as an empty block (value `()`), but when a `Map<K, V>` is
/// the expected type it reads as an empty map — the JS-style shorthand
/// alongside `new Map<K, V>()`. A non-empty `{ k: v }` is a `MapLit` and
/// never reaches here. Restricted to a literal empty block so a block that
/// merely happens to evaluate to unit isn't silently turned into a map.
fn empty_block_as_map(value: &Expr, target: &Type) -> bool {
    matches!(target, Type::Generic(g) if g.base == "Map" && g.args.len() == 2)
        && matches!(
            &value.kind,
            ExprKind::Block(b) if b.stmts.is_empty() && b.tail.is_none()
        )
}
