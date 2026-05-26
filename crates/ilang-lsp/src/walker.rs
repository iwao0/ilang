//! Extracted from `main.rs`.
#![allow(unused_imports)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};



use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use ilang_ast::{
    Block, ClassDecl, EnumDecl, Expr, ExprKind, FieldDecl, FnDecl, GenericTy, InterfaceDecl, Item,
    Param, Pattern, PatternBindings, PatternKind, Program, Span, Stmt, StmtKind,
    Symbol as AstSymbol, Type, VariantPayload,
};
use ilang_parser::parse as parse_program;
use ilang_types::{check, TypeError};

use crate::*;

#[derive(Clone, Debug)]
pub(crate) struct Binding {
    pub(crate) name: String,
    pub(crate) span: Span,
    /// Statically-known type, if we can pin it down. Used both for hover
    /// signature and to resolve `local.field` accesses to the right class.
    pub(crate) ty: Option<Type>,
    /// What kind of binder introduced this (let / param / for-in / match
    /// pattern). Carried into hover signatures so use sites read like
    /// the declaration.
    pub(crate) kind: BindKind,
    /// When `Some`, replaces the kind/ty-derived hover signature.
    /// Used for `let func = fn(name: T): R { ... }` where we want to
    /// show parameter names that `Type::Fn` itself doesn't carry.
    pub(crate) override_signature: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum BindKind {
    Let,
    Param,
    ForIn,
    Pattern,
}

impl BindKind {
    pub(crate) fn render(self, name: &str, ty: Option<&Type>) -> String {
        let prefix = match self {
            BindKind::Let => "let ",
            BindKind::Param => "(parameter) ",
            BindKind::ForIn => "(for-binding) ",
            BindKind::Pattern => "(pattern) ",
        };
        match ty {
            Some(t) => format!("{prefix}{name}: {t}"),
            None => format!("{prefix}{name}"),
        }
    }
}

pub(crate) struct Walker<'a> {
    pub(crate) text: &'a str,
    pub(crate) symbols: &'a HashMap<AstSymbol, Symbol>,
    pub(crate) classes: &'a HashMap<AstSymbol, ClassInfo>,
    /// Top-level fn return types, keyed by name. Used to infer
    /// `let x = call()` bindings.
    pub(crate) fn_returns: &'a HashMap<AstSymbol, Type>,
    /// Hover signatures for `module.name` references that the loader
    /// brought in from a `use module` statement.
    pub(crate) external_signatures: &'a HashMap<AstSymbol, String>,
    /// Doc comments for external (imported) decls, keyed the same as
    /// `external_signatures`.
    pub(crate) external_docs: &'a HashMap<AstSymbol, String>,
    /// Return types for the same set of external fns. Used when
    /// inferring `let x = math.sqrt(...)` etc.
    pub(crate) external_returns: &'a HashMap<AstSymbol, Type>,
    /// Source-file path for each `module.<decl>` so cross-file F12
    /// can navigate into the originating module.
    pub(crate) external_sources: &'a ExternalSources,
    pub(crate) refs: &'a mut Vec<RefEntry>,
    /// Variable-name → class-name index, populated whenever a binding's
    /// statically-known type resolves to a class. Drives completion on
    /// `obj.` for ordinary instance variables.
    pub(crate) var_classes: &'a mut HashMap<AstSymbol, String>,
    /// Variable-name → full type, used for completion on built-in
    /// receivers (`string`, `T[]`) where there's no class entry.
    pub(crate) var_types: &'a mut HashMap<AstSymbol, Type>,
    /// Buffer-local `const NAME: T = …` types. The loader inlines
    /// const references away in merged programs, but the buffer-side
    /// walker still sees them as `Var(NAME)` and needs a way to
    /// recover the const's static type for `let x = NAME`-style
    /// bindings.
    pub(crate) consts: &'a HashMap<AstSymbol, Type>,
    /// Class name of the method body currently being walked, if any.
    /// `walk_fn` saves the previous value before entering a method
    /// body and restores it on return. `infer_expr` reads this when
    /// resolving `Field { obj: This, name }` so hover on
    /// `this.field.field2` chains can find the enclosing class
    /// without `walk_fn`-style threading of `this_class` through
    /// every recursive inference call.
    pub(crate) current_this_class: Option<String>,
}

impl<'a> Walker<'a> {
    /// Walk a `Type` at `start_span` (the first character of the
    /// type token in source) and push hover / F12 entries for each
    /// dotted `Type::Object` name. Suffixes like `[]`, `?`, `.weak`
    /// don't shift the type-name's start, so nested types inherit
    /// `start_span`.
    pub(crate) fn walk_type_at(&mut self, ty: &Type, start_span: Span) {
        match ty {
            Type::Object(name) => self.walk_type_name_at(name.as_str(), start_span),
            Type::Array { elem, .. } => self.walk_type_at(elem, start_span),
            Type::Optional(inner) => self.walk_type_at(inner, start_span),
            Type::Weak(inner) => self.walk_type_at(inner, start_span),
            Type::Generic(g) => self.walk_type_name_at(g.base.as_str(), start_span),
            _ => {}
        }
    }

    /// Resolve and push a Ref for a type-name occurrence at
    /// `start_span`. Handles three shapes:
    ///   * the source literally spells the full dotted name
    ///     (`cocoa.NSObject` in code) → `push_external_dotted_ref`
    ///   * the AST carries a dotted name but the source spells
    ///     just the suffix (typical after `use M { Name }` lets
    ///     the loader rewrite `Name` → `M.Name`) → look up the
    ///     suffix in `external_signatures` and point F12 at the
    ///     selective-import source
    ///   * bare name, either in buffer-local `symbols` or in the
    ///     selective-import maps
    fn walk_type_name_at(&mut self, name: &str, start_span: Span) {
        if name.contains('.') {
            if text::text_at_span_starts_with(self.text, start_span, name) {
                self.push_external_dotted_ref(name, start_span);
                return;
            }
            if let Some((_, suffix)) = name.rsplit_once('.') {
                if text::text_at_span_starts_with(self.text, start_span, suffix) {
                    self.push_external_type_ref(suffix, start_span);
                    return;
                }
            }
            self.push_external_dotted_ref(name, start_span);
            return;
        }
        if let Some(sym) = self.symbols.get(&AstSymbol::intern(name)) {
            self.push_ref_with_doc(
                name,
                start_span,
                sym.span,
                name.len() as u32,
                sym.signature.clone(),
                sym.doc.clone(),
            );
        } else {
            self.push_external_type_ref(name, start_span);
        }
    }

