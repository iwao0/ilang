//! LSP "code action" entry points, split by quick-fix:
//!
//! - [`match_arms`] — `fill_match_arms_at`: cursor in a `match` whose
//!   scrutinee is an enum → emit one new arm per missing variant.
//! - [`init_gen`] — `generate_init_at`: cursor inside a `class` body
//!   that has fields but no `init` → emit a constructor that takes one
//!   param per field and assigns each to `this.field`.
//! - [`interface_impl`] — `implement_interface_methods_at` plus the
//!   bare-ident stub completions for unimplemented interface methods.
//!
//! This module keeps the two shared cursor-locating helpers
//! (`pick_innermost_containing`, `match_brace_range`) and the
//! `textDocument/codeAction` dispatcher (`handle_code_action`).

use std::collections::HashMap;

use ilang_ast::{InterfaceDecl, Span, Symbol as AstSymbol, Type};
use ilang_lexer::tokenize;
use ilang_parser::parse;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, CodeActionResponse,
    Range, TextEdit, Url, WorkspaceEdit,
};

use super::imports::organize_imports;
use super::text;
use super::text_utils::{byte_range_to_lsp_range, byte_to_position};

mod init_gen;
mod interface_impl;
mod match_arms;

pub(crate) use init_gen::generate_init_at;
pub(crate) use interface_impl::{
    implement_interface_methods_at, interface_method_stub_completions_textual,
};
pub(crate) use match_arms::fill_match_arms_at;

/// From an iterator of `(item, lo, hi)` byte ranges, return the
/// innermost one whose `[lo..=hi]` contains `cursor_byte`. "Innermost"
/// is the smallest extent, mirroring how nested scopes shrink toward
/// the cursor. Returns `None` when nothing contains the cursor.
///
/// All four cursor-anchored quick-fixes (`fill_match_arms_at`,
/// `generate_init_at`, `implement_interface_methods_at`,
/// `interface_method_stub_completions_at`) used to inline this same
/// pick-smallest-containing loop; share it here.
pub(super) fn pick_innermost_containing<T>(
    iter: impl IntoIterator<Item = (T, usize, usize)>,
    cursor_byte: usize,
) -> Option<(T, usize, usize)> {
    let mut chosen: Option<(T, usize, usize)> = None;
    for (item, lo, hi) in iter {
        if cursor_byte < lo || cursor_byte > hi {
            continue;
        }
        let extent = hi.saturating_sub(lo);
        match &chosen {
            None => chosen = Some((item, lo, hi)),
            Some((_, c_lo, c_hi)) => {
                if extent < c_hi.saturating_sub(*c_lo) {
                    chosen = Some((item, lo, hi));
                }
            }
        }
    }
    chosen
}

/// Given the span of a `match` keyword token (or any `… { … }` header
/// whose span points at the construct's first token), find the byte
/// range `[lo, hi]` of its block body, where `lo` is the byte offset
/// of the opening `{` and `hi` is the offset of the closing `}`.
pub(super) fn match_brace_range(text: &str, match_kw: Span) -> Option<(usize, usize)> {
    let off = text::line_col_to_offset(text, match_kw.line, match_kw.col)?;
    let bytes = text.as_bytes();
    let mut i = off;
    let mut depth: i32 = 0;
    let mut open: Option<usize> = None;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                if open.is_none() {
                    open = Some(i);
                }
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 && open.is_some() {
                    return Some((open.unwrap(), i));
                }
                i += 1;
            }
            b'"' => {
                // Skip string literal — match keyword can't appear inside.
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                if i < bytes.len() {
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    None
}

/// Orchestrate `textDocument/codeAction`. Tokenises + parses the
/// buffer once, then runs every quick-fix probe whose kind the
/// editor asked for. Caller is expected to have cloned the doc's
/// `text` / `var_types` / `external_interfaces` and dropped the
/// docs lock before calling — keeps parsing off the lock-held
/// critical path.
pub(crate) fn handle_code_action(
    p: &CodeActionParams,
    text: &str,
    var_types: &HashMap<AstSymbol, Type>,
    external_interfaces: &HashMap<AstSymbol, InterfaceDecl>,
) -> Option<CodeActionResponse> {
    let uri = &p.text_document.uri;
    let only = p.context.only.as_ref();
    let want_kind = |k: &CodeActionKind| match only {
        None => true,
        Some(kinds) => kinds.iter().any(|requested| {
            // Match on prefix — e.g. requesting "refactor" should
            // include "refactor.rewrite" too.
            let r = requested.as_str();
            let target = k.as_str();
            target == r || target.starts_with(&format!("{r}."))
        }),
    };
    let want_organize = want_kind(&CodeActionKind::SOURCE_ORGANIZE_IMPORTS)
        || want_kind(&CodeActionKind::SOURCE);
    let want_quickfix = want_kind(&CodeActionKind::QUICKFIX);
    if !want_organize && !want_quickfix {
        return None;
    }
    let tokens = tokenize(text).ok()?;
    let prog = parse(&tokens).ok()?;
    let mut actions: Vec<CodeActionOrCommand> = Vec::new();
    if want_organize {
        if let Some((start_byte, end_byte, new_text)) = organize_imports(text, &prog) {
            let range = byte_range_to_lsp_range(text, start_byte, end_byte);
            actions.push(quickfix_action(
                "Organize imports".into(),
                CodeActionKind::SOURCE_ORGANIZE_IMPORTS,
                uri,
                range,
                new_text,
                None,
            ));
        }
    }
    if want_quickfix {
        if let Some((insert_byte, new_text)) = generate_init_at(text, &prog, p.range.start) {
            let pos = byte_to_position(text, insert_byte);
            actions.push(quickfix_action(
                "Generate init from fields".into(),
                CodeActionKind::QUICKFIX,
                uri,
                Range { start: pos, end: pos },
                new_text,
                None,
            ));
        }
        if let Some((insert_byte, new_text, missing_count)) =
            fill_match_arms_at(text, &prog, var_types, p.range.start)
        {
            let pos = byte_to_position(text, insert_byte);
            let title = if missing_count == 1 {
                "Fill missing match arm".to_string()
            } else {
                format!("Fill {missing_count} missing match arms")
            };
            actions.push(quickfix_action(
                title,
                CodeActionKind::QUICKFIX,
                uri,
                Range { start: pos, end: pos },
                new_text,
                Some(true),
            ));
        }
        if let Some((insert_byte, new_text, missing_count)) =
            implement_interface_methods_at(text, &prog, external_interfaces, p.range.start)
        {
            let pos = byte_to_position(text, insert_byte);
            let title = if missing_count == 1 {
                "Implement missing interface method".to_string()
            } else {
                format!("Implement {missing_count} missing interface methods")
            };
            actions.push(quickfix_action(
                title,
                CodeActionKind::QUICKFIX,
                uri,
                Range { start: pos, end: pos },
                new_text,
                Some(true),
            ));
        }
    }
    if actions.is_empty() {
        None
    } else {
        Some(actions)
    }
}

fn quickfix_action(
    title: String,
    kind: CodeActionKind,
    uri: &Url,
    range: Range,
    new_text: String,
    is_preferred: Option<bool>,
) -> CodeActionOrCommand {
    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    changes.insert(uri.clone(), vec![TextEdit { range, new_text }]);
    CodeActionOrCommand::CodeAction(CodeAction {
        title,
        kind: Some(kind),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        diagnostics: None,
        is_preferred,
        disabled: None,
        data: None,
        command: None,
    })
}
