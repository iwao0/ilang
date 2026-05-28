//! `fill_match_arms_at` — cursor in a `match` whose scrutinee is an
//! enum → emit one new arm per missing variant. Includes the
//! `collect_matches_in_*` walker that records every `match`
//! expression's `{ ... }` byte range (brace-balanced from source,
//! since `Match.span` covers only the `match` keyword).

use std::collections::{HashMap, HashSet};

use ilang_ast::{
    Block, Expr, ExprKind, Item, PatternKind, Program, StmtKind, Symbol as AstSymbol, Type,
    VariantPayload,
};
use tower_lsp::lsp_types::Position;

use super::super::infer_expr_type_with_scope;
use super::super::text::{self, line_start_before};
use super::{match_brace_range, pick_innermost_containing};

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
    let (mexpr, _open, close) =
        pick_innermost_containing(all.iter().copied(), cursor_byte)?;
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
    let close_line_start = line_start_before(text, close);
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
        ExprKind::Await(v) => collect_matches_in_expr(v, text, out),
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

#[cfg(test)]
mod tests {
    use super::*;
    use ilang_lexer::tokenize;
    use ilang_parser::parse;

    fn pos(line: u32, col: u32) -> Position {
        Position { line, character: col }
    }

    fn run(src: &str, cursor: Position) -> Option<String> {
        let toks = tokenize(src).ok()?;
        let prog = parse(&toks).ok()?;
        // Re-derive var_types the way `analyse` would for `let x: Foo = ...`
        // bindings — minimal pass good enough for these unit tests.
        let mut var_types: HashMap<AstSymbol, Type> = HashMap::new();
        for it in &prog.items {
            if let Item::Fn(f) = it {
                for p in f.params.iter() {
                    var_types.insert(p.name.clone(), p.ty.clone());
                }
                collect_let_types(&f.body, &mut var_types);
            }
        }
        let (insert, new_text, _) =
            fill_match_arms_at(src, &prog, &var_types, cursor)?;
        let mut out = src.to_string();
        out.insert_str(insert, &new_text);
        Some(out)
    }

    fn collect_let_types(b: &Block, out: &mut HashMap<AstSymbol, Type>) {
        for s in &b.stmts {
            if let StmtKind::Let { name, ty: Some(t), .. } = &s.kind {
                out.insert(name.clone(), t.clone());
            }
        }
    }

    #[test]
    fn unit_variants_filled() {
        let src = "\
enum Color { Red, Green, Blue }
fn f(c: Color) {
    match c {
        Color.Red { 1 }
    }
}
";
        // cursor inside the match block (line 3 in 0-based).
        let out = run(src, pos(3, 8)).unwrap();
        assert!(out.contains("Color.Green { todo() }"), "out:\n{out}");
        assert!(out.contains("Color.Blue { todo() }"), "out:\n{out}");
    }

    #[test]
    fn wildcard_arm_skips_completion() {
        let src = "\
enum Color { Red, Green, Blue }
fn f(c: Color) {
    match c {
        Color.Red { 1 }
        _ { 0 }
    }
}
";
        assert!(run(src, pos(3, 8)).is_none());
    }

    #[test]
    fn fully_covered_skips_completion() {
        let src = "\
enum Color { Red, Green, Blue }
fn f(c: Color) {
    match c {
        Color.Red { 1 }
        Color.Green { 2 }
        Color.Blue { 3 }
    }
}
";
        assert!(run(src, pos(3, 8)).is_none());
    }

    #[test]
    fn cursor_outside_match_returns_none() {
        let src = "\
enum Color { Red, Green, Blue }
fn f(c: Color) {
    match c {
        Color.Red { 1 }
    }
}
";
        // line 0 is the enum decl — outside any match.
        assert!(run(src, pos(0, 5)).is_none());
    }

    #[test]
    fn tuple_variant_uses_underscore_placeholders() {
        let src = "\
enum Shape { Circle: (f64), Square: (f64, f64) }
fn area(s: Shape): f64 {
    match s {
        Shape.Circle(r) { r }
    }
}
";
        let out = run(src, pos(3, 8)).unwrap();
        assert!(
            out.contains("Shape.Square(_, _) { todo() }"),
            "out:\n{out}"
        );
    }

    #[test]
    fn struct_variant_emits_field_names() {
        let src = "\
enum Msg { Ping, Move: { x: f64, y: f64 } }
fn h(m: Msg) {
    match m {
        Msg.Ping { 1 }
    }
}
";
        let out = run(src, pos(3, 8)).unwrap();
        assert!(
            out.contains("Msg.Move { x, y } { todo() }"),
            "out:\n{out}"
        );
    }
}
