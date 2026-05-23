//! `textDocument/rename` orchestration — resolves the cursor to a
//! decl identity, runs scope-conflict detection, then walks open
//! docs + the on-disk workspace collecting cross-file edits.
//! Extracted from `handlers.rs` so the LSP trait impl only has to
//! thread arguments through.

use std::collections::HashMap;
use std::path::PathBuf;

use ilang_ast::Symbol as AstSymbol;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{Position, TextEdit, Url, WorkspaceEdit};

use crate::analyse::{for_each_closed_workspace_doc, lookup_ref};
use crate::rename_conflicts;
use crate::text::{self, is_keyword, is_valid_identifier, read_word_at, word_at};
use crate::types::Doc;

pub(crate) fn handle_rename(
    docs: &HashMap<Url, Doc>,
    uri: &Url,
    pos: Position,
    new_name: String,
) -> LspResult<Option<WorkspaceEdit>> {
    // Validate the proposed name before touching any buffers.
    // Reporting an LSP error here lets VSCode show the message
    // to the user instead of silently accepting an invalid name
    // and producing un-parseable source.
    if !is_valid_identifier(&new_name) {
        return Err(tower_lsp::jsonrpc::Error::invalid_params(format!(
            "`{new_name}` is not a valid ilang identifier"
        )));
    }
    if is_keyword(&new_name) {
        return Err(tower_lsp::jsonrpc::Error::invalid_params(format!(
            "`{new_name}` is a reserved keyword"
        )));
    }
    let Some(doc) = docs.get(uri) else {
        return Ok(None);
    };
    // Resolve the cursor to a target identity:
    //   (decl_uri, decl_span, name_len)
    // — the file + position + length that uniquely identify the
    // decl every reference points at. When the cursor is on a
    // `use module` import the target lives in another file, so
    // we read its URI from the `RefEntry`.
    let (target_uri, target, decl_name_span, target_sig, target_old_name) =
        if let Some(entry) = lookup_ref(doc, pos) {
            // `this` is a keyword — its RefEntry shares (target_span,
            // target_name_len) with the enclosing class, so letting
            // the rename through would also rewrite every reference
            // to the class. Refuse instead of silently corrupting
            // the file.
            if entry.signature.starts_with("this:") {
                return Ok(None);
            }
            let owner = entry.target_uri.clone().unwrap_or_else(|| uri.clone());
            // Read the old name straight out of the ref's text.
            let old_name = read_word_at(
                &doc.text,
                entry.line,
                entry.start_col,
                entry.end_col,
            )
            .unwrap_or_default();
            (
                owner,
                (entry.target_span, entry.target_name_len),
                entry.target_span,
                entry.signature.clone(),
                old_name,
            )
        } else if let Some((word, _)) = word_at(&doc.text, pos) {
            if let Some(sym) = doc.symbols.get(&AstSymbol::intern(&word)) {
                let name_span = ["fn", "class", "enum", "const", "struct", "union", "interface"]
                    .iter()
                    .find_map(|kw| {
                        text::locate_let_name_with_kw(
                            &doc.text, sym.span, kw, &sym.name,
                        )
                    })
                    .unwrap_or(sym.span);
                (
                    uri.clone(),
                    (sym.span, sym.name.as_str().len() as u32),
                    name_span,
                    sym.signature.clone(),
                    sym.name.clone(),
                )
            } else {
                return Ok(None);
            }
        } else {
            return Ok(None);
        };

    // Semantic scope-conflict check. Bail with an
    // `invalid_params` error so the editor surfaces the message
    // instead of silently corrupting the source.
    let conflict_doc = if target_uri == *uri {
        Some(doc)
    } else {
        docs.get(&target_uri)
    };
    if let Some(target_doc) = conflict_doc {
        if let Err(msg) = rename_conflicts::detect(
            target_doc,
            &target_sig,
            decl_name_span,
            &target_old_name,
            &new_name,
        ) {
            return Err(tower_lsp::jsonrpc::Error::invalid_params(msg));
        }
    }

    // Collect edits per file. For the decl's owning file we
    // also include the decl-site edit; ref-only files only get
    // their cross-file references rewritten.
    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();

    // Track which paths we've already covered via open docs so
    // the workspace walk doesn't double-count them.
    let opened_paths: std::collections::HashSet<PathBuf> = docs
        .keys()
        .filter_map(|u| u.to_file_path().ok())
        .filter_map(|p| p.canonicalize().ok())
        .collect();
    for (doc_uri, d) in docs.iter() {
        let is_owner = doc_uri == &target_uri;
        let edits = collect_doc_edits(d, &target_uri, target, decl_name_span, &new_name, is_owner);
        if !edits.is_empty() {
            changes.insert(doc_uri.clone(), edits);
        }
    }
    // Workspace walk: also pick up references in `.il` files
    // that aren't currently open in the editor. Anchored on
    // the decl's owning file so the walk starts in the same
    // project (`ilang.toml` directory, or the file's parent).
    if let Ok(anchor_path) = target_uri.to_file_path() {
        for_each_closed_workspace_doc(&anchor_path, &opened_paths, |path_uri, doc| {
            let is_owner = path_uri == target_uri;
            let edits = collect_doc_edits(&doc, &target_uri, target, decl_name_span, &new_name, is_owner);
            if !edits.is_empty() {
                changes.insert(path_uri, edits);
            }
        });
    }
    if changes.is_empty() {
        return Ok(None);
    }
    Ok(Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    }))
}

fn collect_doc_edits(
    d: &Doc,
    target_uri: &Url,
    target: (ilang_ast::Span, u32),
    decl_name_span: ilang_ast::Span,
    new_name: &str,
    is_owner: bool,
) -> Vec<TextEdit> {
    let mut edits: Vec<TextEdit> = d
        .refs
        .iter()
        .filter(|r| {
            if r.signature.starts_with("this:") {
                return false;
            }
            if r.target_span != target.0 || r.target_name_len != target.1 {
                return false;
            }
            if is_owner {
                // Local refs in the decl's own file have
                // `target_uri == None`. Cross-file refs here
                // would point at OTHER files, not this one — skip.
                r.target_uri.is_none()
            } else {
                // From another file, the ref must explicitly point
                // back at the decl's owning URI.
                r.target_uri.as_ref() == Some(target_uri)
            }
        })
        .map(|r| TextEdit {
            range: r.lsp_range(),
            new_text: new_name.to_string(),
        })
        .collect();
    if is_owner {
        // Always include the decl site itself. Without this an
        // unused decl would yield zero edits in its own file and
        // VSCode would refuse the rename.
        edits.push(TextEdit {
            range: text::span_to_range(decl_name_span, target.1 as usize),
            new_text: new_name.to_string(),
        });
    }
    edits.sort_by(|a, b| {
        (a.range.start.line, a.range.start.character)
            .cmp(&(b.range.start.line, b.range.start.character))
    });
    edits.dedup_by(|a, b| a.range == b.range);
    edits
}
