//! `textDocument/prepareCallHierarchy` + `callHierarchy/{incoming,outgoing}Calls`.
//!
//! Picks up a function / method / static method under the cursor,
//! then resolves its callers (incoming) or callees (outgoing) by
//! cross-referencing the LSP's per-file `RefEntry` index against
//! per-function span ranges harvested from each file's parsed AST.

use std::collections::HashMap;
use std::path::PathBuf;

use ilang_ast::{ClassDecl, FnDecl, Item, Program, Span, Symbol as AstSymbol};
use ilang_lexer::tokenize;
use ilang_parser::parse;
use serde_json::json;
use tower_lsp::lsp_types::*;

use crate::types::{Doc, RefEntry};
use crate::{analyse_path_to_doc, collect_workspace_il_files, text};

/// Self-contained identity for a function / method, plumbed through
/// the `CallHierarchyItem.data` field so incoming/outgoing requests
/// can recover the target without re-resolving by name.
#[derive(Clone, Debug)]
pub(crate) struct ItemRef {
    pub uri:             Url,
    pub name:            String,
    /// Class name when the item is a method / static method, `None`
    /// for top-level fns. Used to render the detail string and to
    /// disambiguate identically-named methods on different classes
    /// during workspace ref filtering.
    pub container:       Option<String>,
    /// Decl-name span (1-based, matches RefEntry.target_span).
    pub decl_name_span:  Span,
    pub decl_name_len:   u32,
    pub kind:            SymbolKind,
    pub is_static:       bool,
}

impl ItemRef {
    fn to_data(&self) -> serde_json::Value {
        json!({
            "uri": self.uri.as_str(),
            "name": self.name,
            "container": self.container,
            "decl_line": self.decl_name_span.line,
            "decl_col": self.decl_name_span.col,
            "decl_end_line": self.decl_name_span.end_line,
            "decl_end_col": self.decl_name_span.end_col,
            "decl_name_len": self.decl_name_len,
            "kind": symbol_kind_to_u8(self.kind),
            "is_static": self.is_static,
        })
    }
    pub(crate) fn from_data(v: &serde_json::Value) -> Option<Self> {
        let uri_s = v.get("uri")?.as_str()?;
        let uri = Url::parse(uri_s).ok()?;
        let name = v.get("name")?.as_str()?.to_string();
        let container = v.get("container")
            .and_then(|x| if x.is_null() { None } else { x.as_str().map(|s| s.to_string()) });
        let decl_line = v.get("decl_line")?.as_u64()? as u32;
        let decl_col = v.get("decl_col")?.as_u64()? as u32;
        let decl_end_line = v.get("decl_end_line")?.as_u64()? as u32;
        let decl_end_col = v.get("decl_end_col")?.as_u64()? as u32;
        let decl_name_len = v.get("decl_name_len")?.as_u64()? as u32;
        let kind = u8_to_symbol_kind(v.get("kind")?.as_u64()? as u8);
        let is_static = v.get("is_static")?.as_bool().unwrap_or(false);
        Some(Self {
            uri,
            name,
            container,
            decl_name_span: Span::range(decl_line, decl_col, decl_end_line, decl_end_col),
            decl_name_len,
            kind,
            is_static,
        })
    }
    pub(crate) fn to_item(&self, full_range: Range) -> CallHierarchyItem {
        let detail = match &self.container {
            Some(c) => {
                let prefix = if self.is_static { "static " } else { "" };
                Some(format!("{prefix}{c}.{}", self.name))
            }
            None => None,
        };
        CallHierarchyItem {
            name: self.name.clone(),
            kind: self.kind,
            tags: None,
            detail,
            uri: self.uri.clone(),
            range: full_range,
            selection_range: text::span_to_range(
                self.decl_name_span,
                self.decl_name_len as usize,
            ),
            data: Some(self.to_data()),
        }
    }
}