    /// Bare type name not found in `symbols` (so not buffer-local)
    /// — try the selective-import maps. `use cocoa { NSObject }`
    /// lands NSObject's signature under the bare key in
    /// `external_signatures` with the originating source in
    /// `external_sources`, which gives us the F12 target.
    pub(crate) fn push_external_type_ref(&mut self, name: &str, span: Span) {
        let key = AstSymbol::intern(name);
        let Some(sig) = self.external_signatures.get(&key) else {
            return;
        };
        let loc = self.external_sources.get(&key);
        let target_uri = loc.and_then(|l| Url::from_file_path(&l.path).ok());
        let (target_span, target_name_len, no_def) = match loc {
            Some(l) if target_uri.is_some() => (l.span, l.name_len, false),
            _ => (span, name.len() as u32, target_uri.is_none()),
        };
        self.refs.push(RefEntry {
            line: span.line,
            start_col: span.col,
            end_col: span.col + name.len() as u32,
            target_span,
            target_name_len,
            signature: sig.clone(),
            no_definition: no_def,
            target_uri,
            doc: self.external_docs.get(&key).cloned(),
        });
    }

    /// Walk only the fn header (param types + return type) to push
    /// type-name refs. Used for getter / setter accessors whose body
    /// is the @objc desugar's synthetic `__objc_wrapper` — the body
    /// is skipped to avoid `let __recv` / `let __sel` polluting hover,
    /// but the header is still user source and its type identifiers
    /// must show up in documentHighlight / references / rename.
    pub(crate) fn walk_fn_header_type_refs(&mut self, f: &FnDecl) {
        for p in &f.params {
            if let Some(start) =
                locate_type_after_colon(self.text, p.span, p.name.as_str())
            {
                self.walk_type_at(&p.ty, start);
            }
        }
        if let Some(ret) = &f.ret {
            if let Some(start) =
                text::locate_fn_return_type(self.text, f.span, f.name.as_str())
            {
                self.walk_type_at(ret, start);
            }
        }
    }

    pub(crate) fn walk_fn(&mut self, f: &FnDecl, this_class: Option<&str>) {
        let mut scope: Vec<Binding> = Vec::new();
        for p in &f.params {
            let sig = BindKind::Param.render(p.name.as_str(), Some(&p.ty));
            self.push_decl(p.name.as_str(), p.span, sig);
            if let Some(start) =
                locate_type_after_colon(self.text, p.span, p.name.as_str())
            {
                self.walk_type_at(&p.ty, start);
            }
            if let Some(c) = type_to_class(&p.ty) {
                self.var_classes.insert(p.name.clone(), c);
            }
            self.var_types.insert(p.name.clone(), p.ty.clone());
            scope.push(Binding {
                name: p.name.as_str().to_string(),
                span: p.span,
                ty: Some(p.ty.clone()),
                kind: BindKind::Param,
                override_signature: None,
            });
        }
        // Return type — `walk_fn` previously only walked params and
        // body, leaving the return-type identifier without a ref
        // entry. Without one, hover still works via the `word_at`
        // fallback, but documentHighlight / references skip it
        // because they only iterate `doc.refs`.
        if let Some(ret) = &f.ret {
            if let Some(start) =
                text::locate_fn_return_type(self.text, f.span, f.name.as_str())
            {
                self.walk_type_at(ret, start);
            }
        }
        // Set `current_this_class` for the duration of the body so
        // `infer_expr` can resolve `Field { obj: This, name }`
        // (otherwise hover on `this.field.field2` chains misses the
        // outer field's type and the inner field lookup fails). Save
        // / restore the previous value to support nested method
        // bodies (e.g. a closure that itself has a `this`).
        let prev_this_class = self.current_this_class.take();
        self.current_this_class = this_class.map(|s| s.to_string());
        self.walk_block(&f.body, &mut scope, this_class);
        self.current_this_class = prev_this_class;
    }

    pub(crate) fn walk_class(&mut self, c: &ClassDecl) {
        if let Some(parent) = &c.parent {
            if let Some(start) = locate_class_base_name(self.text, c.span, 0) {
                self.walk_type_at(&Type::Object(parent.clone()), start);
            }
        }
        for (idx, ifn) in c.interfaces.iter().enumerate() {
            if let Some(start) = locate_class_base_name(self.text, c.span, idx + 1) {
                self.walk_type_at(&Type::Object(ifn.clone()), start);
            }
        }
        // Field declaration name: hover shows the field decl line.
        for f in &c.fields {
            if is_parser_synth_field(f, c.span) {
                continue;
            }
            self.push_decl_with_doc(
                f.name.as_str(),
                f.span,
                format!("(property) {}.{}: {}", c.name, f.name, f.ty),
                text::extract_doc_above(self.text, f.span.line),
            );
            if let Some(start) =
                locate_type_after_colon(self.text, f.span, f.name.as_str())
            {
                self.walk_type_at(&f.ty, start);
            }
        }
        for f in &c.static_fields {
            let kind = if f.is_const { "static const" } else { "static property" };
            let value = render_const_value_with_src(&f.value, Some(self.text))
                .map(|v| format!(" = {v}"))
                .unwrap_or_default();
            self.push_decl_with_doc(
                f.name.as_str(),
                f.span,
                format!("({}) {}.{}: {}{}", kind, c.name, f.name, f.ty, value),
                text::extract_doc_above(self.text, f.span.line),
            );
            if let Some(start) =
                locate_type_after_colon(self.text, f.span, f.name.as_str())
            {
                self.walk_type_at(&f.ty, start);
            }
        }
        for p in &c.properties {
            // PropertyDecl.span points at the `get` / `set` keyword, so
            // the name identifier sits a few columns to its right. Push
            // a decl entry at that exact location for hover and F12,
            // distinguishing the getter from the setter.
            let prop_doc = text::extract_doc_above(self.text, p.span.line);
            for (kind, accessor_span) in [
                ("getter", p.getter.as_ref().map(|g| g.span)),
                ("setter", p.setter.as_ref().map(|s| s.span)),
            ] {
                let Some(span) = accessor_span else { continue };
                let sig = format!("({kind}) {}.{}: {}", c.name, p.name, p.ty);
                if let Some(name_span) =
                    locate_property_name(self.text, span, p.name.as_str())
                {
                    let accessor_doc =
                        text::extract_doc_above(self.text, span.line)
                            .or_else(|| prop_doc.clone());
                    self.push_decl_with_doc(
                        p.name.as_str(),
                        name_span,
                        sig,
                        accessor_doc,
                    );
                }
            }
        }
        for m in &c.methods {
            if is_parser_synth_helper(m, c.span) {
                continue;
            }
            self.push_decl_with_doc(
                m.name.as_str(),
                m.span,
                format!("(method) {}{}.{}", render_user_attrs(&m.attrs), c.name, fn_body(m)),
                text::extract_doc_above(self.text, m.span.line),
            );
            self.walk_fn(m, Some(c.name.as_str()));
        }
        for m in &c.static_methods {
            if is_parser_synth_helper(m, c.span) {
                continue;
            }
            self.push_decl_with_doc(
                m.name.as_str(),
                m.span,
                format!(
                    "(static method) {}{}.{}",
                    render_user_attrs(&m.attrs),
                    c.name,
                    fn_body(m)
                ),
                text::extract_doc_above(self.text, m.span.line),
            );
            self.walk_fn(m, None);
        }
        for prop in &c.properties {
            // Hover entry for the property name at its declaration
            // site. Matches the static/instance method paths above so
            // hovering on `pub static get black(): NSColor` (or a
            // reference to it) lands on a `(static getter) ... :
            // NSColor` signature instead of the @objc desugar's
            // internal `let __recv` local.
            let kind = match (prop.is_static, prop.getter.is_some(), prop.setter.is_some()) {
                (true, true, _) => "static getter",
                (true, false, true) => "static setter",
                (false, true, _) => "getter",
                (false, false, true) => "setter",
                _ => "property",
            };
            let attr_prefix = prop
                .getter
                .as_ref()
                .or(prop.setter.as_ref())
                .map(|f| render_user_attrs(&f.attrs))
                .unwrap_or_default();
            self.push_decl_with_doc(
                prop.name.as_str(),
                prop.span,
                format!(
                    "({kind}) {attr_prefix}{}.{}: {}",
                    c.name, prop.name, prop.ty
                ),
                text::extract_doc_above(self.text, prop.span.line),
            );
            // Walk the getter / setter bodies like ordinary method
            // bodies so locals and `this.X` resolve normally. Skip
            // @objc-desugared synthetic bodies (marked
            // `__objc_wrapper`) — their `let __recv` / `let __sel`
            // statements borrow the property declaration's span and
            // would shadow the (getter)/(setter) signature pushed
            // above.
            let is_synth_body =
                |f: &FnDecl| f.attrs.iter().any(|a| a.name.as_str() == "$objc.wrapper");
            if let Some(g) = &prop.getter {
                if is_synth_body(g) {
                    self.walk_fn_header_type_refs(g);
                } else {
                    self.walk_fn(g, Some(c.name.as_str()));
                }
            }
            if let Some(s) = &prop.setter {
                if is_synth_body(s) {
                    self.walk_fn_header_type_refs(s);
                } else {
                    self.walk_fn(s, Some(c.name.as_str()));
                }
            }
        }
    }

