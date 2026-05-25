//! Token-aware document formatter for `.il`.
//!
//! Approach: re-lex the source, then walk tokens emitting canonical
//! whitespace between them while passing comments through verbatim.
//! The user's hand-written line breaks are respected (we never join
//! or split lines beyond collapsing 3+ blank lines into 1), so this
//! is a "mini" formatter — closer to gofmt's spirit than rustfmt's.
//!
//! What gets normalised:
//!   - 4-space indent recomputed from `{` / `}` depth
//!   - exactly one space after `,` / `;` / `:` (none before)
//!   - exactly one space around binary operators and `=` family
//!   - no space around `.`, `..`, `..=`, `::`
//!   - no space inside `(` / `[` boundaries
//!   - tabs flattened to spaces, trailing whitespace stripped, runs of
//!     3+ blank lines collapsed
//!
//! What the formatter intentionally avoids:
//!   - line wrapping / column-budget aware breaking
//!   - touching whitespace around `<` / `>` (ambiguous between
//!     comparison and generic brackets — leave the user's choice)
//!   - rewriting comments themselves
//!
//! On lex failure we return `None` so the LSP keeps the buffer as-is.
//!
//! The rules are intentionally conservative — output is meant to be a
//! superset of what users hand-write, not a strict reformat.
use ilang_ast::{Expr, ExprKind, Item, MatchArm, Program, Stmt, StmtKind};
use ilang_lexer::{tokenize, Token, TokenKind};

use crate::text_utils::compute_line_starts;

mod rewrap;

use rewrap::{finalize, rewrap_long_lines, LineState};

pub(super) const INDENT: &str = "    ";
/// Soft column budget. Lines longer than this trigger the
/// "break each comma-separated arg onto its own line" pass for the
/// outermost paren / bracket group on the line. Tuned to match the
/// `///` doc style already used in this codebase.
pub(super) const LINE_BUDGET: usize = 100;

pub fn format(src: &str) -> Option<String> {
    let tokens = tokenize(src).ok()?;
    // Pre-pass: when the program parses, expand `match` whose
    // arms share a line so each arm sits on its own line. This
    // is the AST-aware step (mid-B) — the rest of the formatter
    // is purely token-level and runs unchanged afterwards.
    let pre_src = match ilang_parser::parse(&tokens) {
        Ok(prog) => match_break_pass(src, &prog),
        Err(_) => src.to_string(),
    };
    let pre_tokens = if pre_src == src {
        tokens
    } else {
        match tokenize(&pre_src) {
            Ok(t) => t,
            Err(_) => return None,
        }
    };
    let line_starts = compute_line_starts(&pre_src);
    let mini = format_tokens(&pre_src, &pre_tokens, &line_starts);
    let formatted = rewrap_long_lines(&mini);
    if formatted == src {
        None
    } else {
        Some(formatted)
    }
}

