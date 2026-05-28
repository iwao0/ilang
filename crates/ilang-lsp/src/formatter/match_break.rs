//! AST-aware `match`-break pass. When a `match` has multiple arms that
//! share a source line, rewrite the raw source so each arm sits on its
//! own line before the token-level passes run. Walks the parsed
//! program to find candidate matches, then splices the rewritten block
//! back into the source.

use ilang_ast::{Expr, ExprKind, Item, MatchArm, Program, Stmt, StmtKind};

use crate::text_utils::compute_line_starts;

use super::rewrap::LineState;
use super::{line_col_to_byte, INDENT};


/// AST-aware pass: when a `match` expression has more than one
/// arm and they share a source line, rewrite the source so each
/// arm sits on its own line. Other matches (single-arm, or
/// already broken across lines) are left alone.
///
/// Operates on the raw `src` so the token-level passes that come
/// after see a structurally clean input.
pub(super) fn match_break_pass(src: &str, prog: &Program) -> String {
    let line_starts = compute_line_starts(src);
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    collect_match_breaks(src, &line_starts, prog, &mut replacements);
    if replacements.is_empty() {
        return src.to_string();
    }
    // Apply tail-first so earlier replacements don't shift later
    // byte indices.
    replacements.sort_by_key(|(s, _, _)| std::cmp::Reverse(*s));
    let mut out = src.to_string();
    for (start, end, text) in replacements {
        out.replace_range(start..end, &text);
    }
    out
}

fn collect_match_breaks(
    src: &str,
    line_starts: &[usize],
    prog: &Program,
    out: &mut Vec<(usize, usize, String)>,
) {
    for item in &prog.items {
        match item {
            Item::Fn(f) => walk_block_for_match(src, line_starts, &f.body, out),
            Item::Class(c) => {
                for m in c.methods.iter() {
                    walk_block_for_match(src, line_starts, &m.body, out);
                }
                for sm in c.static_methods.iter() {
                    walk_block_for_match(src, line_starts, &sm.body, out);
                }
                for p in c.properties.iter() {
                    if let Some(g) = &p.getter {
                        walk_block_for_match(src, line_starts, &g.body, out);
                    }
                    if let Some(s) = &p.setter {
                        walk_block_for_match(src, line_starts, &s.body, out);
                    }
                }
                for sf in c.static_fields.iter() {
                    walk_expr_for_match(src, line_starts, &sf.value, out);
                }
            }
            _ => {}
        }
    }
    for s in &prog.stmts {
        walk_stmt_for_match(src, line_starts, s, out);
    }
    if let Some(t) = &prog.tail {
        walk_expr_for_match(src, line_starts, t, out);
    }
}

fn walk_stmt_for_match(
    src: &str,
    line_starts: &[usize],
    s: &Stmt,
    out: &mut Vec<(usize, usize, String)>,
) {
    match &s.kind {
        StmtKind::Let { value, .. }
        | StmtKind::LetTuple { value, .. }
        | StmtKind::LetStruct { value, .. } => {
            walk_expr_for_match(src, line_starts, value, out);
        }
        StmtKind::Expr(e) => walk_expr_for_match(src, line_starts, e, out),
    }
}

fn walk_block_for_match(
    src: &str,
    line_starts: &[usize],
    b: &ilang_ast::Block,
    out: &mut Vec<(usize, usize, String)>,
) {
    for s in b.stmts.iter() {
        walk_stmt_for_match(src, line_starts, s, out);
    }
    if let Some(t) = &b.tail {
        walk_expr_for_match(src, line_starts, t, out);
    }
}

fn walk_expr_for_match(
    src: &str,
    line_starts: &[usize],
    e: &Expr,
    out: &mut Vec<(usize, usize, String)>,
) {
    if let ExprKind::Match { scrutinee, arms } = &e.kind {
        if let Some(replacement) = try_break_match(src, line_starts, e, scrutinee, arms) {
            out.push(replacement);
        }
        // Recurse into scrutinee + arm bodies regardless — nested
        // matches still need consideration.
        walk_expr_for_match(src, line_starts, scrutinee, out);
        for arm in arms.iter() {
            walk_expr_for_match(src, line_starts, &arm.body, out);
        }
        return;
    }
    walk_expr_children_for_match(src, line_starts, e, out);
}

