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
    // Any `__`-prefixed name is reserved for internal synthesis
    // (the @objc desugar emits `__objc_<...>`, `__super_<...>`,
    // `__w_<arg>` wrappers, `__cached_sel`, `__wrap_handle`,
    // `__bind_handle`, `__owns`, etc.). Catch the whole namespace
    // up front so individual prefix entries below are kept only
    // for documentation.
    if name.starts_with("__") {
        return true;
    }
    name.starts_with("$objc.imp.")
        || name.starts_with("_ilang_impl_")
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
                let bases = render_class_bases(c.parent.as_ref(), &c.interfaces);
                let signature = format!(
                    "{}class {}{bases}",
                    render_user_attrs(&c.attrs),
                    c.name
                );
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
                let parent = i
                    .parent
                    .as_ref()
                    .map(|p| format!(" : {p}"))
                    .unwrap_or_default();
                let signature = format!("interface {}{parent}", i.name);
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
                            name, type_params, params, ret, span, libs, ..
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
                            let tps = if type_params.is_empty() {
                                String::new()
                            } else {
                                let names = type_params
                                    .iter()
                                    .map(|s| s.as_str().to_string())
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                format!("<{names}>")
                            };
                            out.insert(
                                name.clone(),
                                Symbol {
                                    name: name.as_str().to_string(),
                                    span: *span,
                                    signature: format!("{libs_prefix}fn {}{}({}){}", name, tps, ps, r),
                                    doc: text::extract_doc_above(src, span.line),
                                },
                            );
                        }
                        ExternCItem::FnDef(f) => put_fn(f, &mut out),
                        ExternCItem::Struct {
                            name, span, is_packed, is_handle, ..
                        } => {
                            if is_synthesized_objc_helper(name.as_str()) {
                                continue;
                            }
                            let attrs = crate::helpers::render_struct_attrs(*is_packed, *is_handle);
                            out.insert(
                                name.clone(),
                                Symbol {
                                    name: name.as_str().to_string(),
                                    span: *span,
                                    signature: format!("{attrs}struct {}", name),
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
                            // Skip the per-block selector-cache class
                            // and similar @objc desugar helpers; their
                            // names are stable artefacts of the
                            // desugar (`__objc_b<line>c<col>_sel_cache`
                            // etc.) and shouldn't surface in hover /
                            // completion alongside user-declared
                            // classes.
                            if is_synthesized_objc_helper(c.name.as_str()) {
                                continue;
                            }
                            let bases = render_class_bases(
                                c.parent.as_ref(),
                                &c.interfaces,
                            );
                            out.insert(
                                c.name.into(),
                                Symbol {
                                    name: c.name.as_str().to_string(),
                                    span: c.span,
                                    signature: format!(
                                        "{}class {}{bases}",
                                        render_user_attrs(&c.attrs),
                                        c.name
                                    ),
                                    doc: text::extract_doc_above(src, c.span.line),
                                },
                            );
                        }
                    }
                }
                // @objc interfaces declared inside the @extern(ObjC)
                // block. The hover signature lists the interface
                // methods so users can see the contract at a
                // glance.
                for iface in b.interfaces.iter() {
                    let methods: Vec<String> = iface
                        .methods
                        .iter()
                        .map(|m| {
                            let opt = if m.is_optional { "?" } else { "" };
                            let ps: Vec<String> = m
                                .params
                                .iter()
                                .map(|p| format!("{}: {}", p.name, p.ty))
                                .collect();
                            let r = match &m.ret {
                                Some(t) => format!(": {t}"),
                                None => String::new(),
                            };
                            format!("    {}{}({}){}", m.name, opt, ps.join(", "), r)
                        })
                        .collect();
                    let header = if iface.is_objc { "@objc interface" } else { "interface" };
                    let parent = iface
                        .parent
                        .as_ref()
                        .map(|p| format!(" : {p}"))
                        .unwrap_or_default();
                    let signature = if methods.is_empty() {
                        format!("{header} {}{parent} {{}}", iface.name)
                    } else {
                        format!(
                            "{header} {}{parent} {{\n{}\n}}",
                            iface.name,
                            methods.join("\n")
                        )
                    };
                    out.insert(
                        iface.name.into(),
                        Symbol {
                            name: iface.name.as_str().to_string(),
                            span: iface.span,
                            signature,
                            doc: text::extract_doc_above(src, iface.span.line),
                        },
                    );
                }
            }
            _ => {}
        }
    }
    // Top-level `let X = ...` bindings. Type is unknown here (no
    // checker output to lean on), so the signature is the bare
    // `let X` form; the diag pre-pass that runs before walking
    // upgrades it to `let X: T` inside `var_types` once `T` is
    // inferred. This entry's job is just to make `doc.symbols`
    // know `X` exists so hover from inside item bodies resolves
    // to the let's declaration site.
    for s in &prog.stmts {
        if let ilang_ast::StmtKind::Let { name, .. } = &s.kind {
            if name.as_str() == "_" {
                continue;
            }
            let name_span = text::locate_let_name(src, s.span, name.as_str())
                .unwrap_or(s.span);
            out.entry(name.clone()).or_insert(Symbol {
                name: name.as_str().to_string(),
                span: name_span,
                signature: format!("let {name}"),
                doc: text::extract_doc_above(src, s.span.line),
            });
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
            is_pub: true,
            doc: None,
            source_path: None,
        },
    );
    out.entry("Console".into()).or_insert(ClassInfo {
        decl_span: Span::dummy(),
        type_params: Vec::new(),
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
            Item::Interface(i) => {
                register_interface_as_class(i, src, &mut out);
            }
            Item::ExternC(b) => {
                for iface in b.interfaces.iter() {
                    register_interface_as_class(iface, src, &mut out);
                }
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
                                        // @extern(C) struct fields have
                                        // no `pub` marker; the type
                                        // checker treats the whole
                                        // struct's fields as accessible
                                        // since the struct is the FFI
                                        // surface itself.
                                        is_pub: true,
                                        doc: text::extract_doc_above(src, f.span.line),
                                        source_path: None,
                                    },
                                );
                            }
                            out.insert(
                                name.clone(),
                                ClassInfo {
                                    decl_span: *span,
                                    type_params: Vec::new(),
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
                        is_pub: f.is_pub,
                        doc: text::extract_doc_above(src, f.span.line),
                        source_path: None,
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
                        is_pub: f.is_pub,
                        doc: text::extract_doc_above(src, f.span.line),
                        source_path: None,
                    },
                );
            }
            let mut getters: HashMap<AstSymbol, MemberInfo> = HashMap::new();
            let mut setters: HashMap<AstSymbol, MemberInfo> = HashMap::new();
            for prop in &c.properties {
                let prop_kind = if prop.is_static { "static property" } else { "property" };
                fields.insert(
                    prop.name.into(),
                    MemberInfo {
                        span: prop.span,
                        signature: format!(
                            "({prop_kind}) {}.{}: {}",
                            c.name, prop.name, prop.ty
                        ),
                        ret_ty: Some(prop.ty.clone()),
                        is_static: prop.is_static,
                        is_pub: prop.is_pub,
                        doc: text::extract_doc_above(src, prop.span.line),
                        source_path: None,
                    },
                );
                let getter_label = if prop.is_static { "static getter" } else { "getter" };
                let setter_label = if prop.is_static { "static setter" } else { "setter" };
                if let Some(g) = &prop.getter {
                    getters.insert(
                        prop.name.into(),
                        MemberInfo {
                            span: g.span,
                            signature: format!(
                                "({getter_label}) {}.{}: {}",
                                c.name, prop.name, prop.ty
                            ),
                            ret_ty: Some(prop.ty.clone()),
                            is_static: prop.is_static,
                            is_pub: prop.is_pub,
                            doc: text::extract_doc_above(src, g.span.line),
                            source_path: None,
                        },
                    );
                }
                if let Some(s) = &prop.setter {
                    setters.insert(
                        prop.name.into(),
                        MemberInfo {
                            span: s.span,
                            signature: format!(
                                "({setter_label}) {}.{}: {}",
                                c.name, prop.name, prop.ty
                            ),
                            ret_ty: Some(prop.ty.clone()),
                            is_static: prop.is_static,
                            is_pub: prop.is_pub,
                            doc: text::extract_doc_above(src, s.span.line),
                            source_path: None,
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
                        "(method) {}{}.{}",
                        render_user_attrs(&m.attrs),
                        c.name,
                        fn_body(m)
                    ),
                    ret_ty: m.ret.clone(),
                    is_static: false,
                    is_pub: m.is_pub,
                    doc: text::extract_doc_above(src, m.span.line),
                    source_path: None,
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
                        "(static method) {}{}.{}",
                        render_user_attrs(&m.attrs),
                        c.name,
                        fn_body(m)
                    ),
                    ret_ty: m.ret.clone(),
                    is_static: true,
                    is_pub: m.is_pub,
                    doc: text::extract_doc_above(src, m.span.line),
                    source_path: None,
                });
            }
            out.insert(
                c.name.into(),
                ClassInfo {
                    decl_span: c.span,
                    type_params: c
                        .type_params
                        .iter()
                        .map(|s| s.as_str().to_string())
                        .collect(),
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

/// Register an `interface` / `@com interface` / `@objc interface` in
/// the class-info map so `value.method()` hover and go-to-def can
/// resolve through the interface's method list. The interface's
/// member table mirrors a fields-only `ClassInfo`, but the slots
/// live under `methods` (interfaces don't have fields). The
/// `interface ID3D12Device : IUnknown` chain is followed at the
/// call site, not here — caller passes the leaf, methods of
/// parents resolve via the `parent` lookup chain in `walker.rs`.
fn register_interface_as_class(
    i: &ilang_ast::InterfaceDecl,
    src: &str,
    out: &mut HashMap<AstSymbol, ClassInfo>,
) {
    let mut methods = HashMap::new();
    for m in i.methods.iter() {
        let params = m
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name, p.ty))
            .collect::<Vec<_>>()
            .join(", ");
        let ret_ty = m.ret.clone();
        let ret_str = match &ret_ty {
            Some(t) => format!(": {t}"),
            None => String::new(),
        };
        methods.insert(
            m.name,
            MemberInfo {
                span: m.span,
                signature: format!("(method) {}.{}({}){}", i.name, m.name, params, ret_str),
                ret_ty,
                is_static: false,
                // Interface methods are part of the public
                // contract — every consumer-side `obj.method()`
                // can reach them.
                is_pub: true,
                doc: text::extract_doc_above(src, m.span.line),
                source_path: None,
            },
        );
    }
    out.insert(
        i.name,
        ClassInfo {
            decl_span: i.span,
            // Interfaces don't carry generic params today.
            type_params: Vec::new(),
            fields: HashMap::new(),
            methods,
            getters: HashMap::new(),
            setters: HashMap::new(),
            external: false,
            init_overloads: 0,
            inits: Vec::new(),
            kind: ClassKind::Interface,
        },
    );
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
                AttrArg::NotStr(s) => format!("not \"{s}\""),
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

#[cfg(test)]
mod tests {
    use super::collect_symbols;
    use ilang_ast::Symbol as AstSymbol;
    use ilang_lexer::tokenize;
    use ilang_parser::parse;

    #[test]
    fn synthesized_objc_helpers_excluded_from_symbols_and_completion() {
        // Source with one user @objc class triggers the desugar's
        // sel-cache helper class; `collect_symbols` should not
        // record it.
        let src = "\
@extern(ObjC) {
    @objc pub class NSObject {
        @objc(\"release\") release()
    }
    @objc pub class MyView : NSObject {
        @objc(\"alloc\") pub static alloc(): MyView
    }
}
";
        let toks = tokenize(src).unwrap();
        let prog = parse(&toks).unwrap();
        let syms = collect_symbols(&prog, src);
        for key in syms.keys() {
            assert!(
                !key.as_str().contains("_sel_cache"),
                "synth helper leaked: {}",
                key.as_str()
            );
            assert!(
                !key.as_str().starts_with("__objc_"),
                "synth helper leaked: {}",
                key.as_str()
            );
        }
        // User-declared classes should still be present.
        assert!(syms.contains_key(&AstSymbol::intern("MyView")));
        assert!(syms.contains_key(&AstSymbol::intern("NSObject")));
    }

    #[test]
    fn collect_symbols_picks_up_objc_interface() {
        // @objc interface declared inside @extern(ObjC) should
        // surface in `doc.symbols` so hover over the name works.
        let src = "\
@extern(ObjC) {
    @objc pub interface MyDel {
        notifyMe(name: i64)
        cleanup?()
    }
}
";
        let toks = tokenize(src).unwrap();
        let prog = parse(&toks).unwrap();
        let syms = collect_symbols(&prog, src);
        let key = AstSymbol::intern("MyDel");
        let sym = syms.get(&key).expect("MyDel should be in symbols");
        assert!(
            sym.signature.contains("@objc interface MyDel"),
            "signature: {}",
            sym.signature
        );
        // The method list should be included in the hover detail.
        assert!(sym.signature.contains("notifyMe(name: i64)"), "{}", sym.signature);
        assert!(sym.signature.contains("cleanup?()"), "{}", sym.signature);
    }
}
