//! `textDocument/inlayHint` provider.
//!
//! Two hint families:
//!
//!   - **Type hints** after `let x = expr` / `for x in iter` /
//!     destructuring binders that don't carry an explicit
//!     annotation. The inferred type is rendered as `: T`.
//!   - **Parameter-name hints** at literal call arguments
//!     (numbers, strings, bools, none/some, array literals). The
//!     hint is rendered as `name:` immediately before the literal.
//!     Limited to literals so identifier arguments (which already
//!     name the value clearly) don't clutter the editor.
//!
//! Both passes re-parse the buffer text and walk the AST locally;
//! cross-file inference reuses the LSP's existing `Doc` data for
//! fn return types and class member lookups.

use ilang_ast::{
    Block, ClassDecl, Expr, ExprKind, FnDecl, Item, Param, Pattern, PatternBindings,
    PatternKind, Program, Span, Stmt, StmtKind, Symbol as AstSymbol, Type,
};
use ilang_lexer::tokenize;
use ilang_parser::parse;
use std::collections::HashMap;
use tower_lsp::lsp_types::*;

use crate::text;
use crate::types::Doc;

/// Compute inlay hints for `doc`'s buffer text within the visible
/// `range`. Hints outside the range are dropped client-side anyway,
/// but pre-filtering keeps the response small for big files.
pub(crate) fn build_hints(doc: &Doc, range: Range) -> Vec<InlayHint> {
    let text = &doc.text;
    let Ok(tokens) = tokenize(text) else { return Vec::new() };
    let Ok(prog) = parse(&tokens) else { return Vec::new() };

    // Build a per-file callee table for parameter-hint resolution.
    // Cross-file calls fall through (no hint emitted).
    let callees = build_callee_table(&prog);

    let mut out: Vec<InlayHint> = Vec::new();
    let cx = Cx {
        text,
        doc,
        callees: &callees,
    };
    for item in &prog.items {
        walk_item_for_hints(&cx, item, None, &mut out);
    }
    out.retain(|h| in_range(h.position, &range));
    out
}

struct Cx<'a> {
    text:    &'a str,
    doc:     &'a Doc,
    callees: &'a CalleeTable,
}

type CalleeTable = HashMap<CalleeKey, Vec<Param>>;

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
enum CalleeKey {
    /// Bare top-level fn (`fn foo(a, b) { … }`).
    Fn(String),
    /// Class method (`class C { fn m(a, b) { … } }`).
    Method { class: String, name: String },
    /// `init` overloads (matched by arity since names collide).
    Init { class: String, arity: usize },
}