    pub(crate) fn walk_interface(&mut self, i: &InterfaceDecl) {
        for m in &i.methods {
            let params = m
                .params
                .iter()
                .map(|p| format!("{}: {}", p.name, p.ty))
                .collect::<Vec<_>>()
                .join(", ");
            let ret = match &m.ret {
                Some(t) => format!(": {t}"),
                None => String::new(),
            };
            let sig = format!("(method) {}.{}({}){}", i.name, m.name, params, ret);
            let name_span = locate_let_name_with_kw(self.text, m.span, "fn", m.name.as_str())
                .unwrap_or(m.span);
            self.push_decl_with_doc(
                m.name.as_str(),
                name_span,
                sig,
                text::extract_doc_above(self.text, m.span.line),
            );
            for p in &m.params {
                if let Some(start) = locate_type_after_colon(self.text, p.span, p.name.as_str()) {
                    self.walk_type_at(&p.ty, start);
                }
            }
        }
    }

    pub(crate) fn walk_block(&mut self, b: &Block, scope: &mut Vec<Binding>, this_class: Option<&str>) {
        let depth = scope.len();
        for s in &b.stmts {
            self.walk_stmt(s, scope, this_class);
        }
        if let Some(t) = &b.tail {
            self.walk_expr(t, scope, this_class);
        }
        scope.truncate(depth);
    }

    pub(crate) fn walk_stmt(&mut self, s: &Stmt, scope: &mut Vec<Binding>, this_class: Option<&str>) {
        match &s.kind {
            StmtKind::Let { name, ty, value, .. } => {
                self.walk_expr(value, scope, this_class);
                let inferred = ty
                    .clone()
                    .or_else(|| self.infer_expr(value, scope));
                // For `let f = fn(name: T): R { ... }` keep the param
                // names in the rendered signature (Type::Fn alone drops
                // them).
                let override_sig = match &value.kind {
                    ExprKind::FnExpr { params, ret, .. } => {
                        let ps = params
                            .iter()
                            .map(|p| format!("{}: {}", p.name, p.ty))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let r = match ret {
                            Some(t) => format!(": {t}"),
                            None => String::new(),
                        };
                        Some(format!("let {name}: fn({ps}){r}"))
                    }
                    _ => None,
                };
                let sig = override_sig
                    .clone()
                    .unwrap_or_else(|| BindKind::Let.render(name.as_str(), inferred.as_ref()));
                // s.span points at the `let` keyword. Locate the actual
                // name position by skipping `let` + whitespace.
                let name_span = locate_let_name(self.text, s.span, name.as_str()).unwrap_or(s.span);
                self.push_decl(name.as_str(), name_span, sig);
                if let Some(c) = inferred.as_ref().and_then(type_to_class) {
                    self.var_classes.insert(name.clone(), c);
                }
                if let Some(t) = inferred.as_ref() {
                    self.var_types.insert(name.clone(), t.clone());
                }
                scope.push(Binding {
                    name: name.as_str().to_string(),
                    span: name_span,
                    ty: inferred,
                    kind: BindKind::Let,
                    override_signature: override_sig,
                });
            }
            StmtKind::LetTuple { elems, value } => {
                self.walk_expr(value, scope, this_class);
                for slot in elems.iter() {
                    if let Some(name) = slot {
                        scope.push(Binding {
                            name: name.as_str().to_string(),
                            span: s.span,
                            ty: None,
                            kind: BindKind::Let,
                            override_signature: None,
                        });
                    }
                }
            }
            StmtKind::LetStruct { class: _, fields, value } => {
                self.walk_expr(value, scope, this_class);
                for f in fields.iter() {
                    scope.push(Binding {
                        name: f.as_str().to_string(),
                        span: s.span,
                        ty: None,
                        kind: BindKind::Let,
                        override_signature: None,
                    });
                }
            }
            StmtKind::Expr(e) => self.walk_expr(e, scope, this_class),
        }
    }

