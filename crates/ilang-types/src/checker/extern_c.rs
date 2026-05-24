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
    /// Walk an `@extern(C) { ... }` block during signature collection.
    /// Each inner item registers into the same tables `Item::Class` /
    /// `Item::Fn` would write to, but with the C-ABI flags pre-set.
    /// Caller has already set `self.in_extern_c = true`.
    pub(super) fn collect_extern_c_signatures(
        &mut self,
        block: &ilang_ast::ExternCBlock,
    ) -> Result<(), TypeError> {
        for item in &block.items {
            match item {
                ilang_ast::ExternCItem::Struct {
                    name,
                    fields,
                    is_packed,
                    is_handle,
                    span,
                    ..
                } => {
                    let synth = ClassDecl {
                        is_pub: false,
                        name: name.clone(),
                        type_params: Box::new([]),
                        parent: None,
                        interfaces: Box::new([]),
                        fields: fields.clone(),
                        methods: Box::new([]),
                        static_methods: Box::new([]),
                        static_fields: Box::new([]),
                        properties: Box::new([]),
                        attrs: Box::new([]),
                        extern_lib: None,
                        is_repr_c: true,
                        is_packed: *is_packed,
                        is_handle: *is_handle,
                        is_union: false,
                        span: *span,
                    };
                    let sig = class_signature(&synth, None, &|_, _| false)?;
                    self.classes.insert(name.clone(), sig);
                }
                ilang_ast::ExternCItem::Union { name, fields, span, .. } => {
                    let synth = ClassDecl {
                        is_pub: false,
                        name: name.clone(),
                        type_params: Box::new([]),
                        parent: None,
                        interfaces: Box::new([]),
                        fields: fields.clone(),
                        methods: Box::new([]),
                        static_methods: Box::new([]),
                        static_fields: Box::new([]),
                        properties: Box::new([]),
                        attrs: Box::new([]),
                        extern_lib: None,
                        is_repr_c: true,
                        is_packed: false,
                        is_handle: false,
                        is_union: true,
                        span: *span,
                    };
                    let sig = class_signature(&synth, None, &|_, _| false)?;
                    self.classes.insert(name.clone(), sig);
                }
                ilang_ast::ExternCItem::FnDecl { name, params, ret, variadic, libs, c_symbol, span, .. } => {
                    // Build a synthetic FnDecl with @extern attribute
                    // so downstream pipeline (loader, JIT) treats it
                    // like an existing top-level extern fn.
                    let mut extern_args = vec![ilang_ast::AttrArg::Path(Box::new([Symbol::intern("C")]))];
                    if *variadic {
                        extern_args.push(ilang_ast::AttrArg::Path(Box::new([Symbol::intern("variadic")])));
                    }
                    let attrs = vec![ilang_ast::Attribute {
                        name: "extern".into(),
                        args: extern_args.into(),
                    }];
                    let synth = FnDecl {
                        is_pub: false,
                        attrs: attrs.into(),
                        name: name.clone(),
                        type_params: Box::new([]),
                        params: params.clone(),
                        ret: ret.clone(),
                        body: ilang_ast::Block { stmts: Vec::new(), tail: None },
                        span: *span,
                        is_override: false,
            is_async: false,
                    };
                    let mut sig = signature_of(&synth);
                    // Mark dlsym'd `@lib` fns so the call-site gate
                    // can reject them outside an `@extern(...) { ... }`
                    // block. The list lets the gate exempt specific
                    // libraries (currently `@lib("objc")` — the ObjC
                    // runtime's retain / release primitives need to
                    // flow through wrapper fns that the call-site
                    // can't move inside an extern block).
                    sig.lib_names = libs.iter().copied().collect();
                    sig.c_symbol = *c_symbol;
                    self.fns.entry(name.clone()).or_default().push(sig);
                }
                ilang_ast::ExternCItem::FnDef(f) => {
                    let sig = signature_of(f);
                    self.fns.entry(f.name.clone()).or_default().push(sig);
                }
                ilang_ast::ExternCItem::Class(c) => {
                    // Resolve parent the same way Item::Class does so
                    // `@objc class NSString : NSObject` inherits NSObject's
                    // fields (notably the synthesised `handle: i64`).
                    let parent_sig = if let Some(pname) = &c.parent {
                        if self.interfaces.contains_key(pname) {
                            None
                        } else {
                            self.classes.get(&pname).cloned()
                        }
                    } else {
                        None
                    };
                    let sig = class_signature(c, parent_sig.as_ref(), &|_, _| false)?;
                    self.classes.insert(c.name.clone(), sig);
                }
            }
        }
        Ok(())
    }

    /// Type-check fn bodies inside an `@extern(C) { ... }` block.
    /// Caller has already set `self.in_extern_c = true`.
    pub(super) fn check_extern_c_bodies(
        &mut self,
        block: &ilang_ast::ExternCBlock,
    ) -> Result<(), TypeError> {
        use super::module_of_name;
        for item in &block.items {
            match item {
                ilang_ast::ExternCItem::FnDef(f) => {
                    // `@__objc_wrapper`-flagged FnDefs are
                    // parser-synthesised dispatch wrappers from an
                    // `@extern(ObjC) { ... }` block. Their
                    // signatures mirror the user's @objc declaration
                    // verbatim (raw pointers and all), so the
                    // raw-pointer rejection that normally guards
                    // ilang-side wrappers is intentionally skipped.
                    let is_objc_wrapper = f
                        .attrs
                        .iter()
                        .any(|a| a.name.as_str() == "__objc_wrapper");
                    if !is_objc_wrapper {
                        self.reject_pointer_in_signature(
                            &format!("fn {:?}", f.name),
                            f.params.iter().map(|p| &p.ty),
                            f.ret.as_ref(),
                            f.span,
                        )?;
                    }
                    // Same module-tracking rule as the outer loop in
                    // `check_program`: every top-level item carries a
                    // module prefix in its name (`cocoa.foo` ⇒ module
                    // `cocoa`), and `current_module` must follow it
                    // so member-access visibility checks judge the
                    // call from the item's perspective. Without
                    // this, a `pub` wrapper inside a sibling-file
                    // `@extern(ObjC)` block calling its own non-pub
                    // helper trips the "module-private" error
                    // because `current_module` still points at the
                    // entry.
                    let saved_module = self.current_module.borrow().clone();
                    *self.current_module.borrow_mut() =
                        module_of_name(f.name.as_str()).to_string();
                    let result = self.check_fn(f, None);
                    *self.current_module.borrow_mut() = saved_module;
                    result?;
                }
                ilang_ast::ExternCItem::Class(c) => {
                    for m in &c.methods {
                        let is_objc_wrapper = m
                            .attrs
                            .iter()
                            .any(|a| a.name.as_str() == "__objc_wrapper");
                        if !is_objc_wrapper {
                            self.reject_pointer_in_signature(
                                &format!("method {:?}.{:?}", c.name, m.name),
                                m.params.iter().map(|p| &p.ty),
                                m.ret.as_ref(),
                                m.span,
                            )?;
                        }
                    }
                    for m in &c.static_methods {
                        let is_objc_wrapper = m
                            .attrs
                            .iter()
                            .any(|a| a.name.as_str() == "__objc_wrapper");
                        if !is_objc_wrapper {
                            self.reject_pointer_in_signature(
                                &format!("static {:?}.{:?}", c.name, m.name),
                                m.params.iter().map(|p| &p.ty),
                                m.ret.as_ref(),
                                m.span,
                            )?;
                        }
                    }
                    let saved_module = self.current_module.borrow().clone();
                    *self.current_module.borrow_mut() =
                        module_of_name(c.name.as_str()).to_string();
                    let result = self.check_class(c);
                    *self.current_module.borrow_mut() = saved_module;
                    result?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Walk every top-level `struct` / `union` declared **outside**
    /// an `@extern(C) { ... }` block (the parser sets
    /// `restrict_c_types: true` on those AST nodes) and reject any
    /// field whose type is, or transitively contains, a C-only type:
    /// `char`, `void`, `size_t`, `ssize_t`, or a raw pointer.
    ///
    /// References to other named structs / unions are followed via
    /// `self.classes` (which already covers both `@extern(C)` and
    /// top-level declarations because they share the same registration
    /// path). A `visiting` set breaks cycles so mutually-recursive
    /// declarations don't hang the validator.
    pub(super) fn validate_restrict_c_structs(
        &self,
        prog: &ilang_ast::Program,
    ) -> Result<(), TypeError> {
        use ilang_ast::ExternCItem;
        for item in &prog.items {
            let Item::ExternC(block) = item else { continue };
            for inner in block.items.iter() {
                let (name, fields, restrict) = match inner {
                    ExternCItem::Struct {
                        name, fields, restrict_c_types, ..
                    }
                    | ExternCItem::Union {
                        name, fields, restrict_c_types, ..
                    } => (name, fields, *restrict_c_types),
                    _ => continue,
                };
                if !restrict {
                    continue;
                }
                let kind = if matches!(inner, ExternCItem::Struct { .. }) { "struct" } else { "union" };
                for f in fields.iter() {
                    let mut visiting: HashSet<Symbol> = HashSet::new();
                    if let Some(reason) = self.find_c_only_type(&f.ty, &mut visiting) {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "top-level `{kind} {name}` field `{fname}` has type `{fty}` which is, or contains, a C-only type ({reason}). \
                                 Move the declaration into `@extern(C) {{ {kind} {name} {{ ... }} }}` if you need C interop, \
                                 or replace the field with an ilang type.",
                                fname = f.name,
                                fty = f.ty,
                            ),
                            span: f.span,
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// `Some(reason)` if `t` is — or transitively contains via named
    /// struct / union references — one of the C-only types we forbid
    /// in a top-level (non-`@extern(C)`) struct or union.
    pub(super) fn find_c_only_type(
        &self,
        t: &Type,
        visiting: &mut HashSet<Symbol>,
    ) -> Option<String> {
        match t {
            Type::CChar | Type::CVoid | Type::Size | Type::SSize => {
                Some(format!("`{t}`"))
            }
            Type::RawPtr { .. } => Some(format!("`{t}`")),
            Type::Array { elem, .. } => self.find_c_only_type(elem, visiting),
            Type::Optional(inner) | Type::Weak(inner) => {
                self.find_c_only_type(inner, visiting)
            }
            Type::Tuple(items) => items
                .iter()
                .find_map(|x| self.find_c_only_type(x, visiting)),
            Type::Generic(g) => g
                .args
                .iter()
                .find_map(|a| self.find_c_only_type(a, visiting)),
            Type::Fn(ft) => ft
                .params
                .iter()
                .find_map(|p| self.find_c_only_type(p, visiting))
                .or_else(|| self.find_c_only_type(&ft.ret, visiting)),
            Type::Object(name) => {
                if !visiting.insert(name.clone()) {
                    return None;
                }
                let res = self.classes.get(&name).and_then(|cs| {
                    // Only descend into C-layout structs / unions.
                    // Plain ilang classes can hold runtime objects
                    // freely; we don't want to forbid them just
                    // because some method signature mentions a C type.
                    if !cs.is_repr_c {
                        return None;
                    }
                    cs.fields.iter().find_map(|(fname, fty)| {
                        self.find_c_only_type(fty, visiting).map(|inner| {
                            format!("{name}.{fname}: {inner}")
                        })
                    })
                });
                visiting.remove(name);
                res
            }
            _ => None,
        }
    }

    /// Walks `params` + `ret` of an ilang-side fn declared inside an
    /// `@extern(C) { ... }` block (i.e. no `@lib(...)`) and rejects
    /// any raw-pointer type — directly or via a `@extern(C) struct`
    /// field that contains one. Raw pointers are meant to stay
    /// inside the FFI block; if a wrapper exposes them, ilang user
    /// code outside the block has no safe way to handle the value.
    pub(super) fn reject_pointer_in_signature<'a>(
        &self,
        what: &str,
        params: impl IntoIterator<Item = &'a Type>,
        ret: Option<&Type>,
        span: Span,
    ) -> Result<(), TypeError> {
        let mut visiting: HashSet<Symbol> = HashSet::new();
        for p in params {
            if let Some(reason) = self.find_raw_pointer(p, &mut visiting) {
                return Err(TypeError::Unsupported {
                    what: format!(
                        "{what}: parameter of type `{p}` exposes a raw pointer ({reason}). \
                         Raw pointers are not allowed in ilang-side wrappers — keep them \
                         inside @lib(...) declarations."
                    ),
                    span,
                });
            }
        }
        if let Some(r) = ret {
            if let Some(reason) = self.find_raw_pointer(r, &mut visiting) {
                return Err(TypeError::Unsupported {
                    what: format!(
                        "{what}: return type `{r}` exposes a raw pointer ({reason}). \
                         Raw pointers are not allowed in ilang-side wrappers — keep them \
                         inside @lib(...) declarations."
                    ),
                    span,
                });
            }
        }
        Ok(())
    }

    /// Returns `Some(reason)` if `t` is a raw pointer or transitively
    /// references one through a `@extern(C) struct` field. `visiting`
    /// breaks cycles in mutually-referencing structs.
    pub(super) fn find_raw_pointer(
        &self,
        t: &Type,
        visiting: &mut HashSet<Symbol>,
    ) -> Option<String> {
        match t {
            Type::RawPtr { .. } => Some(format!("`{t}`")),
            Type::Array { elem, .. } => self.find_raw_pointer(elem, visiting),
            Type::Optional(inner) | Type::Weak(inner) => {
                self.find_raw_pointer(inner, visiting)
            }
            Type::Tuple(items) => items
                .iter()
                .find_map(|x| self.find_raw_pointer(x, visiting)),
            Type::Generic(g) => g
                .args
                .iter()
                .find_map(|a| self.find_raw_pointer(a, visiting)),
            Type::Fn(ft) => ft
                .params
                .iter()
                .find_map(|p| self.find_raw_pointer(p, visiting))
                .or_else(|| self.find_raw_pointer(&ft.ret, visiting)),
            Type::Object(name) => {
                if !visiting.insert(name.clone()) {
                    return None;
                }
                let res = self.classes.get(&name).and_then(|cs| {
                    if !cs.is_repr_c {
                        return None;
                    }
                    cs.fields.iter().find_map(|(fname, fty)| {
                        self.find_raw_pointer(fty, visiting).map(|inner| {
                            format!("{name}.{fname}: {inner}")
                        })
                    })
                });
                visiting.remove(name);
                res
            }
            _ => None,
        }
    }

    pub(super) fn validate_type(
        &self,
        t: &Type,
        span: Span,
        type_params_in_scope: &[Symbol],
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
                    || self.interfaces.contains_key(name)
                    || self.enums.contains_key(name)
                    || type_params_in_scope.iter().any(|p| p == name)
                    // Fallback: when the caller passed an empty
                    // `type_params_in_scope` (e.g. body-local
                    // `let y: T[] = ...`), the active fn's own
                    // type params are still in scope through
                    // `current_type_params`. The caller does
                    // override when it has a more specific list
                    // (class param scoping for fields / methods).
                    || (type_params_in_scope.is_empty()
                        && self
                            .current_type_params
                            .borrow()
                            .iter()
                            .any(|p| p == name))
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
            // Raw C pointer / void / char / size_t / ssize_t — only
            // nameable inside an `@extern(C) { ... }` block.
            Type::RawPtr { inner, .. } => {
                if !*self.in_extern_c.borrow() {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "{t} (raw C pointer types are only nameable inside an @extern(C) {{ ... }} block)"
                        ),
                        span,
                    });
                }
                self.validate_type(inner, span, type_params_in_scope)?;
            }
            Type::CVoid | Type::CChar | Type::Size | Type::SSize => {
                if !*self.in_extern_c.borrow() {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "{t} (C-only type, nameable only inside an @extern(C) {{ ... }} block)"
                        ),
                        span,
                    });
                }
            }
            Type::Tuple(elems) => {
                for e in elems {
                    self.validate_type(e, span, type_params_in_scope)?;
                }
            }
            Type::Fn(ft) => {
                for p in &ft.params {
                    self.validate_type(p, span, type_params_in_scope)?;
                }
                self.validate_type(&ft.ret, span, type_params_in_scope)?;
            }
            Type::Generic(g) => {
                for a in &g.args {
                    self.validate_type(a, span, type_params_in_scope)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

}