fn symbol_kind_to_u8(k: SymbolKind) -> u8 {
    // CallHierarchyItem only carries FUNCTION / METHOD / CONSTRUCTOR.
    // Compress to a single byte so the JSON payload stays small.
    match k {
        SymbolKind::FUNCTION => 1,
        SymbolKind::METHOD => 2,
        SymbolKind::CONSTRUCTOR => 3,
        _ => 1,
    }
}
fn u8_to_symbol_kind(b: u8) -> SymbolKind {
    match b {
        2 => SymbolKind::METHOD,
        3 => SymbolKind::CONSTRUCTOR,
        _ => SymbolKind::FUNCTION,
    }
}

/// One callable in a file: top-level fn, class method, or static
/// method. `body_full_range` covers the entire decl (including
/// signature) so we can detect refs sitting inside it.
struct FnRange {
    name:            String,
    container:       Option<String>,
    is_static:       bool,
    name_span:       Span,
    name_len:        u32,
    /// 1-based inclusive line range covering the whole decl span.
    body_start_line: u32,
    body_end_line:   u32,
    full_range:      Range,
    kind:            SymbolKind,
}

fn collect_fn_ranges(text: &str, prog: &Program) -> Vec<FnRange> {
    let mut out: Vec<FnRange> = Vec::new();
    for item in &prog.items {
        match item {
            Item::Fn(f) => push_fn(text, f, None, false, &mut out),
            Item::Class(c) => push_class_methods(text, c, &mut out),
            Item::ExternC(b) => {
                for inner in b.items.iter() {
                    match inner {
                        ilang_ast::ExternCItem::FnDef(f) => {
                            push_fn(text, f, None, false, &mut out);
                        }
                        ilang_ast::ExternCItem::Class(c) => {
                            push_class_methods(text, c, &mut out);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn push_fn(
    text: &str,
    f: &FnDecl,
    container: Option<&str>,
    is_static: bool,
    out: &mut Vec<FnRange>,
) {
    let name_span = text::locate_let_name_with_kw(text, f.span, "fn", f.name.as_str())
        .unwrap_or(f.span);
    let kind = if f.name.as_str() == "init" && container.is_some() {
        SymbolKind::CONSTRUCTOR
    } else if container.is_some() {
        SymbolKind::METHOD
    } else {
        SymbolKind::FUNCTION
    };
    // FnDecl.span typically points at the `fn` keyword without a
    // useful `end_line`. Recover the body's last line from the
    // block's last stmt / tail expression — without this, body refs
    // sit outside the range and outgoing calls comes back empty.
    let body_end_line = fn_body_end_line(f);
    // LSP ranges are half-open: end at column 0 of the line after
    // the body's last line so the whole decl is contained.
    let full_range = Range {
        start: text::span_to_range(f.span, 0).start,
        end: Position {
            line: body_end_line,
            character: 0,
        },
    };
    out.push(FnRange {
        name: f.name.as_str().to_string(),
        container: container.map(|s| s.to_string()),
        is_static,
        name_span,
        name_len: f.name.as_str().len() as u32,
        body_start_line: f.span.line,
        body_end_line,
        full_range,
        kind,
    });
}

fn fn_body_end_line(f: &FnDecl) -> u32 {
    let mut last = f.span.end_line.max(f.span.line);
    for s in f.body.stmts.iter() {
        let stmt_end = s.span.end_line.max(s.span.line);
        if stmt_end > last {
            last = stmt_end;
        }
    }
    if let Some(t) = &f.body.tail {
        let e = t.span.end_line.max(t.span.line);
        if e > last {
            last = e;
        }
    }
    last
}

fn push_class_methods(text: &str, c: &ClassDecl, out: &mut Vec<FnRange>) {
    let cname = c.name.as_str();
    for m in c.methods.iter() {
        push_fn(text, m, Some(cname), false, out);
    }
    for m in c.static_methods.iter() {
        push_fn(text, m, Some(cname), true, out);
    }
}

/// Find the FnRange whose line range contains `line` (1-based).
fn enclosing_fn<'a>(ranges: &'a [FnRange], line: u32) -> Option<&'a FnRange> {
    // Pick the smallest containing range — fns shouldn't nest in
    // ilang, but a class body straddles its methods. Filter by
    // line containment, then take the one with the latest start so
    // a method wins over its enclosing class.
    ranges
        .iter()
        .filter(|r| r.body_start_line <= line && line <= r.body_end_line)
        .max_by_key(|r| r.body_start_line)
}

/// Build an `ItemRef` for the function under the cursor. Resolves
/// through both refs (cursor on a call site) and bare symbols
/// (cursor on the decl line itself).
pub(crate) fn prepare(
    uri: Url,
    pos: Position,
    doc: &Doc,
    text: &str,
) -> Option<ItemRef> {
    if let Some(entry) = crate::lookup_ref(doc, pos) {
        if entry.no_definition {
            return None;
        }
        let sig = &entry.signature;
        let (kind, container, is_static) = classify_callable(sig)?;
        let owner_uri = entry.target_uri.clone().unwrap_or(uri.clone());
        let name = ref_target_name(doc, entry).unwrap_or_else(|| "<unknown>".to_string());
        return Some(ItemRef {
            uri: owner_uri,
            name,
            container,
            decl_name_span: entry.target_span,
            decl_name_len: entry.target_name_len,
            kind,
            is_static,
        });
    }
    // Fallback: cursor on a top-level fn / class member decl line.
    let Some((word, _)) = crate::word_at(text, pos) else { return None };
    let key = AstSymbol::intern(&word);
    if let Some(sym) = doc.symbols.get(&key) {
        let (kind, container, is_static) = classify_callable(&sym.signature)?;
        let decl_name_span = ["fn", "init"]
            .iter()
            .find_map(|kw| text::locate_let_name_with_kw(text, sym.span, kw, &sym.name))
            .unwrap_or(sym.span);
        return Some(ItemRef {
            uri,
            name: sym.name.clone(),
            container,
            decl_name_span,
            decl_name_len: sym.name.len() as u32,
            kind,
            is_static,
        });
    }
    None
}

/// `(kind, container, is_static)` for a callable signature, or
/// `None` when the signature isn't a function-like decl.
fn classify_callable(sig: &str) -> Option<(SymbolKind, Option<String>, bool)> {
    if let Some(rest) = sig.strip_prefix("(static method) ") {
        return Some((SymbolKind::METHOD, class_from_member_sig(rest), true));
    }
    if let Some(rest) = sig.strip_prefix("(method) ") {
        let cls = class_from_member_sig(rest);
        let kind = if matches!(&cls, Some(c) if rest.contains(&format!("{c}.init("))) {
            SymbolKind::CONSTRUCTOR
        } else {
            SymbolKind::METHOD
        };
        return Some((kind, cls, false));
    }
    if sig.starts_with("fn ") || sig.starts_with("@objc fn ") || sig.starts_with("@extern fn ") {
        return Some((SymbolKind::FUNCTION, None, false));
    }
    None
}

/// Extract the class name from a `ClassName.method(...)` tail, after
/// stripping leading user attribute lines. The hover formatter
/// renders attrs like `@objc("foo:bar:")` on their own lines before
/// the class qualifier; without skipping them, the class name comes
/// out as the attribute-decorated first line.
fn class_from_member_sig(rest: &str) -> Option<String> {
    let last_line = rest.lines().last()?;
    let (cls, _) = last_line.split_once('.')?;
    Some(cls.to_string())
}

/// Best-effort name recovery from a `RefEntry`. Reads the
/// identifier the ref occupies straight out of the buffer.
fn ref_target_name(doc: &Doc, entry: &RefEntry) -> Option<String> {
    let line = entry.line.checked_sub(1)? as usize;
    let line_str = doc.text.lines().nth(line)?;
    let start = entry.start_col.checked_sub(1)? as usize;
    let end = entry.end_col.checked_sub(1)? as usize;
    line_str.get(start..end).map(|s| s.to_string())
}

/// Walk every `.il` file under the workspace, find refs whose
/// `target_span` / `target_uri` matches `item.decl_name_span`, and
/// fold them by enclosing function.
pub(crate) fn incoming_calls(
    item: &ItemRef,
    open_docs: &HashMap<Url, Doc>,
) -> Vec<CallHierarchyIncomingCall> {
    // Key: (caller_uri, caller_name_span). Value: (caller_uri,
    // FnRange, accumulated call ranges in that caller's body).
    let mut by_caller: HashMap<(Url, Span), (Url, FnRange, Vec<Range>)> =
        HashMap::new();
    let Ok(anchor_path) = item.uri.to_file_path() else {
        return Vec::new();
    };
    let mut seen_paths: std::collections::HashSet<PathBuf> =
        std::collections::HashSet::new();
    // Pass 1: open docs (live buffers).
    for (doc_uri, d) in open_docs.iter() {
        if let Ok(p) = doc_uri.to_file_path() {
            if let Ok(canon) = p.canonicalize() {
                seen_paths.insert(canon);
            }
        }
        // For refs in the same file as the decl, target_uri is None.
        // From other files, target_uri must equal `item.uri`.
        let is_owner = doc_uri == &item.uri;
        accumulate_incoming(
            doc_uri,
            &d.text,
            &d.refs,
            item,
            is_owner,
            &mut by_caller,
        );
    }
    // Pass 2: closed files (workspace walk).
    for path in collect_workspace_il_files(&anchor_path) {
        if let Ok(canon) = path.canonicalize() {
            if seen_paths.contains(&canon) {
                continue;
            }
        }
        let Some(doc) = analyse_path_to_doc(&path) else { continue };
        let Ok(path_uri) = Url::from_file_path(&path) else { continue };
        let is_owner = path_uri == item.uri;
        accumulate_incoming(
            &path_uri,
            &doc.text,
            &doc.refs,
            item,
            is_owner,
            &mut by_caller,
        );
    }
    by_caller
        .into_values()
        .map(|(caller_uri, fr, ranges)| CallHierarchyIncomingCall {
            from: ItemRef {
                uri: caller_uri,
                name: fr.name.clone(),
                container: fr.container.clone(),
                decl_name_span: fr.name_span,
                decl_name_len: fr.name_len,
                kind: fr.kind,
                is_static: fr.is_static,
            }
            .to_item(fr.full_range),
            from_ranges: ranges,
        })
        .collect()
}

fn accumulate_incoming(
    doc_uri: &Url,
    text: &str,
    refs: &[RefEntry],
    item: &ItemRef,
    is_owner: bool,
    by_caller: &mut HashMap<(Url, Span), (Url, FnRange, Vec<Range>)>,
) {
    let matching: Vec<&RefEntry> = refs
        .iter()
        .filter(|r| {
            if r.target_name_len != item.decl_name_len {
                return false;
            }
            if r.target_span != item.decl_name_span {
                return false;
            }
            if r.signature.starts_with("this:") {
                return false;
            }
            // Skip the decl's own self-ref (RefEntry pushed for the
            // name on the decl line itself). Otherwise the decl is
            // reported as its own caller.
            if is_owner
                && r.line == item.decl_name_span.line
                && r.start_col == item.decl_name_span.col
            {
                return false;
            }
            // Class-disambiguation: when item is a method, drop refs
            // that classify to a different class.
            if let Some(want_class) = &item.container {
                let Some((_, cls, _)) = classify_callable(&r.signature) else {
                    return false;
                };
                if cls.as_deref() != Some(want_class.as_str()) {
                    return false;
                }
            }
            if is_owner {
                r.target_uri.is_none()
            } else {
                r.target_uri.as_ref() == Some(&item.uri)
            }
        })
        .collect();
    if matching.is_empty() {
        return;
    }
    let Ok(tokens) = tokenize(text) else { return };
    let Ok(prog) = parse(&tokens) else { return };
    let ranges = collect_fn_ranges(text, &prog);
    for r in matching {
        let Some(enclosing) = enclosing_fn(&ranges, r.line) else {
            continue;
        };
        let call_range = r.lsp_range();
        let key = (doc_uri.clone(), enclosing.name_span);
        let entry = by_caller.entry(key).or_insert_with(|| {
            (
                doc_uri.clone(),
                FnRange {
                    name:            enclosing.name.clone(),
                    container:       enclosing.container.clone(),
                    is_static:       enclosing.is_static,
                    name_span:       enclosing.name_span,
                    name_len:        enclosing.name_len,
                    body_start_line: enclosing.body_start_line,
                    body_end_line:   enclosing.body_end_line,
                    full_range:      enclosing.full_range,
                    kind:            enclosing.kind,
                },
                Vec::new(),
            )
        });
        entry.2.push(call_range);
    }
}

/// Walk the function body for the item's file and harvest every
/// Call / MethodCall site that resolves to a known callable.
pub(crate) fn outgoing_calls(item: &ItemRef, doc: &Doc) -> Vec<CallHierarchyOutgoingCall> {
    let Ok(tokens) = tokenize(&doc.text) else { return Vec::new() };
    let Ok(prog) = parse(&tokens) else { return Vec::new() };
    let ranges = collect_fn_ranges(&doc.text, &prog);
    let Some(target) = ranges.iter().find(|r| {
        r.name == item.name
            && r.container.as_deref() == item.container.as_deref()
            && r.name_span == item.decl_name_span
    }) else {
        return Vec::new();
    };
    let mut by_callee: HashMap<(Url, Span), (ItemRef, Vec<Range>)> = HashMap::new();
    for r in &doc.refs {
        if r.line < target.body_start_line || r.line > target.body_end_line {
            continue;
        }
        if r.signature.starts_with("this:") {
            continue;
        }
        // Skip the decl name itself — that's not a call site, it's
        // where the function is being declared.
        if r.line == target.name_span.line && r.start_col == target.name_span.col {
            continue;
        }
        let Some((kind, container, is_static)) = classify_callable(&r.signature) else {
            continue;
        };
        let callee_uri = r.target_uri.clone().unwrap_or(item.uri.clone());
        let name = ref_target_name(doc, r).unwrap_or_else(|| "<unknown>".to_string());
        let callee = ItemRef {
            uri: callee_uri.clone(),
            name,
            container,
            decl_name_span: r.target_span,
            decl_name_len: r.target_name_len,
            kind,
            is_static,
        };
        let call_range = r.lsp_range();
        let key = (callee_uri, r.target_span);
        let entry = by_callee.entry(key).or_insert_with(|| (callee, Vec::new()));
        entry.1.push(call_range);
    }
    by_callee
        .into_values()
        .map(|(callee, ranges)| {
            // Compute a reasonable `range` for the callee item.
            // We don't know the callee decl's body extent without
            // parsing its file, so reuse the selection_range — VSCode
            // accepts equal range/selection_range.
            let sel = text::span_to_range(
                callee.decl_name_span,
                callee.decl_name_len as usize,
            );
            CallHierarchyOutgoingCall {
                to: callee.to_item(sel),
                from_ranges: ranges,
            }
        })
        .collect()
}

/// Build the `range` field for the prepare response by re-parsing
/// the file to find the function decl's full extent. Falls back to
/// the selection range when the parse fails.
pub(crate) fn full_range_for(item: &ItemRef, text: &str) -> Range {
    if let Ok(tokens) = tokenize(text) {
        if let Ok(prog) = parse(&tokens) {
            let ranges = collect_fn_ranges(text, &prog);
            if let Some(fr) = ranges.iter().find(|r| {
                r.name == item.name
                    && r.container.as_deref() == item.container.as_deref()
                    && r.name_span == item.decl_name_span
            }) {
                return fr.full_range;
            }
        }
    }
    text::span_to_range(item.decl_name_span, item.decl_name_len as usize)
}