    pub(crate) fn walk_expr(&mut self, e: &Expr, scope: &mut Vec<Binding>, this_class: Option<&str>) {
        match &e.kind {
            ExprKind::Var(name) => self.walk_expr_var(name, e.span, scope, this_class),
            ExprKind::This => {
                if let Some(c) = this_class {
                    if let Some(info) = self.classes.get(&AstSymbol::intern(c)) {
                        // `this` is 4 chars; e.span points at it.
                        self.push_ref("this", e.span, info.decl_span, c.len() as u32, format!("this: {c}"));
                    }
                }
            }
            ExprKind::Field { obj, name } => self.walk_expr_field(obj, name, scope, this_class),
            ExprKind::MethodCall { obj, method, args } => {
                self.walk_expr_method_call(obj, method, args, scope, this_class)
            }
            ExprKind::Call { callee, args } => {
                self.walk_expr_call(callee, args, e.span, scope, this_class)
            }
            ExprKind::New { class, args, .. } => {
                self.walk_expr_new(class, args, e.span, scope, this_class)
            }
            ExprKind::EnumCtor { enum_name, variant, args } => {
                self.walk_expr_enum_ctor(enum_name, variant, args, e.span, scope, this_class)
            }
            ExprKind::Unary { expr, .. } => self.walk_expr(expr, scope, this_class),
            ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
                self.walk_expr(lhs, scope, this_class);
                self.walk_expr(rhs, scope, this_class);
            }
            ExprKind::If { cond, then_branch, else_branch } => {
                self.walk_expr(cond, scope, this_class);
                self.walk_block(then_branch, scope, this_class);
                if let Some(e) = else_branch {
                    self.walk_expr(e, scope, this_class);
                }
            }
            ExprKind::IfLet { name, expr, then_branch, else_branch } => {
                self.walk_expr(expr, scope, this_class);
                let inner_ty = self.infer_expr(expr, scope).and_then(|t| match t {
                    Type::Optional(inner) => Some(*inner),
                    _ => None,
                });
                let name_span =
                    locate_if_let_some_name(self.text, e.span, name.as_str()).unwrap_or(e.span);
                let sig = BindKind::Let.render(name.as_str(), inner_ty.as_ref());
                self.push_decl(name.as_str(), name_span, sig);
                if let Some(c) = inner_ty.as_ref().and_then(type_to_class) {
                    self.var_classes.insert(name.clone(), c);
                }
                if let Some(t) = inner_ty.as_ref() {
                    self.var_types.insert(name.clone(), t.clone());
                }
                let depth = scope.len();
                scope.push(Binding {
                    name: name.as_str().to_string(),
                    span: name_span,
                    ty: inner_ty,
                    kind: BindKind::Let,
                    override_signature: None,
                });
                self.walk_block(then_branch, scope, this_class);
                scope.truncate(depth);
                if let Some(eb) = else_branch {
                    self.walk_expr(eb, scope, this_class);
                }
            }
            ExprKind::While { cond, body } => {
                self.walk_expr(cond, scope, this_class);
                self.walk_block(body, scope, this_class);
            }
            ExprKind::ForIn { var, iter, body } => {
                self.walk_expr(iter, scope, this_class);
                let depth = scope.len();
                let elem_ty = match self.infer_expr(iter, scope) {
                    Some(Type::Array { elem, .. }) => Some(*elem),
                    _ => None,
                };
                let sig = BindKind::ForIn.render(var.as_str(), elem_ty.as_ref());
                self.push_decl(var.as_str(), iter.span, sig);
                scope.push(Binding {
                    name: var.as_str().to_string(),
                    span: iter.span,
                    ty: elem_ty,
                    kind: BindKind::ForIn,
                    override_signature: None,
                });
                self.walk_block(body, scope, this_class);
                scope.truncate(depth);
            }
            ExprKind::Loop { body } => self.walk_block(body, scope, this_class),
            ExprKind::Block(b) => self.walk_block(b, scope, this_class),
            ExprKind::Break(opt) | ExprKind::Return(opt) => {
                if let Some(v) = opt {
                    self.walk_expr(v, scope, this_class);
                }
            }
            ExprKind::Assign { target, value } => {
                if let Some(b) = scope.iter().rev().find(|b| b.name == target.as_str()) {
                    let sig = b
                        .override_signature
                        .clone()
                        .unwrap_or_else(|| b.kind.render(target.as_str(), b.ty.as_ref()));
                    self.push_ref(target.as_str(), e.span, b.span, target.as_str().len() as u32, sig);
                } else if let Some(m) = this_class.and_then(|c| self.classes.get(&AstSymbol::intern(c))).and_then(
                    |info| {
                        info.setters
                            .get(target)
                            .or_else(|| info.fields.get(target))
                    },
                ) {
                    self.push_ref(
                        target.as_str(),
                        e.span,
                        m.span,
                        target.as_str().len() as u32,
                        m.signature.clone(),
                    );
                } else if let Some(sym) = self.symbols.get(target) {
                    self.push_ref(
                        target.as_str(),
                        e.span,
                        sym.span,
                        sym.name.as_str().len() as u32,
                        sym.signature.clone(),
                    );
                }
                self.walk_expr(value, scope, this_class);
            }
            ExprKind::AssignField { obj, field, value, is_init: _ } => {
                self.walk_expr(obj, scope, this_class);
                if let Some(class) = self.resolve_obj_class(obj, scope, this_class) {
                    if let Some(info) = self.classes.get(&AstSymbol::intern(&class)) {
                        if let Some(m) = info
                            .setters
                            .get(field)
                            .or_else(|| info.fields.get(field))
                        {
                            if let Some((line, col)) = locate_dot_name(self.text, obj.span, field.as_str())
                            {
                                let (target, no_def, uri) = member_target(
                                    m,
                                    info,
                                    &class,
                                    self.external_sources,
                                    line,
                                    col,
                                );
                                self.refs.push(RefEntry {
                                    line,
                                    start_col: col,
                                    end_col: col + field.as_str().len() as u32,
                                    target_span: target,
                                    target_name_len: field.as_str().len() as u32,
                                    signature: m.signature.clone(),
                                    no_definition: no_def,
                                    target_uri: uri,
                                    doc: m.doc.clone(),
                                });
                            }
                        }
                    }
                }
                self.walk_expr(value, scope, this_class);
            }
            ExprKind::AssignIndex { obj, index, value } => {
                self.walk_expr(obj, scope, this_class);
                self.walk_expr(index, scope, this_class);
                self.walk_expr(value, scope, this_class);
            }
            ExprKind::Cast { expr, .. } => self.walk_expr(expr, scope, this_class),
            ExprKind::FnExpr { params, body, .. } => {
                // Closures capture outer locals by value at runtime, but
                // for hover/F12 it's useful to resolve them inside the
                // body too — start from the enclosing scope and add the
                // closure's own params on top.
                let mut inner: Vec<Binding> = scope.clone();
                for p in params {
                    let sig = BindKind::Param.render(p.name.as_str(), Some(&p.ty));
                    self.push_decl(p.name.as_str(), p.span, sig);
                    // Mirror what `let` does: register the param in the
                    // document-wide var_classes / var_types maps so
                    // completion's `resolve_receiver_class` (which only
                    // sees those maps, not the per-walk `Binding`
                    // scope) can dot-into a lambda param like
                    // `fn(e: gui.MouseEvent) { e.| }`. Last-write-wins
                    // when the same name recurs across sibling lambdas
                    // — same trade-off the `let` path already accepts.
                    if let Some(c) = type_to_class(&p.ty) {
                        self.var_classes.insert(p.name.clone(), c);
                    }
                    self.var_types.insert(p.name.clone(), p.ty.clone());
                    inner.push(Binding {
                        name: p.name.as_str().to_string(),
                        span: p.span,
                        ty: Some(p.ty.clone()),
                        kind: BindKind::Param,
                        override_signature: None,
                    });
                }
                self.walk_block(body, &mut inner, this_class);
            }
            ExprKind::Array(es) | ExprKind::Tuple(es) => {
                for x in es {
                    self.walk_expr(x, scope, this_class);
                }
            }
            ExprKind::StructLit { class, fields, field_name_spans } => {
                // Hover on each field name (`cbSize: 80`) resolves
                // to the matching declaration on the named class /
                // struct, so the editor shows the field's declared
                // type. Field-name spans come straight from the
                // parser; values still walk normally below.
                if let Some(info) = self.classes.get(class) {
                    for (i, (fname, _)) in fields.iter().enumerate() {
                        let Some(name_span) = field_name_spans.get(i) else {
                            continue;
                        };
                        let Some(m) = info.fields.get(fname) else {
                            continue;
                        };
                        let (target, no_def, uri) = member_target(
                            m,
                            info,
                            class.as_str(),
                            self.external_sources,
                            name_span.line,
                            name_span.col,
                        );
                        self.refs.push(RefEntry {
                            line: name_span.line,
                            start_col: name_span.col,
                            end_col: name_span.col + fname.as_str().len() as u32,
                            target_span: target,
                            target_name_len: fname.as_str().len() as u32,
                            signature: m.signature.clone(),
                            no_definition: no_def,
                            target_uri: uri,
                            doc: m.doc.clone(),
                        });
                    }
                }
                for (_, x) in fields {
                    self.walk_expr(x, scope, this_class);
                }
            }
            ExprKind::MapLit(pairs) => {
                for (k, v) in pairs {
                    self.walk_expr(k, scope, this_class);
                    self.walk_expr(v, scope, this_class);
                }
            }
            ExprKind::Index { obj, index } => {
                self.walk_expr(obj, scope, this_class);
                self.walk_expr(index, scope, this_class);
            }
            ExprKind::Range { start, end, .. } => {
                if let Some(s) = start {
                    self.walk_expr(s, scope, this_class);
                }
                if let Some(e) = end {
                    self.walk_expr(e, scope, this_class);
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.walk_expr(scrutinee, scope, this_class);
                for arm in arms {
                    let depth = scope.len();
                    bind_pattern(&arm.pattern, scope);
                    self.walk_expr(&arm.body, scope, this_class);
                    scope.truncate(depth);
                }
            }
            ExprKind::SuperCall { args, .. } => {
                for a in args {
                    self.walk_expr(a, scope, this_class);
                }
            }
            _ => {}
        }
    }

