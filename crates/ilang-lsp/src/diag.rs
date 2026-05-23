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
    PatternKind, Program, Span, Stmt, StmtKind, Symbol, Symbol as AstSymbol, Type, VariantPayload,
};
use ilang_parser::parse as parse_program;
use ilang_types::{check, TypeError};

use crate::*;

/// Diagnostic + the source file the span originated in. Carrying
/// the source separately lets the backend route each entry to
/// the right document URI: a cross-module error reported by the
/// type checker against `foundation/io.il` should appear there,
/// not on whichever buffer the user happens to be editing.
#[derive(Debug, Clone)]
pub(crate) struct DiagEntry {
    pub(crate) source_file: Symbol,
    pub(crate) diagnostic: Diagnostic,
}

pub(crate) fn diag(span: Span, msg: String) -> DiagEntry {
    DiagEntry {
        source_file: span.source_file,
        diagnostic: Diagnostic {
            range: span_full_to_range(span),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("ilang".into()),
            message: msg,
            ..Diagnostic::default()
        },
    }
}

/// Same shape as `diag`, but tagged as a warning so editors paint
/// it in the non-fatal style (yellow squiggle by default in
/// VS Code). Used for `@deprecated` call-site notices and any
/// other non-fatal type-checker outputs.
pub(crate) fn warn_diag(span: Span, msg: String) -> DiagEntry {
    DiagEntry {
        source_file: span.source_file,
        diagnostic: Diagnostic {
            range: span_full_to_range(span),
            severity: Some(DiagnosticSeverity::WARNING),
            source: Some("ilang".into()),
            message: msg,
            tags: Some(vec![DiagnosticTag::DEPRECATED]),
            ..Diagnostic::default()
        },
    }
}

