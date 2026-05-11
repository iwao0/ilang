//! LSP "code action" entry points:
//!
//! - `fill_match_arms_at`: cursor in a `match` whose scrutinee is an
//!   enum → emit one new arm per missing variant.
//! - `generate_init_at`: cursor inside a `class` body that has
//!   fields but no `init` → emit a constructor that takes one param
//!   per field and assigns each to `this.field`.
//!
//! Plus the shared `collect_matches_in_*` walker that records every
//! `match` expression's `{ ... }` byte range, used by
//! `fill_match_arms_at`.

use std::collections::{HashMap, HashSet};

use ilang_ast::{
    Block, ClassDecl, Expr, ExprKind, Item, PatternKind, Program, Span, StmtKind,
    Symbol as AstSymbol, Type, VariantPayload,
};
use tower_lsp::lsp_types::Position;

use super::infer_expr_type_with_scope;
use super::text;

/// Find an enclosing `match` expression at `cursor` and, when its
/// scrutinee resolves to an enum declared in `prog`, return the byte
/// offset just before the closing `}` along with the source text to
/// insert (one new arm per missing variant) and the count of arms
/// added. Returns `None` when no completion is needed (no match,
/// non-enum scrutinee, wildcard arm present, all variants covered,
/// or unresolvable enum).
pub(crate) fn fill_match_arms_at(
    text: &str,
    prog: &Program,
    var_types: &HashMap<AstSymbol, Type>,
    cursor: Position,
) -> Option<(usize, String, usize)> {
    // Build a flat list of (match_expr, brace_open_byte, brace_close_byte)
    // for every match in the file.
    let mut all: Vec<(&Expr, usize, usize)> = Vec::new();
    for item in &prog.items {
        if let Item::Fn(f) = item {
            collect_matches_in_block(&f.body, text, &mut all);
        }
        if let Item::Class(c) = item {
            for m in c.methods.iter() {
                collect_matches_in_block(&m.body, text, &mut all);
            }
        }
    }
    // Pick innermost match whose `{ ... }` contains the cursor.
    let cursor_byte =
        text::line_col_to_offset(text, cursor.line + 1, cursor.character + 1)?;
    let mut chosen: Option<(&Expr, usize, usize)> = None;
    for (e, lo, hi) in &all {
        if cursor_byte < *lo || cursor_byte > *hi {
            continue;
        }
        let span = (*hi).saturating_sub(*lo);
        match chosen {
            None => chosen = Some((*e, *lo, *hi)),
            Some((_, c_lo, c_hi)) => {
                if span < c_hi.saturating_sub(c_lo) {
                    chosen = Some((*e, *lo, *hi));
                }
            }
        }
    }
    let (mexpr, _open, close) = chosen?;
    let ExprKind::Match { scrutinee, arms } = &mexpr.kind else {
        return None;
    };
    // Bail if the user already has a wildcard arm — match is exhaustive.
    if arms
        .iter()
        .any(|a| matches!(a.pattern.kind, PatternKind::Wildcard))
    {
        return None;
    }
    let enum_name = scrutinee_enum_name(scrutinee, var_types)?;
    let edecl = prog.items.iter().find_map(|it| match it {
        Item::Enum(e) if e.name.as_str() == enum_name.as_str() => Some(e),
        _ => None,
    })?;
    // Variants already covered, by name.
    let mut covered: HashSet<String> = HashSet::new();
    for a in arms.iter() {
        if let PatternKind::Variant { variant, .. } = &a.pattern.kind {
            covered.insert(variant.as_str().to_string());
        }
    }
    let missing: Vec<&ilang_ast::Variant> = edecl
        .variants
        .iter()
        .filter(|v| !covered.contains(v.name.as_str()))
        .collect();
    if missing.is_empty() {
        return None;
    }
    // Indentation: copy the closing `}`'s line indent so each new
    // arm sits one level deeper.
    let close_line_start = {
        let bytes = text.as_bytes();
        let mut i = close;
        while i > 0 && bytes[i - 1] != b'\n' {
            i -= 1;
        }
        i
    };
    let base_indent: String = text[close_line_start..close]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    let arm_indent = format!("{base_indent}    ");
    let mut out = String::new();
    for v in &missing {
        out.push_str(&arm_indent);
        out.push_str(enum_name.as_str());
        out.push('.');
        out.push_str(v.name.as_str());
        match &v.payload {
            VariantPayload::Unit => {}
            VariantPayload::Tuple(elems) => {
                out.push('(');
                let placeholders: Vec<&str> =
                    elems.iter().map(|_| "_").collect();
                out.push_str(&placeholders.join(", "));
                out.push(')');
            }
            VariantPayload::Struct(fields) => {
                out.push_str(" { ");
                let names: Vec<&str> =
                    fields.iter().map(|f| f.name.as_str()).collect();
                out.push_str(&names.join(", "));
                out.push_str(" }");
            }
        }
        out.push_str(" { todo() }\n");
    }
    Some((close_line_start, out, missing.len()))
}