    fn walk_expr_var(
        &mut self,
        name: &AstSymbol,
        span: Span,
        scope: &[Binding],
        this_class: Option<&str>,
    ) {
        if let Some(b) = scope.iter().rev().find(|b| b.name == name.as_str()) {
            let sig = b
                .override_signature
                .clone()
                .unwrap_or_else(|| b.kind.render(name.as_str(), b.ty.as_ref()));
            self.push_ref(name.as_str(), span, b.span, name.as_str().len() as u32, sig);
        } else if name.as_str().contains('.') {
            self.push_external_dotted_ref(name.as_str(), span);
        } else if let Some(m) = this_class.and_then(|c| self.classes.get(&AstSymbol::intern(c))).and_then(
            |info| {
                info.getters
                    .get(name)
                    .or_else(|| info.fields.get(name))
                    .or_else(|| info.methods.get(name))
            },
        ) {
            // Implicit-`this` member access inside a class method.
            self.push_ref(name.as_str(), span, m.span, name.as_str().len() as u32, m.signature.clone());
        } else if let Some(sym) = self.symbols.get(name) {
            // Top-level lets are registered in `symbols` with a bare
            // `let X` signature (collect_symbols can't see the
            // inferred type). The diag pre-pass fills in `var_types`
            // with the resolved type, so prefer that for the rendered
            // signature here.
            let sig = self
                .var_types
                .get(name)
                .map(|t| format!("let {name}: {t}"))
                .unwrap_or_else(|| sym.signature.clone());
            self.push_ref(
                name.as_str(),
                span,
                sym.span,
                sym.name.as_str().len() as u32,
                sig,
            );
        } else if let Some(sig) = self.external_signatures.get(name) {
            // Selectively-imported bare name (`use M { X }`). Source /
            // doc info was harvested under the bare key.
            let loc = self.external_sources.get(name);
            let target_uri = loc.and_then(|l| Url::from_file_path(&l.path).ok());
            let (target_span, target_name_len, no_def) = match loc {
                Some(l) if target_uri.is_some() => (l.span, l.name_len, false),
                _ => (span, name.as_str().len() as u32, target_uri.is_none()),
            };
            self.refs.push(RefEntry {
                line: span.line,
                start_col: span.col,
                end_col: span.col + name.as_str().len() as u32,
                target_span,
                target_name_len,
                signature: sig.clone(),
                no_definition: no_def,
                target_uri,
                doc: self.external_docs.get(name).cloned(),
            });
        }
    }

    fn walk_expr_field(
        &mut self,
        obj: &Expr,
        name: &AstSymbol,
        scope: &mut Vec<Binding>,
        this_class: Option<&str>,
    ) {
        self.walk_expr(obj, scope, this_class);
        // Built-in `.length` on string / array.
        if name == "length" {
            let prefix = match self.infer_expr(obj, scope) {
                Some(Type::Str) => Some("string".to_string()),
                Some(Type::Array { elem, .. }) => Some(format!("{elem}[]")),
                _ => None,
            };
            if let Some(prefix) = prefix {
                if let Some((line, col)) = locate_dot_name(self.text, obj.span, name.as_str()) {
                    self.refs.push(RefEntry {
                        line,
                        start_col: col,
                        end_col: col + name.as_str().len() as u32,
                        target_span: obj.span,
                        target_name_len: name.as_str().len() as u32,
                        signature: format!("(property) {prefix}.length: i64"),
                        no_definition: true,
                        target_uri: None,
                        doc: None,
                    });
                    return;
                }
            }
        }
        if let Some(class) = self.resolve_obj_class(obj, scope, this_class) {
            if let Some(info) = self.classes.get(&AstSymbol::intern(&class)) {
                if let Some(m) = info
                    .getters
                    .get(name)
                    .or_else(|| info.fields.get(name))
                    .or_else(|| info.methods.get(name))
                {
                    if let Some((line, col)) = locate_dot_name(self.text, obj.span, name.as_str()) {
                        let (target, no_def, uri) = member_target(
                            m,
                            info,
                            &class,
                            self.external_sources,
                            line,
                            col,
                        );
                        self.refs.push(RefEntry {
                            line,
                            start_col: col,
                            end_col: col + name.as_str().len() as u32,
                            target_span: target,
                            target_name_len: name.as_str().len() as u32,
                            signature: m.signature.clone(),
                            no_definition: no_def,
                            target_uri: uri,
                            doc: m.doc.clone(),
                        });
                    }
                }
            }
        }
        // Enum variant access: `EnumName.Variant` parses as a Field,
        // with `obj` resolving to a known external enum. Look up the
        // composite `EnumName.Variant` key in the external maps
        // (populated by `register_enum_variants*`) and push a ref so
        // hover / F12 land on the variant declaration.
        if let Some(obj_name) = enum_obj_name(obj) {
            let key = AstSymbol::intern(&format!("{obj_name}.{}", name));
            if let Some(sig) = self.external_signatures.get(&key).cloned() {
                if sig.starts_with("(variant)") {
                    if let Some((line, col)) =
                        locate_dot_name(self.text, obj.span, name.as_str())
                    {
                        let loc = self.external_sources.get(&key);
                        let target_uri = loc.and_then(|l| Url::from_file_path(&l.path).ok());
                        let (target_span, target_name_len, no_def) = match loc {
                            Some(l) if target_uri.is_some() => (l.span, l.name_len, false),
                            _ => (
                                Span::new(line, col),
                                name.as_str().len() as u32,
                                target_uri.is_none(),
                            ),
                        };
                        self.refs.push(RefEntry {
                            line,
                            start_col: col,
                            end_col: col + name.as_str().len() as u32,
                            target_span,
                            target_name_len,
                            signature: sig,
                            no_definition: no_def,
                            target_uri,
                            doc: self.external_docs.get(&key).cloned(),
                        });
                    }
                }
            }
        }
    }