/// Visit every direct child Expr of `e`. Mirrors the type checker's
/// `walk_children` but only as much as we need for match discovery.
fn walk_expr_children_for_match(
    src: &str,
    line_starts: &[usize],
    e: &Expr,
    out: &mut Vec<(usize, usize, String)>,
) {
    macro_rules! visit {
        ($child:expr) => { walk_expr_for_match(src, line_starts, $child, out) };
    }
    match &e.kind {
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::Var(_)
        | ExprKind::This
        | ExprKind::None
        | ExprKind::Continue => {}
        ExprKind::Some(x) => visit!(x),
        ExprKind::Await(x) => visit!(x),
        ExprKind::Unary { expr, .. } => visit!(expr),
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            visit!(lhs);
            visit!(rhs);
        }
        ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. } => visit!(expr),
        ExprKind::Call { args, .. } => {
            for a in args.iter() {
                visit!(a);
            }
        }
        ExprKind::MethodCall { obj, args, .. } => {
            visit!(obj);
            for a in args.iter() {
                visit!(a);
            }
        }
        ExprKind::SuperCall { args, .. } => {
            for a in args.iter() {
                visit!(a);
            }
        }
        ExprKind::Field { obj, .. } => visit!(obj),
        ExprKind::Index { obj, index } => {
            visit!(obj);
            visit!(index);
        }
        ExprKind::Assign { value, .. } => visit!(value),
        ExprKind::AssignField { obj, value, .. } => {
            visit!(obj);
            visit!(value);
        }
        ExprKind::AssignIndex { obj, index, value } => {
            visit!(obj);
            visit!(index);
            visit!(value);
        }
        ExprKind::FnExpr { body, .. } => walk_block_for_match(src, line_starts, body, out),
        ExprKind::Array(elements) | ExprKind::Tuple(elements) => {
            for e in elements.iter() {
                visit!(e);
            }
        }
        ExprKind::StructLit { fields, .. } => {
            for (_, expr) in fields.iter() {
                visit!(expr);
            }
        }
        ExprKind::MapLit(entries) => {
            for (k, v) in entries.iter() {
                visit!(k);
                visit!(v);
            }
        }
        ExprKind::Block(b) => walk_block_for_match(src, line_starts, b, out),
        ExprKind::If { cond, then_branch, else_branch } => {
            visit!(cond);
            walk_block_for_match(src, line_starts, then_branch, out);
            if let Some(e) = else_branch {
                visit!(e);
            }
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            visit!(expr);
            walk_block_for_match(src, line_starts, then_branch, out);
            if let Some(e) = else_branch {
                visit!(e);
            }
        }
        ExprKind::While { cond, body } => {
            visit!(cond);
            walk_block_for_match(src, line_starts, body, out);
        }
        ExprKind::Loop { body } => walk_block_for_match(src, line_starts, body, out),
        ExprKind::ForIn { iter, body, .. } => {
            visit!(iter);
            walk_block_for_match(src, line_starts, body, out);
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                visit!(s);
            }
            if let Some(e) = end {
                visit!(e);
            }
        }
        ExprKind::Return(opt) | ExprKind::Break(opt) => {
            if let Some(x) = opt {
                visit!(x);
            }
        }
        ExprKind::New { args, .. } => {
            for a in args.iter() {
                visit!(a);
            }
        }
        ExprKind::EnumCtor { args, .. } => match args {
            ilang_ast::CtorArgs::Unit => {}
            ilang_ast::CtorArgs::Tuple(es) => {
                for e in es.iter() {
                    visit!(e);
                }
            }
            ilang_ast::CtorArgs::Struct(fs) => {
                for (_, e) in fs.iter() {
                    visit!(e);
                }
            }
        },
        ExprKind::Match { scrutinee, arms } => {
            visit!(scrutinee);
            for arm in arms.iter() {
                visit!(&arm.body);
            }
        }
        ExprKind::Template { parts } => {
            for p in parts.iter() {
                if let ilang_ast::TemplatePart::Expr(e2) = p {
                    visit!(e2);
                }
            }
        }
        ExprKind::Closure { .. } => {}
    }
}

