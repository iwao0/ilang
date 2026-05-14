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
    pub fn check(&mut self, prog: &Program) -> Result<Type, TypeError> {
        // Pass 0: refuse to redefine built-in names. Otherwise a user
        // `class Console { ... }` would silently overwrite the built-in
        // signature and `console.log` would call the user code.
        for item in &prog.items {
            match item {
                Item::Class(c) if is_reserved_class(c.name.as_str()) => {
                    return Err(TypeError::ReservedName {
                        name: c.name.clone(),
                        span: c.span,
                    });
                }
                Item::Enum(e) if is_reserved_class(e.name.as_str()) => {
                    return Err(TypeError::ReservedName {
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
            if let Item::Interface(i) = item {
                let mut methods = Vec::with_capacity(i.methods.len());
                let mut seen: HashSet<Symbol> = HashSet::new();
                for m in i.methods.iter() {
                    if !seen.insert(m.name.clone()) {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "interface {:?} declares method {:?} more than once",
                                i.name, m.name
                            ),
                            span: m.span,
                        });
                    }
                    methods.push(InterfaceMethodSig {
                        name: m.name.clone(),
                        params: m.params.iter().map(|p| p.ty.clone()).collect(),
                        ret: m.ret.clone().unwrap_or(Type::Unit),
                    });
                }
                let module = module_of_name(i.name.as_str()).to_string();
                self.interfaces.insert(
                    i.name.clone(),
                    InterfaceSig { methods, is_pub: i.is_pub, module },
                );
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
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "fn {:?} mixes a generic declaration with another overload — \
                                 generic functions cannot share a name with other fns",
                                f.name
                            ),
                            span: f.span,
                        });
                    }
                    if entry.iter().any(|s| s.params == sig.params) {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "fn {:?} has a duplicate overload (same parameter types as a \
                                 previous declaration)",
                                f.name
                            ),
                            span: f.span,
                        });
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
                            Some(self.classes.get(&pname).cloned().ok_or_else(|| {
                                TypeError::UndefinedClass {
                                    name: pname.clone(),
                                    span: c.span,
                                }
                            })?)
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
                    let sig = class_signature(c, parent_sig.as_ref(), &is_sub)?;
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
                    result?;
                }
                Item::Interface(i) => {
                    let mut methods = Vec::with_capacity(i.methods.len());
                    let mut seen: HashSet<Symbol> = HashSet::new();
                    for m in i.methods.iter() {
                        if !seen.insert(m.name.clone()) {
                            return Err(TypeError::Unsupported {
                                what: format!(
                                    "interface {:?} declares method {:?} more than once",
                                    i.name, m.name
                                ),
                                span: m.span,
                            });
                        }
                        methods.push(InterfaceMethodSig {
                            name: m.name.clone(),
                            params: m.params.iter().map(|p| p.ty.clone()).collect(),
                            ret: m.ret.clone().unwrap_or(Type::Unit),
                        });
                    }
                    let module = module_of_name(i.name.as_str()).to_string();
                    self.interfaces.insert(
                        i.name.clone(),
                        InterfaceSig { methods, is_pub: i.is_pub, module },
                    );
                }
            }
        }
        // Now every struct / union (both `@extern(C)` and top-level)
        // is registered in `self.classes`, so we can validate the
        // top-level (`restrict_c_types: true`) ones — they must not
        // mention any C-only type, transitively.
        self.validate_restrict_c_structs(prog)?;

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
                } else {
                    self.infer_literal_type(value)
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
                Item::Fn(f) => self.check_fn(f, None)?,
                Item::Class(c) => self.check_class(c)?,
                Item::Enum(e) => self.check_enum(e)?,
                Item::Use(_) | Item::Const(_) => {}
                Item::ExternC(block) => {
                    *self.in_extern_c.borrow_mut() = true;
                    let result = self.check_extern_c_bodies(block);
                    *self.in_extern_c.borrow_mut() = false;
                    result?;
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
                    return Err(TypeError::ReservedName {
                        name: name.clone(),
                        span: s.span,
                    });
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
            last = self.check_stmt(s, &mut env, None, None, 0)?;
            *self.current_module.borrow_mut() = saved_module;
        }
        if let Some(t) = &prog.tail {
            last = self.check_expr(t, &env, None, None, 0)?;
        }
        self.vars = env;
        Ok(last)
    }

}