    fn walk_expr_method_call(
        &mut self,
        obj: &Expr,
        method: &AstSymbol,
        args: &[Expr],
        scope: &mut Vec<Binding>,
        this_class: Option<&str>,
    ) {
        self.walk_expr(obj, scope, this_class);
        for a in args {
            self.walk_expr(a, scope, this_class);
        }
        // Built-in string / array / Map methods.
        let builtin = match self.infer_expr(obj, scope) {
            Some(Type::Str) => string_method_sig(method.as_str())
                .map(|s| (s, string_method_doc(method.as_str()))),
            Some(Type::Array { elem, .. }) => array_method_sig(method.as_str(), &elem)
                .map(|s| (s, array_method_doc(method.as_str()))),
            Some(Type::Generic(g)) if g.base.as_str() == "Map" && g.args.len() == 2 => {
                map_method_sig(method.as_str(), &g.args[0], &g.args[1])
                    .map(|s| (s, map_method_doc(method.as_str())))
            }
            // Numeric primitives + bool: `toString` etc. So `i64`
            // values hover with the same kind of popup that `string`
            // / array receivers get.
            Some(t) if t.is_numeric() || matches!(t, Type::Bool) => {
                primitive_method_sig(method.as_str(), &t)
                    .map(|s| (s, primitive_method_doc(method.as_str())))
            }
            _ => None,
        };
        if let Some((sig, doc_text)) = builtin {
            if let Some((line, col)) = locate_dot_name(self.text, obj.span, method.as_str()) {
                self.refs.push(RefEntry {
                    line,
                    start_col: col,
                    end_col: col + method.as_str().len() as u32,
                    target_span: obj.span,
                    target_name_len: method.as_str().len() as u32,
                    signature: sig,
                    no_definition: true,
                    target_uri: None,
                    doc: doc_text.map(|s| s.to_string()),
                });
                return;
            }
        }
        if let Some(class) = self.resolve_obj_class(obj, scope, this_class) {
            if let Some(info) = self.classes.get(&AstSymbol::intern(&class)) {
                if let Some(m) = info.methods.get(&AstSymbol::intern(method.as_str())) {
                    if let Some((line, col)) = locate_dot_name(self.text, obj.span, method.as_str()) {
                        let (target, no_def, uri) = member_target(
                            m,
                            info,
                            &class,
                            self.external_sources,
                            line,
                            col,
                        );
                        self.refs.push(RefEntry {
                            line,
                            start_col: col,
                            end_col: col + method.as_str().len() as u32,
                            target_span: target,
                            target_name_len: method.as_str().len() as u32,
                            signature: m.signature.clone(),
                            no_definition: no_def,
                            target_uri: uri,
                            doc: m.doc.clone(),
                        });
                    }
                }
            }
        }
    }

    fn walk_expr_call(
        &mut self,
        callee: &AstSymbol,
        args: &[Expr],
        span: Span,
        scope: &mut Vec<Binding>,
        this_class: Option<&str>,
    ) {
        if let Some(b) = scope.iter().rev().find(|b| b.name.as_str() == callee.as_str()) {
            let sig = b
                .override_signature
                .clone()
                .unwrap_or_else(|| b.kind.render(callee.as_str(), b.ty.as_ref()));
            self.push_ref(callee.as_str(), span, b.span, callee.as_str().len() as u32, sig);
        } else if let Some(m) = this_class
            .and_then(|c| self.classes.get(&AstSymbol::intern(c)))
            .and_then(|info| info.methods.get(&AstSymbol::intern(callee.as_str())))
        {
            // Implicit-`this` method call inside a class method.
            self.push_ref(
                callee.as_str(),
                span,
                m.span,
                callee.as_str().len() as u32,
                m.signature.clone(),
            );
        } else if let Some(sym) = self.symbols.get(callee) {
            self.push_ref(
                callee.as_str(),
                span,
                sym.span,
                sym.name.as_str().len() as u32,
                sym.signature.clone(),
            );
        } else if callee.as_str().contains('.') {
            self.push_external_dotted_ref(callee.as_str(), span);
        } else if let Some(sig) = ffi_helper_signature(callee.as_str()) {
            // Same use_span guard as push_ref — synthesised calls
            // (e.g. `cstrFromString` inside the @objc class desugar)
            // borrow nearby user spans.
            if text::text_at_span_starts_with(self.text, span, callee.as_str()) {
                self.refs.push(RefEntry {
                    line: span.line,
                    start_col: span.col,
                    end_col: span.col + callee.as_str().len() as u32,
                    target_span: span,
                    target_name_len: callee.as_str().len() as u32,
                    signature: sig.to_string(),
                    no_definition: true,
                    target_uri: None,
                    doc: None,
                });
            }
        }
        for a in args {
            self.walk_expr(a, scope, this_class);
        }
    }