pub(crate) fn build_doc(
    text: String,
    prog: &Program,
    external_signatures: &HashMap<AstSymbol, String>,
    external_returns: &HashMap<AstSymbol, Type>,
    external_classes: &HashMap<AstSymbol, ClassInfo>,
    external_sources: &ExternalSources,
    external_docs: &HashMap<AstSymbol, String>,
    external_interfaces: &HashMap<AstSymbol, ilang_ast::InterfaceDecl>,
    external_enums: &HashMap<AstSymbol, ilang_ast::EnumDecl>,
) -> Doc {
    let symbols = collect_symbols(prog, &text);
    let mut classes = collect_classes(prog, &text);
    install_builtin_classes(&mut classes);
    // Merge in classes the loader pulled in via `use module`. Buffer-
    // local classes win on name collisions.
    for (k, v) in external_classes {
        classes.entry(k.clone()).or_insert_with(|| v.clone());
    }
    // Selective imports — `use M { X }` — also need the bare-name
    // entry so `resolve_obj_class(Var("X"))` and the static-method
    // dispatch (`X.alloc()`) find the class via `classes.get("X")`.
    // The merged-program scan only registered the dotted `M.X`
    // key; alias it back to the bare key the source actually
    // uses. Falls through to any `*.X` match in
    // `external_classes` so a name imported from an umbrella
    // module (`use cocoa { NSWindow }`, where cocoa.il does `pub
    // use appkit as _ { * }`) still finds its underlying
    // `appkit.NSWindow` entry.
    for item in &prog.items {
        let Item::Use(u) = item else { continue };
        let Some(names) = &u.selective else { continue };
        let module = u.module.as_str();
        for name in names.iter() {
            if classes.contains_key(name) {
                continue;
            }
            let direct = AstSymbol::intern(&format!("{module}.{name}"));
            let found = external_classes.get(&direct).cloned().or_else(|| {
                let suffix = format!(".{}", name.as_str());
                external_classes
                    .iter()
                    .find(|(k, _)| k.as_str().ends_with(&suffix))
                    .map(|(_, v)| v.clone())
            });
            if let Some(info) = found {
                classes.insert(name.clone(), info);
            }
        }
    }
    flatten_inherited_members(prog, &mut classes);
    // Register `<Enum>.<Variant>` entries for buffer-local enums so
    // `Enum.` completion (the `external_signatures`-prefix path)
    // surfaces variants alongside cross-module enums. Sub-modules
    // (`is_submodule = true`) get an empty `external_signatures`
    // from the caller — without this, completion on a locally-
    // declared enum would have nothing to enumerate.
    let mut external_signatures = external_signatures.clone();
    for item in &prog.items {
        if let Item::Enum(e) = item {
            register_enum_variants(e, e.name.as_str(), &mut external_signatures, Some(&text));
        }
    }
    let external_signatures = &external_signatures;
    let mut consts: HashMap<AstSymbol, Type> = HashMap::new();
    for item in &prog.items {
        if let Item::Const(c) = item {
            if let Some(t) = c
                .ty
                .clone()
                .or_else(|| infer_expr_type_with_scope(&c.value, &[]))
            {
                consts.insert(c.name.clone(), t);
            }
        }
    }
    let mut fn_returns: HashMap<AstSymbol, Type> = HashMap::new();
    for item in &prog.items {
        match item {
            Item::Fn(f) => {
                if let Some(t) = &f.ret {
                    fn_returns.insert(f.name.clone(), t.clone());
                }
            }
            Item::ExternC(b) => {
                for inner in &b.items {
                    match inner {
                        ilang_ast::ExternCItem::FnDecl { name, ret: Some(t), .. } => {
                            fn_returns.insert(name.clone(), t.clone());
                        }
                        ilang_ast::ExternCItem::FnDef(f) => {
                            if let Some(t) = &f.ret {
                                fn_returns.insert(f.name.clone(), t.clone());
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    let mut refs = Vec::new();
    let mut var_classes: HashMap<AstSymbol, String> = HashMap::new();
    let mut var_types: HashMap<AstSymbol, Type> = HashMap::new();
    {
        let mut walker = Walker {
            text: &text,
            symbols: &symbols,
            classes: &classes,
            fn_returns: &fn_returns,
            external_signatures,
            external_docs,
            external_returns,
            external_sources,
            refs: &mut refs,
            var_classes: &mut var_classes,
            var_types: &mut var_types,
            consts: &consts,
            current_this_class: None,
        };
        // Pre-pass: register every top-level `let X = expr`'s
        // inferred type into `var_types` (and a hover decl into
        // `symbols`) so item bodies that reference `X` resolve to
        // the right signature. The full walk below re-walks each
        // stmt to push value-expression refs; `push_decl` /
        // `var_types` are idempotent, so the second visit
        // overwrites with the same data.
        for s in &prog.stmts {
            if let StmtKind::Let { name, ty, value, .. } = &s.kind {
                let inferred = ty
                    .clone()
                    .or_else(|| walker.infer_expr(value, &[]));
                let sig = BindKind::Let.render(name.as_str(), inferred.as_ref());
                let name_span = text::locate_let_name(&text, s.span, name.as_str())
                    .unwrap_or(s.span);
                walker.push_decl(name.as_str(), name_span, sig);
                if let Some(t) = inferred {
                    walker.var_types.insert(name.clone(), t.clone());
                    if let Some(c) = type_to_class(&t) {
                        walker.var_classes.insert(name.clone(), c);
                    }
                }
            }
        }
        for item in &prog.items {
            match item {
                Item::Fn(f) => walker.walk_fn(f, None),
                Item::Class(c) => walker.walk_class(c),
                Item::Interface(i) => walker.walk_interface(i),
                Item::Use(u) => {
                    // `use module` — push a hover entry on the module
                    // identifier itself, with F12 navigating to the
                    // module file's first line.
                    if let Some(name_span) = locate_let_name_with_kw(
                        &text,
                        u.span,
                        "use",
                        u.module.as_str(),
                    ) {
                        let loc = walker.external_sources.get(&u.module);
                        let target_uri = loc
                            .and_then(|l| Url::from_file_path(&l.path).ok());
                        let (target_span, target_name_len, no_def) = match &loc {
                            Some(l) if target_uri.is_some() => (l.span, l.name_len, false),
                            _ => (name_span, u.module.as_str().len() as u32, target_uri.is_none()),
                        };
                        // Module-level doc — top-of-file `///` block
                        // harvested into `external_docs[u.module]` by
                        // `walk_module`. Surfaces on hover over the
                        // module name in `use foundation` etc.
                        let mod_doc = walker
                            .external_docs
                            .get(&u.module)
                            .cloned();
                        walker.refs.push(RefEntry {
                            line: name_span.line,
                            start_col: name_span.col,
                            end_col: name_span.col + u.module.as_str().len() as u32,
                            target_span,
                            target_name_len,
                            signature: format!("(module) {}", u.module),
                            no_definition: no_def,
                            target_uri,
                            doc: mod_doc,
                        });
                    }
                    // `use module { name1, name2 }` — push a hover /
                    // F12 entry on each selectively-imported name so
                    // hovering or jumping from the import line itself
                    // works the same as from a use site.
                    if let Some(names) = &u.selective {
                        for name in names.iter() {
                            let Some((line, col)) =
                                locate_selective_name(&text, u.span, name.as_str())
                            else {
                                continue;
                            };
                            let key = AstSymbol::intern(name.as_str());
                            let sig = walker
                                .external_signatures
                                .get(&key)
                                .cloned()
                                .unwrap_or_else(|| format!("(import) {name}"));
                            let loc = walker.external_sources.get(&key);
                            let target_uri = loc
                                .and_then(|l| Url::from_file_path(&l.path).ok());
                            let (target_span, target_name_len, no_def) = match loc {
                                Some(l) if target_uri.is_some() => (l.span, l.name_len, false),
                                _ => (
                                    Span::new(line, col),
                                    name.as_str().len() as u32,
                                    target_uri.is_none(),
                                ),
                            };
                            walker.refs.push(RefEntry {
                                line,
                                start_col: col,
                                end_col: col + name.as_str().len() as u32,
                                target_span,
                                target_name_len,
                                signature: sig,
                                no_definition: no_def,
                                target_uri,
                                doc: walker.external_docs.get(&key).cloned(),
                            });
                        }
                    }
                }
                Item::ExternC(b) => {
                    for inner in &b.items {
                        match inner {
                            ilang_ast::ExternCItem::FnDef(f) => walker.walk_fn(f, None),
                            ilang_ast::ExternCItem::Class(c) => walker.walk_class(c),
                            ilang_ast::ExternCItem::Struct {
                                name, fields, ..
                            }
                            | ilang_ast::ExternCItem::Union {
                                name, fields, ..
                            } => {
                                for f in fields {
                                    walker.push_decl_with_doc(
                                        f.name.as_str(),
                                        f.span,
                                        format!("(property) {}.{}: {}", name, f.name, f.ty),
                                        text::extract_doc_above(walker.text, f.span.line),
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Item::Enum(e) => {
                    // Push a hover / F12 entry on each variant name
                    // at its declaration site. The signature reuses
                    // `register_enum_variants` formatting (preserves
                    // hex / underscore literals via the buffer text).
                    let mut tmp: HashMap<AstSymbol, String> = HashMap::new();
                    register_enum_variants(e, e.name.as_str(), &mut tmp, Some(walker.text));
                    for v in e.variants.iter() {
                        let key = AstSymbol::intern(&format!(
                            "{}.{}", e.name, v.name
                        ));
                        let sig = tmp
                            .get(&key)
                            .cloned()
                            .unwrap_or_else(|| {
                                format!("(variant) {}.{}", e.name, v.name)
                            });
                        walker.push_decl_with_doc(
                            v.name.as_str(),
                            v.span,
                            sig,
                            text::extract_doc_above(walker.text, v.span.line),
                        );
                    }
                }
                _ => {}
            }
        }
        // Top-level stmts/tail (script-style code outside any fn).
        let mut top_scope: Vec<Binding> = Vec::new();
        for s in &prog.stmts {
            walker.walk_stmt(s, &mut top_scope, None);
        }
        if let Some(t) = &prog.tail {
            walker.walk_expr(t, &mut top_scope, None);
        }
    }
    refs.sort_by_key(|r| (r.line, r.start_col));
    // Local-buffer interface declarations (top-level + @objc
    // interfaces inside `@extern(ObjC)` blocks). Keyed by bare
    // name so the partial-parse completion path can resolve a
    // reference like `class C : MyDel { … }` without re-parsing.
    let mut local_interfaces: HashMap<AstSymbol, ilang_ast::InterfaceDecl> =
        HashMap::new();
    let mut local_enums: HashMap<AstSymbol, ilang_ast::EnumDecl> =
        HashMap::new();
    let mut selective_use_names: std::collections::HashSet<AstSymbol> =
        std::collections::HashSet::new();
    for item in &prog.items {
        match item {
            ilang_ast::Item::Interface(i) => {
                local_interfaces.insert(i.name, i.clone());
            }
            ilang_ast::Item::Enum(e) => {
                local_enums.insert(e.name, e.clone());
            }
            ilang_ast::Item::ExternC(b) => {
                for iface in b.interfaces.iter() {
                    local_interfaces.insert(iface.name, iface.clone());
                }
            }
            ilang_ast::Item::Use(u) => {
                if let Some(names) = u.selective.as_ref() {
                    for n in names.iter() {
                        selective_use_names.insert(*n);
                    }
                }
            }
            _ => {}
        }
    }
    Doc {
        text,
        symbols,
        classes,
        refs,
        var_classes,
        var_types,
        external_signatures: external_signatures.clone(),
        external_docs: external_docs.clone(),
        external_returns: external_returns.clone(),
        external_sources: external_sources.clone(),
        external_interfaces: external_interfaces.clone(),
        local_interfaces,
        local_enums,
        external_enums: external_enums.clone(),
        selective_use_names,
    }
}

/// Propagate parent-class members into buffer-local subclasses.
/// `collect_external_classes` already flattens the externals (so e.g.
/// `cocoa.NSView` carries `NSObject.handle` and every intermediate's
/// methods), but the local `collect_classes` pass doesn't walk
/// inheritance. Without this, a buffer-local
/// `class GuiView : NSView { ... }` reports no `handle` field on
/// hover / completion of `this.handle` even though the merged program
/// has it through inheritance.
///
/// Local-to-local chains walk hop by hop; the first hop into an
/// external entry already has every ancestor's members folded in by
/// `collect_external_classes`, so we stop there to avoid trying to
/// resolve a bare `NSResponder` parent that only lives on the
/// external entry.
fn flatten_inherited_members(prog: &Program, classes: &mut HashMap<AstSymbol, ClassInfo>) {
    let local_parents: HashMap<AstSymbol, AstSymbol> = prog
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Class(c) => c.parent.as_ref().map(|p| (c.name, p.clone())),
            _ => None,
        })
        .chain(prog.items.iter().flat_map(|it| {
            let mut v: Vec<(AstSymbol, AstSymbol)> = Vec::new();
            if let Item::ExternC(b) = it {
                for inner in b.items.iter() {
                    if let ilang_ast::ExternCItem::Class(c) = inner {
                        if let Some(p) = &c.parent {
                            v.push((c.name, p.clone()));
                        }
                    }
                }
            }
            v.into_iter()
        }))
        .collect();
    let resolve_key = |classes: &HashMap<AstSymbol, ClassInfo>, name: &AstSymbol| {
        if classes.contains_key(name) {
            return Some(name.clone());
        }
        let suffix = format!(".{}", name.as_str());
        classes
            .keys()
            .find(|k| k.as_str().ends_with(&suffix))
            .cloned()
    };
    let child_names: Vec<AstSymbol> = local_parents.keys().cloned().collect();
    for child in child_names {
        let mut visited: HashSet<AstSymbol> = HashSet::new();
        let mut acc_fields: HashMap<AstSymbol, crate::types::MemberInfo> = HashMap::new();
        let mut acc_methods: HashMap<AstSymbol, crate::types::MemberInfo> = HashMap::new();
        let mut acc_getters: HashMap<AstSymbol, crate::types::MemberInfo> = HashMap::new();
        let mut acc_setters: HashMap<AstSymbol, crate::types::MemberInfo> = HashMap::new();
        let mut cursor = local_parents.get(&child).cloned();
        while let Some(parent_name) = cursor {
            if !visited.insert(parent_name.clone()) {
                break;
            }
            let Some(key) = resolve_key(classes, &parent_name) else { break };
            if let Some(info) = classes.get(&key) {
                for (k, v) in &info.fields {
                    acc_fields.entry(k.clone()).or_insert_with(|| v.clone());
                }
                for (k, v) in &info.methods {
                    acc_methods.entry(k.clone()).or_insert_with(|| v.clone());
                }
                for (k, v) in &info.getters {
                    acc_getters.entry(k.clone()).or_insert_with(|| v.clone());
                }
                for (k, v) in &info.setters {
                    acc_setters.entry(k.clone()).or_insert_with(|| v.clone());
                }
            }
            cursor = local_parents.get(&parent_name).cloned();
        }
        if let Some(info) = classes.get_mut(&child) {
            for (k, v) in acc_fields {
                info.fields.entry(k).or_insert(v);
            }
            for (k, v) in acc_methods {
                info.methods.entry(k).or_insert(v);
            }
            for (k, v) in acc_getters {
                info.getters.entry(k).or_insert(v);
            }
            for (k, v) in acc_setters {
                info.setters.entry(k).or_insert(v);
            }
        }
    }
}
