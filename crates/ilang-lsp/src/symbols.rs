//! Extracted from `main.rs`.
#![allow(unused_imports)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};



use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use ilang_ast::{
    Block, ClassDecl, EnumDecl, Expr, ExprKind, FnDecl, Item, Param, Pattern, PatternBindings,
    PatternKind, Program, Span, Stmt, StmtKind, Symbol as AstSymbol, Type, VariantPayload,
};
use ilang_parser::parse as parse_program;
use ilang_types::{check, TypeError};

use crate::*;

/// Parser-synthesised helpers emitted by `@extern(ObjC) { ... }`
/// desugar — get_class, sel_register, allocate_class_pair, the
/// per-method `__objc_..._msg_*` aliases, the subclass IMP /
/// _ilang_impl_ pair, and so on. They're invisible in source and
/// their generated call sites borrow nearby user spans, so without
/// this filter hovering on the class name picks one up as a ref and
/// shows its synthetic signature.
pub(crate) fn is_synthesized_objc_helper(name: &str) -> bool {
    name.starts_with("__objc_")
        || name.starts_with("ilang_objc_imp__")
        || name.starts_with("_ilang_impl_")
        || name.starts_with("__super_")
        || name == "__wrap_handle"
        || name == "__bind_handle"
}

pub(crate) fn collect_symbols(prog: &Program, src: &str) -> HashMap<AstSymbol, Symbol> {
    use ilang_ast::ExternCItem;
    let mut out = HashMap::new();
    let put_fn = |f: &FnDecl, m: &mut HashMap<AstSymbol, Symbol>| {
        if is_synthesized_objc_helper(f.name.as_str()) {
            return;
        }
        m.insert(
            f.name.into(),
            Symbol {
                name: f.name.as_str().to_string(),
                span: f.span,
                signature: fn_signature(f),
                doc: text::extract_doc_above(src, f.span.line),
            },
        );
    };
    for item in &prog.items {
        match item {
            Item::Fn(f) => put_fn(f, &mut out),
            Item::Class(c) => {
                let signature = format!("{}class {}", render_user_attrs(&c.attrs), c.name);
                out.insert(
                    c.name.into(),
                    Symbol {
                        name: c.name.as_str().to_string(),
                        span: c.span,
                        signature,
                        doc: text::extract_doc_above(src, c.span.line),
                    },
                );
            }
            Item::Interface(i) => {
                let signature = format!("interface {}", i.name);
                out.insert(
                    i.name.into(),
                    Symbol {
                        name: i.name.as_str().to_string(),
                        span: i.span,
                        signature,
                        doc: text::extract_doc_above(src, i.span.line),
                    },
                );
            }
            Item::Enum(e) => {
                let variants = e
                    .variants
                    .iter()
                    .map(|v| match &v.payload {
                        VariantPayload::Unit => v.name.as_str().to_string(),
                        _ => format!("{}(...)", v.name),
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let repr = e
                    .repr_ty
                    .as_ref()
                    .map(|t| format!(": {t}"))
                    .unwrap_or_default();
                let flags_prefix = if e.flags { "@flags\n" } else { "" };
                let signature = format!(
                    "{}enum {}{} {{ {} }}",
                    flags_prefix, e.name, repr, variants
                );
                out.insert(
                    e.name.into(),
                    Symbol {
                        name: e.name.as_str().to_string(),
                        span: e.span,
                        signature,
                        doc: text::extract_doc_above(src, e.span.line),
                    },
                );
            }
            Item::Const(c) => {
                let ty = match c
                    .ty
                    .clone()
                    .or_else(|| infer_expr_type_with_scope(&c.value, &[]))
                {
                    Some(t) => format!(": {t}"),
                    None => String::new(),
                };
                let value = render_const_value_with_src(&c.value, Some(src))
                    .map(|v| format!(" = {v}"))
                    .unwrap_or_default();
                let signature = format!("const {}{}{}", c.name, ty, value);
                out.insert(
                    c.name.into(),
                    Symbol {
                        name: c.name.as_str().to_string(),
                        span: c.span,
                        signature,
                        doc: text::extract_doc_above(src, c.span.line),
                    },
                );
            }
            Item::ExternC(b) => {
                for inner in &b.items {
                    match inner {
                        ExternCItem::FnDecl {
                            name, params, ret, span, libs, ..
                        } => {
                            if is_synthesized_objc_helper(name.as_str()) {
                                continue;
                            }
                            let ps = params
                                .iter()
                                .map(|p| format!("{}: {}", p.name, p.ty))
                                .collect::<Vec<_>>()
                                .join(", ");
                            let r = match ret {
                                Some(t) => format!(": {t}"),
                                None => String::new(),
                            };
                            let libs_prefix = if libs.is_empty() {
                                String::new()
                            } else {
                                let names = libs
                                    .iter()
                                    .map(|l| format!("\"{l}\""))
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                format!("@lib({names})\n")
                            };
                            out.insert(
                                name.clone(),
                                Symbol {
                                    name: name.as_str().to_string(),
                                    span: *span,
                                    signature: format!("{libs_prefix}fn {}({}){}", name, ps, r),
                                    doc: text::extract_doc_above(src, span.line),
                                },
                            );
                        }
                        ExternCItem::FnDef(f) => put_fn(f, &mut out),
                        ExternCItem::Struct { name, span, .. } => {
                            if is_synthesized_objc_helper(name.as_str()) {
                                continue;
                            }
                            out.insert(
                                name.clone(),
                                Symbol {
                                    name: name.as_str().to_string(),
                                    span: *span,
                                    signature: format!("struct {}", name),
                                    doc: text::extract_doc_above(src, span.line),
                                },
                            );
                        }
                        ExternCItem::Union { name, span, .. } => {
                            if is_synthesized_objc_helper(name.as_str()) {
                                continue;
                            }
                            out.insert(
                                name.clone(),
                                Symbol {
                                    name: name.as_str().to_string(),
                                    span: *span,
                                    signature: format!("union {}", name),
                                    doc: text::extract_doc_above(src, span.line),
                                },
                            );
                        }
                        ExternCItem::Class(c) => {
                            out.insert(
                                c.name.into(),
                                Symbol {
                                    name: c.name.as_str().to_string(),
                                    span: c.span,
                                    signature: format!(
                                        "{}class {}",
                                        render_user_attrs(&c.attrs),
                                        c.name
                                    ),
                                    doc: text::extract_doc_above(src, c.span.line),
                                },
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Inject hover info for built-in singletons / classes that the type
/// checker pre-registers (e.g. `console.log`). The buffer doesn't
/// declare these, so users would otherwise see no hover.
pub(crate) fn install_builtin_classes(out: &mut HashMap<AstSymbol, ClassInfo>) {
    let mut methods: HashMap<AstSymbol, MemberInfo> = HashMap::new();
    methods.insert(
        "log".into(),
        MemberInfo {
            span: Span::dummy(),
            signature: "(method) Console.log(...args): ()".to_string(),
            ret_ty: Some(Type::Unit),
            is_static: false,
                doc: None,
        },
    );
    out.entry("Console".into()).or_insert(ClassInfo {
        decl_span: Span::dummy(),
        fields: HashMap::new(),
        methods,
        getters: HashMap::new(),
        setters: HashMap::new(),
        external: true,
        init_overloads: 0,
                                    inits: Vec::new(),
        kind: ClassKind::Class,
    });
}

pub(crate) fn collect_classes(prog: &Program, src: &str) -> HashMap<AstSymbol, ClassInfo> {
    use ilang_ast::ExternCItem;
    let mut classes: Vec<&ClassDecl> = Vec::new();
    let mut out = HashMap::new();
    for item in &prog.items {
        match item {
            Item::Class(c) => classes.push(c),
            Item::ExternC(b) => {
                for inner in &b.items {
                    match inner {
                        ExternCItem::Class(c) => classes.push(c),
                        // Treat extern structs / unions like classes for
                        // field-resolution purposes: build a fields-only
                        // ClassInfo so `point.x` hovers / F12s.
                        ExternCItem::Struct { name, fields: fs, span, .. }
                        | ExternCItem::Union { name, fields: fs, span, .. } => {
                            let kind = matches!(
                                inner,
                                ExternCItem::Struct { .. }
                            )
                                .then_some(ClassKind::Struct)
                                .unwrap_or(ClassKind::Union);
                            let mut fields = HashMap::new();
                            for f in fs {
                                fields.insert(
                                    f.name.into(),
                                    MemberInfo {
                                        span: f.span,
                                        signature: format!(
                                            "(property) {}.{}: {}",
                                            name, f.name, f.ty
                                        ),
                                        ret_ty: Some(f.ty.clone()),
                                        is_static: false,
                                        doc: text::extract_doc_above(src, f.span.line),
                                    },
                                );
                            }
                            out.insert(
                                name.clone(),
                                ClassInfo {
                                    decl_span: *span,
                                    fields,
                                    methods: HashMap::new(),
                                    getters: HashMap::new(),
                                    setters: HashMap::new(),
                                    external: false,
                                    init_overloads: 0,
                                    inits: Vec::new(),
                                    kind,
                                },
                            );
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    for c in classes {
        // Mirror the original body — each block builds a ClassInfo
        // identical to the original `Item::Class` path.
        {
            let mut fields = HashMap::new();
            for f in &c.fields {
                fields.insert(
                    f.name.into(),
                    MemberInfo {
                        span: f.span,
                        signature: format!("(property) {}.{}: {}", c.name, f.name, f.ty),
                        ret_ty: Some(f.ty.clone()),
                        is_static: false,
                        doc: text::extract_doc_above(src, f.span.line),
                    },
                );
            }
            for f in &c.static_fields {
                let kind = if f.is_const { "static const" } else { "static property" };
                let value = render_const_value_with_src(&f.value, Some(src))
                    .map(|v| format!(" = {v}"))
                    .unwrap_or_default();
                fields.insert(
                    f.name.into(),
                    MemberInfo {
                        span: f.span,
                        signature: format!(
                            "({}) {}.{}: {}{}",
                            kind, c.name, f.name, f.ty, value
                        ),
                        ret_ty: Some(f.ty.clone()),
                        is_static: true,
                        doc: text::extract_doc_above(src, f.span.line),
                    },
                );
            }
            let mut getters: HashMap<AstSymbol, MemberInfo> = HashMap::new();
            let mut setters: HashMap<AstSymbol, MemberInfo> = HashMap::new();
            for prop in &c.properties {
                fields.insert(
                    prop.name.into(),
                    MemberInfo {
                        span: prop.span,
                        signature: format!(
                            "(property) {}.{}: {}",
                            c.name, prop.name, prop.ty
                        ),
                        ret_ty: Some(prop.ty.clone()),
                        is_static: false,
                        doc: text::extract_doc_above(src, prop.span.line),
                    },
                );
                if let Some(g) = &prop.getter {
                    getters.insert(
                        prop.name.into(),
                        MemberInfo {
                            span: g.span,
                            signature: format!(
                                "(getter) {}.{}: {}",
                                c.name, prop.name, prop.ty
                            ),
                            ret_ty: Some(prop.ty.clone()),
                            is_static: false,
                            doc: text::extract_doc_above(src, g.span.line),
                        },
                    );
                }
                if let Some(s) = &prop.setter {
                    setters.insert(
                        prop.name.into(),
                        MemberInfo {
                            span: s.span,
                            signature: format!(
                                "(setter) {}.{}: {}",
                                c.name, prop.name, prop.ty
                            ),
                            ret_ty: Some(prop.ty.clone()),
                            is_static: false,
                            doc: text::extract_doc_above(src, s.span.line),
                        },
                    );
                }
            }
            let mut methods = HashMap::new();
            let mut init_overloads = 0usize;
            let mut inits: Vec<MemberInfo> = Vec::new();
            for m in &c.methods {
                let info = MemberInfo {
                    span: m.span,
                    signature: format!(
                        "{}(method) {}.{}",
                        render_user_attrs(&m.attrs),
                        c.name,
                        fn_body(m)
                    ),
                    ret_ty: m.ret.clone(),
                    is_static: false,
                    doc: text::extract_doc_above(src, m.span.line),
                };
                if m.name == "init" {
                    init_overloads += 1;
                    inits.push(info.clone());
                }
                methods.entry(m.name.clone()).or_insert(info);
            }
            for m in &c.static_methods {
                methods.entry(m.name.clone()).or_insert(MemberInfo {
                    span: m.span,
                    signature: format!(
                        "{}(static method) {}.{}",
                        render_user_attrs(&m.attrs),
                        c.name,
                        fn_body(m)
                    ),
                    ret_ty: m.ret.clone(),
                    is_static: true,
                    doc: text::extract_doc_above(src, m.span.line),
                });
            }
            out.insert(
                c.name.into(),
                ClassInfo {
                    decl_span: c.span,
                    fields,
                    methods,
                    getters,
                    setters,
                    external: false,
                    init_overloads,
                    inits,
                    kind: ClassKind::Class,
                },
            );
        }
    }
    out
}

pub(crate) fn fn_signature(f: &FnDecl) -> String {
    format!("{}fn {}", render_user_attrs(&f.attrs), fn_body(f))
}

/// `name(params): ret` — the part that comes after `fn` / `(method)` /
/// `(static method)`.
pub(crate) fn fn_body(f: &FnDecl) -> String {
    let params = f
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, p.ty))
        .collect::<Vec<_>>()
        .join(", ");
    let ret = match &f.ret {
        Some(t) => format!(": {t}"),
        None => String::new(),
    };
    format!("{}({}){}", f.name, params, ret)
}

/// Render the user-visible attributes (e.g. `@objc("alloc")`) as a
/// newline-terminated prefix. Parser-internal markers like
/// `__objc_wrapper` are filtered out — they exist only to disable
/// downstream checks, not for documentation.
pub(crate) fn render_user_attrs(attrs: &[ilang_ast::Attribute]) -> String {
    use ilang_ast::AttrArg;
    let mut out = String::new();
    for a in attrs.iter() {
        let n = a.name.as_str();
        if n.starts_with("__") {
            continue;
        }
        let args = a
            .args
            .iter()
            .map(|x| match x {
                AttrArg::Str(s) => format!("\"{s}\""),
                AttrArg::Path(p) => p
                    .iter()
                    .map(|s| s.as_str().to_string())
                    .collect::<Vec<_>>()
                    .join("."),
                _ => String::new(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        if args.is_empty() {
            out.push_str(&format!("@{n}\n"));
        } else {
            out.push_str(&format!("@{n}({args})\n"));
        }
    }
    out
}