/// AST-aware pass: when a `match` expression has more than one
/// arm and they share a source line, rewrite the source so each
/// arm sits on its own line. Other matches (single-arm, or
/// already broken across lines) are left alone.
///
/// Operates on the raw `src` so the token-level passes that come
/// after see a structurally clean input.
fn match_break_pass(src: &str, prog: &Program) -> String {
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

/// `line` / `col` are 1-based; returns the byte offset of that
/// character in `src`. Walks the line from its start to count
/// characters (so multi-byte UTF-8 cols still resolve correctly).
fn line_col_to_byte(src: &str, line_starts: &[usize], line: u32, col: u32) -> usize {
    let line_idx = (line as usize).saturating_sub(1).min(line_starts.len().saturating_sub(1));
    let line_start = line_starts[line_idx];
    let target_col = col as usize;
    let mut byte = line_start;
    let mut current_col = 1usize;
    for ch in src[line_start..].chars() {
        if current_col >= target_col {
            return byte;
        }
        byte += ch.len_utf8();
        current_col += 1;
        if ch == '\n' {
            // Shouldn't happen for valid spans, but be safe.
            return byte;
        }
    }
    byte
}

fn token_byte_range(
    src: &str,
    line_starts: &[usize],
    tok: &Token,
) -> (usize, usize) {
    let start = line_col_to_byte(src, line_starts, tok.span.line, tok.span.col);
    let last_char_byte = line_col_to_byte(src, line_starts, tok.span.end_line, tok.span.end_col);
    let last_char = src[last_char_byte..].chars().next();
    let end = last_char_byte + last_char.map(|c| c.len_utf8()).unwrap_or(1);
    (start, end)
}

fn format_tokens(src: &str, tokens: &[Token], line_starts: &[usize]) -> String {
    let mut out = String::with_capacity(src.len());
    let mut brace_depth: i32 = 0;
    // Indent for unclosed `(...)` / `[...]` whose open spans a
    // newline before its close. Each such open bumps depth by 1
    // until matched. Single-line `f(a, b)` doesn't bump.
    let mut paren_depth: i32 = 0;
    let mut paren_stack: Vec<bool> = Vec::new();
    let mut prev_end: usize = 0;
    let mut prev_kind: Option<&TokenKind> = None;
    let mut prev_prev_kind: Option<&TokenKind> = None;
    let mut at_line_start = true;
    // `@objc(...)` attribute tracking. Set when we see the leading
    // `@ objc (` triple; each nested `(` bumps, each `)` decrements;
    // when it returns to 0 the matching close of the attribute has
    // been emitted and we force a newline before the next token so
    // `@objc("sel") pub release()` always reflows to two lines.
    let mut objc_attr_depth: i32 = 0;
    let mut just_closed_objc_attr = false;

    for (idx, tok) in tokens.iter().enumerate() {
        if matches!(tok.kind, TokenKind::Eof) {
            // Trailing gap (after last code token).
            let trailing = &src[prev_end..];
            emit_gap(
                &mut out,
                trailing,
                brace_depth + paren_depth,
                prev_kind,
                None,
                &mut at_line_start,
            );
            break;
        }
        let (start_byte, end_byte) = token_byte_range(src, line_starts, tok);
        let gap = &src[prev_end..start_byte];

        let total_depth = brace_depth + paren_depth;
        // `}` closes a brace-block — outdent before printing.
        // Likewise the matching close of a multi-line `(...)` /
        // `[...]` should sit at the OUTER indent.
        let closing_multiline_paren = matches!(
            tok.kind,
            TokenKind::RParen | TokenKind::RBracket
        ) && paren_stack.last().copied().unwrap_or(false);
        let indent_depth = if matches!(tok.kind, TokenKind::RBrace)
            || closing_multiline_paren
        {
            (total_depth - 1).max(0)
        } else {
            total_depth.max(0)
        };

        // If the previous gap closed an `@objc(...)` attribute and
        // the user didn't already put a newline before this token,
        // force one so the next item (method header, class header,
        // etc.) sits on its own indented line.
        let force_break = just_closed_objc_attr
            && !matches!(tok.kind, TokenKind::At)
            && !gap.contains('\n');
        if force_break {
            // Strip any trailing space that emit_gap may have added.
            while out.ends_with(' ') {
                out.pop();
            }
            out.push('\n');
            push_indent(&mut out, indent_depth);
        } else {
            emit_gap(
                &mut out,
                gap,
                indent_depth,
                prev_kind,
                Some((&tok.kind, prev_prev_kind)),
                &mut at_line_start,
            );
        }
        just_closed_objc_attr = false;

        // Emit the token's source slice verbatim — this preserves
        // numeric suffixes / hex / escapes / Unicode in identifiers.
        out.push_str(&src[start_byte..end_byte]);
        at_line_start = false;

        // Update depth tracking from this token.
        match tok.kind {
            TokenKind::LBrace => brace_depth += 1,
            TokenKind::RBrace => brace_depth -= 1,
            TokenKind::LParen | TokenKind::LBracket => {
                // Multi-line if the next non-EOF token starts on a
                // later line than this open delimiter.
                let multi = tokens
                    .get(idx + 1)
                    .map(|t| t.span.line > tok.span.end_line)
                    .unwrap_or(false);
                paren_stack.push(multi);
                if multi {
                    paren_depth += 1;
                }
                if objc_attr_depth > 0 && matches!(tok.kind, TokenKind::LParen) {
                    objc_attr_depth += 1;
                } else if matches!(tok.kind, TokenKind::LParen)
                    && is_objc_attr_open(prev_kind, prev_prev_kind)
                {
                    objc_attr_depth = 1;
                }
            }
            TokenKind::RParen | TokenKind::RBracket => {
                let was_multi = paren_stack.pop().unwrap_or(false);
                if was_multi {
                    paren_depth -= 1;
                }
                if objc_attr_depth > 0 && matches!(tok.kind, TokenKind::RParen) {
                    objc_attr_depth -= 1;
                    if objc_attr_depth == 0 {
                        just_closed_objc_attr = true;
                    }
                }
            }
            _ => {}
        }
        prev_end = end_byte;
        prev_prev_kind = prev_kind;
        prev_kind = Some(&tok.kind);
    }

    // Final touch-ups: strip trailing whitespace per line, collapse
    // 3+ blank-line runs to 1, ensure exactly one trailing newline.
    finalize(&out)
}

/// Emit the run of whitespace + comments between two tokens (or
/// before the first / after the last). `next` is `Some((kind, prev_prev))`
/// when there's a following token; `None` for the final trailing gap.
fn emit_gap(
    out: &mut String,
    gap: &str,
    indent_depth: i32,
    prev: Option<&TokenKind>,
    next: Option<(&TokenKind, Option<&TokenKind>)>,
    at_line_start: &mut bool,
) {
    let items = scan_gap(gap);
    let has_newline = items
        .iter()
        .any(|i| matches!(i, GapItem::Newlines(n) if *n > 0));
    let has_comment = items.iter().any(|i| matches!(i, GapItem::Comment(_)));

    // Leading gap (no prev token): just emit comments + newlines
    // verbatim with indent. Used for files starting with comments.
    if prev.is_none() {
        emit_gap_with_breaks(out, &items, indent_depth, at_line_start);
        return;
    }

    if !has_newline && !has_comment {
        // Pure intra-line whitespace — apply canonical rule.
        if let Some((next_kind, prev_prev_kind)) = next {
            if needs_space(prev.unwrap(), next_kind, prev_prev_kind) {
                out.push(' ');
            }
        }
        return;
    }

    let trailing_inline_comment =
        emit_gap_with_breaks(out, &items, indent_depth, at_line_start);
    if trailing_inline_comment {
        if let Some((next_kind, _)) = next {
            // Mirror `needs_space`'s "tight to the left" rules so
            // a trailing block comment doesn't push a space before
            // a close-bracket / separator / chain operator.
            if !matches!(
                next_kind,
                TokenKind::RParen
                    | TokenKind::RBracket
                    | TokenKind::RBrace
                    | TokenKind::Comma
                    | TokenKind::Semicolon
                    | TokenKind::Colon
                    | TokenKind::ColonColon
                    | TokenKind::Dot
                    | TokenKind::DotDot
                    | TokenKind::DotDotEq
                    | TokenKind::Question
            ) {
                out.push(' ');
            }
        }
    }
}

/// Returns `true` if the gap ended with a block comment that has
/// no following newline — caller should add a space before the
/// next token unless that token is a close-bracket / separator.
fn emit_gap_with_breaks(
    out: &mut String,
    items: &[GapItem],
    indent_depth: i32,
    at_line_start: &mut bool,
) -> bool {
    let mut just_newlined = false;
    let mut last_inline_comment = false;
    for item in items {
        match item {
            GapItem::Newlines(n) => {
                let want = (*n).min(2); // cap blank-line runs
                for _ in 0..want {
                    // Strip any trailing spaces before the newline.
                    while out.ends_with(' ') {
                        out.pop();
                    }
                    out.push('\n');
                }
                if want > 0 {
                    just_newlined = true;
                    *at_line_start = true;
                    last_inline_comment = false;
                }
            }
            GapItem::Comment(text) => {
                if just_newlined {
                    push_indent(out, indent_depth);
                    just_newlined = false;
                    *at_line_start = false;
                } else if !out.is_empty() && !out.ends_with(' ') && !out.ends_with('\n') {
                    out.push(' ');
                }
                out.push_str(text);
                // Line comments are always followed by a newline
                // (the lexer's whitespace consumes the `\n` into
                // the next gap), so they never need an inline
                // trailing space. Block comments DO when they sit
                // mid-expression.
                last_inline_comment = !text.starts_with("//");
            }
        }
    }
    if just_newlined {
        push_indent(out, indent_depth);
    }
    last_inline_comment
}

/// `( ` just observed; was the run `@ objc (`? Used by the formatter
/// to recognise the start of an `@objc("selector:")` attribute and
/// force a newline once its matching `)` closes.
fn is_objc_attr_open(prev: Option<&TokenKind>, prev_prev: Option<&TokenKind>) -> bool {
    matches!(prev, Some(TokenKind::Ident(n)) if n.as_str() == "objc")
        && matches!(prev_prev, Some(TokenKind::At))
}

fn push_indent(out: &mut String, depth: i32) {
    for _ in 0..depth.max(0) {
        out.push_str(INDENT);
    }
}

#[derive(Debug)]
enum GapItem {
    /// Number of newlines in this run of whitespace.
    Newlines(u32),
    /// A comment, including its `//` / `/* ... */` delimiters.
    Comment(String),
}

/// Walk the gap text (whitespace + comments) and produce a flat
/// list of items in source order. Whitespace runs are collapsed
/// into `Newlines(n)` (n = newline count); comments come through
/// verbatim.
fn scan_gap(gap: &str) -> Vec<GapItem> {
    let bytes = gap.as_bytes();
    let mut items: Vec<GapItem> = Vec::new();
    let mut i = 0usize;
    let mut newlines: u32 = 0;
    let mut in_ws = true;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            if newlines > 0 || in_ws {
                items.push(GapItem::Newlines(newlines));
                newlines = 0;
            }
            // Line comment to end of line (no newline char yet).
            let start = i;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            items.push(GapItem::Comment(
                std::str::from_utf8(&bytes[start..i])
                    .unwrap_or("")
                    .trim_end_matches([' ', '\t'])
                    .to_string(),
            ));
            in_ws = false;
            continue;
        }
        if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            if newlines > 0 || in_ws {
                items.push(GapItem::Newlines(newlines));
                newlines = 0;
            }
            // Block comment, possibly nested.
            let start = i;
            i += 2;
            let mut depth: u32 = 1;
            while i < bytes.len() && depth > 0 {
                if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                    depth += 1;
                    i += 2;
                } else if i + 1 < bytes.len() && bytes[i] == b'*' && bytes[i + 1] == b'/' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            items.push(GapItem::Comment(
                std::str::from_utf8(&bytes[start..i]).unwrap_or("").to_string(),
            ));
            in_ws = false;
            continue;
        }
        if c == b'\n' {
            newlines += 1;
            in_ws = true;
            i += 1;
        } else if c == b' ' || c == b'\t' || c == b'\r' {
            in_ws = true;
            i += 1;
        } else {
            // Shouldn't happen for a well-formed gap (lexer would
            // have eaten anything non-comment / non-whitespace),
            // but be defensive.
            i += 1;
        }
    }
    if in_ws && newlines > 0 {
        items.push(GapItem::Newlines(newlines));
    } else if newlines > 0 {
        items.push(GapItem::Newlines(newlines));
    }
    items
}