fn build_callee_table(prog: &Program) -> CalleeTable {
    let mut out: CalleeTable = HashMap::new();
    for item in &prog.items {
        match item {
            Item::Fn(f) => {
                out.entry(CalleeKey::Fn(f.name.as_str().to_string()))
                    .or_insert_with(|| f.params.to_vec());
            }
            Item::Class(c) => collect_class_callees(c, &mut out),
            Item::ExternC(b) => {
                for inner in b.items.iter() {
                    match inner {
                        ilang_ast::ExternCItem::FnDef(f) => {
                            out.entry(CalleeKey::Fn(f.name.as_str().to_string()))
                                .or_insert_with(|| f.params.to_vec());
                        }
                        ilang_ast::ExternCItem::FnDecl { name, params, .. } => {
                            out.entry(CalleeKey::Fn(name.as_str().to_string()))
                                .or_insert_with(|| params.to_vec());
                        }
                        ilang_ast::ExternCItem::Class(c) => {
                            collect_class_callees(c, &mut out);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn collect_class_callees(c: &ClassDecl, out: &mut CalleeTable) {
    let cname = c.name.as_str().to_string();
    for m in c.methods.iter() {
        if m.name.as_str() == "init" {
            out.entry(CalleeKey::Init {
                class: cname.clone(),
                arity: m.params.len(),
            })
            .or_insert_with(|| m.params.to_vec());
        } else {
            out.entry(CalleeKey::Method {
                class: cname.clone(),
                name: m.name.as_str().to_string(),
            })
            .or_insert_with(|| m.params.to_vec());
        }
    }
    for m in c.static_methods.iter() {
        out.entry(CalleeKey::Method {
            class: cname.clone(),
            name: m.name.as_str().to_string(),
        })
        .or_insert_with(|| m.params.to_vec());
    }
}

fn walk_item_for_hints(
    cx: &Cx,
    item: &Item,
    this_class: Option<&str>,
    out: &mut Vec<InlayHint>,
) {
    match item {
        Item::Fn(f) => walk_fn(cx, f, this_class, out),
        Item::Class(c) => {
            let cname = c.name.as_str();
            for m in c.methods.iter() {
                walk_fn(cx, m, Some(cname), out);
            }
            for m in c.static_methods.iter() {
                walk_fn(cx, m, Some(cname), out);
            }
        }
        Item::ExternC(b) => {
            for inner in b.items.iter() {
                if let ilang_ast::ExternCItem::FnDef(f) = inner {
                    walk_fn(cx, f, this_class, out);
                }
                if let ilang_ast::ExternCItem::Class(c) = inner {
                    let cname = c.name.as_str();
                    for m in c.methods.iter() {
                        walk_fn(cx, m, Some(cname), out);
                    }
                    for m in c.static_methods.iter() {
                        walk_fn(cx, m, Some(cname), out);
                    }
                }
            }
        }
        _ => {}
    }
}

fn walk_fn(
    cx: &Cx,
    f: &FnDecl,
    this_class: Option<&str>,
    out: &mut Vec<InlayHint>,
) {
    // Param types are already on the decl — no inference hint needed
    // for them. Walk the body to surface let / for-binder types and
    // call-site parameter names.
    let mut scope: Vec<Binding> = Vec::new();
    for p in f.params.iter() {
        scope.push(Binding {
            name: p.name.as_str().to_string(),
            ty:   Some(p.ty.clone()),
        });
    }
    walk_block(cx, &f.body, &mut scope, this_class, out);
}

#[derive(Clone)]
struct Binding {
    name: String,
    ty:   Option<Type>,
}

fn walk_block(
    cx: &Cx,
    b: &Block,
    scope: &mut Vec<Binding>,
    this_class: Option<&str>,
    out: &mut Vec<InlayHint>,
) {
    let base_len = scope.len();
    for s in b.stmts.iter() {
        walk_stmt(cx, s, scope, this_class, out);
    }
    if let Some(t) = &b.tail {
        walk_expr(cx, t, scope, this_class, out);
    }
    scope.truncate(base_len);
}

fn walk_stmt(
    cx: &Cx,
    s: &Stmt,
    scope: &mut Vec<Binding>,
    this_class: Option<&str>,
    out: &mut Vec<InlayHint>,
) {
    match &s.kind {
        StmtKind::Let { name, ty, value, .. } => {
            walk_expr(cx, value, scope, this_class, out);
            let inferred = ty.clone().or_else(|| infer(cx, value, scope, this_class));
            // Don't annotate the discard binding `_` — it's already
            // an explicit "ignore this value" marker; the type hint
            // is just noise.
            if ty.is_none() && name.as_str() != "_" {
                if let Some(t) = &inferred {
                    push_type_hint(cx.text, s.span, "let", name.as_str(), t, out);
                }
            }
            scope.push(Binding {
                name: name.as_str().to_string(),
                ty:   inferred,
            });
        }
        StmtKind::Expr(e) => walk_expr(cx, e, scope, this_class, out),
        // LetTuple / LetStruct destructuring — punt for v1; the
        // inferred element types are awkward to render inline.
        StmtKind::LetTuple { value, .. } | StmtKind::LetStruct { value, .. } => {
            walk_expr(cx, value, scope, this_class, out);
        }
    }
}

fn walk_expr(
    cx: &Cx,
    e: &Expr,
    scope: &mut Vec<Binding>,
    this_class: Option<&str>,
    out: &mut Vec<InlayHint>,
) {
    match &e.kind {
        ExprKind::Call { callee, args } => {
            for a in args.iter() {
                walk_expr(cx, a, scope, this_class, out);
            }
            push_param_hints_for_call(cx, callee.as_str(), this_class, args, out);
        }
        ExprKind::MethodCall { obj, method, args } => {
            walk_expr(cx, obj, scope, this_class, out);
            for a in args.iter() {
                walk_expr(cx, a, scope, this_class, out);
            }
            if let Some(class) = resolve_obj_class(cx, obj, scope, this_class) {
                push_param_hints_for_method(cx, &class, method.as_str(), args, out);
            }
        }
        ExprKind::New { class, args, .. } => {
            for a in args.iter() {
                walk_expr(cx, a, scope, this_class, out);
            }
            let key = CalleeKey::Init {
                class: class.as_str().to_string(),
                arity: args.len(),
            };
            if let Some(params) = cx.callees.get(&key) {
                push_literal_arg_hints(args, params, out);
            }
        }
        ExprKind::If { cond, then_branch, else_branch, .. } => {
            walk_expr(cx, cond, scope, this_class, out);
            walk_block(cx, then_branch, scope, this_class, out);
            if let Some(e) = else_branch {
                walk_expr(cx, e, scope, this_class, out);
            }
        }
        ExprKind::IfLet { expr: value, then_branch, else_branch, .. } => {
            walk_expr(cx, value, scope, this_class, out);
            // `if let some(x) = ...` introduces a local; we don't try
            // to type it here, but the recursive walk should still
            // see other bindings inside the branch.
            walk_block(cx, then_branch, scope, this_class, out);
            if let Some(e) = else_branch {
                walk_expr(cx, e, scope, this_class, out);
            }
        }
        ExprKind::While { cond, body } => {
            walk_expr(cx, cond, scope, this_class, out);
            walk_block(cx, body, scope, this_class, out);
        }
        ExprKind::Loop { body } => walk_block(cx, body, scope, this_class, out),
        ExprKind::Block(b) => walk_block(cx, b, scope, this_class, out),
        ExprKind::Match { scrutinee, arms } => {
            walk_expr(cx, scrutinee, scope, this_class, out);
            for arm in arms.iter() {
                let base = scope.len();
                introduce_pattern_bindings(&arm.pattern, scope);
                walk_expr(cx, &arm.body, scope, this_class, out);
                scope.truncate(base);
            }
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            walk_expr(cx, lhs, scope, this_class, out);
            walk_expr(cx, rhs, scope, this_class, out);
        }
        ExprKind::Unary { expr, .. } => walk_expr(cx, expr, scope, this_class, out),
        ExprKind::Cast { expr, .. } => walk_expr(cx, expr, scope, this_class, out),
        ExprKind::Field { obj, .. } => walk_expr(cx, obj, scope, this_class, out),
        ExprKind::Index { obj, index } => {
            walk_expr(cx, obj, scope, this_class, out);
            walk_expr(cx, index, scope, this_class, out);
        }
        ExprKind::Array(elems) => {
            for e in elems.iter() {
                walk_expr(cx, e, scope, this_class, out);
            }
        }
        ExprKind::ForIn { var, iter, body } => {
            walk_expr(cx, iter, scope, this_class, out);
            let iter_ty = infer(cx, iter, scope, this_class);
            let elem_ty = iter_ty.as_ref().and_then(|t| match t {
                Type::Array { elem, .. } => Some((**elem).clone()),
                _ => None,
            });
            if let Some(t) = &elem_ty {
                push_type_hint(cx.text, e.span, "for", var.as_str(), t, out);
            }
            scope.push(Binding {
                name: var.as_str().to_string(),
                ty:   elem_ty,
            });
            walk_block(cx, body, scope, this_class, out);
            scope.pop();
        }
        ExprKind::Assign { target: _, value } => {
            walk_expr(cx, value, scope, this_class, out);
        }
        ExprKind::AssignField { obj, value, .. } => {
            walk_expr(cx, obj, scope, this_class, out);
            walk_expr(cx, value, scope, this_class, out);
        }
        ExprKind::AssignIndex { obj, index, value } => {
            walk_expr(cx, obj, scope, this_class, out);
            walk_expr(cx, index, scope, this_class, out);
            walk_expr(cx, value, scope, this_class, out);
        }
        ExprKind::Return(Some(e)) | ExprKind::Await(e) | ExprKind::Some(e) => {
            walk_expr(cx, e, scope, this_class, out);
        }
        ExprKind::FnExpr { body, .. } => walk_block(cx, body, scope, this_class, out),
        _ => {}
    }
}

fn introduce_pattern_bindings(p: &Pattern, scope: &mut Vec<Binding>) {
    if let PatternKind::Variant { bindings, .. } = &p.kind {
        match bindings {
            PatternBindings::Tuple(names) => {
                for n in names.iter() {
                    scope.push(Binding {
                        name: n.as_str().to_string(),
                        ty:   None,
                    });
                }
            }
            PatternBindings::Struct(fields) => {
                for (_, alias) in fields.iter() {
                    scope.push(Binding {
                        name: alias.as_str().to_string(),
                        ty:   None,
                    });
                }
            }
            PatternBindings::Unit => {}
        }
    }
}

fn push_type_hint(
    text: &str,
    decl_span: Span,
    kw: &str,
    name: &str,
    ty: &Type,
    out: &mut Vec<InlayHint>,
) {
    let Some(name_span) =
        text::locate_let_name_with_kw(text, decl_span, kw, name)
    else {
        return;
    };
    let line = name_span.line.saturating_sub(1);
    let character = name_span.col.saturating_sub(1).saturating_add(name.len() as u32);
    out.push(InlayHint {
        position: Position { line, character },
        label: InlayHintLabel::String(format!(": {ty}")),
        kind: Some(InlayHintKind::TYPE),
        text_edits: None,
        tooltip: None,
        padding_left: Some(false),
        padding_right: Some(false),
        data: None,
    });
}

fn push_param_hints_for_call(
    cx: &Cx,
    callee: &str,
    this_class: Option<&str>,
    args: &[Expr],
    out: &mut Vec<InlayHint>,
) {
    // Try bare-name fn, then implicit-`this` method when we have a
    // class context, then `Class.method` static dispatch.
    if let Some(params) = cx.callees.get(&CalleeKey::Fn(callee.to_string())) {
        push_literal_arg_hints(args, params, out);
        return;
    }
    if let Some(class) = this_class {
        let key = CalleeKey::Method {
            class: class.to_string(),
            name:  callee.to_string(),
        };
        if let Some(params) = cx.callees.get(&key) {
            push_literal_arg_hints(args, params, out);
            return;
        }
    }
    if let Some((cls, m)) = callee.rsplit_once('.') {
        let key = CalleeKey::Method {
            class: cls.to_string(),
            name:  m.to_string(),
        };
        if let Some(params) = cx.callees.get(&key) {
            push_literal_arg_hints(args, params, out);
        }
    }
}

fn push_param_hints_for_method(
    cx: &Cx,
    class: &str,
    method: &str,
    args: &[Expr],
    out: &mut Vec<InlayHint>,
) {
    let key = CalleeKey::Method {
        class: class.to_string(),
        name:  method.to_string(),
    };
    if let Some(params) = cx.callees.get(&key) {
        push_literal_arg_hints(args, params, out);
    }
}

fn push_literal_arg_hints(
    args: &[Expr],
    params: &[Param],
    out: &mut Vec<InlayHint>,
) {
    for (i, arg) in args.iter().enumerate() {
        if !is_literal_arg(&arg.kind) {
            continue;
        }
        let Some(p) = params.get(i) else { continue };
        let line = arg.span.line.saturating_sub(1);
        let character = arg.span.col.saturating_sub(1);
        out.push(InlayHint {
            position: Position { line, character },
            label: InlayHintLabel::String(format!("{}:", p.name)),
            kind: Some(InlayHintKind::PARAMETER),
            text_edits: None,
            tooltip: None,
            padding_left: Some(false),
            padding_right: Some(true),
            data: None,
        });
    }
}

/// `true` for argument shapes where the call site doesn't already
/// name the value clearly: literals (numbers / strings / bools /
/// none / array literals). Identifier / field / call arguments
/// already carry a name and would just clutter the hint stream.
fn is_literal_arg(k: &ExprKind) -> bool {
    matches!(
        k,
        ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Str(_)
            | ExprKind::Bool(_)
            | ExprKind::None
            | ExprKind::Array(_)
    ) || matches!(
        k,
        ExprKind::Unary { op: ilang_ast::UnOp::Neg, expr }
            if matches!(expr.kind, ExprKind::Int(_) | ExprKind::Float(_))
    )
}

/// Lightweight type inference for let / for-binder hints. Handles
/// the common shapes; unresolved expressions return `None` and the
/// hint is skipped.
fn infer(
    cx: &Cx,
    e: &Expr,
    scope: &[Binding],
    this_class: Option<&str>,
) -> Option<Type> {
    match &e.kind {
        ExprKind::Int(_) => Some(Type::I64),
        ExprKind::Float(_) => Some(Type::F64),
        ExprKind::Str(_) => Some(Type::Str),
        ExprKind::Bool(_) => Some(Type::Bool),
        ExprKind::None => None,
        ExprKind::Some(inner) => infer(cx, inner, scope, this_class)
            .map(|t| Type::Optional(Box::new(t))),
        ExprKind::New { class, .. } => Some(Type::Object(*class)),
        ExprKind::Var(name) => {
            if let Some(b) = scope.iter().rev().find(|b| b.name == name.as_str()) {
                return b.ty.clone();
            }
            cx.doc.var_types.get(name).cloned()
        }
        ExprKind::Call { callee, .. } => {
            cx.doc
                .external_returns
                .get(callee)
                .cloned()
                .or_else(|| {
                    let fn_key = CalleeKey::Fn(callee.as_str().to_string());
                    cx.callees.get(&fn_key);
                    None
                })
        }
        ExprKind::MethodCall { obj, method, .. } => {
            let cls = resolve_obj_class(cx, obj, scope, this_class)?;
            let info = cx.doc.classes.get(&AstSymbol::intern(&cls))?;
            info.methods
                .get(&AstSymbol::intern(method.as_str()))
                .and_then(|m| m.ret_ty.clone())
        }
        ExprKind::Field { obj, name } => {
            let cls = resolve_obj_class(cx, obj, scope, this_class)?;
            let info = cx.doc.classes.get(&AstSymbol::intern(&cls))?;
            info.fields
                .get(name)
                .or_else(|| info.getters.get(name))
                .and_then(|m| m.ret_ty.clone())
        }
        ExprKind::Cast { ty, .. } => Some(ty.clone()),
        ExprKind::Array(elems) => {
            let elem = infer(cx, elems.first()?, scope, this_class)?;
            Some(Type::Array {
                elem:  Box::new(elem),
                fixed: None,
            })
        }
        _ => None,
    }
}

fn resolve_obj_class(
    cx: &Cx,
    obj: &Expr,
    scope: &[Binding],
    this_class: Option<&str>,
) -> Option<String> {
    match &obj.kind {
        ExprKind::This => this_class.map(|s| s.to_string()),
        ExprKind::Var(name) => {
            if let Some(b) = scope.iter().rev().find(|b| b.name == name.as_str()) {
                return type_to_class(b.ty.as_ref()?);
            }
            cx.doc
                .var_types
                .get(name)
                .and_then(|t| type_to_class(t))
                .or_else(|| {
                    if cx.doc.classes.contains_key(name) {
                        Some(name.as_str().to_string())
                    } else {
                        None
                    }
                })
        }
        ExprKind::New { class, .. } => Some(class.as_str().to_string()),
        _ => infer(cx, obj, scope, this_class).and_then(|t| type_to_class(&t)),
    }
}

fn type_to_class(t: &Type) -> Option<String> {
    match t {
        Type::Object(s) => Some(s.as_str().to_string()),
        Type::Optional(inner) => type_to_class(inner),
        _ => None,
    }
}

fn in_range(p: Position, r: &Range) -> bool {
    let after_start = (p.line, p.character) >= (r.start.line, r.start.character);
    let before_end = (p.line, p.character) <= (r.end.line, r.end.character);
    after_start && before_end
}
