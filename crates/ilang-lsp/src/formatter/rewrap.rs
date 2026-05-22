//! Long-line rewrapping pass.
//!
//! Walks each output line and, when one exceeds `LINE_BUDGET`,
//! breaks its outermost paren / bracket group (one with top-level
//! commas) onto multiple lines (one element per line). Lines under
//! budget pass through unchanged. Idempotent: a line that's already
//! multi-line has its delimiters split across lines so this pass
//! leaves it alone.
//!
//! `finalize` is the trailing-whitespace / blank-run cleanup that
//! runs at the very end of formatting.

use super::{INDENT, LINE_BUDGET};

pub(super) fn rewrap_long_lines(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for line in s.split('\n') {
        rewrap_line_recursive(line, &mut out);
    }
    // `split('\n')` produces an extra empty trailing element when
    // the input ends in `\n` — trim it back to a single newline.
    if out.ends_with("\n\n") {
        out.pop();
    }
    out
}

/// Break `line` over budget, then re-check every line produced by
/// the break — `test.expect(some.long.call(a, b, c) as i64, 0)`
/// first breaks at the outer `test.expect(` group, leaving its
/// first sub-line `some.long.call(a, b, c) as i64,` still over the
/// budget. Without the recursion the second format pass would break
/// it instead, costing idempotence.
fn rewrap_line_recursive(line: &str, out: &mut String) {
    if line.chars().count() <= LINE_BUDGET {
        out.push_str(line);
        out.push('\n');
        return;
    }
    match try_break_long_line(line) {
        Some(broken) => {
            // `broken` is one or more `\n`-terminated lines. Recurse
            // through each so any sub-line still over budget gets
            // its own break.
            for sub in broken.split_inclusive('\n') {
                let sub_no_nl = sub.strip_suffix('\n').unwrap_or(sub);
                rewrap_line_recursive(sub_no_nl, out);
            }
        }
        None => {
            out.push_str(line);
            out.push('\n');
        }
    }
}

