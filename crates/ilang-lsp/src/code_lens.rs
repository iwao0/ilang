//! `textDocument/codeLens` + `codeLens/resolve` provider.
//!
//! Two lens families:
//!   - **References**: every top-level fn / class / interface /
//!     enum / class method gets an "N references" lens that opens
//!     the references peek on click.
//!   - **Implementations**: every class / interface gets an
//!     "N implementations" lens that opens the implementation
//!     peek.
//!
//! The initial `codeLens` call returns lenses with empty
//! commands so the editor can render them quickly. The
//! `codeLens/resolve` call computes the count for one lens at a
//! time — VSCode only resolves visible ones, so a large workspace
//! doesn't blow up on each refresh.

use ilang_ast::{ClassDecl, FnDecl, Item};
use ilang_lexer::tokenize;
use ilang_parser::parse;
use serde::{Deserialize, Serialize};
use tower_lsp::lsp_types::{CodeLens, Command, Position, Range, Url};

use crate::text;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub(crate) enum LensData {
    /// "N references" lens. `name` identifies the symbol whose
    /// refs we'll count workspace-wide. The decl-name span lets
    /// us match RefEntry.target_span exactly so we don't
    /// confuse same-named members on other classes.
    References {
        uri:        Url,
        name:       String,
        decl_line:  u32,
        decl_col:   u32,
        decl_name_len: u32,
    },
    /// "N implementations" lens. Routes through the existing
    /// `implementation::Target` machinery on click.
    Implementations {
        uri:           Url,
        name:          String,
        is_interface:  bool,
        decl_line:     u32,
        decl_col:      u32,
        decl_name_len: u32,
    },
}

/// Build the unresolved lens list for `text`. Each lens carries
/// `data` so `codeLens/resolve` can recover the target without
/// re-parsing the file.
pub(crate) fn build(uri: &Url, text: &str) -> Vec<CodeLens> {
    let Ok(tokens) = tokenize(text) else { return Vec::new() };
    let Ok(prog) = parse(&tokens) else { return Vec::new() };
    let mut out = Vec::new();
    for item in &prog.items {
        push_item_lenses(uri, text, item, &mut out);
    }
    out
}

fn push_item_lenses(uri: &Url, text: &str, item: &Item, out: &mut Vec<CodeLens>) {
    match item {
        Item::Fn(f) => push_refs(uri, text, f.span, "fn", f.name.as_str(), out),
        Item::Class(c) => {
            push_class_lenses(uri, text, c, false, out);
        }
        Item::Interface(i) => {
            let name = i.name.as_str();
            push_refs(uri, text, i.span, "interface", name, out);
            push_impl(uri, text, i.span, "interface", name, true, out);
        }
        Item::Enum(e) => {
            let name = e.name.as_str();
            push_refs(uri, text, e.span, "enum", name, out);
        }
        Item::ExternC(b) => {
            for inner in b.items.iter() {
                match inner {
                    ilang_ast::ExternCItem::FnDef(f) => {
                        push_refs(uri, text, f.span, "fn", f.name.as_str(), out);
                    }
                    ilang_ast::ExternCItem::Class(c) => {
                        push_class_lenses(uri, text, c, false, out);
                    }
                    _ => {}
                }
            }
            for iface in b.interfaces.iter() {
                let name = iface.name.as_str();
                push_refs(uri, text, iface.span, "interface", name, out);
                push_impl(uri, text, iface.span, "interface", name, true, out);
            }
        }
        _ => {}
    }
}

fn push_class_lenses(
    uri: &Url,
    text: &str,
    c: &ClassDecl,
    is_interface: bool,
    out: &mut Vec<CodeLens>,
) {
    let kw = if c.is_union {
        "union"
    } else if c.is_repr_c {
        "struct"
    } else {
        "class"
    };
    let name = c.name.as_str();
    push_refs(uri, text, c.span, kw, name, out);
    push_impl(uri, text, c.span, kw, name, is_interface, out);
    // Per-method references lens. Skip `init` overloads to keep
    // the constructor area from over-decorating.
    for m in c.methods.iter() {
        if m.name.as_str() == "init" {
            continue;
        }
        push_method_refs(uri, text, m, out);
    }
    for m in c.static_methods.iter() {
        push_method_refs(uri, text, m, out);
    }
}

fn push_refs(
    uri: &Url,
    text: &str,
    decl_span: ilang_ast::Span,
    kw: &str,
    name: &str,
    out: &mut Vec<CodeLens>,
) {
    let Some(name_span) = text::locate_let_name_with_kw(text, decl_span, kw, name)
    else {
        return;
    };
    let range = text::span_to_range(name_span, name.len());
    // `target_span` in `RefEntry` is always the decl's keyword
    // span, not the name's — `Symbol.span = c.span` in
    // `symbols.rs`. Store the keyword position here so resolve
    // matches every ref site.
    let data = LensData::References {
        uri:           uri.clone(),
        name:          name.to_string(),
        decl_line:     decl_span.line,
        decl_col:      decl_span.col,
        decl_name_len: name.len() as u32,
    };
    push_lens(range, data, out);
}

