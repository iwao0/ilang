//! Helpers for `textDocument/references` and the CodeLens "Peek
//! References" resolve path — collect every workspace location that
//! targets a specific decl, and locate decl-name spans so navigation
//! lands on the identifier rather than the leading `class` / `fn`
//! keyword. Extracted from `handlers.rs`.

use std::collections::HashMap;
use std::path::PathBuf;

use tower_lsp::lsp_types::{Location, Position, Range, Url};

use crate::analyse::{analyse_path_to_doc, collect_workspace_il_files};
use crate::text;

/// Collect every workspace `Location` whose RefEntry targets the
/// decl identified by (`target_uri`, `target_span`, `name_len`).
/// Mirrors the inline logic in the references handler so the
/// CodeLens resolve path can populate "Peek References" without
/// going through `textDocument/references` round-tripping.
pub(crate) fn collect_reference_locations(
    target_uri: &Url,
    target_span: ilang_ast::Span,
    name_len: u32,
    snapshot: &HashMap<Url, crate::types::Doc>,
) -> Vec<Location> {
    let mut out: Vec<Location> = Vec::new();
    let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    let push = |out: &mut Vec<Location>,
                doc_uri: &Url,
                doc: &crate::types::Doc,
                is_owner: bool| {
        for r in doc.refs.iter() {
            if r.signature.starts_with("this:") { continue; }
            if r.target_span != target_span || r.target_name_len != name_len { continue; }
            let matches = if is_owner {
                r.target_uri.is_none()
            } else {
                r.target_uri.as_ref() == Some(target_uri)
            };
            if !matches { continue; }
            out.push(Location {
                uri: doc_uri.clone(),
                range: Range {
                    start: Position {
                        line: r.line.saturating_sub(1),
                        character: r.start_col.saturating_sub(1),
                    },
                    end: Position {
                        line: r.line.saturating_sub(1),
                        character: r.end_col.saturating_sub(1),
                    },
                },
            });
        }
    };
    for (doc_uri, doc) in snapshot.iter() {
        if let Ok(p) = doc_uri.to_file_path() {
            if let Ok(c) = p.canonicalize() {
                seen.insert(c);
            }
        }
        let is_owner = doc_uri == target_uri;
        push(&mut out, doc_uri, doc, is_owner);
    }
    if let Ok(anchor) = target_uri.to_file_path() {
        for path in collect_workspace_il_files(&anchor) {
            if let Ok(c) = path.canonicalize() {
                if seen.contains(&c) { continue; }
            }
            let Some(doc) = analyse_path_to_doc(&path) else { continue };
            let Ok(uri) = Url::from_file_path(&path) else { continue };
            let is_owner = uri == *target_uri;
            push(&mut out, &uri, &doc, is_owner);
        }
    }
    out.sort_by(|a, b| {
        (a.uri.as_str(), a.range.start.line, a.range.start.character)
            .cmp(&(b.uri.as_str(), b.range.start.line, b.range.start.character))
    });
    out.dedup_by(|a, b| a.uri == b.uri && a.range == b.range);
    out
}

/// Locate the identifier inside the decl at `decl_span` so F12 /
/// implementation / etc. land on the name itself rather than the
/// `class` / `fn` / `enum` keyword (or the leading `@attr`). When
/// the target file is open in `current_uri`, reuse `current_text`
/// to avoid disk IO; otherwise read the target from disk. Falls
/// back to a span at `decl_span` with `name_len` characters when
/// the locate misses.
pub(crate) fn locate_decl_name_range(
    target_uri: &Url,
    current_uri: &Url,
    current_text: &str,
    decl_span: ilang_ast::Span,
    name: &str,
    name_len: usize,
) -> Range {
    let owned;
    let target_text: &str = if target_uri == current_uri {
        current_text
    } else {
        owned = target_uri
            .to_file_path()
            .ok()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        &owned
    };
    if !name.is_empty() {
        if let Some(span) = scan_decl_name(target_text, decl_span, name) {
            return text::span_to_range(span, name.len());
        }
    }
    text::span_to_range(decl_span, name_len)
}

/// Forward-scan `text` from `decl_span` for the identifier `name`
/// preceded by a decl keyword (`class` / `fn` / ...). Tolerates
/// any leading attribute / modifier sequence (`@objc pub static
/// fn …`) by walking past them token-by-token until a keyword
/// matches, then matching the name on the same line. Returns the
/// span of the matched identifier when found.
pub(crate) fn scan_decl_name(
    text: &str,
    decl_span: ilang_ast::Span,
    name: &str,
) -> Option<ilang_ast::Span> {
    let bytes = text.as_bytes();
    let start = text::line_col_to_offset(text, decl_span.line, decl_span.col)?;
    let name_bytes = name.as_bytes();
    let keywords: &[&str] = &[
        "class",
        "interface",
        "enum",
        "struct",
        "union",
        "const",
        "fn",
    ];
    let mut i = start;
    // Bounded scan — decls rarely have more than a couple of
    // attribute / modifier tokens before the keyword. Cap so a
    // pathological line can't blow up the search.
    let limit = (start + 256).min(bytes.len());
    while i < limit {
        // Try each keyword at position i.
        for kw in keywords {
            let kb = kw.as_bytes();
            if i + kb.len() > bytes.len() {
                continue;
            }
            if &bytes[i..i + kb.len()] != kb {
                continue;
            }
            // Keyword's right boundary must be a word break so
            // `class` doesn't match the middle of `classified`.
            let after_kw = bytes.get(i + kb.len()).copied().unwrap_or(b' ');
            if after_kw.is_ascii_alphanumeric() || after_kw == b'_' {
                continue;
            }
            let mut j = i + kb.len();
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            if j + name_bytes.len() > bytes.len() {
                continue;
            }
            if &bytes[j..j + name_bytes.len()] != name_bytes {
                continue;
            }
            let after_name = bytes.get(j + name_bytes.len()).copied().unwrap_or(b' ');
            if after_name.is_ascii_alphanumeric() || after_name == b'_' {
                continue;
            }
            let (line, col) = text::offset_to_line_col(text, j)?;
            return Some(ilang_ast::Span::new(line, col));
        }
        // Stop at the end of the decl's first source line — the
        // name lives in the header, not in the body.
        if bytes[i] == b'\n' {
            break;
        }
        i += 1;
    }
    None
}