    fn walk_expr_new(
        &mut self,
        class: &AstSymbol,
        args: &[Expr],
        span: Span,
        scope: &mut Vec<Binding>,
        this_class: Option<&str>,
    ) {
        let info = self.classes.get(class);
        let class_sig = info
            .map(|i| class_hover(class.as_str(), i))
            .unwrap_or_else(|| format!("class {class}"));
        // The `new` keyword span is at `span`; the class name sits
        // after `new ` so locate it explicitly. Without this, our ref
        // entries would land on the keyword (and the dotted-name
        // suffix wouldn't be found).
        //
        // When `new ClassName` isn't actually present in the source
        // (e.g. a synth `new NSUserActivity(...)` the @objc desugar
        // drops into the alloc method body), locate fails and the AST
        // span points at the user's declaration line — which would
        // hijack hover on unrelated tokens like the return-type colon.
        // Walk the args (still useful for nested type refs) then skip
        // the class ref entirely.
        let class_str = class.as_str();
        let Some(class_start) = locate_let_name_with_kw(
            self.text,
            span,
            "new",
            class_str.split('.').next().unwrap_or(class_str),
        ) else {
            for a in args {
                self.walk_expr(a, scope, this_class);
            }
            return;
        };
        // F12 jumps to init when there is one; otherwise to the class
        // declaration itself. `init_member` is `None` for classes
        // without a defined init.
        let init_member = info.and_then(|i| i.methods.get(&"init".into()));
        if let Some(dot) = class_str.find('.') {
            let prefix = &class_str[..dot];
            let suffix = &class_str[dot + 1..];
            let prefix_loc = self.external_sources.get(&AstSymbol::intern(prefix));
            let prefix_uri = prefix_loc.and_then(|l| Url::from_file_path(&l.path).ok());
            let (prefix_target_span, prefix_target_name_len, prefix_no_def) = match prefix_loc {
                Some(l) if prefix_uri.is_some() => (l.span, l.name_len, false),
                _ => (class_start, prefix.len() as u32, true),
            };
            self.refs.push(RefEntry {
                line: class_start.line,
                start_col: class_start.col,
                end_col: class_start.col + prefix.len() as u32,
                target_span: prefix_target_span,
                target_name_len: prefix_target_name_len,
                signature: format!("(module) {prefix}"),
                no_definition: prefix_no_def,
                target_uri: prefix_uri,
                doc: None,
            });
            if let Some((line, col)) = locate_dot_name(self.text, class_start, suffix) {
                let loc = self.external_sources.get(class);
                let target_uri = loc.and_then(|l| Url::from_file_path(&l.path).ok());
                let is_external = info.map(|i| i.external).unwrap_or(true);
                let (target_span, target_name_len, no_def) = match (init_member, is_external) {
                    (Some(im), false) => (im.span, suffix.len() as u32, false),
                    (Some(im), true) if target_uri.is_some() => {
                        (im.span, "init".len() as u32, false)
                    }
                    _ => match info {
                        Some(i) if !i.external => (i.decl_span, suffix.len() as u32, false),
                        _ => match loc {
                            Some(l) if target_uri.is_some() => (l.span, l.name_len, false),
                            _ => (class_start, suffix.len() as u32, target_uri.is_none()),
                        },
                    },
                };
                self.refs.push(RefEntry {
                    line,
                    start_col: col,
                    end_col: col + suffix.len() as u32,
                    target_span,
                    target_name_len,
                    signature: class_sig,
                    no_definition: no_def,
                    target_uri,
                    doc: init_member.and_then(|m| m.doc.clone()),
                });
            }
        } else if let Some(sym) = self.symbols.get(class) {
            let target_span = init_member.map(|m| m.span).unwrap_or(sym.span);
            self.refs.push(RefEntry {
                line: class_start.line,
                start_col: class_start.col,
                end_col: class_start.col + class.as_str().len() as u32,
                target_span,
                target_name_len: class.as_str().len() as u32,
                signature: class_sig,
                no_definition: false,
                target_uri: None,
                doc: init_member
                    .and_then(|m| m.doc.clone())
                    .or_else(|| sym.doc.clone()),
            });
        }
        for a in args {
            self.walk_expr(a, scope, this_class);
        }
    }

    fn walk_expr_enum_ctor(
        &mut self,
        enum_name: &AstSymbol,
        variant: &AstSymbol,
        args: &ilang_ast::CtorArgs,
        span: Span,
        scope: &mut Vec<Binding>,
        this_class: Option<&str>,
    ) {
        if let Some(sym) = self.symbols.get(enum_name) {
            self.push_ref(
                enum_name.as_str(),
                span,
                sym.span,
                sym.name.as_str().len() as u32,
                sym.signature.clone(),
            );
        }
        // Push a separate RefEntry for the variant name so hover / F12
        // work on `Enum.variant` at the variant half too. The
        // composite `Enum.variant` key is populated by
        // `register_enum_variants` for both buffer-local and
        // cross-module enums.
        let key = AstSymbol::intern(&format!("{}.{}", enum_name, variant));
        if let Some(sig) = self.external_signatures.get(&key).cloned() {
            if let Some((line, col)) = locate_dot_name(self.text, span, variant.as_str()) {
                let loc = self.external_sources.get(&key);
                let target_uri = loc.and_then(|l| Url::from_file_path(&l.path).ok());
                let (target_span, target_name_len, no_def) = match loc {
                    Some(l) if target_uri.is_some() => (l.span, l.name_len, false),
                    _ => (
                        Span::new(line, col),
                        variant.as_str().len() as u32,
                        target_uri.is_none(),
                    ),
                };
                self.refs.push(RefEntry {
                    line,
                    start_col: col,
                    end_col: col + variant.as_str().len() as u32,
                    target_span,
                    target_name_len,
                    signature: sig,
                    no_definition: no_def,
                    target_uri,
                    doc: self.external_docs.get(&key).cloned(),
                });
            }
        }
        match args {
            ilang_ast::CtorArgs::Tuple(es) => {
                for x in es {
                    self.walk_expr(x, scope, this_class);
                }
            }
            ilang_ast::CtorArgs::Struct(pairs) => {
                for (_, x) in pairs {
                    self.walk_expr(x, scope, this_class);
                }
            }
            ilang_ast::CtorArgs::Unit => {}
        }
    }