fn push_method_refs(uri: &Url, text: &str, m: &FnDecl, out: &mut Vec<CodeLens>) {
    let Some(name_span) =
        text::locate_let_name_with_kw(text, m.span, "fn", m.name.as_str())
    else {
        return;
    };
    let range = text::span_to_range(name_span, m.name.as_str().len());
    let data = LensData::References {
        uri:           uri.clone(),
        name:          m.name.as_str().to_string(),
        decl_line:     m.span.line,
        decl_col:      m.span.col,
        decl_name_len: m.name.as_str().len() as u32,
    };
    push_lens(range, data, out);
}

fn push_impl(
    uri: &Url,
    text: &str,
    decl_span: ilang_ast::Span,
    kw: &str,
    name: &str,
    is_interface: bool,
    out: &mut Vec<CodeLens>,
) {
    let Some(name_span) = text::locate_let_name_with_kw(text, decl_span, kw, name)
    else {
        return;
    };
    let range = text::span_to_range(name_span, name.len());
    let data = LensData::Implementations {
        uri:           uri.clone(),
        name:          name.to_string(),
        is_interface,
        decl_line:     name_span.line,
        decl_col:      name_span.col,
        decl_name_len: name.len() as u32,
    };
    push_lens(range, data, out);
}

fn push_lens(range: Range, data: LensData, out: &mut Vec<CodeLens>) {
    out.push(CodeLens {
        range,
        command: None,
        data: Some(serde_json::to_value(data).unwrap_or(serde_json::Value::Null)),
    });
}

/// Count workspace-wide references whose decl-name span equals
/// `decl_line` / `decl_col` / `decl_name_len`. Kept here for any
/// future caller that wants just the count without the location
/// list — the live resolve path uses the LocationS form via the
/// handlers' helper.
#[allow(dead_code)]
pub(crate) fn count_references(
    target_uri: &Url,
    decl_line: u32,
    decl_col: u32,
    decl_name_len: u32,
    open_docs: &std::collections::HashMap<Url, crate::types::Doc>,
) -> usize {
    use ilang_ast::Span;
    use std::collections::HashSet;
    let target_span = Span::new(decl_line, decl_col);
    let mut count: usize = 0;
    let mut seen_paths: HashSet<std::path::PathBuf> = HashSet::new();
    for (doc_uri, d) in open_docs.iter() {
        if let Ok(p) = doc_uri.to_file_path() {
            if let Ok(c) = p.canonicalize() {
                seen_paths.insert(c);
            }
        }
        let is_owner = doc_uri == target_uri;
        count += d
            .refs
            .iter()
            .filter(|r| {
                if r.signature.starts_with("this:") { return false; }
                if r.target_name_len != decl_name_len { return false; }
                if r.target_span != target_span { return false; }
                if is_owner {
                    r.target_uri.is_none()
                } else {
                    r.target_uri.as_ref() == Some(target_uri)
                }
            })
            .count();
    }
    if let Ok(anchor_path) = target_uri.to_file_path() {
        crate::analyse::for_each_closed_workspace_doc(&anchor_path, &seen_paths, |path_uri, doc| {
            let is_owner = path_uri == *target_uri;
            count += doc
                .refs
                .iter()
                .filter(|r| {
                    if r.signature.starts_with("this:") { return false; }
                    if r.target_name_len != decl_name_len { return false; }
                    if r.target_span != target_span { return false; }
                    if is_owner {
                        r.target_uri.is_none()
                    } else {
                        r.target_uri.as_ref() == Some(target_uri)
                    }
                })
                .count();
        });
    }
    count
}

/// Decode the JSON payload back into a [`LensData`]. Returns
/// `None` when the payload doesn't match the expected shape (an
/// older lens from a previous server version).
pub(crate) fn decode_data(v: &serde_json::Value) -> Option<LensData> {
    serde_json::from_value(v.clone()).ok()
}

/// Helper for resolved commands: `editor.action.showReferences`
/// arguments are `[uri, position, locations]`. Build the
/// argument list as JSON values.
pub(crate) fn show_references_args(
    uri: &Url,
    pos: Position,
    locations: Vec<tower_lsp::lsp_types::Location>,
) -> Vec<serde_json::Value> {
    vec![
        serde_json::to_value(uri).unwrap(),
        serde_json::to_value(pos).unwrap(),
        serde_json::to_value(locations).unwrap(),
    ]
}

/// `editor.action.peekImplementation` takes `[uri, position]`.
pub(crate) fn peek_implementation_args(
    uri: &Url,
    pos: Position,
) -> Vec<serde_json::Value> {
    vec![
        serde_json::to_value(uri).unwrap(),
        serde_json::to_value(pos).unwrap(),
    ]
}

/// Build the resolved `Command` for a References lens. The
/// caller looks up the actual locations and passes them in so
/// VSCode opens the peek directly without an intermediate
/// `textDocument/references` round-trip.
pub(crate) fn references_command(
    uri: &Url,
    pos: Position,
    locations: Vec<tower_lsp::lsp_types::Location>,
) -> Command {
    let n = locations.len();
    Command {
        title: format!("{} reference{}", n, if n == 1 { "" } else { "s" }),
        command: "editor.action.showReferences".to_string(),
        arguments: Some(show_references_args(uri, pos, locations)),
    }
}

pub(crate) fn implementations_command(
    uri: &Url,
    pos: Position,
    count: usize,
) -> Command {
    Command {
        title: format!("{} implementation{}", count, if count == 1 { "" } else { "s" }),
        command: "editor.action.peekImplementation".to_string(),
        arguments: Some(peek_implementation_args(uri, pos)),
    }
}
