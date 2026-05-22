//! Per-file cache for `workspace/symbol` requests.
//!
//! Each entry stores the file's mtime and the pre-extracted list of
//! symbol records (top-level decls + class members + enum variants).
//! On a request, files whose disk mtime hasn't changed reuse the
//! cached entry list; only the (cheap) name filter runs per query.
//! Open buffers always re-parse — their text is the live in-memory
//! version, which mtime can't represent.

use std::path::Path;
use std::time::SystemTime;

use ilang_ast::{ClassDecl, Item, Program, Span};
use ilang_lexer::tokenize;
use ilang_parser::parse;
use tower_lsp::lsp_types::{Range, SymbolKind};

use crate::text;

/// One cached `.il` file. `entries` is the flat list of symbols
/// the workspace-symbol handler emits — filtered against each
/// query at request time.
#[derive(Clone)]
pub(crate) struct Entry {
    pub mtime: Option<SystemTime>,
    pub items: Vec<Symbol>,
}

#[derive(Clone)]
pub(crate) struct Symbol {
    pub name:      String,
    pub kind:      SymbolKind,
    pub container: Option<String>,
    pub range:     Range,
}

/// Build the symbol list for `text` parsed as a `.il` file. Returns
/// an empty list when the file doesn't tokenize / parse — same
/// failure mode as the existing inline path.
pub(crate) fn build(text: &str) -> Vec<Symbol> {
    let Ok(tokens) = tokenize(text) else { return Vec::new() };
    let Ok(prog) = parse(&tokens) else { return Vec::new() };
    let mut out = Vec::new();
    collect_program(text, &prog, &mut out);
    out
}

fn collect_program(text: &str, prog: &Program, out: &mut Vec<Symbol>) {
    for item in &prog.items {
        collect_item(text, item, None, out);
    }
}

fn collect_item(text: &str, item: &Item, container: Option<&str>, out: &mut Vec<Symbol>) {
    match item {
        Item::Fn(f) => push_top(
            text, f.span, "fn", f.name.as_str(), SymbolKind::FUNCTION,
            container, out,
        ),
        Item::Class(c) => collect_class(text, c, container, out),
        Item::Interface(i) => {
            push_top(
                text, i.span, "interface", i.name.as_str(),
                SymbolKind::INTERFACE, container, out,
            );
            for m in i.methods.iter() {
                push_top(
                    text, m.span, "fn", m.name.as_str(),
                    SymbolKind::METHOD, Some(i.name.as_str()), out,
                );
            }
        }
        Item::Enum(e) => {
            push_top(
                text, e.span, "enum", e.name.as_str(),
                SymbolKind::ENUM, container, out,
            );
            for v in e.variants.iter() {
                push_at_span(
                    v.span, v.name.as_str(), SymbolKind::ENUM_MEMBER,
                    Some(e.name.as_str()), out,
                );
            }
        }
        Item::Const(c) => push_top(
            text, c.span, "const", c.name.as_str(),
            SymbolKind::CONSTANT, container, out,
        ),
        Item::ExternC(b) => {
            for inner in b.items.iter() {
                match inner {
                    ilang_ast::ExternCItem::FnDef(f) => push_top(
                        text, f.span, "fn", f.name.as_str(),
                        SymbolKind::FUNCTION, container, out,
                    ),
                    ilang_ast::ExternCItem::FnDecl { name, span, .. } => push_top(
                        text, *span, "fn", name.as_str(),
                        SymbolKind::FUNCTION, container, out,
                    ),
                    ilang_ast::ExternCItem::Class(c) => {
                        collect_class(text, c, container, out)
                    }
                    ilang_ast::ExternCItem::Struct { name, fields, span, .. } => {
                        push_top(
                            text, *span, "struct", name.as_str(),
                            SymbolKind::STRUCT, container, out,
                        );
                        for f in fields.iter() {
                            push_at_span(
                                f.span, f.name.as_str(), SymbolKind::FIELD,
                                Some(name.as_str()), out,
                            );
                        }
                    }
                    ilang_ast::ExternCItem::Union { name, fields, span, .. } => {
                        push_top(
                            text, *span, "union", name.as_str(),
                            SymbolKind::STRUCT, container, out,
                        );
                        for f in fields.iter() {
                            push_at_span(
                                f.span, f.name.as_str(), SymbolKind::FIELD,
                                Some(name.as_str()), out,
                            );
                        }
                    }
                }
            }
            for iface in b.interfaces.iter() {
                collect_item(text, &Item::Interface(iface.clone()), container, out);
            }
            for c in b.consts.iter() {
                collect_item(text, &Item::Const(c.clone()), container, out);
            }
        }
        Item::Use(_) => {}
    }
}

fn collect_class(
    text: &str,
    c: &ClassDecl,
    container: Option<&str>,
    out: &mut Vec<Symbol>,
) {
    let kw = if c.is_union {
        "union"
    } else if c.is_repr_c {
        "struct"
    } else {
        "class"
    };
    let kind = if c.is_union || c.is_repr_c {
        SymbolKind::STRUCT
    } else {
        SymbolKind::CLASS
    };
    push_top(text, c.span, kw, c.name.as_str(), kind, container, out);
    let class_name = c.name.as_str();
    for f in c.fields.iter() {
        push_at_span(
            f.span, f.name.as_str(), SymbolKind::FIELD,
            Some(class_name), out,
        );
    }
    for f in c.static_fields.iter() {
        let k = if f.is_const { SymbolKind::CONSTANT } else { SymbolKind::FIELD };
        push_at_span(
            f.span, f.name.as_str(), k, Some(class_name), out,
        );
    }
    for p in c.properties.iter() {
        push_at_span(
            p.span, p.name.as_str(), SymbolKind::PROPERTY,
            Some(class_name), out,
        );
    }
    for m in c.methods.iter() {
        let k = if m.name.as_str() == "init" {
            SymbolKind::CONSTRUCTOR
        } else {
            SymbolKind::METHOD
        };
        push_top(text, m.span, "fn", m.name.as_str(), k, Some(class_name), out);
    }
    for m in c.static_methods.iter() {
        push_top(
            text, m.span, "fn", m.name.as_str(), SymbolKind::METHOD,
            Some(class_name), out,
        );
    }
}

fn push_top(
    text: &str,
    decl_span: Span,
    kw: &str,
    name: &str,
    kind: SymbolKind,
    container: Option<&str>,
    out: &mut Vec<Symbol>,
) {
    let name_span = text::locate_let_name_with_kw(text, decl_span, kw, name)
        .unwrap_or(decl_span);
    out.push(Symbol {
        name:      name.to_string(),
        kind,
        container: container.map(|s| s.to_string()),
        range:     text::span_to_range(name_span, name.len()),
    });
}

fn push_at_span(
    name_span: Span,
    name: &str,
    kind: SymbolKind,
    container: Option<&str>,
    out: &mut Vec<Symbol>,
) {
    out.push(Symbol {
        name:      name.to_string(),
        kind,
        container: container.map(|s| s.to_string()),
        range:     text::span_to_range(name_span, name.len()),
    });
}

/// Disk mtime for the file, or `None` if metadata isn't readable.
pub(crate) fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