/// Decide whether a single space goes between `prev` and `next`
/// when no newline / comment intervenes. `prev_prev` lets us
/// distinguish unary `-` / `+` (preceded by an op-like token)
/// from binary ones (preceded by an expression-end token).
fn needs_space(
    prev: &TokenKind,
    next: &TokenKind,
    prev_prev: Option<&TokenKind>,
) -> bool {
    use TokenKind::*;

    // No space inside open / before close parens & brackets.
    // Braces (`{` / `}`) keep a space — block bodies on a single
    // line look like `{ expr }` in this codebase. Empty blocks
    // `{}` likewise pass through unchanged.
    if matches!(prev, LParen | LBracket) {
        return false;
    }
    if matches!(next, RParen | RBracket) {
        return false;
    }

    // Separators bind tight to the left.
    if matches!(next, Comma | Semicolon | Colon | ColonColon) {
        return false;
    }

    // No space around `.` / `..` / `..=` / `::`.
    if matches!(prev, Dot | DotDot | DotDotEq | ColonColon) {
        return false;
    }
    if matches!(next, Dot | DotDot | DotDotEq) {
        return false;
    }

    // No space before `?` (Optional type / `as?`).
    if matches!(next, Question) {
        return false;
    }

    // Suffix `?` followed by another suffix / atom expects no
    // space when it's still part of a type (`A?[]`, `A?` alone).
    // Without proper context this would be misclassified, so
    // leave the default (space) to keep things readable.
    let _ = prev; // silence unused warning when handled below

    // Unary `!` / `~` / prefix `-` / `+`: no space after.
    if matches!(prev, Bang | Tilde) {
        return false;
    }
    // Attribute prefix `@` binds tight to the following ident
    // (`@flags`, `@extern`, `@lib`).
    if matches!(prev, At) {
        return false;
    }
    if matches!(prev, Minus | Plus)
        && prev_prev.map(|p| !is_expression_end(p)).unwrap_or(true)
    {
        return false;
    }

    // Function call / indexing: no space between expression-end and `(` / `[`.
    if matches!(next, LParen | LBracket) {
        if prev_kind_is_callable(prev) {
            return false;
        }
        return true;
    }

    // Default: one space.
    true
}

