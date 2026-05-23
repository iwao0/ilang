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
        StmtKind::LetTuple { elems, value } => {
            walk_expr(cx, value, scope, this_class, out);
            // `let (a, b, ...) = expr` — infer the value's type and
            // render the tuple shape after the closing `)`. The
            // pattern's slot count narrows the hint when inference
            // gives us a shorter / longer tuple.
            if let Some(ty) = infer(cx, value, scope, this_class) {
                if let Type::Tuple(elems_ty) = &ty {
                    if elems_ty.len() == elems.len() {
                        push_destructure_hint(cx.text, s.span, b')', &ty, out);
                    }
                }
            }
            for slot in elems.iter() {
                if let Some(name) = slot {
                    scope.push(Binding {
                        name: name.as_str().to_string(),
                        ty:   None,
                    });
                }
            }
        }
        StmtKind::LetStruct { class, value, .. } => {
            walk_expr(cx, value, scope, this_class, out);
            // `let ClassName { f1, f2 } = expr` — the type is the
            // declared class. Show it as `: ClassName` after the
            // closing `}` so the user can confirm the rhs's static
            // type matches the destructure shape.
            let ty = Type::Object(*class);
            push_destructure_hint(cx.text, s.span, b'}', &ty, out);
        }
    }
}

/// Push an inlay hint just after the closing delimiter (`)` for
/// tuple destructures, `}` for struct destructures) on the
/// stmt's first line. Source scan looks forward from the `let`
/// keyword for the first matching close at depth 0 of the
/// destructure pattern.
fn push_destructure_hint(
    text: &str,
    stmt_span: Span,
    close: u8,
    ty: &Type,
    out: &mut Vec<InlayHint>,
) {
    let Some(off) = text::line_col_to_offset(text, stmt_span.line, stmt_span.col) else {
        return;
    };
    let bytes = text.as_bytes();
    let open = if close == b')' { b'(' } else { b'{' };
    let mut depth: i32 = 0;
    let mut found: Option<usize> = None;
    let mut i = off;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\n' {
            // Tuple / struct destructure is conventionally a
            // single line; bail if we overshoot.
            break;
        }
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                found = Some(i + 1);
                break;
            }
        }
        i += 1;
    }
    let Some(byte_pos) = found else { return };
    let Some((line, col)) = text::offset_to_line_col(text, byte_pos) else {
        return;
    };
    out.push(InlayHint {
        position: text::lsp_position(line, col),
        label: InlayHintLabel::String(format!(": {ty}")),
        kind: Some(InlayHintKind::TYPE),
        text_edits: None,
        tooltip: None,
        padding_left: Some(false),
        padding_right: Some(false),
        data: None,
    });
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
                push_literal_arg_hints_with_params(args, params, out);
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
    out.push(InlayHint {
        position: text::lsp_position(name_span.line, name_span.col + name.len() as u32),
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
    // 1. in-file top-level fn.
    if let Some(params) = cx.callees.get(&CalleeKey::Fn(callee.to_string())) {
        push_literal_arg_hints_with_params(args, params, out);
        return;
    }
    // 2. implicit-`this` method in a class body.
    if let Some(class) = this_class {
        let key = CalleeKey::Method {
            class: class.to_string(),
            name:  callee.to_string(),
        };
        if let Some(params) = cx.callees.get(&key) {
            push_literal_arg_hints_with_params(args, params, out);
            return;
        }
    }
    // 3. `Class.method` static dispatch through a dotted callee.
    if let Some((cls, m)) = callee.rsplit_once('.') {
        let key = CalleeKey::Method {
            class: cls.to_string(),
            name:  m.to_string(),
        };
        if let Some(params) = cx.callees.get(&key) {
            push_literal_arg_hints_with_params(args, params, out);
            return;
        }
    }
    // 4. Cross-file fallback: look up the callee in the LSP's
    //    external signatures table. The string carries the param
    //    list; parse the names out.
    if let Some(names) = lookup_external_param_names(cx, callee) {
        push_literal_arg_hints_with_names(args, &names, out);
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
        push_literal_arg_hints_with_params(args, params, out);
        return;
    }
    // Cross-file: try the class's MemberInfo signature.
    if let Some(info) = cx.doc.classes.get(&AstSymbol::intern(class)) {
        if let Some(m) = info.methods.get(&AstSymbol::intern(method)) {
            if let Some(names) = parse_param_names_from_signature(&m.signature) {
                push_literal_arg_hints_with_names(args, &names, out);
            }
        }
    }
}

/// Look `callee` up in `Doc.external_signatures` and pull the
/// param-name list out of the signature string. Used for hints on
/// calls to fns imported via `use module`.
fn lookup_external_param_names(cx: &Cx, callee: &str) -> Option<Vec<String>> {
    let key = AstSymbol::intern(callee);
    let sig = cx.doc.external_signatures.get(&key)?;
    parse_param_names_from_signature(sig)
}

/// Extract param names from a signature string. Handles both
/// `fn name(p1: T1, p2: T2): R` and `(method) Class.name(p1: T1,
/// p2: T2): R` shapes. Returns `None` when the string doesn't
/// look like a callable signature.
fn parse_param_names_from_signature(sig: &str) -> Option<Vec<String>> {
    let open = sig.find('(')?;
    let bytes = sig.as_bytes();
    let mut depth: i32 = 0;
    let mut close: Option<usize> = None;
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    let inside = &sig[open + 1..close];
    if inside.trim().is_empty() {
        return Some(Vec::new());
    }
    // Split on top-level `,` (no nested generics inside a param
    // type — ilang formats them with `<...>` which doesn't
    // overlap with our paren counter above, so a plain split
    // suffices).
    let mut out = Vec::new();
    let mut bracket_depth: i32 = 0;
    let mut start = 0usize;
    let bytes = inside.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'<' | b'(' | b'[' => bracket_depth += 1,
            b'>' | b')' | b']' => bracket_depth -= 1,
            b',' if bracket_depth == 0 => {
                push_param_name(&inside[start..i], &mut out);
                start = i + 1;
            }
            _ => {}
        }
    }
    push_param_name(&inside[start..], &mut out);
    Some(out)
}