/// Find the outermost `(...)` / `[...]` group on `line` whose
/// content has at least one top-level comma, and break it across
/// lines (one element per line, one indent level deeper than the
/// line's leading indent). Returns the rewrapped multi-line string
/// (with trailing `\n`s) when a break point was found.
fn try_break_long_line(line: &str) -> Option<String> {
    let leading_spaces = line.chars().take_while(|c| *c == ' ').count();
    let base_depth = (leading_spaces / INDENT.len()) as i32;

    // Walk the line, tracking `(` / `[` depth and string / comment
    // mode, looking for the first outermost paren / bracket with
    // top-level commas.
    let bytes = line.as_bytes();
    let mut state = LineState::Code;
    let mut stack: Vec<(usize, u8)> = Vec::new();
    // (open_byte_idx, depth_at_open, has_top_level_comma)
    let mut group_open: Option<(usize, u8)> = None;
    let mut top_commas: Vec<usize> = Vec::new();
    // Count of `{` opened on this line that are still unmatched when
    // the rewrap target is hit. Each adds a level of indent that
    // `format_tokens` will apply on the next pass — without bumping
    // `base_depth` here, the two passes pick different indents and
    // formatting isn't idempotent (`Vertex { pos: [ ... ]` is the
    // canonical case).
    let mut open_braces: i32 = 0;
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        let next = bytes.get(i + 1).copied();
        match state {
            LineState::Code => match c {
                b'/' if next == Some(b'/') => return None, // line comment present, leave alone
                b'/' if next == Some(b'*') => {
                    state = LineState::Block(1);
                    i += 2;
                    continue;
                }
                b'"' => {
                    state = LineState::Str;
                    i += 1;
                    continue;
                }
                b'{' => {
                    open_braces += 1;
                    i += 1;
                }
                b'}' => {
                    if open_braces > 0 {
                        open_braces -= 1;
                    }
                    i += 1;
                }
                b'(' | b'[' => {
                    if stack.is_empty() && group_open.is_none() {
                        group_open = Some((i, c));
                        top_commas.clear();
                    }
                    stack.push((i, c));
                    i += 1;
                }
                b')' | b']' => {
                    let last = stack.pop();
                    if stack.is_empty() {
                        if let Some((open_idx, open_c)) = group_open.take() {
                            let want_close = if open_c == b'(' { b')' } else { b']' };
                            if last.is_some() && c == want_close && !top_commas.is_empty() {
                                return Some(emit_broken(
                                    line,
                                    open_idx,
                                    i,
                                    &top_commas,
                                    base_depth + open_braces,
                                ));
                            }
                            // Group ended without commas (e.g.
                            // `(x + y)` grouping). Keep scanning;
                            // there may be another group later on
                            // the line that's still over budget.
                        }
                    }
                    i += 1;
                }
                b',' if stack.len() == 1 && group_open.is_some() => {
                    top_commas.push(i);
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
                    continue;
                }
                if c == b'*' && next == Some(b'/') {
                    state = if d == 1 {
                        LineState::Code
                    } else {
                        LineState::Block(d - 1)
                    };
                    i += 2;
                    continue;
                }
                i += 1;
            }
        }
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LineState {
    Code,
    Str,
    Block(u32),
}

/// Build the rewrapped form: head up through `open_idx + 1`,
/// each comma-separated element on its own line at `base_depth+1`,
/// closing delimiter + tail on a fresh line at `base_depth`.
fn emit_broken(
    line: &str,
    open_idx: usize,
    close_idx: usize,
    commas: &[usize],
    base_depth: i32,
) -> String {
    let bytes = line.as_bytes();
    let head = &line[..=open_idx]; // includes the `(` / `[`
    // Element ranges between (open_idx+1) ... commas ... close_idx
    let mut elem_ranges: Vec<(usize, usize)> = Vec::new();
    let mut start = open_idx + 1;
    for &c_idx in commas {
        elem_ranges.push((start, c_idx));
        start = c_idx + 1;
    }
    elem_ranges.push((start, close_idx));
    let close_char = bytes[close_idx] as char;
    let tail = &line[close_idx + 1..];

    // Preserve the source's trailing-comma choice — both forms
    // (`f(a, b, c)` and `f(a, b, c,)`) are valid ilang, and the
    // formatter shouldn't add or drop one. The trailing slot is
    // empty exactly when the source had a trailing comma, so the
    // last element is "real" if the trimmed last range is non-empty.
    let had_trailing_comma = elem_ranges
        .last()
        .map(|&(s, e)| line[s..e].trim().is_empty())
        .unwrap_or(false);

    let inner_indent = INDENT.repeat((base_depth + 1).max(0) as usize);
    let close_indent = INDENT.repeat(base_depth.max(0) as usize);
    let mut out = String::new();
    out.push_str(head);
    out.push('\n');
    let real_elems: Vec<(usize, usize)> = elem_ranges
        .into_iter()
        .filter(|(s, e)| !line[*s..*e].trim().is_empty())
        .collect();
    let last_idx = real_elems.len().saturating_sub(1);
    for (i, (s, e)) in real_elems.iter().enumerate() {
        let elem = line[*s..*e].trim();
        out.push_str(&inner_indent);
        out.push_str(elem);
        // Inner separators always get a `,`; the trailing one is
        // emitted only if the source had one.
        if i != last_idx || had_trailing_comma {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str(&close_indent);
    out.push(close_char);
    out.push_str(tail);
    out.push('\n');
    out
}

/// Strip per-line trailing whitespace, collapse 3+ blank-line
/// runs to one blank, and ensure exactly one trailing newline.
pub(super) fn finalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut blank_run: u32 = 0;
    for line in s.split('\n') {
        let trimmed = line.trim_end_matches([' ', '\t']);
        if trimmed.is_empty() {
            blank_run += 1;
            if blank_run <= 2 {
                // up to one blank line (=> at most 2 consecutive '\n')
                out.push_str("\n");
            }
        } else {
            blank_run = 0;
            out.push_str(trimmed);
            out.push('\n');
        }
    }
    // Trim trailing blanks so file ends with exactly one newline.
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    out
}