/// True for tokens that can sit at the right edge of an
/// expression — i.e. before a binary operator or before a
/// function-call `(`. Used to disambiguate prefix vs binary
/// `-` / `+` and to decide whether `(` opens a call.
fn is_expression_end(t: &TokenKind) -> bool {
    use TokenKind::*;
    matches!(
        t,
        Ident(_)
            | Int(_)
            | Float(_)
            | Str(_)
            | True
            | False
            | This
            | RParen
            | RBracket
            | RBrace
            | Question
    )
}

fn prev_kind_is_callable(t: &TokenKind) -> bool {
    use TokenKind::*;
    matches!(t, Ident(_) | RParen | RBracket | RBrace | This | Super)
}


#[cfg(test)]
mod tests {
    use super::format;

    fn fmt(src: &str) -> String {
        format(src).unwrap_or_else(|| src.to_string())
    }

    #[test]
    fn objc_attr_breaks_onto_its_own_line() {
        let src = concat!(
            "@extern(ObjC) {\n",
            "    @objc pub class NSObject {\n",
            "        @objc(\"release\") pub release()\n",
            "    }\n",
            "}\n",
        );
        let want = concat!(
            "@extern(ObjC) {\n",
            "    @objc pub class NSObject {\n",
            "        @objc(\"release\")\n",
            "        pub release()\n",
            "    }\n",
            "}\n",
        );
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn objc_attr_already_on_own_line_is_a_no_op() {
        let src = concat!(
            "@extern(ObjC) {\n",
            "    @objc pub class NSObject {\n",
            "        @objc(\"release\")\n",
            "        pub release()\n",
            "    }\n",
            "}\n",
        );
        assert_eq!(format(src), None);
    }

    #[test]
    fn already_canonical() {
        let src = "fn main() {\n    let x = 1\n    x + 2\n}\n";
        assert_eq!(format(src), None);
    }

    #[test]
    fn fixes_indent() {
        let src = "fn main() {\nlet x = 1\nx + 2\n}\n";
        let want = "fn main() {\n    let x = 1\n    x + 2\n}\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn normalizes_assignment_spacing() {
        let src = "let x=1\n";
        let want = "let x = 1\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn normalizes_comma_spacing() {
        let src = "fn f(a:i64,b:i64):i64{a+b}\n";
        let want = "fn f(a: i64, b: i64): i64 { a + b }\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn no_space_inside_parens() {
        let src = "let x = ( 1 + 2 )\n";
        let want = "let x = (1 + 2)\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn no_space_around_dot() {
        let src = "let n = obj . field\n";
        let want = "let n = obj.field\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn collapses_extra_spaces() {
        let src = "let   x   =   1\n";
        let want = "let x = 1\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn preserves_line_comment() {
        let src = "let x = 1 // value\nlet y = 2\n";
        // Already canonical — formatter should be a no-op.
        assert_eq!(format(src), None);
    }

    #[test]
    fn block_comment_kept_inline() {
        let src = "let x = 1 /* note */ + 2\n";
        assert_eq!(format(src), None);
    }

    #[test]
    fn collapses_blank_runs() {
        let src = "let x = 1\n\n\n\nlet y = 2\n";
        let want = "let x = 1\n\nlet y = 2\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn closing_brace_outdents() {
        let src = "fn f() {\n    let x = 1\n    }\n";
        let want = "fn f() {\n    let x = 1\n}\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn range_no_space() {
        let src = "for i in 1 .. 5 { }\n";
        // Empty `{ }` block bodies stay as-is (block braces keep
        // a space inside; collapsing to `{}` would diverge from
        // the rest of the codebase's style).
        let want = "for i in 1..5 { }\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn optional_suffix_no_space() {
        let src = "let a : i64 ? = none\n";
        let want = "let a: i64? = none\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn unary_minus_no_space() {
        let src = "let x = -1\nlet y = a + -b\n";
        // Both are well-formed; formatter shouldn't insert a stray
        // space after the unary `-`.
        assert_eq!(format(src), None);
    }

    #[test]
    fn idempotent_on_messy_input() {
        let src = "class Foo{\na:i32\nb:string\ninit(x:i32,y:string){this.a=x;this.b=y}\ngreet():string{\"hi \"+this.b}\n}\nlet f=new Foo(1,\"a\")\nfor i in 0..10{console.log(i)}\n";
        let once = format(src).expect("first pass should reformat");
        let twice = format(&once);
        assert_eq!(
            twice, None,
            "second pass should be a no-op (idempotent), got: {twice:?}"
        );
    }

    #[test]
    fn preserves_inline_block_comment() {
        let src = "let x = 1 /* note */ + 2\n";
        // Already canonical — should be a no-op.
        assert_eq!(format(src), None);
    }

    #[test]
    fn tabs_become_four_spaces() {
        let src = "fn main() {\n\tlet x = 1\n}\n";
        let want = "fn main() {\n    let x = 1\n}\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn nested_braces() {
        let src = "fn outer() {\nfn inner() {\nlet x = 1\n}\n}\n";
        let want = "fn outer() {\n    fn inner() {\n        let x = 1\n    }\n}\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn ignores_braces_in_strings() {
        let src = "let s = \"a {b} c\"\nlet t = 1\n";
        assert_eq!(format(src), None);
    }

    #[test]
    fn ignores_braces_in_line_comments() {
        let src = "// what about { this }\nlet t = 1\n";
        assert_eq!(format(src), None);
    }

    #[test]
    fn ignores_braces_in_block_comments() {
        let src = "/* { not\n   a } block */\nlet t = 1\n";
        assert_eq!(format(src), None);
    }

    #[test]
    fn long_call_breaks_args() {
        // 100+ char single-line call should fan out one arg per line.
        let src = "let r = something_long_named_function(\
                   alpha_value, beta_value, gamma_value, \
                   delta_value, epsilon_value, zeta_value)\n";
        let out = fmt(src);
        // Head line ends with `(`.
        let lines: Vec<&str> = out.split('\n').collect();
        assert!(lines[0].ends_with('('), "head: {:?}", lines[0]);
        // Each arg on its own line, indented one level. The source
        // had no trailing comma, so the last element doesn't get one
        // (formatter preserves the source's trailing-comma choice).
        assert!(lines.iter().any(|l| l == &"    alpha_value,"), "lines: {lines:#?}");
        assert!(lines.iter().any(|l| l == &"    zeta_value"), "lines: {lines:#?}");
        // Closing paren back at base indent.
        assert!(lines.iter().any(|l| l == &")"), "lines: {lines:#?}");
    }

    #[test]
    fn short_call_stays_inline() {
        let src = "let r = f(a, b, c)\n";
        assert_eq!(format(src), None);
    }

    #[test]
    fn long_array_literal_breaks() {
        let src = "let xs: i64[] = [\
                   100000, 200000, 300000, 400000, 500000, 600000, \
                   700000, 800000, 900000, 1000000, 1100000]\n";
        let out = fmt(src);
        assert!(out.contains("[\n"), "expected break after `[`:\n{out}");
        assert!(out.contains("\n]"), "expected close on own line:\n{out}");
    }

    #[test]
    fn broken_call_stays_idempotent() {
        // Pre-broken multi-line — second pass shouldn't merge.
        let src = "let r = foo(\n    a,\n    b,\n    c,\n)\n";
        // Already canonical (4-space inner indent, `)` at column 0).
        assert_eq!(format(src), None);
    }

    #[test]
    fn match_arms_get_their_own_lines() {
        // Comma-separated arms on one line — valid ilang. Formatter
        // should expand each arm onto its own line.
        let src = "let v = match n { 0 { \"zero\" }, 1 { \"one\" }, _ { \"other\" } }\n";
        let out = fmt(src);
        // Each arm sits on its own line, indented one level deep
        // relative to the `match`'s opening line.
        assert!(out.contains("match n {\n"), "out:\n{out}");
        assert!(out.contains("    0 { \"zero\" }"), "out:\n{out}");
        assert!(out.contains("    1 { \"one\" }"), "out:\n{out}");
        assert!(out.contains("    _ { \"other\" }"), "out:\n{out}");
        assert!(out.contains("\n}"), "closing brace on own line:\n{out}");
    }

    #[test]
    fn match_already_broken_stays() {
        let src = "\
let v = match n {
    0 { \"zero\" }
    1 { \"one\" }
    _ { \"other\" }
}
";
        // Already canonical — no change.
        assert_eq!(format(src), None);
    }

    #[test]
    fn single_arm_match_stays_inline() {
        let src = "let v = match n { _ { 0 } }\n";
        // Don't break a one-arm match (less than 2 arms — no point).
        assert_eq!(format(src), None);
    }

    #[test]
    fn match_break_idempotent() {
        let src = "let v = match n { 0 { \"zero\" } 1 { \"one\" } _ { \"other\" } }\n";
        let once = fmt(src);
        let twice = format(&once);
        assert!(twice.is_none(), "second pass changed: {twice:?}");
    }

    /// Walks every `.il` file under the cargo workspace, runs the
    /// formatter, and verifies the formatted output still tokenises
    /// cleanly. Catches whole-file regressions where the formatter
    /// loses or shifts syntactically meaningful tokens.
    #[test]
    fn formatter_preserves_lexability_on_corpus() {
        use std::path::PathBuf;
        // Workspace root is the parent of this crate's manifest.
        let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace = crate_dir.parent().and_then(|p| p.parent()).unwrap();
        let mut paths: Vec<PathBuf> = Vec::new();
        collect_il(workspace, &mut paths);
        let mut tested = 0usize;
        for p in paths {
            let src = match std::fs::read_to_string(&p) {
                Ok(s) => s,
                Err(_) => continue,
            };
            // Original must parse; otherwise nothing to compare.
            if ilang_lexer::tokenize(&src).is_err() {
                continue;
            }
            let formatted = format(&src).unwrap_or_else(|| src.clone());
            let orig_toks =
                ilang_lexer::tokenize(&src).expect("orig tokenises");
            let fmt_toks = match ilang_lexer::tokenize(&formatted) {
                Ok(t) => t,
                Err(_) => panic!(
                    "formatter broke lex of {}: \n{}",
                    p.display(),
                    formatted
                ),
            };
            // Token kinds must line up — formatter can't drop or
            // invent any token. Spans and `leading_newline` may
            // shift legitimately, so compare kinds only.
            let orig_kinds: Vec<_> =
                orig_toks.iter().map(|t| &t.kind).collect();
            let fmt_kinds: Vec<_> =
                fmt_toks.iter().map(|t| &t.kind).collect();
            assert_eq!(
                orig_kinds,
                fmt_kinds,
                "token sequence diverged in {}",
                p.display()
            );
            // Idempotency on real code.
            let again = format(&formatted);
            assert!(
                again.is_none(),
                "format not idempotent on {}: {:?}",
                p.display(),
                again
            );
            tested += 1;
        }
        assert!(tested > 0, "didn't find any .il fixtures");
    }

    fn collect_il(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().and_then(|n| n.to_str()) == Some("target") {
                    continue;
                }
                collect_il(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("il") {
                out.push(path);
            }
        }
    }
}
