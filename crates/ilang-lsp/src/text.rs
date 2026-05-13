//! Pure text / span helpers shared by the LSP. None of these reach
//! into project-specific data structures — they operate on the raw
//! source string + a `Span` (1-based line/col) and return either an
//! offset into the byte slice or another `Span`.

use ilang_ast::Span;
use ilang_lexer::{tokenize, TokenKind};
use tower_lsp::lsp_types::{Position, Range};

/// Convert a 1-based (line, col) into a byte offset into `text`.
pub(crate) fn line_col_to_offset(text: &str, line: u32, col: u32) -> Option<usize> {
    let mut cur_line = 1u32;
    let mut line_start = 0usize;
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if cur_line == line {
            return Some(line_start + col.saturating_sub(1) as usize);
        }
        if b == b'\n' {
            cur_line += 1;
            line_start = i + 1;
        }
    }
    if cur_line == line {
        return Some(line_start + col.saturating_sub(1) as usize);
    }
    None
}

/// Inverse of `line_col_to_offset`.
pub(crate) fn offset_to_line_col(text: &str, offset: usize) -> Option<(u32, u32)> {
    let bytes = text.as_bytes();
    if offset > bytes.len() {
        return None;
    }
    let mut line = 1u32;
    let mut line_start = 0usize;
    for (i, &b) in bytes.iter().enumerate().take(offset) {
        if b == b'\n' {
            line += 1;
            line_start = i + 1;
        }
    }
    let col = (offset - line_start) as u32 + 1;
    Some((line, col))
}

/// Locate the start of the type token in a `name: T` form (field
/// declarations, params, etc.). Skips the `name` identifier, the
/// trailing whitespace, the `:`, more whitespace, and lands on the
/// first character of the type. Returns `None` if the layout
/// doesn't match (e.g., no `:` follows the name).
pub(crate) fn locate_type_after_colon(
    text: &str,
    name_span: Span,
    name: &str,
) -> Option<Span> {
    let off = line_col_to_offset(text, name_span.line, name_span.col)?;
    let bytes = text.as_bytes();
    let mut i = off + name.len();
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b':' {
        return None;
    }
    i += 1;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let b = bytes[i];
    if !b.is_ascii_alphabetic() && b != b'_' {
        return None;
    }
    let (line, col) = offset_to_line_col(text, i)?;
    Some(Span::new(line, col))
}

