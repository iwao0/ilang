//! Top-level item walks: `walk_fn`, `walk_class`, `walk_interface`,
//! plus `walk_fn_header_type_refs` for the @objc desugar's getter /
//! setter wrappers. Also houses the parser-synth filters
//! (`is_parser_synth_helper`, `is_parser_synth_field`) used by
//! `walk_class` and a few sibling modules.

use super::*;

impl<'a> Walker<'a> {
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