/// Decide whether to break a match. Returns the (start_byte,
/// end_byte, replacement_text) when the match should be expanded,
/// or `None` to leave it alone.
fn try_break_match(
    src: &str,
    line_starts: &[usize],
    match_expr: &Expr,
    scrutinee: &Expr,
    arms: &[MatchArm],
) -> Option<(usize, usize, String)> {
    if arms.len() < 2 {
        return None;
    }
    // The match's source range starts at the `match` keyword and
    // ends after the matching `}` of the match block. Locate the
    // matching `}` by walking the source from `match` and balancing
    // braces (only `{` / `}` outside strings / comments count).
    let match_start = line_col_to_byte(
        src,
        line_starts,
        match_expr.span.line,
        match_expr.span.col,
    );
    let match_end = match find_match_block_end(src, match_start) {
        Some(e) => e,
        None => return None,
    };
    let match_block_open = match find_match_block_open(src, match_start) {
        Some(o) => o,
        None => return None,
    };

    // Multi-line already? If the existing match doesn't have any
    // two arms sharing a line, leave it alone.
    let mut on_same_line = false;
    for w in arms.windows(2) {
        if w[0].span.line == w[1].span.line {
            on_same_line = true;
            break;
        }
    }
    if !on_same_line {
        return None;
    }

    // Build the new layout. `match` keyword + scrutinee text +
    // ` {` + newline + each arm on its own line + closing `}`.
    let leading_indent = leading_indent_of(src, line_starts, match_expr.span.line);
    let inner_indent = format!("{leading_indent}{INDENT}");

    // Scrutinee text: chars between end of `match` keyword and the
    // match block's `{`. Trim whitespace.
    let scrut_start = line_col_to_byte(
        src,
        line_starts,
        scrutinee.span.line,
        scrutinee.span.col,
    );
    let scrut_text = src[scrut_start..match_block_open].trim();

    // Per-arm text: from each arm's start to the next arm's start
    // (or to the closing `}` for the last). Trim whitespace.
    let mut arm_texts: Vec<String> = Vec::with_capacity(arms.len());
    for (i, arm) in arms.iter().enumerate() {
        let start =
            line_col_to_byte(src, line_starts, arm.span.line, arm.span.col);
        let end = if i + 1 < arms.len() {
            line_col_to_byte(
                src,
                line_starts,
                arms[i + 1].span.line,
                arms[i + 1].span.col,
            )
        } else {
            match_end - 1 // before the closing `}`
        };
        if start >= end {
            return None;
        }
        let text = src[start..end].trim();
        // Drop a trailing `,` that the source might have had to
        // separate this arm from the next; the canonical form
        // doesn't use commas between arms.
        let text = text.trim_end_matches(',').trim_end();
        arm_texts.push(text.to_string());
    }

    let mut new_text = String::new();
    new_text.push_str("match ");
    new_text.push_str(scrut_text);
    new_text.push_str(" {\n");
    for arm in &arm_texts {
        new_text.push_str(&inner_indent);
        new_text.push_str(arm);
        new_text.push('\n');
    }
    new_text.push_str(&leading_indent);
    new_text.push('}');
    Some((match_start, match_end, new_text))
}