/// Locate the Nth base name in a class declaration's `: Base1, Base2`
/// list. `class_span` points at the `class` keyword.
pub(crate) fn locate_class_base_name(
    text: &str,
    class_span: Span,
    index: usize,
) -> Option<Span> {
    let off = line_col_to_offset(text, class_span.line, class_span.col)?;
    let bytes = text.as_bytes();
    let mut i = off;
    while i < bytes.len() && bytes[i] != b'{' {
        if bytes[i] == b':' {
            i += 1;
            break;
        }
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let mut found = 0usize;
    while i < bytes.len() {
        while i < bytes.len()
            && (bytes[i] == b' ' || bytes[i] == b'\t' || bytes[i] == b',')
        {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] == b'{' {
            return None;
        }
        if found == index {
            let b = bytes[i];
            if !b.is_ascii_alphabetic() && b != b'_' {
                return None;
            }
            let (line, col) = offset_to_line_col(text, i)?;
            return Some(Span::new(line, col));
        }
        found += 1;
        while i < bytes.len() && bytes[i] != b',' && bytes[i] != b'{' {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'{' {
            return None;
        }
    }
    None
}

/// Locate the property name after a `get` or `set` keyword.
pub(crate) fn locate_property_name(text: &str, kw_span: Span, name: &str) -> Option<Span> {
    let off = line_col_to_offset(text, kw_span.line, kw_span.col)?;
    let bytes = text.as_bytes();
    // Skip 3 keyword chars (`get` / `set`).
    let mut i = off + 3;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    let nb = name.as_bytes();
    if bytes.len() - i >= nb.len() && &bytes[i..i + nb.len()] == nb {
        let next = bytes.get(i + nb.len()).copied().unwrap_or(b' ');
        if !next.is_ascii_alphanumeric() && next != b'_' {
            let (line, col) = offset_to_line_col(text, i)?;
            return Some(name_span(line, col, name));
        }
    }
    None
}

/// Locate the `name` token after a `let` keyword. The Stmt span points
/// at `let`, so we skip the keyword + whitespace to land on the binder.
pub(crate) fn locate_let_name(text: &str, stmt_span: Span, name: &str) -> Option<Span> {
    locate_let_name_with_kw(text, stmt_span, "let", name)
}

/// Same as `locate_let_name` but parameterised on the keyword length —
/// works for `use`, `let`, etc.
pub(crate) fn locate_let_name_with_kw(
    text: &str,
    kw_span: Span,
    kw: &str,
    name: &str,
) -> Option<Span> {
    let off = line_col_to_offset(text, kw_span.line, kw_span.col)?;
    let bytes = text.as_bytes();
    let mut i = off + kw.len();
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    let nb = name.as_bytes();
    if bytes.len() - i >= nb.len() && &bytes[i..i + nb.len()] == nb {
        let next = bytes.get(i + nb.len()).copied().unwrap_or(b' ');
        if !next.is_ascii_alphanumeric() && next != b'_' {
            let (line, col) = offset_to_line_col(text, i)?;
            return Some(name_span(line, col, name));
        }
    }
    None
}

/// Find the `name` identifier that follows the next `.` after `obj_span`.
/// Returns its (line, col). Used to attach a precise span to `Field` and
/// `MethodCall` references whose AST nodes only carry the receiver's
/// span.
pub(crate) fn locate_dot_name(text: &str, obj_span: Span, name: &str) -> Option<(u32, u32)> {
    let offset = line_col_to_offset(text, obj_span.line, obj_span.col)?;
    let bytes = text.as_bytes();
    let mut i = offset;
    let mut paren_depth: i32 = 0;
    let mut bracket_depth: i32 = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'(' => paren_depth += 1,
            b')' => paren_depth -= 1,
            b'[' => bracket_depth += 1,
            b']' => bracket_depth -= 1,
            b'.' if paren_depth <= 0 && bracket_depth <= 0 => {
                let mut j = i + 1;
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                    j += 1;
                }
                let nb = name.as_bytes();
                if bytes.len() - j >= nb.len() && &bytes[j..j + nb.len()] == nb {
                    let next = bytes.get(j + nb.len()).copied().unwrap_or(b' ');
                    if !next.is_ascii_alphanumeric() && next != b'_' {
                        return offset_to_line_col(text, j);
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Find the bare identifier `name` inside a `use M { ... }` selective
/// import, starting at the `use` keyword's span. Returns the
/// (line, col) of the matching identifier, skipping content between
/// `use M` and the opening `{`. Stops at the closing `}`.
pub(crate) fn locate_selective_name(
    text: &str,
    use_span: Span,
    name: &str,
) -> Option<(u32, u32)> {
    let off = line_col_to_offset(text, use_span.line, use_span.col)?;
    let bytes = text.as_bytes();
    // Walk forward to the opening `{`.
    let mut i = off;
    while i < bytes.len() && bytes[i] != b'{' {
        if bytes[i] == b'\n' {
            // Selective-import braces are required on the same logical
            // line as the `use M` form; abandon if we hit EOL first.
            return None;
        }
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    i += 1; // step past `{`
    let nb = name.as_bytes();
    while i < bytes.len() && bytes[i] != b'}' {
        let b = bytes[i];
        // Identifier start: ASCII letter or `_`.
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            let mut j = i;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_')
            {
                j += 1;
            }
            if &bytes[start..j] == nb {
                return offset_to_line_col(text, start);
            }
            i = j;
            continue;
        }
        i += 1;
    }
    None
}

/// Extract a Rust-style doc comment block (`/// line` form) immediately
/// above the line containing `decl_line` (1-based). Returns the joined
/// body lines (without the leading `///` or single space) or `None`
/// when no contiguous `///` block precedes the decl.
pub(crate) fn extract_doc_above(text: &str, decl_line: u32) -> Option<String> {
    if decl_line <= 1 {
        return None;
    }
    // Only collect lines 0..decl_line-1 — we never look past the decl
    // itself. `split` is lazy, so `take` lets it stop early instead of
    // scanning the entire (possibly multi-thousand-line) source.
    let lines: Vec<&str> = text
        .split('\n')
        .take(decl_line.saturating_sub(1) as usize)
        .collect();
    let mut doc_lines: Vec<&str> = Vec::new();
    // Decl is at lines[decl_line - 1] (0-based). Walk back from there.
    let mut i = (decl_line as usize).saturating_sub(2); // line above
    loop {
        let Some(line) = lines.get(i) else { break };
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("///") {
            // Strip a single leading space (so `/// foo` -> `foo`,
            // `///foo` -> `foo`, `/// foo bar` -> `foo bar`).
            let body = rest.strip_prefix(' ').unwrap_or(rest);
            doc_lines.push(body);
            if i == 0 {
                break;
            }
            i -= 1;
            continue;
        }
        // Allow `@attribute(args)` between docs and decl; everything
        // else (blank line, code) ends the block.
        if trimmed.starts_with('@') || trimmed.is_empty() && doc_lines.is_empty() {
            // Blank line *before* any doc lines → no doc block here.
            // `@attr` lines between docs and decl are silently skipped.
            if i == 0 {
                break;
            }
            i -= 1;
            continue;
        }
        break;
    }
    if doc_lines.is_empty() {
        return None;
    }
    doc_lines.reverse();
    Some(doc_lines.join("\n"))
}

/// Build a single-line span that covers the given identifier's full
/// extent (start at `(line, col)`, ending after `name` chars).
fn name_span(line: u32, col: u32, name: &str) -> Span {
    let chars = name.chars().count() as u32;
    let end_col = col + chars.saturating_sub(1);
    Span::range(line, col, line, end_col.max(col))
}

/// Convert a 1-based ilang `Span` to a 0-based LSP `Range`. `len` is the
/// number of characters to highlight starting at `span.col` — used when
/// the caller has the identifier length but `span.end_col` points
/// somewhere else (e.g. the span was widened to cover a whole
/// expression). For spans whose extent is already correct, prefer
/// `span_full_to_range`.
pub(crate) fn span_to_range(span: Span, len: usize) -> Range {
    let line = span.line.saturating_sub(1);
    let start_char = span.col.saturating_sub(1);
    let end_char = start_char + len as u32;
    Range {
        start: Position {
            line,
            character: start_char,
        },
        end: Position {
            line,
            character: end_char,
        },
    }
}

/// Convert a 1-based ilang `Span` to a 0-based LSP `Range`, using the
/// span's recorded extent (`end_line` / `end_col`). Span's `end_col` is
/// inclusive in 1-based coords, which matches LSP's exclusive 0-based
/// end (`end.character = span.end_col`).
pub(crate) fn span_full_to_range(span: Span) -> Range {
    Range {
        start: Position {
            line: span.line.saturating_sub(1),
            character: span.col.saturating_sub(1),
        },
        end: Position {
            line: span.end_line.saturating_sub(1),
            character: span.end_col,
        },
    }
}

/// Find the identifier under the cursor by re-tokenising the source and
/// returning the first identifier whose span covers the position.
pub(crate) fn word_at(src: &str, pos: Position) -> Option<(String, u32)> {
    let tokens = tokenize(src).ok()?;
    let line = pos.line + 1;
    let col = pos.character + 1;
    for tok in &tokens {
        if let TokenKind::Ident(name) = &tok.kind {
            if tok.span.line == line {
                let start = tok.span.col;
                let end = start + name.len() as u32;
                if col >= start && col <= end {
                    return Some((name.clone(), start));
                }
            }
        }
    }
    None
}

/// Walk back from the cursor over whitespace and a leading `.` to find
/// the receiver identifier — used by completion to figure out what
/// class's members to list.
/// Sentinel receiver returned by `receiver_before_dot` when the
/// expression before `.` is a string literal (`"abc".`) — caller
/// recognises this and surfaces the built-in string methods.
pub(crate) const STR_LITERAL_RECEIVER: &str = "\"\"";

pub(crate) fn receiver_before_dot(text: &str, pos: Position) -> Option<String> {
    let line = pos.line + 1;
    let col = pos.character + 1;
    let mut off = line_col_to_offset(text, line, col)?;
    let bytes = text.as_bytes();
    if off > bytes.len() {
        return None;
    }
    while off > 0 && matches!(bytes[off - 1], b' ' | b'\t') {
        off -= 1;
    }
    if off == 0 || bytes[off - 1] != b'.' {
        return None;
    }
    off -= 1;
    while off > 0 && matches!(bytes[off - 1], b' ' | b'\t') {
        off -= 1;
    }
    // String literal: receiver ends with a closing `"`. Walk back
    // through the literal body to find the matching opening `"`
    // (respecting `\"` escapes). Return a sentinel so the completion
    // handler routes through the built-in string-method list.
    if off > 0 && bytes[off - 1] == b'"' {
        let mut i = off - 1;
        loop {
            if i == 0 {
                return None;
            }
            i -= 1;
            if bytes[i] == b'"' {
                let mut bs = 0;
                let mut k = i;
                while k > 0 && bytes[k - 1] == b'\\' {
                    bs += 1;
                    k -= 1;
                }
                if bs % 2 == 0 {
                    return Some(STR_LITERAL_RECEIVER.to_string());
                }
            }
        }
    }
    let end = off;
    while off > 0 {
        let b = bytes[off - 1];
        if b.is_ascii_alphanumeric() || b == b'_' || b == b'.' {
            off -= 1;
        } else {
            break;
        }
    }
    let s = std::str::from_utf8(&bytes[off..end]).ok()?.to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Given a signature label like `(method) Counter.init(a: i64, b: i64)`,
/// return byte-offset ranges for each parameter span. The LSP client
/// uses them to bold the active parameter.
pub(crate) fn parameter_offsets(label: &str) -> Vec<(u32, u32)> {
    let bytes = label.as_bytes();
    let Some(close) = bytes.iter().rposition(|&b| b == b')') else {
        return Vec::new();
    };
    let mut depth = 0i32;
    let mut open: Option<usize> = None;
    let mut i = close;
    loop {
        match bytes[i] {
            b')' => depth += 1,
            b'(' => {
                depth -= 1;
                if depth == 0 {
                    open = Some(i);
                    break;
                }
            }
            _ => {}
        }
        if i == 0 {
            break;
        }
        i -= 1;
    }
    let Some(open) = open else {
        return Vec::new();
    };
    if close <= open + 1 {
        return Vec::new();
    }
    let mut out: Vec<(u32, u32)> = Vec::new();
    let mut start = open + 1;
    let mut paren_depth = 0i32;
    let mut bracket_depth = 0i32;
    for i in start..close {
        let b = bytes[i];
        match b {
            b'(' => paren_depth += 1,
            b')' => paren_depth -= 1,
            b'[' => bracket_depth += 1,
            b']' => bracket_depth -= 1,
            b',' if paren_depth == 0 && bracket_depth == 0 => {
                let s = trim_offset(bytes, start, i);
                if s.0 < s.1 {
                    out.push((s.0 as u32, s.1 as u32));
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    let s = trim_offset(bytes, start, close);
    if s.0 < s.1 {
        out.push((s.0 as u32, s.1 as u32));
    }
    out
}

fn trim_offset(bytes: &[u8], mut s: usize, mut e: usize) -> (usize, usize) {
    while s < e && (bytes[s] == b' ' || bytes[s] == b'\t') {
        s += 1;
    }
    while e > s && (bytes[e - 1] == b' ' || bytes[e - 1] == b'\t') {
        e -= 1;
    }
    (s, e)
}

pub(crate) struct CallContext {
    pub callee: String,
    pub is_new: bool,
    pub arg_index: usize,
}

/// Find the `callee(...)` containing the cursor by scanning backwards
/// past balanced parens / brackets.
pub(crate) fn call_context_at(text: &str, pos: Position) -> Option<CallContext> {
    let line = pos.line + 1;
    let col = pos.character + 1;
    let mut off = line_col_to_offset(text, line, col)?;
    let bytes = text.as_bytes();
    if off > bytes.len() {
        return None;
    }
    let mut paren_depth: i32 = 0;
    let mut bracket_depth: i32 = 0;
    let mut commas: usize = 0;
    while off > 0 {
        off -= 1;
        let b = bytes[off];
        match b {
            b')' | b']' => {
                if b == b')' {
                    paren_depth += 1;
                } else {
                    bracket_depth += 1;
                }
            }
            b'(' => {
                if paren_depth > 0 {
                    paren_depth -= 1;
                } else {
                    break;
                }
            }
            b'[' => {
                if bracket_depth > 0 {
                    bracket_depth -= 1;
                }
            }
            b',' if paren_depth == 0 && bracket_depth == 0 => {
                commas += 1;
            }
            _ => {}
        }
    }
    if bytes.get(off).copied() != Some(b'(') {
        return None;
    }
    let mut i = off;
    while i > 0 && matches!(bytes[i - 1], b' ' | b'\t') {
        i -= 1;
    }
    let end = i;
    while i > 0 {
        let b = bytes[i - 1];
        if b.is_ascii_alphanumeric() || b == b'_' || b == b'.' {
            i -= 1;
        } else {
            break;
        }
    }
    let mut callee = std::str::from_utf8(&bytes[i..end]).ok()?.to_string();
    if callee.is_empty() {
        return None;
    }
    // String literal as the method receiver — `"abc".method(`. The
    // identifier walker above kept the leading `.` but couldn't enter
    // the string body. Rewrite the callee to start with the
    // `STR_LITERAL_RECEIVER` sentinel so the signature-help handler
    // routes through the built-in string-method table.
    if callee.starts_with('.') && i > 0 && bytes[i - 1] == b'"' {
        let mut k = i - 1;
        let mut found = false;
        while k > 0 {
            k -= 1;
            if bytes[k] == b'"' {
                let mut bs = 0;
                let mut q = k;
                while q > 0 && bytes[q - 1] == b'\\' {
                    bs += 1;
                    q -= 1;
                }
                if bs % 2 == 0 {
                    found = true;
                    break;
                }
            }
        }
        if found {
            callee = format!("{}{}", STR_LITERAL_RECEIVER, callee);
        }
    }
    let mut j = i;
    while j > 0 && matches!(bytes[j - 1], b' ' | b'\t') {
        j -= 1;
    }
    let is_new = j >= 3
        && &bytes[j - 3..j] == b"new"
        && {
            let prev = if j >= 4 { Some(bytes[j - 4]) } else { None };
            prev.map(|c| !c.is_ascii_alphanumeric() && c != b'_')
                .unwrap_or(true)
        };
    Some(CallContext {
        callee,
        is_new,
        arg_index: commas,
    })
}
