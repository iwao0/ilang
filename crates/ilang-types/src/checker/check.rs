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

impl TypeChecker {
    /// Run the full check pass. Returns the program's final type
    /// (the last stmt / tail expression, or `Unit`) alongside every
    /// `TypeError` collected during the walk. An empty `Vec` means
    /// the program type-checks cleanly. Errors do not short-circuit
    /// — each independent failure (per top-level item, per stmt, per
    /// call argument, per class member) is recorded so a single pass
    /// surfaces the full diagnostic list.
    pub fn check(&mut self, prog: &Program) -> (Type, Vec<TypeError>) {
        // Each `check` call starts with a clean error slate so the
        // REPL / chunked-CLI path (which reuses one `TypeChecker`
        // across multiple chunks) doesn't replay previous chunks'
        // errors.
        self.reset_errors();
        // Pass 0: refuse to redefine built-in names. Otherwise a user
        // `class Console { ... }` would silently overwrite the built-in
        // signature and `console.log` would call the user code.
        for item in &prog.items {
            match item {
                Item::Class(c) if is_reserved_class(c.name.as_str()) => {
                    self.record(TypeError::ReservedName {
                        name: c.name.clone(),
                        span: c.span,
                    });
                }
                Item::Enum(e) if is_reserved_class(e.name.as_str()) => {
                    self.record(TypeError::ReservedName {
                        name: e.name.clone(),
                        span: e.span,
                    });
                }
                _ => {}
            }
        }
        // Pre-pass: collect every interface signature so that the
        // class-collection pass below can look them up when reclassifying
        // `class C: Iface { ... }` (where the parser left the interface
        // name in the `parent` slot).
        for item in &prog.items {
            // Collect from both `Item::Interface` (top-level) and
            // `@objc interface` / `@com interface` declarations
            // nested inside `@extern(C)` / `@extern(ObjC)` blocks.
            // Type validation runs in a later pass, after classes
            // are registered — interface methods commonly mention
            // sibling struct / class names that the registration
            // order hasn't reached yet.
            let iface_list: Vec<&ilang_ast::InterfaceDecl> = match item {
                Item::Interface(i) => vec![i],
                Item::ExternC(b) => b.interfaces.iter().collect(),
                _ => continue,
            };
            for i in iface_list {
                // Reject a second declaration of the same interface
                // name. Without this, a later definition silently
                // overwrites the earlier `self.interfaces` entry, so
                // every method/parent resolution downstream becomes
                // declaration-order dependent. (Cross-module `pub use`
                // duplicates are caught separately by `dup_pub`; this
                // covers same-program duplicates including non-pub
                // declarations.)
                if self.interfaces.contains_key(&i.name) {
                    self.record(TypeError::Unsupported {
                        what: format!(
                            "interface {:?} is declared more than once",
                            i.name,
                        ),
                        span: i.span,
                    });
                    continue;
                }
                let mut methods = Vec::with_capacity(i.methods.len());
                let mut seen: HashSet<Symbol> = HashSet::new();
                for m in i.methods.iter() {
                    if !seen.insert(m.name.clone()) {
                        self.record(TypeError::Unsupported {
                            what: format!(
                                "interface {:?} declares method {:?} more than once",
                                i.name, m.name
                            ),
                            span: m.span,
                        });
                        continue;
                    }
                    methods.push(InterfaceMethodSig {
                        name: m.name.clone(),
                        params: m.params.iter().map(|p| p.ty.clone()).collect(),
                        ret: m.ret.clone().unwrap_or(Type::Unit),
                        is_optional: m.is_optional,
                    });
                }
                let module = module_of_name(i.name.as_str()).to_string();
                self.interfaces.insert(
                    i.name.clone(),
                    InterfaceSig {
                        methods,
                        is_pub: i.is_pub,
                        module,
                        is_com: i.is_com,
                        parent: i.parent.clone(),
                    },
                );
            }
        }
        // Validate every interface's parent chain. Each named parent
        // must resolve to a registered interface, and the chain must
        // terminate — otherwise the method-lookup walk in
        // `expr/calls.rs` and the MIR slot / signature walks would
        // spin forever on `interface A : B { } interface B : A { }`.
        // On any failure we record the error and null out the offending
        // interface's `parent` so the rest of the type-check pass
        // (which doesn't short-circuit on errors) can't itself loop.
        for item in &prog.items {
            let iface_list: Vec<&ilang_ast::InterfaceDecl> = match item {
                Item::Interface(i) => vec![i],
                Item::ExternC(b) => b.interfaces.iter().collect(),
                _ => continue,
            };
            for i in iface_list {
                if i.parent.is_none() {
                    continue;
                }
                let mut visited: HashSet<Symbol> = HashSet::new();
                visited.insert(i.name.clone());
                let mut cur = i.parent.clone();
                let mut err: Option<TypeError> = None;
                while let Some(p) = cur {
                    if !self.interfaces.contains_key(&p) {
                        err = Some(TypeError::Unsupported {
                            what: format!(
                                "interface {:?} extends unknown interface {:?}",
                                i.name, p,
                            ),
                            span: i.span,
                        });
                        break;
                    }
                    if !visited.insert(p.clone()) {
                        err = Some(TypeError::Unsupported {
                            what: format!(
                                "interface {:?} has an inheritance cycle through {:?}",
                                i.name, p,
                            ),
                            span: i.span,
                        });
                        break;
                    }
                    cur = self.interfaces.get(&p).and_then(|s| s.parent.clone());
                }
                if let Some(e) = err {
                    self.record(e);
                    if let Some(sig) = self.interfaces.get_mut(&i.name) {
                        sig.parent = None;
                    }
                }
            }
        }
        for item in &prog.items {
            match item {
                Item::Fn(f) => {
                    let sig = signature_of(f);
                    let entry = self.fns.entry(f.name.clone()).or_default();
                    // Reject (a) generic + non-generic same name and
                    // (b) two overloads with identical param types.
                    // (a) keeps overload resolution simple — generic
                    // resolution is already its own special path, so we
                    // require a name to be EITHER one generic fn OR
                    // a set of non-generic overloads.
                    let any_generic = !sig.type_params.is_empty()
                        || entry.iter().any(|s| !s.type_params.is_empty());
                    if any_generic && !entry.is_empty() {
                        self.record(TypeError::Unsupported {
                            what: format!(
                                "fn {:?} mixes a generic declaration with another overload — \
                                 generic functions cannot share a name with other fns",
                                f.name
                            ),
                            span: f.span,
                        });
                        continue;
                    }
                    if entry.iter().any(|s| s.params == sig.params) {
                        self.record(TypeError::Unsupported {
                            what: format!(
                                "fn {:?} has a duplicate overload (same parameter types as a \
                                 previous declaration)",
                                f.name
                            ),
                            span: f.span,
                        });
                        continue;
                    }
                    entry.push(sig);
                }
                Item::Class(c) => {
                    // Resolve parent (must be already registered).
                    // If the parser put an interface name in the
                    // `parent` slot — the parser can't distinguish
                    // class from interface — treat the class as
                    // having no parent for signature purposes; the
                    // class still implements the interface.
                    let parent_sig = if let Some(pname) = &c.parent {
                        if self.interfaces.contains_key(pname) {
                            None
                        } else {
                            match self.classes.get(&pname).cloned() {
                                Some(s) => Some(s),
                                None => {
                                    self.record(TypeError::UndefinedClass {
                                        name: pname.clone(),
                                        span: c.span,
                                    });
                                    // Skip this class entirely — without a
                                    // resolved parent we'd produce dozens of
                                    // bogus errors against missing methods /
                                    // fields. The single UndefinedClass is
                                    // the actionable diagnostic.
                                    continue;
                                }
                            }
                        }
                    } else {
                        None
                    };
                    let classes_ref = &self.classes;
                    // The closure is called from `class_signature`
                    // while the current class `c` is still being
                    // built — its sig isn't in `classes_ref` yet —
                    // so cover-overriding-method covariant-return
                    // checks like `class A extends S { override
                    // clone(): A { ... } }` need to start the parent
                    // walk from the in-progress decl directly.
                    let cur_name = c.name.clone();
                    let cur_parent = c.parent.clone();
                    let is_sub = move |child: Symbol, ancestor: Symbol| -> bool {
                        if child == ancestor {
                            return true;
                        }
                        let mut cur = if child == cur_name {
                            cur_parent.clone()
                        } else {
                            classes_ref.get(&child).and_then(|c| c.parent)
                        };
                        while let Some(name) = cur {
                            if name == ancestor {
                                return true;
                            }
                            cur = classes_ref.get(&name).and_then(|c| c.parent);
                        }
                        false
                    };
                    let sig = match class_signature(c, parent_sig.as_ref(), &is_sub) {
                        Ok(s) => s,
                        Err(e) => {
                            self.record(e);
                            // Register a partial sig so the body
                            // pass still sees the declared parent
                            // link — without this, every `super(...)`
                            // / `super.method(...)` in the failed
                            // class piles on a misleading "super used
                            // in class X, which has no parent" error
                            // on top of the real diagnostic.
                            let mut partial = ClassSig::default();
                            partial.parent = c.parent.clone();
                            partial.type_params = c.type_params.to_vec();
                            partial
                        }
                    };
                    // `is_sub`'s shared borrow of `self.classes` ends
                    // here via NLL — no explicit `drop` needed before
                    // the mutable `insert` below.
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
                Item::ExternC(block) => {
                    // Walk the block's items in extern_c context so
                    // raw pointer / C-only types are accepted.
                    *self.in_extern_c.borrow_mut() = true;
                    let result = self.collect_extern_c_signatures(block);
                    *self.in_extern_c.borrow_mut() = false;
                    self.record_if_err(result);
                }
                // `Item::Interface` is fully handled by the pre-pass
                // at the top of `check`: it registers the sig,
                // dedups methods, rejects duplicate interface names,
                // and validates the parent chain. Re-running the
                // registration here would overwrite the pre-pass's
                // null-out of a cycle-rejected `parent`, and would
                // also drop the duplicate-detection diagnostic since
                // the second `insert` is silent. The method-type
                // validation pass further down still walks every
                // interface decl directly.
                Item::Interface(_) => {}
            }
        }
        // (`@objc interface` declarations inside `@extern(ObjC)`
        // blocks were already picked up by the pre-pass at the
        // top of this function — they share `InterfaceSig` with
        // top-level interfaces.)
        // Now every struct / union (both `@extern(C)` and top-level)
        // is registered in `self.classes`, so we can validate the
        // top-level (`restrict_c_types: true`) ones — they must not
        // mention any C-only type, transitively.
        self.record_if_err(self.validate_restrict_c_structs(prog));

        // Validate interface method signatures now that every
        // sibling class / enum / struct name resolves. Extern-
        // nested interfaces (`@extern(C) { @com interface … }` and
        // `@extern(ObjC) { interface … }`) check under
        // `in_extern_c = true` so raw pointers / C-only types are
        // legal in their signatures; top-level interfaces use the
        // default `false`, so a plain `pub interface Bad { foo(p:
        // *const u8) }` is rejected at declaration time instead
        // of leaking the C-only type through the call site.
        for item in &prog.items {
            let (iface_list, scope_is_extern_c): (
                Vec<&ilang_ast::InterfaceDecl>,
                bool,
            ) = match item {
                Item::Interface(i) => (vec![i], false),
                Item::ExternC(b) => (b.interfaces.iter().collect(), true),
                _ => continue,
            };
            let prev = *self.in_extern_c.borrow();
            *self.in_extern_c.borrow_mut() = scope_is_extern_c;
            for i in iface_list {
                for m in i.methods.iter() {
                    for p in m.params.iter() {
                        self.record_if_err(self.validate_type(&p.ty, p.span, &[]));
                    }
                    if let Some(r) = &m.ret {
                        // Same return restriction as fn decls:
                        // fixed-length heap-element arrays can't be
                        // returned.
                        if let ilang_ast::Type::Array { elem, fixed: Some(_) } = r {
                            if self.fixed_elem_is_heap(elem) {
                                self.record_if_err(Err(TypeError::Unsupported {
                                    what: format!(
                                        "interface method return type {r} \
                                         (fixed-length arrays with heap elements \
                                         can't be returned)"
                                    ),
                                    span: m.span,
                                }));
                            }
                        }
                        self.record_if_err(self.validate_type(r, m.span, &[]));
                    }
                }
            }
            *self.in_extern_c.borrow_mut() = prev;
        }

        // Pre-register top-level `let X: T = expr` bindings as
        // module-level globals so fn bodies can read / write them.
        // The script-stmts loop further down still type-checks each
        // initializer expression — we just need the names visible
        // during fn-body checking. Without this pass, top-level
        // `let X: T = expr` was only reachable from sibling script-
        // stmts (and even then only in the entry file).
        for s in &prog.stmts {
            if let StmtKind::Let { name, ty, value, is_const, .. } = &s.kind {
                if is_reserved_global(name.as_str()) {
                    continue;
                }
                let bind_ty = if let Some(t) = ty {
                    Some(t.clone())
                } else if let Some(t) = self.infer_literal_type(value) {
                    Some(t)
                } else {
                    // Non-literal RHS (`let app = sharedApplication()` etc).
                    // Run a full check_expr against the partial environment
                    // collected so far so fns / methods / classes (signature
                    // tables are already populated by this point) can
                    // contribute a type. Errors are ignored — they'll
                    // resurface when the stmt body gets checked for real
                    // later, with full diagnostics. Without this, the
                    // top-level let stays invisible to free-fn / method
                    // bodies and they fail with "undefined variable".
                    // Speculative check: any error here is discarded
                    // (and excluded from the accumulator) — the same
                    // expression is re-checked for real in the stmt
                    // loop below where its errors get properly
                    // reported. Without the truncate, we'd
                    // double-report.
                    let env_snapshot: Vars = self.vars.clone();
                    let saved = self.errors.borrow().len();
                    let out = self.check_expr(value, &env_snapshot, None, None, 0).ok();
                    self.errors.borrow_mut().truncate(saved);
                    out
                };
                if let Some(t) = bind_ty {
                    self.vars.insert(name.clone(), t);
                }
                if *is_const {
                    self.top_level_consts.borrow_mut().insert(name.clone());
                }
            }
        }

        for item in &prog.items {
            // Each top-level item belongs to a module — derived from
            // the loader-prefixed name (`sdl.X` ⇒ `"sdl"`, plain
            // entry items ⇒ `""`). Set `current_module` so member
            // access checks know whose perspective they're judging.
            let saved_module = self.current_module.borrow().clone();
            let item_module = match item {
                Item::Fn(f) => module_of_name(f.name.as_str()).to_string(),
                Item::Class(c) => module_of_name(c.name.as_str()).to_string(),
                Item::Enum(e) => module_of_name(e.name.as_str()).to_string(),
                _ => saved_module.clone(),
            };
            *self.current_module.borrow_mut() = item_module;
            match item {
                Item::Fn(f) => self.record_if_err(self.check_fn(f, None)),
                Item::Class(c) => self.record_if_err(self.check_class(c)),
                Item::Enum(e) => self.record_if_err(self.check_enum(e)),
                Item::Use(_) | Item::Const(_) => {}
                Item::ExternC(block) => {
                    *self.in_extern_c.borrow_mut() = true;
                    let result = self.check_extern_c_bodies(block);
                    *self.in_extern_c.borrow_mut() = false;
                    self.record_if_err(result);
                }
                Item::Interface(_) => {}
            }
            *self.current_module.borrow_mut() = saved_module;
        }

        let mut env: Vars = self.vars.clone();
        let mut last = Type::Unit;
        for s in &prog.stmts {
            // Refuse to redefine built-in globals at top level so a
            // wayward `let console = ...` cannot disable `console.log`.
            // Inner-scope shadowing is still allowed.
            if let StmtKind::Let { name, .. } = &s.kind {
                if is_reserved_global(name.as_str()) {
                    self.record(TypeError::ReservedName {
                        name: name.clone(),
                        span: s.span,
                    });
                    continue;
                }
            }
            // Top-level stmts merged in from a sub-module carry
            // `source_module = Some(M)`. Set `current_module` to
            // M while checking so cross-module visibility judges
            // access from the module's perspective, not the
            // entry's. Restored after each stmt.
            let saved_module = self.current_module.borrow().clone();
            if let Some(m) = &s.source_module {
                *self.current_module.borrow_mut() = m.as_str().to_string();
            }
            last = self.or_record(self.check_stmt(s, &mut env, None, None, 0));
            *self.current_module.borrow_mut() = saved_module;
        }
        if let Some(t) = &prog.tail {
            last = self.or_record(self.check_expr(t, &env, None, None, 0));
        }
        self.vars = env;
        (last, self.errors.borrow().clone())
    }

}
