//! `textDocument/documentSymbol` helpers — converts the parsed AST
//! into the nested `DocumentSymbol` tree that VSCode's outline view
//! consumes. Extracted from `handlers.rs` to keep the LSP trait impl
//! focused on dispatch.

use ilang_ast::{ClassDecl, FnDecl, Item, Span};
use tower_lsp::lsp_types::{DocumentSymbol, Range, SymbolKind};

use crate::text;
use crate::walker::is_parser_synth_field;

#[allow(deprecated)]
pub(crate) fn make_doc_sym(
    name: String,
    detail: Option<String>,
    kind: SymbolKind,
    range: Range,
    selection_range: Range,
    children: Option<Vec<DocumentSymbol>>,
) -> DocumentSymbol {
    // LSP requires `selectionRange` ⊆ `range`. Some decl spans don't
    // carry proper end positions, so the keyword-only `range` can land
    // strictly before the name `selection_range`. Expand `range` to
    // contain `selection_range` (and every child) when that happens.
    let mut range = range;
    expand_range(&mut range, &selection_range);
    if let Some(ch) = children.as_ref() {
        for c in ch {
            expand_range(&mut range, &c.range);
        }
    }
    DocumentSymbol {
        name,
        detail,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children,
    }
}

/// Grow `outer` so that `inner` is fully contained: start = min, end = max.
pub(crate) fn expand_range(outer: &mut Range, inner: &Range) {
    if (inner.start.line, inner.start.character)
        < (outer.start.line, outer.start.character)
    {
        outer.start = inner.start;
    }
    if (inner.end.line, inner.end.character)
        > (outer.end.line, outer.end.character)
    {
        outer.end = inner.end;
    }
}

/// Locate the identifier span for a top-level / nested decl. Falls
/// back to a zero-width span at `decl_span` when the name isn't
/// found on the recorded line (e.g. parser-synthesised decls).
pub(crate) fn name_range(
    line_starts: &[usize],
    text: &str,
    decl_span: Span,
    kw: &str,
    name: &str,
) -> Range {
    let name_span = text::locate_let_name_with_kw_at(line_starts, text, decl_span, kw, name)
        .unwrap_or(decl_span);
    text::span_to_range(name_span, name.len())
}

pub(crate) fn render_fn_detail(f: &FnDecl) -> String {
    let params = f
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, p.ty))
        .collect::<Vec<_>>()
        .join(", ");
    match &f.ret {
        Some(t) => format!("({params}): {t}"),
        None => format!("({params})"),
    }
}

pub(crate) fn collect_item_symbol(line_starts: &[usize], text: &str, item: &Item, out: &mut Vec<DocumentSymbol>) {
    match item {
        Item::Fn(f) => {
            let sel = name_range(line_starts, text, f.span, "fn", f.name.as_str());
            out.push(make_doc_sym(
                f.name.as_str().to_string(),
                Some(render_fn_detail(f)),
                SymbolKind::FUNCTION,
                text::span_full_to_range(f.span),
                sel,
                None,
            ));
        }
        Item::Class(c) => {
            out.push(class_symbol(line_starts, text, c));
        }
        Item::Interface(i) => {
            let sel = name_range(line_starts, text, i.span, "interface", i.name.as_str());
            let mut children: Vec<DocumentSymbol> = Vec::new();
            for m in i.methods.iter() {
                let m_sel = name_range(line_starts, text, m.span, "fn", m.name.as_str());
                let params = m
                    .params
                    .iter()
                    .map(|p| format!("{}: {}", p.name, p.ty))
                    .collect::<Vec<_>>()
                    .join(", ");
                let detail = match &m.ret {
                    Some(t) => format!("({params}): {t}"),
                    None => format!("({params})"),
                };
                children.push(make_doc_sym(
                    m.name.as_str().to_string(),
                    Some(detail),
                    SymbolKind::METHOD,
                    text::span_full_to_range(m.span),
                    m_sel,
                    None,
                ));
            }
            out.push(make_doc_sym(
                i.name.as_str().to_string(),
                None,
                SymbolKind::INTERFACE,
                text::span_full_to_range(i.span),
                sel,
                if children.is_empty() { None } else { Some(children) },
            ));
        }
        Item::Enum(e) => {
            let sel = name_range(line_starts, text, e.span, "enum", e.name.as_str());
            let mut children: Vec<DocumentSymbol> = Vec::new();
            for v in e.variants.iter() {
                let v_sel = text::span_to_range(v.span, v.name.as_str().len());
                children.push(make_doc_sym(
                    v.name.as_str().to_string(),
                    None,
                    SymbolKind::ENUM_MEMBER,
                    text::span_full_to_range(v.span),
                    v_sel,
                    None,
                ));
            }
            out.push(make_doc_sym(
                e.name.as_str().to_string(),
                None,
                SymbolKind::ENUM,
                text::span_full_to_range(e.span),
                sel,
                if children.is_empty() { None } else { Some(children) },
            ));
        }
        Item::Const(c) => {
            let sel = name_range(line_starts, text, c.span, "const", c.name.as_str());
            let detail = c.ty.as_ref().map(|t| format!(": {t}"));
            out.push(make_doc_sym(
                c.name.as_str().to_string(),
                detail,
                SymbolKind::CONSTANT,
                text::span_full_to_range(c.span),
                sel,
                None,
            ));
        }
        Item::ExternC(b) => {
            for inner in b.items.iter() {
                match inner {
                    ilang_ast::ExternCItem::FnDef(f) => {
                        let name = f.name.as_str().to_string();
                        let sel = name_range(line_starts, text, f.span, "fn", &name);
                        out.push(make_doc_sym(
                            name,
                            Some(render_fn_detail(f)),
                            SymbolKind::FUNCTION,
                            text::span_full_to_range(f.span),
                            sel,
                            None,
                        ));
                    }
                    ilang_ast::ExternCItem::FnDecl { name, params, ret, span, .. } => {
                        let name_s = name.as_str().to_string();
                        let sel = name_range(line_starts, text, *span, "fn", &name_s);
                        let plist = params
                            .iter()
                            .map(|p| format!("{}: {}", p.name, p.ty))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let detail = match ret {
                            Some(t) => format!("({plist}): {t}"),
                            None => format!("({plist})"),
                        };
                        out.push(make_doc_sym(
                            name_s,
                            Some(detail),
                            SymbolKind::FUNCTION,
                            text::span_full_to_range(*span),
                            sel,
                            None,
                        ));
                    }
                    ilang_ast::ExternCItem::Class(c) => {
                        out.push(class_symbol(line_starts, text, c));
                    }
                    ilang_ast::ExternCItem::Struct { name, fields, span, .. } => {
                        let sel = name_range(line_starts, text, *span, "struct", name.as_str());
                        let mut children: Vec<DocumentSymbol> = Vec::new();
                        for f in fields.iter() {
                            let f_sel = text::span_to_range(f.span, f.name.as_str().len());
                            children.push(make_doc_sym(
                                f.name.as_str().to_string(),
                                Some(format!(": {}", f.ty)),
                                SymbolKind::FIELD,
                                text::span_full_to_range(f.span),
                                f_sel,
                                None,
                            ));
                        }
                        out.push(make_doc_sym(
                            name.as_str().to_string(),
                            None,
                            SymbolKind::STRUCT,
                            text::span_full_to_range(*span),
                            sel,
                            if children.is_empty() { None } else { Some(children) },
                        ));
                    }
                    ilang_ast::ExternCItem::Union { name, fields, span, .. } => {
                        let sel = name_range(line_starts, text, *span, "union", name.as_str());
                        let mut children: Vec<DocumentSymbol> = Vec::new();
                        for f in fields.iter() {
                            let f_sel = text::span_to_range(f.span, f.name.as_str().len());
                            children.push(make_doc_sym(
                                f.name.as_str().to_string(),
                                Some(format!(": {}", f.ty)),
                                SymbolKind::FIELD,
                                text::span_full_to_range(f.span),
                                f_sel,
                                None,
                            ));
                        }
                        out.push(make_doc_sym(
                            name.as_str().to_string(),
                            None,
                            SymbolKind::STRUCT,
                            text::span_full_to_range(*span),
                            sel,
                            if children.is_empty() { None } else { Some(children) },
                        ));
                    }
                }
            }
            for iface in b.interfaces.iter() {
                collect_item_symbol(line_starts, text, &Item::Interface(iface.clone()), out);
            }
            for c in b.consts.iter() {
                collect_item_symbol(line_starts, text, &Item::Const(c.clone()), out);
            }
        }
        Item::Use(_) => {}
    }
}

