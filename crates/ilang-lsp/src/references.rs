//! Helpers for `textDocument/references` and the CodeLens "Peek
//! References" resolve path — collect every workspace location that
//! targets a specific decl, and locate decl-name spans so navigation
//! lands on the identifier rather than the leading `class` / `fn`
//! keyword. Extracted from `handlers.rs`.

use std::collections::HashMap;
use std::path::PathBuf;

use ilang_ast::Symbol as AstSymbol;
use tower_lsp::lsp_types::{Location, Position, Range, Url};

use crate::analyse::{for_each_closed_workspace_doc, lookup_ref};
use crate::text::{self, word_at};
use crate::types::Doc;

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
    cache: Option<&crate::types::ClosedDocCache>,
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
                range: r.lsp_range(),
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
        for_each_closed_workspace_doc(&anchor, &seen, cache, |uri, doc| {
            let is_owner = uri == *target_uri;
            push(&mut out, &uri, doc, is_owner);
        });
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
    // Build the line-start table once; the two coordinate conversions
    // below would otherwise each rescan the buffer from byte 0.
    let line_starts = crate::text_utils::compute_line_starts(text);
    let start = text::line_col_to_offset_at(&line_starts, text, decl_span.line, decl_span.col)?;
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
            let (line, col) = text::offset_to_line_col_at(&line_starts, text, j)?;
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

pub(crate) fn handle_references(
    docs: &HashMap<Url, Doc>,
    uri: &Url,
    pos: Position,
    include_decl: bool,
    cache: Option<&crate::types::ClosedDocCache>,
) -> Option<Vec<Location>> {
    let doc = docs.get(uri)?;
    // Resolve the cursor to the same (decl_uri, decl_span, name_len,
    // decl_name_span) tuple `rename` uses; the only difference is
    // we collect `Location`s instead of `TextEdit`s.
    let (target_uri, target, decl_name_span) = if let Some(entry) = lookup_ref(doc, pos) {
        if entry.signature.starts_with("this:") {
            return None;
        }
        let owner = entry.target_uri.clone().unwrap_or_else(|| uri.clone());
        // `target_span` lands on the decl keyword (`class` / `fn`).
        // Re-scan the decl header so the decl-site location points
        // at the identifier, not the leading `class ` slice. For
        // cross-file refs we look up the target doc when it's open
        // in the snapshot; closed files fall through to the keyword
        // span (we'd need a disk read to do better here).
        let name = text::read_word_at(
            &doc.text, entry.line, entry.start_col, entry.end_col,
        )
        .unwrap_or_default();
        let target_text: Option<&str> = if entry.target_uri.is_none() {
            Some(doc.text.as_str())
        } else {
            docs.get(&owner).map(|d| d.text.as_str())
        };
        let name_span = match (target_text, name.is_empty()) {
            (Some(t), false) => {
                scan_decl_name(t, entry.target_span, &name).unwrap_or(entry.target_span)
            }
            _ => entry.target_span,
        };
        (owner, (entry.target_span, entry.target_name_len), name_span)
    } else if let Some((word, _)) = word_at(&doc.text, pos) {
        let sym = doc.symbols.get(&AstSymbol::intern(&word))?;
        let name_span = ["fn", "class", "enum", "const"]
            .iter()
            .find_map(|kw| {
                text::locate_let_name_with_kw(&doc.text, sym.span, kw, &sym.name)
            })
            .unwrap_or(sym.span);
        (uri.clone(), (sym.span, sym.name.as_str().len() as u32), name_span)
    } else {
        return None;
    };

    let mut locations: Vec<Location> = Vec::new();
    let opened_paths: std::collections::HashSet<PathBuf> = docs
        .keys()
        .filter_map(|u| u.to_file_path().ok())
        .filter_map(|p| p.canonicalize().ok())
        .collect();
    for (doc_uri, d) in docs.iter() {
        let is_owner = doc_uri == &target_uri;
        push_ref_locations(&mut locations, doc_uri, d, &target_uri, target);
        if is_owner && include_decl {
            locations.push(decl_location(doc_uri, decl_name_span, target.1));
        }
    }
    if let Ok(anchor_path) = target_uri.to_file_path() {
        for_each_closed_workspace_doc(&anchor_path, &opened_paths, cache, |path_uri, d| {
            let is_owner = path_uri == target_uri;
            push_ref_locations(&mut locations, &path_uri, d, &target_uri, target);
            if is_owner && include_decl {
                locations.push(decl_location(&path_uri, decl_name_span, target.1));
            }
        });
    }
    // Stable, de-duplicated output.
    locations.sort_by(|a, b| {
        (a.uri.as_str(), a.range.start.line, a.range.start.character)
            .cmp(&(b.uri.as_str(), b.range.start.line, b.range.start.character))
    });
    locations.dedup_by(|a, b| a.uri == b.uri && a.range == b.range);
    if locations.is_empty() {
        None
    } else {
        Some(locations)
    }
}