fn push_param_name(chunk: &str, out: &mut Vec<String>) {
    let trimmed = chunk.trim();
    if trimmed.is_empty() {
        return;
    }
    let name = trimmed.split(':').next().unwrap_or(trimmed).trim();
    out.push(name.to_string());
}

fn push_literal_arg_hints_with_params(
    args: &[Expr],
    params: &[Param],
    out: &mut Vec<InlayHint>,
) {
    push_literal_arg_hints(args, params.iter().map(|p| p.name.as_str()), out);
}

fn push_literal_arg_hints_with_names(
    args: &[Expr],
    names: &[String],
    out: &mut Vec<InlayHint>,
) {
    push_literal_arg_hints(args, names.iter().map(|s| s.as_str()), out);
}

fn push_literal_arg_hints<'a, I>(args: &[Expr], names: I, out: &mut Vec<InlayHint>)
where
    I: IntoIterator<Item = &'a str>,
{
    let names: Vec<&str> = names.into_iter().collect();
    for (i, arg) in args.iter().enumerate() {
        if !is_literal_arg(&arg.kind) {
            continue;
        }
        let Some(name) = names.get(i) else { continue };
        out.push(InlayHint {
            position: text::lsp_position(arg.span.line, arg.span.col),
            label: InlayHintLabel::String(format!("{name}:")),
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
        ExprKind::Tuple(elems) => {
            let mut types = Vec::with_capacity(elems.len());
            for e in elems.iter() {
                types.push(infer(cx, e, scope, this_class)?);
            }
            Some(Type::Tuple(types.into_boxed_slice()))
        }
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