/// Find the innermost `class` whose body `{...}` contains the cursor
/// and, when the class has fields but no `init` method, return the
/// byte offset and source text for an inserted constructor that
/// takes one parameter per field and assigns each to `this.field`.
/// Skips `@extern("...")` opaque-handle classes and `@extern(C)
/// struct` classes (init is rejected for both).
pub(crate) fn generate_init_at(
    text: &str,
    prog: &Program,
    cursor: Position,
) -> Option<(usize, String)> {
    let cursor_byte =
        text::line_col_to_offset(text, cursor.line + 1, cursor.character + 1)?;
    let mut chosen: Option<(&ClassDecl, usize, usize)> = None;
    for it in &prog.items {
        let Item::Class(c) = it else { continue };
        let Some((open, close)) = match_brace_range(text, c.span) else {
            continue;
        };
        if cursor_byte < open || cursor_byte > close {
            continue;
        }
        let extent = close.saturating_sub(open);
        match chosen {
            None => chosen = Some((c, open, close)),
            Some((_, c_open, c_close)) => {
                if extent < c_close.saturating_sub(c_open) {
                    chosen = Some((c, open, close));
                }
            }
        }
    }
    let (cls, _open, close) = chosen?;
    if cls.extern_lib.is_some() || cls.is_repr_c {
        return None;
    }
    if cls.fields.is_empty() {
        return None;
    }
    if cls
        .methods
        .iter()
        .any(|m| m.name.as_str() == "init")
    {
        return None;
    }
    // Indentation: copy the closing `}`'s line indent for the class
    // and indent body / params one level deeper.
    let close_line_start = {
        let bytes = text.as_bytes();
        let mut i = close;
        while i > 0 && bytes[i - 1] != b'\n' {
            i -= 1;
        }
        i
    };
    let base_indent: String = text[close_line_start..close]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    let body_indent = format!("{base_indent}    ");
    let assign_indent = format!("{body_indent}    ");
    let params: Vec<String> = cls
        .fields
        .iter()
        .map(|f| format!("{}: {}", f.name.as_str(), f.ty))
        .collect();
    let mut out = String::new();
    out.push_str(&body_indent);
    out.push_str("init(");
    out.push_str(&params.join(", "));
    out.push_str(") {\n");
    for f in cls.fields.iter() {
        out.push_str(&assign_indent);
        out.push_str("this.");
        out.push_str(f.name.as_str());
        out.push_str(" = ");
        out.push_str(f.name.as_str());
        out.push('\n');
    }
    out.push_str(&body_indent);
    out.push_str("}\n");
    Some((close_line_start, out))
}

/// Recursively walk a block, recording every `match` expression's
/// `{ ... }` byte range (using brace-balance from the source text,
/// since `Match.span` covers only the `match` keyword).
fn collect_matches_in_block<'a>(
    block: &'a Block,
    text: &str,
    out: &mut Vec<(&'a Expr, usize, usize)>,
) {
    for s in &block.stmts {
        if let StmtKind::Expr(e) = &s.kind {
            collect_matches_in_expr(e, text, out);
        } else if let StmtKind::Let { value, .. } = &s.kind {
            collect_matches_in_expr(value, text, out);
        } else if let StmtKind::LetTuple { value, .. } = &s.kind {
            collect_matches_in_expr(value, text, out);
        } else if let StmtKind::LetStruct { value, .. } = &s.kind {
            collect_matches_in_expr(value, text, out);
        }
    }
    if let Some(t) = &block.tail {
        collect_matches_in_expr(t, text, out);
    }
}

