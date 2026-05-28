//! Word / identifier helpers around the cursor: the typed prefix, the
//! word under a position, fuzzy subsequence matching, and the
//! identifier / keyword classifiers.

use ilang_lexer::{tokenize, TokenKind};
use tower_lsp::lsp_types::Position;


/// Walk `text` back from byte offset `off` over alphanumeric +
/// underscore characters and return the resulting identifier prefix
/// (empty when `off` is not preceded by an ident char). Used by the
/// completion handler to know what the user has typed so far so it
/// can hand VSCode a `filter_text` that's guaranteed to match.
pub(crate) fn typed_prefix_at(text: &str, off: usize) -> String {
    let bytes = text.as_bytes();
    let end = off.min(bytes.len());
    let mut i = end;
    while i > 0 {
        let b = bytes[i - 1];
        if b.is_ascii_alphanumeric() || b == b'_' {
            i -= 1;
        } else {
            break;
        }
    }
    std::str::from_utf8(&bytes[i..end]).unwrap_or("").to_string()
}

/// `true` when every character of `needle` (already lowercased)
/// appears in `haystack` in order, case-insensitively. Cheap
/// subsequence check the LSP uses to decide whether a label is a
/// plausible match for the typed prefix before handing it to the
/// client.
pub(crate) fn subsequence_ci(haystack: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return true;
    }
    let mut needle = needle_lower.chars();
    let mut want = match needle.next() {
        Some(c) => c,
        None => return true,
    };
    for h in haystack.chars().flat_map(|c| c.to_lowercase()) {
        if h == want {
            match needle.next() {
                Some(c) => want = c,
                None => return true,
            }
        }
    }
    false
}


/// Read the source word sitting between the 1-based `start_col`
/// and `end_col` (exclusive) on `line`. Returns `None` when the
/// line / columns don't index a real slice (e.g. malformed span).
pub(crate) fn read_word_at(text: &str, line: u32, start_col: u32, end_col: u32) -> Option<String> {
    let line_str = text.lines().nth(line.checked_sub(1)? as usize)?;
    let s = start_col.checked_sub(1)? as usize;
    let e = end_col.checked_sub(1)? as usize;
    line_str.get(s..e).map(|x| x.to_string())
}

/// `true` when `s` is a syntactically valid ilang identifier:
/// non-empty, first char is ASCII letter or `_`, rest is ASCII
/// alphanumeric or `_`. Kept ASCII-only to match the lexer.
pub(crate) fn is_valid_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `true` when `s` is one of ilang's reserved keywords. Used by
/// rename validation to refuse `class`, `if`, etc. as a new name.
pub(crate) fn is_keyword(s: &str) -> bool {
    matches!(
        s,
        "fn" | "class"
            | "interface"
            | "enum"
            | "use"
            | "super"
            | "override"
            | "init"
            | "deinit"
            | "static"
            | "get"
            | "set"
            | "let"
            | "const"
            | "if"
            | "elif"
            | "else"
            | "while"
            | "loop"
            | "for"
            | "in"
            | "match"
            | "new"
            | "as"
            | "true"
            | "false"
            | "none"
            | "some"
            | "return"
            | "break"
            | "continue"
            | "this"
            | "pub"
            | "struct"
            | "union"
            | "async"
            | "await"
    )
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