fn push_ref_locations(
    out: &mut Vec<Location>,
    doc_uri: &Url,
    d: &Doc,
    target_uri: &Url,
    target: (ilang_ast::Span, u32),
) {
    let is_owner = doc_uri == target_uri;
    for r in d.refs.iter() {
        if r.signature.starts_with("this:") { continue; }
        if r.target_span != target.0 || r.target_name_len != target.1 { continue; }
        let matches = if is_owner {
            r.target_uri.is_none()
        } else {
            r.target_uri.as_ref() == Some(target_uri)
        };
        if !matches { continue; }
        out.push(Location {
            uri: doc_uri.clone(),
            range: r.lsp_range(),
        });
    }
}

fn decl_location(uri: &Url, decl_name_span: ilang_ast::Span, name_len: u32) -> Location {
    Location {
        uri: uri.clone(),
        range: text::span_to_range(decl_name_span, name_len as usize),
    }
}

pub(crate) fn handle_document_highlight(
    doc: &Doc,
    pos: Position,
) -> Option<Vec<tower_lsp::lsp_types::DocumentHighlight>> {
    use tower_lsp::lsp_types::{DocumentHighlight, DocumentHighlightKind};

    // Resolve the cursor to the same (target_span, name_len) the
    // rename / references handlers use, then collect every in-file
    // ref pointing at that target. Decl-name span is included so
    // the cursor on the decl itself still highlights its uses below.
    // `decl_in_this_file` flags whether the decl actually lives here
    // — the cross-file case skips the decl hit entirely.
    let (target, decl_name_span, decl_name_len, decl_in_this_file) =
        if let Some(entry) = lookup_ref(doc, pos) {
            if entry.signature.starts_with("this:") {
                return None;
            }
            // `target_span` is the decl keyword (`class` / `fn` / ...).
            // Re-scan the decl header for the actual name so the
            // decl-site hit lands on the identifier instead of the
            // `class ` slice that the keyword span covers.
            let local = entry.target_uri.is_none();
            let name_span = if local {
                let name = text::read_word_at(
                    &doc.text, entry.line, entry.start_col, entry.end_col,
                )
                .unwrap_or_default();
                if name.is_empty() {
                    entry.target_span
                } else {
                    scan_decl_name(&doc.text, entry.target_span, &name)
                        .unwrap_or(entry.target_span)
                }
            } else {
                entry.target_span
            };
            (
                (entry.target_span, entry.target_name_len),
                name_span,
                entry.target_name_len,
                local,
            )
        } else if let Some((word, _)) = word_at(&doc.text, pos) {
            let sym = doc.symbols.get(&AstSymbol::intern(&word))?;
            let name_span = ["fn", "class", "enum", "const", "struct", "union", "interface"]
                .iter()
                .find_map(|kw| {
                    text::locate_let_name_with_kw(&doc.text, sym.span, kw, &sym.name)
                })
                .unwrap_or(sym.span);
            (
                (sym.span, sym.name.as_str().len() as u32),
                name_span,
                sym.name.as_str().len() as u32,
                true,
            )
        } else {
            return None;
        };
    let mut hits: Vec<DocumentHighlight> = Vec::new();
    // Cross-file refs (where `target_uri` is set) point at a decl in
    // another file — skip them, document highlight is local.
    for r in &doc.refs {
        if r.signature.starts_with("this:") {
            continue;
        }
        if r.target_uri.is_some() {
            continue;
        }
        if r.target_span != target.0 || r.target_name_len != target.1 {
            continue;
        }
        hits.push(DocumentHighlight {
            range: r.lsp_range(),
            kind: Some(DocumentHighlightKind::TEXT),
        });
    }
    // Include the decl itself when we can locate it in this file.
    if decl_in_this_file {
        hits.push(DocumentHighlight {
            range: text::span_to_range(decl_name_span, decl_name_len as usize),
            kind: Some(DocumentHighlightKind::TEXT),
        });
    }
    hits.sort_by(|a, b| {
        (a.range.start.line, a.range.start.character)
            .cmp(&(b.range.start.line, b.range.start.character))
    });
    hits.dedup_by(|a, b| a.range == b.range);
    if hits.is_empty() {
        None
    } else {
        Some(hits)
    }
}