/// Walk forward from the `match` keyword's byte offset, returning
/// the byte offset of the `{` that opens the match's arms block.
/// Skips strings, line comments, and block comments.
fn find_match_block_open(src: &str, match_start: usize) -> Option<usize> {
    let mut i = match_start;
    let bytes = src.as_bytes();
    let mut state = LineState::Code;
    let mut paren_depth: i32 = 0;
    let mut bracket_depth: i32 = 0;
    let mut seen_match = false;
    while i < bytes.len() {
        let c = bytes[i];
        let next = bytes.get(i + 1).copied();
        match state {
            LineState::Code => {
                if !seen_match {
                    // Skip the `match` keyword itself.
                    if c == b'm' && i + 5 <= bytes.len() && &bytes[i..i + 5] == b"match" {
                        seen_match = true;
                        i += 5;
                        continue;
                    }
                    i += 1;
                    continue;
                }
                match c {
                    b'/' if next == Some(b'/') => {
                        while i < bytes.len() && bytes[i] != b'\n' {
                            i += 1;
                        }
                    }
                    b'/' if next == Some(b'*') => {
                        state = LineState::Block(1);
                        i += 2;
                    }
                    b'"' => {
                        state = LineState::Str;
                        i += 1;
                    }
                    b'(' => {
                        paren_depth += 1;
                        i += 1;
                    }
                    b')' => {
                        paren_depth -= 1;
                        i += 1;
                    }
                    b'[' => {
                        bracket_depth += 1;
                        i += 1;
                    }
                    b']' => {
                        bracket_depth -= 1;
                        i += 1;
                    }
                    b'{' if paren_depth == 0 && bracket_depth == 0 => {
                        return Some(i);
                    }
                    _ => i += 1,
                }
            }
            LineState::Str => {
                if c == b'\\' && next.is_some() {
                    i += 2;
                    continue;
                }
                if c == b'"' {
                    state = LineState::Code;
                }
                i += 1;
            }
            LineState::Block(d) => {
                if c == b'/' && next == Some(b'*') {
                    state = LineState::Block(d + 1);
                    i += 2;
                } else if c == b'*' && next == Some(b'/') {
                    state = if d == 1 { LineState::Code } else { LineState::Block(d - 1) };
                    i += 2;
                } else {
                    i += 1;
                }
            }
        }
    }
    None
}

/// Find the byte offset just past the `}` that closes the `match`
/// at `match_start`. Returns None if unbalanced.
fn find_match_block_end(src: &str, match_start: usize) -> Option<usize> {
    let block_open = find_match_block_open(src, match_start)?;
    let bytes = src.as_bytes();
    let mut i = block_open + 1;
    let mut state = LineState::Code;
    let mut depth: i32 = 1;
    while i < bytes.len() && depth > 0 {
        let c = bytes[i];
        let next = bytes.get(i + 1).copied();
        match state {
            LineState::Code => match c {
                b'/' if next == Some(b'/') => {
                    while i < bytes.len() && bytes[i] != b'\n' {
                        i += 1;
                    }
                }
                b'/' if next == Some(b'*') => {
                    state = LineState::Block(1);
                    i += 2;
                }
                b'"' => {
                    state = LineState::Str;
                    i += 1;
                }
                b'{' => {
                    depth += 1;
                    i += 1;
                }
                b'}' => {
                    depth -= 1;
                    i += 1;
                }
                _ => i += 1,
            },
            LineState::Str => {
                if c == b'\\' && next.is_some() {
                    i += 2;
                    continue;
                }
                if c == b'"' {
                    state = LineState::Code;
                }
                i += 1;
            }
            LineState::Block(d) => {
                if c == b'/' && next == Some(b'*') {
                    state = LineState::Block(d + 1);
                    i += 2;
                } else if c == b'*' && next == Some(b'/') {
                    state = if d == 1 { LineState::Code } else { LineState::Block(d - 1) };
                    i += 2;
                } else {
                    i += 1;
                }
            }
        }
    }
    if depth == 0 {
        Some(i)
    } else {
        None
    }
}

/// Return the leading whitespace of the given source line.
fn leading_indent_of(src: &str, line_starts: &[usize], line: u32) -> String {
    let line_idx = (line as usize).saturating_sub(1).min(line_starts.len().saturating_sub(1));
    let start = line_starts[line_idx];
    let mut out = String::new();
    for ch in src[start..].chars() {
        if ch == ' ' || ch == '\t' {
            out.push(if ch == '\t' { ' ' } else { ch });
        } else {
            break;
        }
    }
    out
}
