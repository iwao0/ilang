//! `walk_block` / `walk_stmt` — drive the per-statement scope push /
//! pop and forward each statement's RHS to [`walk_expr`]. The let
//! bindings here are what populate `var_classes` / `var_types` so
//! completion's receiver-dot lookups can find them later.

use super::*;

impl<'a> Walker<'a> {
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
                let name_span = text::locate_let_name_at(self.line_starts, self.text, s.span, name.as_str()).unwrap_or(s.span);
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
}