    /// Walker-aware variant of `infer_expr_type_with_scope` that can
    /// also resolve `Call(callee)` to the callee's declared return
    /// type and `MethodCall` to the resolved method's return type.
    /// `ClassName.staticMethod()` — parsed as a single dotted callee
    /// rather than `Class.method` MethodCall. Resolve through the
    /// class's `methods` table so chained calls like
    /// `Foo.alloc().init()` can infer past the first hop.
    fn infer_dotted_static_call(&self, callee: &str) -> Option<Type> {
        let (cls, m) = callee.rsplit_once('.')?;
        let info = self.classes.get(&AstSymbol::intern(cls))?;
        info.methods.get(&AstSymbol::intern(m))?.ret_ty.clone()
    }

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
            ExprKind::Call { callee, .. } => self
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
                .or_else(|| self.infer_dotted_static_call(callee.as_str())),
            ExprKind::MethodCall { obj, method, .. } => {
                if let Some(Type::Generic(g)) = self.infer_expr(obj, scope) {
                    if let Some(t) = infer_map_method_type(&g, method.as_str()) {
                        return Some(t);
                    }
                }
                let this_class = self.current_this_class.as_deref();
                let class = self.resolve_obj_class(obj, scope, this_class)?;
                let info = self.classes.get(&AstSymbol::intern(&class))?;
                info.methods.get(&AstSymbol::intern(method.as_str()))?.ret_ty.clone()
            }
            ExprKind::Field { obj, name } => {
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

    /// For a dotted name like `math.sqrt`, push a hover-only ref entry
    /// at the suffix position (`.sqrt`). Used for names brought in via
    /// `use module` — the loader resolves these to a full signature
    /// but we don't have file-level spans for F12.
    pub(crate) fn push_external_dotted_ref(&mut self, dotted: &str, receiver_span: Span) {
        let Some(sig) = self.external_signatures.get(&AstSymbol::intern(dotted)) else {
            return;
        };
        let segments: Vec<&str> = dotted.split('.').collect();
        if segments.len() < 2 {
            return;
        }
        // The AST may carry more segments than the source literally
        // wrote — `use std.math as math` aliases `math.abs(...)` to
        // the canonical `std.math.abs` callee, but only `math.abs`
        // shows in the buffer. Find which segment of the dotted
        // chain the buffer starts at by matching the identifier at
        // `receiver_span` against the segments; treat everything
        // before that as a logical prefix (skipped for refs).
        let source_head = crate::text::read_identifier_at(self.text, receiver_span);
        let start_idx = match source_head {
            Some(head) => segments.iter().position(|s| *s == head).unwrap_or(0),
            None => 0,
        };
        // Each ref produces one RefEntry per segment so hover on
        // each segment shows the right level of detail:
        //   * intermediate segments → `(module) <cumulative>` with
        //     doc pulled from the matching top-of-file `///` block
        //   * the last segment → the item's own signature + doc
        for i in start_idx..segments.len() {
            let seg = segments[i];
            let is_last = i + 1 == segments.len();
            let cumulative: String = segments[..=i].join(".");
            // For an intermediate segment we synthesise a module
            // hover; for the leaf we look up the actual item.
            let (signature, doc, loc, name_len_hint) = if is_last {
                let item_loc = self.external_sources.get(&AstSymbol::intern(dotted));
                let item_doc = self.external_docs.get(&AstSymbol::intern(dotted)).cloned();
                (sig.clone(), item_doc, item_loc, seg.len() as u32)
            } else {
                let mod_loc = self.external_sources.get(&AstSymbol::intern(&cumulative));
                let mod_doc = self.external_docs.get(&AstSymbol::intern(&cumulative)).cloned();
                (format!("(module) {cumulative}"), mod_doc, mod_loc, seg.len() as u32)
            };
            let target_uri = loc.and_then(|l| Url::from_file_path(&l.path).ok());
            let (target_span, target_name_len, no_def) = match loc {
                Some(l) if target_uri.is_some() => (l.span, l.name_len, false),
                _ => (receiver_span, name_len_hint, true),
            };
            // First *visible* segment sits at receiver_span.col;
            // later segments are placed by walking the source for
            // each remaining dotted suffix so their column matches
            // the literal buffer position.
            let (line, start_col) = if i == start_idx {
                (receiver_span.line, receiver_span.col)
            } else {
                let tail: String = segments[i..].join(".");
                let Some((l, c)) = locate_dot_name(self.text, receiver_span, &tail) else {
                    continue;
                };
                (l, c)
            };
            self.refs.push(RefEntry {
                line,
                start_col,
                end_col: start_col + seg.len() as u32,
                target_span,
                target_name_len,
                signature,
                no_definition: no_def,
                target_uri,
                doc,
            });
        }
    }

    pub(crate) fn push_decl(&mut self, name: &str, span: Span, signature: String) {
        self.push_decl_with_doc(name, span, signature, None);
    }

    pub(crate) fn push_decl_with_doc(
        &mut self,
        name: &str,
        span: Span,
        signature: String,
        doc: Option<String>,
    ) {
        // Synthesised desugar names (`__cached_sel`,
        // `__objc_b<line>c<col>_sel_cache`, the `_ilang_impl_<name>`
        // pair from the @objc subclass IMP rename, …) borrow user
        // source spans from the surrounding declaration, so their
        // refs hijack hover at unrelated tokens. Filter through
        // `is_synthesized_objc_helper` so every desugar-emitted name
        // is rejected uniformly, not just the `__`-prefixed subset.
        if crate::symbols::is_synthesized_objc_helper(name) {
            return;
        }
        self.refs.push(RefEntry {
            line: span.line,
            start_col: span.col,
            end_col: span.col + name.len() as u32,
            target_span: span,
            target_name_len: name.len() as u32,
            signature,
            no_definition: false,
            target_uri: None,
            doc,
        });
    }

    pub(crate) fn push_ref(
        &mut self,
        name: &str,
        use_span: Span,
        target_span: Span,
        target_name_len: u32,
        signature: String,
    ) {
        self.push_ref_with_doc(name, use_span, target_span, target_name_len, signature, None)
    }

    pub(crate) fn push_ref_with_doc(
        &mut self,
        name: &str,
        use_span: Span,
        target_span: Span,
        target_name_len: u32,
        signature: String,
        doc: Option<String>,
    ) {
        // Parser-synthesised calls (the `@objc class` desugar emits
        // a pile of `cstrFromString("ClassName")`, `__get_class(...)`,
        // etc.) reuse user spans so error messages stay anchored
        // somewhere sensible. They confuse hover though — without
        // this check, hovering on the class name picks up the
        // synthesised Call ref instead of `class ClassName`. Drop
        // any push whose use_span doesn't actually contain `name`
        // in the source text.
        if !text::text_at_span_starts_with(self.text, use_span, name) {
            return;
        }
        self.refs.push(RefEntry {
            line: use_span.line,
            start_col: use_span.col,
            end_col: use_span.col + name.len() as u32,
            target_span,
            target_name_len,
            signature,
            no_definition: false,
            target_uri: None,
            doc,
        });
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

/// `true` when this method came from a parser-side synthesis pass
/// (mainly the `@objc class` desugar's `__wrap_handle` /
/// `__bind_handle` / `__bind_handle_unowned` / `__super_…`
/// helpers) and should NOT be surfaced as a hover entry. These
/// methods carry the class-level span by construction — pushing
/// a decl for them would paint the class name itself with the
/// synthesised method's signature ("(static method)
/// SKNode.__wrap_handle(h: i64): SKNode" instead of "class
/// SKNode"). The `__` prefix combined with the span-equality
/// check keeps the screen for any user code that happens to use
/// double-underscore names with real spans of its own.
///
/// Also catches the auto-lift's synthesized `alloc` / `init` /
/// `register` trio (no `__` prefix but always sit at the class
/// declaration's span). Without this they'd register their decls
/// at the class name's coordinates and hovering on
/// `class InputScene : SKScene` would surface
/// `(static method) InputScene.register()` instead of the class
/// itself. User-written `alloc` / `init` / `register` keep their
/// real source spans, so they sail through unchanged.
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

fn is_parser_synth_helper(m: &FnDecl, class_span: Span) -> bool {
    if m.span != class_span {
        return false;
    }
    let n = m.name.as_str();
    n.starts_with("__") || matches!(n, "alloc" | "init" | "register" | "deinit")
}

/// `true` when `f` is a desugar-inserted field whose span borrows
/// the surrounding class's `class`-keyword span — the `handle` /
/// `__owns` pair from `@objc class` and the `current` /
/// `__async_promise` pair from the async state-machine desugar.
/// User-written fields point at their own name token, so the span
/// equality alone is a reliable discriminator. Without this filter
/// the synth fields hijack hover, document highlight, outline and
/// workspace symbol search at the `class` keyword.
pub(crate) fn is_parser_synth_field(f: &FieldDecl, class_span: Span) -> bool {
    f.span == class_span
}
