//! `walk_expr` and its per-`ExprKind` sub-handlers. Each handler is
//! responsible for pushing the hover / F12 ref entries that the
//! expression should surface, then recursing into its child
//! expressions. Control-flow shapes (`if` / `while` / `for-in` /
//! `match` / closures) also manage their own scope push / pop here.

use super::*;

impl<'a> Walker<'a> {
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
                    text::locate_if_let_some_name_at(self.line_starts, self.text, e.span, name.as_str()).unwrap_or(e.span);
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
                            if let Some((line, col)) = text::locate_dot_name_at(self.line_starts, self.text, obj.span, field.as_str())
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
                if let Some((line, col)) = text::locate_dot_name_at(self.line_starts, self.text, obj.span, name.as_str()) {
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
                    if let Some((line, col)) = text::locate_dot_name_at(self.line_starts, self.text, obj.span, name.as_str()) {
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
                        text::locate_dot_name_at(self.line_starts, self.text, obj.span, name.as_str())
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
            Some(Type::Generic(g)) if g.base.as_str() == "Set" && g.args.len() == 1 => {
                set_method_sig(method.as_str(), &g.args[0])
                    .map(|s| (s, set_method_doc(method.as_str())))
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
            if let Some((line, col)) = text::locate_dot_name_at(self.line_starts, self.text, obj.span, method.as_str()) {
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
                    if let Some((line, col)) = text::locate_dot_name_at(self.line_starts, self.text, obj.span, method.as_str()) {
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
            if text::text_at_span_starts_with_at(self.line_starts, self.text, span, callee.as_str()) {
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
        let Some(class_start) = text::locate_let_name_with_kw_at(
            self.line_starts,
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
            if let Some((line, col)) = text::locate_dot_name_at(self.line_starts, self.text, class_start, suffix) {
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
            if let Some((line, col)) = text::locate_dot_name_at(self.line_starts, self.text, span, variant.as_str()) {
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
}
