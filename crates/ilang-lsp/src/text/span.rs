//! Source ↔ offset conversions and `Span` / `Range` / `Position`
//! builders. The bottom of the LSP's text toolbox: every other text
//! helper resolves a 1-based `Span` to a byte offset through
//! `line_col_to_offset` (and back via `offset_to_line_col`).

use ilang_ast::{Program, Span};
use ilang_lexer::tokenize;
use ilang_parser::parse;
use tower_lsp::lsp_types::{Position, Range};

use crate::text_utils::compute_line_starts;


/// Tokenise + parse `text` into a `Program`. Returns `None` if either
/// step fails — the lex/parse error itself is discarded since LSP
/// passes that rely on a fresh parse here just want best-effort.
/// Callers that need the diagnostic information should call
/// `tokenize` / `parse` directly.
pub(crate) fn try_parse(text: &str) -> Option<Program> {
    let tokens = tokenize(text).ok()?;
    parse(&tokens).ok()
}

/// Convert a 1-based (line, col) into a byte offset into `text`.
///
/// `col` is a 1-based **character** index (matches the lexer's
/// `Span.col`, which increments per `char` rather than per byte —
/// see `ilang_lexer::scanner`). Multi-byte UTF-8 columns therefore
/// resolve to the byte where the *char* sits, not `line_start + col`.
pub(crate) fn line_col_to_offset(text: &str, line: u32, col: u32) -> Option<usize> {
    line_col_to_offset_at(&compute_line_starts(text), text, line, col)
}

/// Table-driven `line_col_to_offset`. `line_starts[k]` is the byte
/// offset of line `k+1`'s first character (the layout produced by
/// `compute_line_starts`). Finding the line is O(1); only the
/// within-line character walk for `col` remains. Callers that resolve
/// many positions against the same buffer (the ref walker, inlay
/// hints) build the table once and reuse it instead of paying a
/// from-byte-0 scan per lookup.
pub(crate) fn line_col_to_offset_at(
    line_starts: &[usize],
    text: &str,
    line: u32,
    col: u32,
) -> Option<usize> {
    let line_start = *line_starts.get((line.checked_sub(1)?) as usize)?;
    let target_col = col.saturating_sub(1) as usize;
    let mut byte = line_start;
    let mut walked = 0usize;
    for c in text[line_start..].chars() {
        if walked >= target_col || c == '\n' {
            return Some(byte);
        }
        byte += c.len_utf8();
        walked += 1;
    }
    Some(byte)
}

/// `true` when the source text starting at `span` (1-based line /
/// col) begins with `name`. Used to drop parser-synthesised refs
/// whose AST span borrows a nearby user span but doesn't actually
/// hold the callee text — those would otherwise hijack hover on
/// neighbouring identifiers. `line_starts` is the precomputed
/// line-start table for `text` (see `compute_line_starts`).
pub(crate) fn text_at_span_starts_with_at(
    line_starts: &[usize],
    text: &str,
    span: Span,
    name: &str,
) -> bool {
    let Some(off) = line_col_to_offset_at(line_starts, text, span.line, span.col) else {
        return false;
    };
    text.as_bytes()
        .get(off..off + name.len())
        .map(|s| s == name.as_bytes())
        .unwrap_or(false)
}

/// Convert a byte offset to a 1-based `(line, col)` (the inverse of
/// `line_col_to_offset`). `col` is a 1-based **character** index
/// (matches the lexer's `Span.col`). The line is found by a binary
/// search over `line_starts`; only the within-line character count
/// for the column remains.
pub(crate) fn offset_to_line_col_at(
    line_starts: &[usize],
    text: &str,
    offset: usize,
) -> Option<(u32, u32)> {
    if offset > text.len() {
        return None;
    }
    // `line_starts[0]` is always 0, so at least one start is `<= offset`
    // and `line` is `>= 1`.
    let line = line_starts.partition_point(|&s| s <= offset);
    let line_start = line_starts[line - 1];
    let col = text[line_start..offset].chars().count() as u32 + 1;
    Some((line as u32, col))
}


/// Convert a 1-based `(line, col)` pair (the lexer's coord system)
/// to a 0-based LSP `Position`. Used at the many sites that thread
/// loose line / col integers through without a full `Span` in hand.
pub(crate) fn lsp_position(line: u32, col: u32) -> Position {
    Position {
        line: line.saturating_sub(1),
        character: col.saturating_sub(1),
    }
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


/// Walk back from `offset` to the byte position of the start of its
/// containing line — either the byte just after the previous `\n`, or
/// `0` if `offset` lies on the first line. Used by code-action
/// quick-fixes that need to copy the indentation of a closing-brace
/// line into newly-generated code.
pub(crate) fn line_start_before(text: &str, offset: usize) -> usize {
    let bytes = text.as_bytes();
    let mut i = offset.min(bytes.len());
    while i > 0 && bytes[i - 1] != b'\n' {
        i -= 1;
    }
    i
}