fn collect_matches_in_expr<'a>(
    e: &'a Expr,
    text: &str,
    out: &mut Vec<(&'a Expr, usize, usize)>,
) {
    if let ExprKind::Match { scrutinee, arms } = &e.kind {
        if let Some((lo, hi)) = match_brace_range(text, e.span) {
            out.push((e, lo, hi));
        }
        collect_matches_in_expr(scrutinee, text, out);
        for a in arms.iter() {
            collect_matches_in_expr(&a.body, text, out);
        }
    }
    match &e.kind {
        ExprKind::Block(b) => collect_matches_in_block(b, text, out),
        ExprKind::If { cond, then_branch, else_branch } => {
            collect_matches_in_expr(cond, text, out);
            collect_matches_in_block(then_branch, text, out);
            if let Some(eb) = else_branch {
                collect_matches_in_expr(eb, text, out);
            }
        }
        ExprKind::While { cond, body } => {
            collect_matches_in_expr(cond, text, out);
            collect_matches_in_block(body, text, out);
        }
        ExprKind::Loop { body } => collect_matches_in_block(body, text, out),
        ExprKind::ForIn { iter, body, .. } => {
            collect_matches_in_expr(iter, text, out);
            collect_matches_in_block(body, text, out);
        }
        ExprKind::Call { args, .. } => {
            for a in args.iter() {
                collect_matches_in_expr(a, text, out);
            }
        }
        ExprKind::MethodCall { obj, args, .. } => {
            collect_matches_in_expr(obj, text, out);
            for a in args.iter() {
                collect_matches_in_expr(a, text, out);
            }
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            collect_matches_in_expr(lhs, text, out);
            collect_matches_in_expr(rhs, text, out);
        }
        ExprKind::Logical { lhs, rhs, .. } => {
            collect_matches_in_expr(lhs, text, out);
            collect_matches_in_expr(rhs, text, out);
        }
        ExprKind::Unary { expr, .. } => collect_matches_in_expr(expr, text, out),
        ExprKind::Assign { value, .. } => {
            collect_matches_in_expr(value, text, out);
        }
        ExprKind::AssignField { obj, value, .. } => {
            collect_matches_in_expr(obj, text, out);
            collect_matches_in_expr(value, text, out);
        }
        ExprKind::AssignIndex { obj, index, value } => {
            collect_matches_in_expr(obj, text, out);
            collect_matches_in_expr(index, text, out);
            collect_matches_in_expr(value, text, out);
        }
        ExprKind::Index { obj, index } => {
            collect_matches_in_expr(obj, text, out);
            collect_matches_in_expr(index, text, out);
        }
        ExprKind::Field { obj, .. } => {
            collect_matches_in_expr(obj, text, out);
        }
        ExprKind::Cast { expr, .. } => collect_matches_in_expr(expr, text, out),
        ExprKind::TypeTest { expr, .. } => {
            collect_matches_in_expr(expr, text, out);
        }
        ExprKind::TypeDowncast { expr, .. } => {
            collect_matches_in_expr(expr, text, out);
        }
        ExprKind::Return(Some(v)) | ExprKind::Break(Some(v)) => {
            collect_matches_in_expr(v, text, out);
        }
        ExprKind::Some(v) => collect_matches_in_expr(v, text, out),
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            collect_matches_in_expr(expr, text, out);
            collect_matches_in_block(then_branch, text, out);
            if let Some(eb) = else_branch {
                collect_matches_in_expr(eb, text, out);
            }
        }
        ExprKind::Array(items) | ExprKind::Tuple(items) => {
            for a in items.iter() {
                collect_matches_in_expr(a, text, out);
            }
        }
        ExprKind::StructLit { fields, .. } => {
            for (_, v) in fields.iter() {
                collect_matches_in_expr(v, text, out);
            }
        }
        ExprKind::MapLit(entries) => {
            for (k, v) in entries.iter() {
                collect_matches_in_expr(k, text, out);
                collect_matches_in_expr(v, text, out);
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                collect_matches_in_expr(s, text, out);
            }
            if let Some(e2) = end {
                collect_matches_in_expr(e2, text, out);
            }
        }
        ExprKind::New { args, .. } => {
            for a in args.iter() {
                collect_matches_in_expr(a, text, out);
            }
        }
        ExprKind::EnumCtor { args, .. } => match args {
            ilang_ast::CtorArgs::Tuple(es) => {
                for a in es.iter() {
                    collect_matches_in_expr(a, text, out);
                }
            }
            ilang_ast::CtorArgs::Struct(fs) => {
                for (_, v) in fs.iter() {
                    collect_matches_in_expr(v, text, out);
                }
            }
            ilang_ast::CtorArgs::Unit => {}
        },
        ExprKind::FnExpr { body, .. } => collect_matches_in_block(body, text, out),
        ExprKind::Match { .. } => {} // already handled above
        _ => {}
    }
}

/// Given the span of a `match` keyword token, find the byte range
/// `[lo, hi]` of its block body, where `lo` is the byte offset of
/// the opening `{` and `hi` is the offset of the closing `}`.
fn match_brace_range(text: &str, match_kw: Span) -> Option<(usize, usize)> {
    let off = text::line_col_to_offset(text, match_kw.line, match_kw.col)?;
    let bytes = text.as_bytes();
    let mut i = off;
    let mut depth: i32 = 0;
    let mut open: Option<usize> = None;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                if open.is_none() {
                    open = Some(i);
                }
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 && open.is_some() {
                    return Some((open.unwrap(), i));
                }
                i += 1;
            }
            b'"' => {
                // Skip string literal — match keyword can't appear inside.
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                if i < bytes.len() {
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    None
}

/// Resolve a match scrutinee to the user-defined name it carries
/// (enum or class — the type checker hasn't necessarily run, so a
/// bare `Object("Foo")` is accepted; the caller verifies that the
/// name resolves to an enum decl). `None` for non-named types.
fn scrutinee_enum_name(
    scrutinee: &Expr,
    var_types: &HashMap<AstSymbol, Type>,
) -> Option<AstSymbol> {
    let ty = match &scrutinee.kind {
        ExprKind::Var(name) => var_types.get(name).cloned(),
        _ => infer_expr_type_with_scope(scrutinee, &[]),
    };
    match ty? {
        Type::Enum(name) | Type::Object(name) => Some(name),
        _ => None,
    }
}
