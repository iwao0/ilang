//! Extracted from `main.rs`.
#![allow(unused_imports)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};



use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use ilang_ast::{
    Block, ClassDecl, EnumDecl, Expr, ExprKind, FnDecl, InterfaceDecl, Item, Param, Pattern,
    PatternBindings, PatternKind, Program, Span, Stmt, StmtKind, Symbol as AstSymbol, Type,
    VariantPayload,
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
}

impl<'a> Walker<'a> {
    /// Walk a `Type` at `start_span` (the first character of the
    /// type token in source) and push hover / F12 entries for each
    /// dotted `Type::Object` name. Suffixes like `[]`, `?`, `.weak`
    /// don't shift the type-name's start, so nested types inherit
    /// `start_span`.
    pub(crate) fn walk_type_at(&mut self, ty: &Type, start_span: Span) {
        match ty {
            Type::Object(name) => {
                if name.as_str().contains('.') {
                    self.push_external_dotted_ref(name.as_str(), start_span);
                } else if let Some(sym) = self.symbols.get(name) {
                    self.push_ref(
                        name.as_str(),
                        start_span,
                        sym.span,
                        name.as_str().len() as u32,
                        sym.signature.clone(),
                    );
                }
            }
            Type::Array { elem, .. } => self.walk_type_at(elem, start_span),
            Type::Optional(inner) => self.walk_type_at(inner, start_span),
            Type::Weak(inner) => self.walk_type_at(inner, start_span),
            Type::Generic(g) => {
                if g.base.as_str().contains('.') {
                    self.push_external_dotted_ref(g.base.as_str(), start_span);
                } else if let Some(sym) = self.symbols.get(&g.base) {
                    self.push_ref(
                        g.base.as_str(),
                        start_span,
                        sym.span,
                        g.base.as_str().len() as u32,
                        sym.signature.clone(),
                    );
                }
            }
            _ => {}
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
        self.walk_block(&f.body, &mut scope, this_class);
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
            self.push_decl_with_doc(
                m.name.as_str(),
                m.span,
                format!("{}(method) {}.{}", render_user_attrs(&m.attrs), c.name, fn_body(m)),
                text::extract_doc_above(self.text, m.span.line),
            );
            self.walk_fn(m, Some(c.name.as_str()));
        }
        for m in &c.static_methods {
            self.push_decl_with_doc(
                m.name.as_str(),
                m.span,
                format!(
                    "{}(static method) {}.{}",
                    render_user_attrs(&m.attrs),
                    c.name,
                    fn_body(m)
                ),
                text::extract_doc_above(self.text, m.span.line),
            );
            self.walk_fn(m, None);
        }
        for prop in &c.properties {
            // Treat the getter/setter body like a method body so locals
            // and `this.X` resolve normally.
            if let Some(g) = &prop.getter {
                self.walk_fn(g, Some(c.name.as_str()));
            }
            if let Some(s) = &prop.setter {
                self.walk_fn(s, Some(c.name.as_str()));
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
            ExprKind::Var(name) => {
                if let Some(b) = scope.iter().rev().find(|b| b.name == name.as_str()) {
                    let sig = b
                        .override_signature
                        .clone()
                        .unwrap_or_else(|| b.kind.render(name.as_str(), b.ty.as_ref()));
                    self.push_ref(name.as_str(), e.span, b.span, name.as_str().len() as u32, sig);
                } else if name.as_str().contains('.') {
                    self.push_external_dotted_ref(name.as_str(), e.span);
                } else if let Some(m) = this_class.and_then(|c| self.classes.get(&AstSymbol::intern(c))).and_then(
                    |info| {
                        info.getters
                            .get(name)
                            .or_else(|| info.fields.get(name))
                            .or_else(|| info.methods.get(name))
                    },
                ) {
                    // Implicit-`this` member access inside a class method.
                    self.push_ref(name.as_str(), e.span, m.span, name.as_str().len() as u32, m.signature.clone());
                } else if let Some(sym) = self.symbols.get(name) {
                    // Top-level lets are registered in `symbols` with
                    // a bare `let X` signature (collect_symbols can't
                    // see the inferred type). The diag pre-pass fills
                    // in `var_types` with the resolved type, so prefer
                    // that for the rendered signature here.
                    let sig = self
                        .var_types
                        .get(name)
                        .map(|t| format!("let {name}: {t}"))
                        .unwrap_or_else(|| sym.signature.clone());
                    self.push_ref(
                        name.as_str(),
                        e.span,
                        sym.span,
                        sym.name.as_str().len() as u32,
                        sig,
                    );
                } else if let Some(sig) = self.external_signatures.get(name) {
                    // Selectively-imported bare name (`use M { X }`).
                    // Source / doc info was harvested under the bare key.
                    let loc = self.external_sources.get(name);
                    let target_uri = loc
                        .and_then(|l| Url::from_file_path(&l.path).ok());
                    let (target_span, target_name_len, no_def) = match loc {
                        Some(l) if target_uri.is_some() => (l.span, l.name_len, false),
                        _ => (e.span, name.as_str().len() as u32, target_uri.is_none()),
                    };
                    self.refs.push(RefEntry {
                        line: e.span.line,
                        start_col: e.span.col,
                        end_col: e.span.col + name.as_str().len() as u32,
                        target_span,
                        target_name_len,
                        signature: sig.clone(),
                        no_definition: no_def,
                        target_uri,
                        doc: self.external_docs.get(name).cloned(),
                    });
                }
            }
            ExprKind::This => {
                if let Some(c) = this_class {
                    if let Some(info) = self.classes.get(&AstSymbol::intern(c)) {
                        // `this` is 4 chars; e.span points at it.
                        self.push_ref("this", e.span, info.decl_span, c.len() as u32, format!("this: {c}"));
                    }
                }
            }
            ExprKind::Field { obj, name } => {
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
                // Enum variant access: `EnumName.Variant` parses as a
                // Field, with `obj` resolving to a known external enum.
                // Look up the composite `EnumName.Variant` key in the
                // external maps (populated by `register_enum_variants*`)
                // and push a ref so hover / F12 land on the variant
                // declaration.
                if let Some(obj_name) = enum_obj_name(obj) {
                    let key = AstSymbol::intern(&format!("{obj_name}.{}", name));
                    if let Some(sig) = self.external_signatures.get(&key).cloned() {
                        if sig.starts_with("(variant)") {
                            if let Some((line, col)) =
                                locate_dot_name(self.text, obj.span, name.as_str())
                            {
                                let loc = self.external_sources.get(&key);
                                let target_uri = loc
                                    .and_then(|l| Url::from_file_path(&l.path).ok());
                                let (target_span, target_name_len, no_def) = match loc {
                                    Some(l) if target_uri.is_some() => {
                                        (l.span, l.name_len, false)
                                    }
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
            ExprKind::MethodCall { obj, method, args } => {
                self.walk_expr(obj, scope, this_class);
                for a in args {
                    self.walk_expr(a, scope, this_class);
                }
                // Built-in string / array methods.
                let builtin = match self.infer_expr(obj, scope) {
                    Some(Type::Str) => string_method_sig(method.as_str())
                        .map(|s| (s, string_method_doc(method.as_str()))),
                    Some(Type::Array { elem, .. }) => array_method_sig(method.as_str(), &elem)
                        .map(|s| (s, array_method_doc(method.as_str()))),
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
                            if let Some((line, col)) = locate_dot_name(self.text, obj.span, method.as_str())
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
            ExprKind::Call { callee, args } => {
                if let Some(b) = scope.iter().rev().find(|b| b.name.as_str() == callee.as_str()) {
                    let sig = b
                        .override_signature
                        .clone()
                        .unwrap_or_else(|| b.kind.render(callee.as_str(), b.ty.as_ref()));
                    self.push_ref(callee.as_str(), e.span, b.span, callee.as_str().len() as u32, sig);
                } else if let Some(m) = this_class
                    .and_then(|c| self.classes.get(&AstSymbol::intern(c)))
                    .and_then(|info| info.methods.get(&AstSymbol::intern(callee.as_str())))
                {
                    // Implicit-`this` method call inside a class method.
                    self.push_ref(
                        callee.as_str(),
                        e.span,
                        m.span,
                        callee.as_str().len() as u32,
                        m.signature.clone(),
                    );
                } else if let Some(sym) = self.symbols.get(callee) {
                    self.push_ref(
                        callee.as_str(),
                        e.span,
                        sym.span,
                        sym.name.as_str().len() as u32,
                        sym.signature.clone(),
                    );
                } else if callee.as_str().contains('.') {
                    self.push_external_dotted_ref(callee.as_str(), e.span);
                } else if let Some(sig) = ffi_helper_signature(callee.as_str()) {
                    // Same use_span guard as push_ref — synthesised
                    // calls (e.g. `cstrFromString` inside the @objc
                    // class desugar) borrow nearby user spans.
                    if text::text_at_span_starts_with(self.text, e.span, callee.as_str()) {
                        self.refs.push(RefEntry {
                            line: e.span.line,
                            start_col: e.span.col,
                            end_col: e.span.col + callee.as_str().len() as u32,
                            target_span: e.span,
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
            ExprKind::New { class, args, .. } => {
                let info = self.classes.get(class);
                let class_sig = info
                    .map(|i| class_hover(class.as_str(), i))
                    .unwrap_or_else(|| format!("class {class}"));
                // The `new` keyword span is at e.span; the class name
                // sits after `new ` so locate it explicitly. Without
                // this, our ref entries would land on the keyword
                // (and the dotted-name suffix wouldn't be found).
                let class_str = class.as_str();
                let class_start = locate_let_name_with_kw(
                    self.text,
                    e.span,
                    "new",
                    class_str.split('.').next().unwrap_or(class_str),
                )
                .unwrap_or(e.span);
                // F12 jumps to init when there is one; otherwise to the
                // class declaration itself. `init_member` is `None` for
                // classes without a defined init.
                let init_member = info.and_then(|i| i.methods.get(&"init".into()));
                if let Some(dot) = class_str.find('.') {
                    let prefix = &class_str[..dot];
                    let suffix = &class_str[dot + 1..];
                    let prefix_loc = self.external_sources.get(&AstSymbol::intern(prefix));
                    let prefix_uri = prefix_loc
                        .and_then(|l| Url::from_file_path(&l.path).ok());
                    let (prefix_target_span, prefix_target_name_len, prefix_no_def) =
                        match prefix_loc {
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
                        let target_uri = loc
                            .and_then(|l| Url::from_file_path(&l.path).ok());
                        let is_external = info.map(|i| i.external).unwrap_or(true);
                        let (target_span, target_name_len, no_def) = match (init_member, is_external) {
                            (Some(im), false) => (im.span, suffix.len() as u32, false),
                            (Some(im), true) if target_uri.is_some() => {
                                (im.span, "init".len() as u32, false)
                            }
                            _ => match info {
                                Some(i) if !i.external => {
                                    (i.decl_span, suffix.len() as u32, false)
                                }
                                _ => match loc {
                                    Some(l) if target_uri.is_some() => {
                                        (l.span, l.name_len, false)
                                    }
                                    _ => {
                                        (class_start, suffix.len() as u32, target_uri.is_none())
                                    }
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
            ExprKind::EnumCtor { enum_name, variant, args } => {
                if let Some(sym) = self.symbols.get(enum_name) {
                    self.push_ref(
                        enum_name.as_str(),
                        e.span,
                        sym.span,
                        sym.name.as_str().len() as u32,
                        sym.signature.clone(),
                    );
                }
                // Push a separate RefEntry for the variant name so
                // hover / F12 work on `Enum.variant` at the variant
                // half too. The composite `Enum.variant` key is
                // populated by `register_enum_variants` for both
                // buffer-local and cross-module enums.
                let key = AstSymbol::intern(&format!(
                    "{}.{}", enum_name, variant
                ));
                if let Some(sig) = self.external_signatures.get(&key).cloned() {
                    if let Some((line, col)) =
                        locate_dot_name(self.text, e.span, variant.as_str())
                    {
                        let loc = self.external_sources.get(&key);
                        let target_uri = loc
                            .and_then(|l| Url::from_file_path(&l.path).ok());
                        let (target_span, target_name_len, no_def) = match loc {
                            Some(l) if target_uri.is_some() => {
                                (l.span, l.name_len, false)
                            }
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
            ExprKind::StructLit { fields, .. } => {
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

    /// Walker-aware variant of `infer_expr_type_with_scope` that can
    /// also resolve `Call(callee)` to the callee's declared return
    /// type and `MethodCall` to the resolved method's return type.
    pub(crate) fn infer_expr(&self, e: &Expr, scope: &[Binding]) -> Option<Type> {
        match &e.kind {
            ExprKind::Var(name) => {
                // Locals shadow consts — try scope first, then the
                // module-level const map.
                if let Some(b) = scope.iter().rev().find(|b| b.name == name.as_str())
                {
                    return b.ty.clone();
                }
                self.consts.get(name).cloned()
            }
            ExprKind::Call { callee, .. } => self
                .fn_returns
                .get(callee)
                .or_else(|| self.external_returns.get(callee))
                .cloned()
                .or_else(|| {
                    // `ClassName.staticMethod()` — parsed as a single
                    // dotted callee, not as MethodCall. Resolve through
                    // the class's `methods` table so chained calls
                    // like `Foo.alloc().init()` can infer past the
                    // first hop.
                    let (cls, m) = callee.as_str().rsplit_once('.')?;
                    let info = self.classes.get(&AstSymbol::intern(cls))?;
                    info.methods.get(&AstSymbol::intern(m))?.ret_ty.clone()
                }),
            ExprKind::MethodCall { obj, method, .. } => {
                let class = self.resolve_obj_class(obj, scope, None)?;
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
                if let Some(class) = self.resolve_obj_class(obj, scope, None) {
                    if let Some(info) = self.classes.get(&AstSymbol::intern(&class)) {
                        if let Some(t) = info.fields.get(name).and_then(|f| f.ret_ty.clone()) {
                            return Some(t);
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
            ExprKind::Block(b) => b.tail.as_ref().and_then(|t| self.infer_expr(t, scope)),
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
        let Some(dot) = dotted.find('.') else {
            return;
        };
        let prefix = &dotted[..dot];
        let suffix = &dotted[dot + 1..];
        // Hover at the receiver name itself (e.g. `math` in `math.sqrt`).
        // The Call/Var AST span points at the start of the dotted form.
        // F12 on the prefix navigates to the start of the module file.
        let prefix_loc = self.external_sources.get(&AstSymbol::intern(prefix));
        let prefix_uri = prefix_loc
            .and_then(|l| Url::from_file_path(&l.path).ok());
        let (prefix_target_span, prefix_target_name_len, prefix_no_def) = match prefix_loc {
            Some(l) if prefix_uri.is_some() => (l.span, l.name_len, false),
            _ => (receiver_span, prefix.len() as u32, true),
        };
        self.refs.push(RefEntry {
            line: receiver_span.line,
            start_col: receiver_span.col,
            end_col: receiver_span.col + prefix.len() as u32,
            target_span: prefix_target_span,
            target_name_len: prefix_target_name_len,
            signature: format!("(module) {prefix}"),
            no_definition: prefix_no_def,
            target_uri: prefix_uri,
            doc: None,
        });
        if let Some((line, col)) = locate_dot_name(self.text, receiver_span, suffix) {
            // F12 on the suffix (e.g. `.sqrt` in `math.sqrt`) navigates
            // to the actual decl line in the source file when we know
            // it; otherwise hover-only.
            let loc = self.external_sources.get(&AstSymbol::intern(dotted));
            let target_uri = loc
                .and_then(|l| Url::from_file_path(&l.path).ok());
            let (target_span, target_name_len) = match loc {
                Some(l) if target_uri.is_some() => (l.span, l.name_len),
                _ => (receiver_span, suffix.len() as u32),
            };
            self.refs.push(RefEntry {
                line,
                start_col: col,
                end_col: col + suffix.len() as u32,
                target_span,
                target_name_len,
                signature: sig.clone(),
                no_definition: target_uri.is_none(),
                target_uri,
                doc: self.external_docs.get(&AstSymbol::intern(dotted)).cloned(),
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
            doc: None,
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
