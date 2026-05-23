//! Cross-document `collect_external_*` passes — after `walk_module*`
//! merges every imported module into a single `Program`, these
//! functions distill it back into the lookup maps the LSP keeps per
//! buffer (classes / interfaces / fn + const signatures). Pure
//! reductions over the AST; no I/O.

#![allow(unused_imports)]

use std::collections::HashMap;
use std::path::PathBuf;

use ilang_ast::{
    ClassDecl, Expr, Item, Program, Span, Symbol as AstSymbol, Type,
};

use super::enums::register_enum_variants;
use super::{is_extern_c_item_pub, ExternalLoc};
use crate::helpers::{
    infer_expr_type_with_scope, render_class_bases, render_const_value,
    render_const_value_with_src,
};
use crate::symbols::{fn_body, fn_signature, render_user_attrs};
use crate::text;
use crate::types::{ClassInfo, ClassKind, MemberInfo};
use crate::ExternalSources;

pub(crate) fn collect_external_classes(
    prog: &Program,
    sources: &ExternalSources,
) -> HashMap<AstSymbol, ClassInfo> {
    use ilang_ast::ExternCItem;
    let mut classes: Vec<&ClassDecl> = Vec::new();
    let mut out: HashMap<AstSymbol, ClassInfo> = HashMap::new();
    let mut src_cache: HashMap<PathBuf, String> = HashMap::new();
    pub(crate) fn ensure_src<'a>(
        cache: &'a mut HashMap<PathBuf, String>,
        sources: &ExternalSources,
        class_key: &AstSymbol,
    ) -> Option<&'a str> {
        let path = sources.get(class_key)?.path.clone();
        if !cache.contains_key(&path) {
            let txt = std::fs::read_to_string(&path).ok()?;
            cache.insert(path.clone(), txt);
        }
        cache.get(&path).map(|s| s.as_str())
    }
    pub(crate) fn field_doc_at(
        cache: &mut HashMap<PathBuf, String>,
        sources: &ExternalSources,
        class_key: &AstSymbol,
        line: u32,
    ) -> Option<String> {
        let s = ensure_src(cache, sources, class_key)?;
        text::extract_doc_above(s, line)
    }
    pub(crate) fn static_field_value(
        cache: &mut HashMap<PathBuf, String>,
        sources: &ExternalSources,
        class_key: &AstSymbol,
        value: &Expr,
    ) -> Option<String> {
        let s = ensure_src(cache, sources, class_key)?;
        render_const_value_with_src(value, Some(s))
    }
    for item in &prog.items {
        match item {
            Item::Class(c) if c.name.as_str().contains('.') => classes.push(c),
            Item::Interface(i) if i.name.as_str().contains('.') => {
                register_external_interface(i, sources, &mut src_cache, &mut out);
            }
            Item::ExternC(b) => {
                for iface in b.interfaces.iter() {
                    if iface.name.as_str().contains('.') {
                        register_external_interface(iface, sources, &mut src_cache, &mut out);
                    }
                }
                for inner in &b.items {
                    match inner {
                        ExternCItem::Class(c) if c.name.as_str().contains('.') => classes.push(c),
                        ExternCItem::Struct { name, fields: fs, span, .. }
                        | ExternCItem::Union { name, fields: fs, span, .. }
                            if name.as_str().contains('.') =>
                        {
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
                                        doc: field_doc_at(&mut src_cache, sources, name, f.span.line),
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
                                    external: true,
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
    for c in &classes {
        // Bare class name used inside member signatures. The receiver
        // type already establishes the module context, so showing
        // `cocoa.NSApplication.setActivationPolicy` instead of
        // `NSApplication.setActivationPolicy` is noise. Keep the
        // qualified `c.name` for catalog keys / doc lookups; only the
        // human-rendered signature lines drop the prefix.
        let class_label = c
            .name
            .as_str()
            .rsplit_once('.')
            .map(|(_, t)| t)
            .unwrap_or(c.name.as_str());
        let mut fields = HashMap::new();
        for f in &c.fields {
            fields.insert(
                f.name.into(),
                MemberInfo {
                    span: f.span,
                    signature: format!("(property) {}.{}: {}", class_label, f.name, f.ty),
                    ret_ty: Some(f.ty.clone()),
                    is_static: false,
                    doc: field_doc_at(&mut src_cache, sources, &c.name, f.span.line),
                },
            );
        }
        for f in &c.static_fields {
            let kind = if f.is_const { "static const" } else { "static property" };
            let value = static_field_value(&mut src_cache, sources, &c.name, &f.value)
                .map(|v| format!(" = {v}"))
                .unwrap_or_default();
            fields.insert(
                f.name.into(),
                MemberInfo {
                    span: f.span,
                    signature: format!(
                        "({}) {}.{}: {}{}",
                        kind, class_label, f.name, f.ty, value
                    ),
                    ret_ty: Some(f.ty.clone()),
                    is_static: true,
                    doc: field_doc_at(&mut src_cache, sources, &c.name, f.span.line),
                },
            );
        }
        let mut getters: HashMap<AstSymbol, MemberInfo> = HashMap::new();
        let mut setters: HashMap<AstSymbol, MemberInfo> = HashMap::new();
        for prop in &c.properties {
            let prop_doc = field_doc_at(&mut src_cache, sources, &c.name, prop.span.line);
            let prop_kind = if prop.is_static { "static property" } else { "property" };
            fields.insert(
                prop.name.into(),
                MemberInfo {
                    span: prop.span,
                    signature: format!(
                        "({prop_kind}) {}.{}: {}",
                        class_label, prop.name, prop.ty
                    ),
                    ret_ty: Some(prop.ty.clone()),
                    is_static: prop.is_static,
                    doc: prop_doc.clone(),
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
                            class_label, prop.name, prop.ty
                        ),
                        ret_ty: Some(prop.ty.clone()),
                        is_static: prop.is_static,
                        doc: field_doc_at(&mut src_cache, sources, &c.name, g.span.line).or_else(|| prop_doc.clone()),
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
                            class_label, prop.name, prop.ty
                        ),
                        ret_ty: Some(prop.ty.clone()),
                        is_static: prop.is_static,
                        doc: field_doc_at(&mut src_cache, sources, &c.name, s.span.line).or_else(|| prop_doc.clone()),
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
                    class_label,
                    fn_body(m)
                ),
                ret_ty: m.ret.clone(),
                is_static: false,
                doc: field_doc_at(&mut src_cache, sources, &c.name, m.span.line),
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
                    class_label,
                    fn_body(m)
                ),
                is_static: true,
                ret_ty: m.ret.clone(),
                doc: field_doc_at(&mut src_cache, sources, &c.name, m.span.line),
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
                external: true,
                init_overloads,
                inits,
                kind: ClassKind::Class,
            },
        );
    }
    // Inherit members from the parent chain. Without this, hovering
    // `sprite.setPosition(...)` where `sprite: SKSpriteNode` would
    // fail because `setPosition` is declared on `SKNode` (the
    // parent) and only sits in `SKNode`'s methods map. Walk every
    // class's parent chain and copy fields / methods / getters /
    // setters into the child, leaving existing keys alone so
    // direct overrides win.
    let mut parents: HashMap<AstSymbol, AstSymbol> = classes
        .iter()
        .filter_map(|c| c.parent.as_ref().map(|p| (c.name.clone(), p.clone())))
        .collect();
    // COM / `@objc` interfaces (`interface ID3DBlob : IUnknown`) also
    // express single-parent inheritance via the AST's `parent` field.
    // Without this, hovering `blob.Release()` on an ID3DBlob receiver
    // fails because `Release` is declared on the IUnknown parent
    // interface — same shape as the class case above.
    for item in &prog.items {
        match item {
            Item::Interface(i) => {
                if let Some(p) = &i.parent {
                    parents.entry(i.name.clone()).or_insert(p.clone());
                }
            }
            Item::ExternC(b) => {
                for iface in b.interfaces.iter() {
                    if let Some(p) = &iface.parent {
                        parents.entry(iface.name.clone()).or_insert(p.clone());
                    }
                }
            }
            _ => {}
        }
    }
    let class_names: Vec<AstSymbol> = out.keys().cloned().collect();
    for child_name in class_names {
        // Walk the parent chain. Each step looks the parent up either
        // by the recorded (possibly already-prefixed) name or by any
        // `*.<bare>` match — same fallback used at the call-site
        // alias pass — so an umbrella-imported parent resolves too.
        let mut visited: std::collections::HashSet<AstSymbol> =
            std::collections::HashSet::new();
        let mut accumulated_fields: HashMap<AstSymbol, MemberInfo> = HashMap::new();
        let mut accumulated_methods: HashMap<AstSymbol, MemberInfo> = HashMap::new();
        let mut accumulated_getters: HashMap<AstSymbol, MemberInfo> = HashMap::new();
        let mut accumulated_setters: HashMap<AstSymbol, MemberInfo> = HashMap::new();
        let mut cursor = parents.get(&child_name).cloned();
        while let Some(parent_name) = cursor {
            if !visited.insert(parent_name.clone()) {
                break;
            }
            let resolved_key = if out.contains_key(&parent_name) {
                Some(parent_name.clone())
            } else {
                let suffix = format!(".{}", parent_name.as_str());
                out.keys()
                    .find(|k| k.as_str().ends_with(&suffix))
                    .cloned()
            };
            let Some(key) = resolved_key else { break };
            if let Some(info) = out.get(&key) {
                for (k, v) in &info.fields {
                    accumulated_fields.entry(k.clone()).or_insert_with(|| v.clone());
                }
                for (k, v) in &info.methods {
                    accumulated_methods.entry(k.clone()).or_insert_with(|| v.clone());
                }
                for (k, v) in &info.getters {
                    accumulated_getters.entry(k.clone()).or_insert_with(|| v.clone());
                }
                for (k, v) in &info.setters {
                    accumulated_setters.entry(k.clone()).or_insert_with(|| v.clone());
                }
            }
            cursor = parents.get(&key).cloned();
        }
        if let Some(info) = out.get_mut(&child_name) {
            for (k, v) in accumulated_fields {
                info.fields.entry(k).or_insert(v);
            }
            for (k, v) in accumulated_methods {
                info.methods.entry(k).or_insert(v);
            }
            for (k, v) in accumulated_getters {
                info.getters.entry(k).or_insert(v);
            }
            for (k, v) in accumulated_setters {
                info.setters.entry(k).or_insert(v);
            }
        }
    }
    out
}

/// Register a cross-module interface (`directx12.ID3D12Device`,
/// `cocoa.NSWindowDelegate`, …) so `receiver.method(...)` hovers
/// resolve through the interface's method list when the receiver
/// flows in from another file. Mirrors `register_interface_as_class`
/// in `symbols.rs` but reads doc comments from the declaring file
/// via `sources` instead of the buffer.
fn register_external_interface(
    i: &ilang_ast::InterfaceDecl,
    sources: &ExternalSources,
    src_cache: &mut HashMap<PathBuf, String>,
    out: &mut HashMap<AstSymbol, ClassInfo>,
) {
    let iface_label = i
        .name
        .as_str()
        .rsplit_once('.')
        .map(|(_, t)| t)
        .unwrap_or(i.name.as_str());
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
        // Resolve the interface's declaring file via `sources` so we
        // can pull `///` doc comments above the method out of disk.
        // Skip silently when the file can't be read; we just lose the
        // doc, the rest of the hover still works.
        let doc = sources.get(&i.name).and_then(|loc| {
            let path = loc.path.clone();
            if !src_cache.contains_key(&path) {
                let txt = std::fs::read_to_string(&path).ok()?;
                src_cache.insert(path.clone(), txt);
            }
            let s = src_cache.get(&path)?.as_str();
            text::extract_doc_above(s, m.span.line)
        });
        methods.insert(
            m.name,
            MemberInfo {
                span: m.span,
                signature: format!("(method) {}.{}({}){}", iface_label, m.name, params, ret_str),
                ret_ty,
                is_static: false,
                doc,
            },
        );
    }
    out.insert(
        i.name,
        ClassInfo {
            decl_span: i.span,
            fields: HashMap::new(),
            methods,
            getters: HashMap::new(),
            setters: HashMap::new(),
            external: true,
            init_overloads: 0,
            inits: Vec::new(),
            kind: ClassKind::Interface,
        },
    );
}

pub(crate) fn collect_external_signatures(
    prog: &Program,
) -> (HashMap<AstSymbol, String>, HashMap<AstSymbol, Type>) {
    use ilang_ast::ExternCItem;
    let mut out = HashMap::new();
    let mut rets: HashMap<AstSymbol, Type> = HashMap::new();
    let put_dotted = |name: &str, sig: String, m: &mut HashMap<AstSymbol, String>| {
        if name.contains('.') {
            m.insert(name.into(), sig);
        }
    };
    for item in &prog.items {
        match item {
            Item::Fn(f) => {
                if !f.is_pub {
                    continue;
                }
                put_dotted(f.name.as_str(), fn_signature(f), &mut out);
                if let Some(t) = &f.ret {
                    if f.name.as_str().contains('.') {
                        rets.insert(f.name.clone(), t.clone());
                    }
                }
            }
            Item::Const(c) => {
                if !c.is_pub {
                    continue;
                }
                let resolved_ty = c
                    .ty
                    .clone()
                    .or_else(|| infer_expr_type_with_scope(&c.value, &[]));
                let ty = match &resolved_ty {
                    Some(t) => format!(": {t}"),
                    None => String::new(),
                };
                // Record the const's type alongside fn returns so a
                // buffer-side `let x = ExternConst` can recover the
                // type via the same external-returns fallback path.
                if let Some(t) = resolved_ty {
                    if c.name.as_str().contains('.') {
                        rets.insert(c.name.clone(), t);
                    }
                }
                let value = render_const_value(&c.value)
                    .map(|v| format!(" = {v}"))
                    .unwrap_or_default();
                put_dotted(c.name.as_str(), format!("const {}{ty}{value}", c.name), &mut out);
            }
            Item::Class(c) => {
                if !c.is_pub {
                    continue;
                }
                let bases = render_class_bases(c.parent.as_ref(), &c.interfaces);
                put_dotted(
                    c.name.as_str(),
                    format!("{}class {}{bases}", render_user_attrs(&c.attrs), c.name),
                    &mut out,
                );
            }
            Item::Enum(e) => {
                if !e.is_pub {
                    continue;
                }
                let repr = e
                    .repr_ty
                    .as_ref()
                    .map(|t| format!(": {t}"))
                    .unwrap_or_default();
                let flags_prefix = if e.flags { "@flags\n" } else { "" };
                put_dotted(
                    e.name.as_str(),
                    format!("{}enum {}{}", flags_prefix, e.name, repr),
                    &mut out,
                );
                if e.name.as_str().contains('.') {
                    // No source available in the merged-Program scan;
                    // variant values render as decimal here.
                    register_enum_variants(e, e.name.as_str(), &mut out, None);
                }
            }
            Item::ExternC(b) => {
                for inner in &b.items {
                    if !is_extern_c_item_pub(inner) {
                        continue;
                    }
                    match inner {
                        ExternCItem::FnDecl {
                            name, params, ret, libs, ..
                        } => {
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
                            put_dotted(
                                name.as_str(),
                                format!("{libs_prefix}fn {}({}){}", name, ps, r),
                                &mut out,
                            );
                            if let Some(t) = ret {
                                if name.as_str().contains('.') {
                                    rets.insert(name.clone(), t.clone());
                                }
                            }
                        }
                        ExternCItem::FnDef(f) => {
                            put_dotted(f.name.as_str(), fn_signature(f), &mut out);
                            if let Some(t) = &f.ret {
                                if f.name.as_str().contains('.') {
                                    rets.insert(f.name.clone(), t.clone());
                                }
                            }
                        }
                        ExternCItem::Struct { name, .. } => {
                            put_dotted(name.as_str(), format!("struct {}", name), &mut out);
                        }
                        ExternCItem::Union { name, .. } => {
                            put_dotted(name.as_str(), format!("union {}", name), &mut out);
                        }
                        ExternCItem::Class(c) => {
                            let bases = render_class_bases(c.parent.as_ref(), &c.interfaces);
                            put_dotted(
                                c.name.as_str(),
                                format!(
                                    "{}class {}{bases}",
                                    render_user_attrs(&c.attrs),
                                    c.name
                                ),
                                &mut out,
                            );
                        }
                    }
                }
                // @objc interfaces declared in the same block.
                for iface in b.interfaces.iter() {
                    if !iface.is_pub {
                        continue;
                    }
                    let header = if iface.is_objc { "@objc interface" } else { "interface" };
                    let parent = iface
                        .parent
                        .as_ref()
                        .map(|p| format!(" : {p}"))
                        .unwrap_or_default();
                    put_dotted(
                        iface.name.as_str(),
                        format!("{header} {}{parent}", iface.name),
                        &mut out,
                    );
                }
            }
            _ => {}
        }
    }
    (out, rets)
}

/// Collect every `interface` / `@objc interface` declaration in
/// the loaded program, keyed both by the bare name and by the
/// module-prefixed name (when the loader has already applied a
/// prefix). Drives the "implement missing interface methods"
/// code action: a class body that names
/// `NSApplicationDelegate` (bare, via `use cocoa { … }`) or
/// `cocoa.NSApplicationDelegate` (whole-module reference) finds
/// the same `InterfaceDecl` through this map.
pub(crate) fn collect_external_interfaces(
    prog: &Program,
) -> HashMap<AstSymbol, ilang_ast::InterfaceDecl> {
    let mut out: HashMap<AstSymbol, ilang_ast::InterfaceDecl> = HashMap::new();
    for it in &prog.items {
        match it {
            Item::Interface(i) => {
                out.insert(i.name, i.clone());
                if let Some(bare) = i.name.as_str().rsplit_once('.').map(|(_, t)| t) {
                    out.insert(AstSymbol::intern(bare), i.clone());
                }
            }
            Item::ExternC(b) => {
                for iface in b.interfaces.iter() {
                    out.insert(iface.name, iface.clone());
                    if let Some(bare) = iface
                        .name
                        .as_str()
                        .rsplit_once('.')
                        .map(|(_, t)| t)
                    {
                        out.insert(AstSymbol::intern(bare), iface.clone());
                    }
                }
            }
            _ => {}
        }
    }
    out
}