pub(crate) fn class_symbol(line_starts: &[usize], text: &str, c: &ClassDecl) -> DocumentSymbol {
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
    let sel = name_range(line_starts, text, c.span, kw, c.name.as_str());
    let mut children: Vec<DocumentSymbol> = Vec::new();
    for f in c.fields.iter() {
        if is_parser_synth_field(f, c.span) {
            continue;
        }
        let f_sel = text::span_to_range(f.span, f.name.as_str().len());
        children.push(make_doc_sym(
            f.name.as_str().to_string(),
            Some(format!(": {}", f.ty)),
            SymbolKind::FIELD,
            text::span_full_to_range(f.span),
            f_sel,
            None,
        ));
    }
    for f in c.static_fields.iter() {
        let f_sel = text::span_to_range(f.span, f.name.as_str().len());
        let detail = if f.is_const {
            Some(format!("const: {}", f.ty))
        } else {
            Some(format!("static: {}", f.ty))
        };
        children.push(make_doc_sym(
            f.name.as_str().to_string(),
            detail,
            if f.is_const { SymbolKind::CONSTANT } else { SymbolKind::FIELD },
            text::span_full_to_range(f.span),
            f_sel,
            None,
        ));
    }
    for p in c.properties.iter() {
        let p_sel = text::span_to_range(p.span, p.name.as_str().len());
        children.push(make_doc_sym(
            p.name.as_str().to_string(),
            Some(format!(": {}", p.ty)),
            SymbolKind::PROPERTY,
            text::span_full_to_range(p.span),
            p_sel,
            None,
        ));
    }
    for m in c.methods.iter() {
        let m_sel = name_range(line_starts, text, m.span, "fn", m.name.as_str());
        let sym_kind = if m.name.as_str() == "init" {
            SymbolKind::CONSTRUCTOR
        } else {
            SymbolKind::METHOD
        };
        children.push(make_doc_sym(
            m.name.as_str().to_string(),
            Some(render_fn_detail(m)),
            sym_kind,
            text::span_full_to_range(m.span),
            m_sel,
            None,
        ));
    }
    for m in c.static_methods.iter() {
        let m_sel = name_range(line_starts, text, m.span, "fn", m.name.as_str());
        children.push(make_doc_sym(
            m.name.as_str().to_string(),
            Some(render_fn_detail(m)),
            SymbolKind::METHOD,
            text::span_full_to_range(m.span),
            m_sel,
            None,
        ));
    }
    make_doc_sym(
        c.name.as_str().to_string(),
        None,
        kind,
        text::span_full_to_range(c.span),
        sel,
        if children.is_empty() { None } else { Some(children) },
    )
}
