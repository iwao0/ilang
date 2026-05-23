//! Per-module `walk_module` / `walk_module_aliased` — load a target
//! `.il` file from disk, parse it, and register every public item it
//! defines (plus follow `pub use` chains so umbrella modules reach
//! the file that actually declares each class). Drives both the
//! plain `use M` import path and the umbrella alias path that
//! collapses `pub use core.*` rebindings under the importer's name.

#![allow(unused_imports)]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use ilang_ast::{Item, Span, Symbol as AstSymbol, Type};
use ilang_lexer::tokenize;
use ilang_parser::parse;

use super::enums::register_enum_variants_with_sources;
use super::{is_extern_c_item_pub, ExternalLoc};
use crate::helpers::{
    infer_expr_type_with_scope, render_class_bases, render_const_value_with_src,
    render_struct_attrs,
};
use crate::symbols::{fn_body, render_user_attrs};
use crate::text;
use crate::ExternalSources;

pub(crate) fn walk_module(
    prefix: &str,
    entry_dir: &Path,
    extra: &[PathBuf],
    visited: &mut HashSet<PathBuf>,
    out: &mut HashMap<AstSymbol, String>,
    sources: &mut ExternalSources,
    docs: &mut HashMap<AstSymbol, String>,
    const_types: &mut HashMap<AstSymbol, Type>,
) {
    let (module_path, module_src) =
        if let Some(s) = ilang_parser::loader::builtin_module_source(prefix) {
            // Prefer the real on-disk `stdlib/<name>.il` so F12 lands
            // in an actual file. Falls back to the synthetic
            // `<builtin>/<name>.il` key in release-only installs where
            // the source tree isn't present (the rest of the LSP — hover,
            // completion — still works off the embedded source string).
            let real = ilang_parser::loader::builtin_module_path(prefix)
                .unwrap_or_else(|| PathBuf::from(format!("<builtin>/{prefix}.il")));
            (real, s.to_string())
        } else {
            // Mirror `loader::resolve_module`: try `<dir>/M.il` first,
            // then fall back to `<dir>/M/mod.il` (Rust-style subfolder
            // umbrella). Without the second arm F12 / hover go blank
            // on every name that lives behind a `pub use mod.*`
            // umbrella, because the harvest never finds the parsed
            // declarations.
            let mut candidates = vec![entry_dir.to_path_buf()];
            candidates.extend(extra.iter().cloned());
            let Some((p, s)) = candidates.into_iter().find_map(|d| {
                let direct = d.join(format!("{prefix}.il"));
                if let Ok(src) = std::fs::read_to_string(&direct) {
                    return Some((direct, src));
                }
                let nested = d.join(prefix).join("mod.il");
                std::fs::read_to_string(&nested).ok().map(|src| (nested, src))
            }) else {
                return;
            };
            (p, s)
        };
    if !visited.insert(module_path.clone()) {
        return;
    }
    // F12 on the module name itself (e.g. `sdl` in `use sdl` or
    // `new sdl.Window()`) navigates to the start of the module file.
    sources.entry(prefix.into()).or_insert(ExternalLoc {
        path: module_path.clone(),
        span: Span::new(1, 1),
        name_len: 0,
    });
    // Top-of-file `///` block — the module-level doc. Surfaces on
    // hover over `use foundation` etc. The signature line is a
    // simple `(module) {prefix}` placeholder so the hover renders
    // something even when the file has no top doc.
    out.entry(AstSymbol::intern(prefix))
        .or_insert_with(|| format!("(module) {prefix}"));
    if let Some(d) = text::extract_module_doc(&module_src) {
        docs.entry(AstSymbol::intern(prefix)).or_insert(d);
    }
    let Ok(tokens) = tokenize(&module_src) else { return };
    let Ok(mod_prog) = parse(&tokens) else { return };
    let mod_dir = module_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let track = |key: &str,
                 span: Span,
                 name_len: u32,
                 sources: &mut ExternalSources,
                 p: &PathBuf| {
        sources.insert(
            key.into(),
            ExternalLoc {
                path: p.clone(),
                span,
                name_len,
            },
        );
    };
    for it in &mod_prog.items {
        match it {
            Item::Const(c) => {
                let resolved_ty = c
                    .ty
                    .clone()
                    .or_else(|| infer_expr_type_with_scope(&c.value, &[]));
                let ty = match &resolved_ty {
                    Some(t) => format!(": {t}"),
                    None => String::new(),
                };
                let value = render_const_value_with_src(&c.value, Some(&module_src))
                    .map(|v| format!(" = {v}"))
                    .unwrap_or_default();
                let key = format!("{prefix}.{}", c.name);
                out.insert(AstSymbol::intern(&key), format!("const {key}{ty}{value}"));
                if let Some(t) = resolved_ty {
                    const_types.insert(AstSymbol::intern(&key), t);
                }
                track(&key, c.span, c.name.as_str().len() as u32, sources, &module_path);
                if let Some(d) = text::extract_doc_above(&module_src, c.span.line) {
                    docs.insert(AstSymbol::intern(&key), d);
                }
            }
            Item::Fn(f) => {
                let key = format!("{prefix}.{}", f.name);
                let sig = format!("fn {}", fn_body(f));
                out.insert(AstSymbol::intern(&key), format!("fn {}", sig.trim_start_matches("fn ")));
                track(&key, f.span, f.name.as_str().len() as u32, sources, &module_path);
                if let Some(d) = text::extract_doc_above(&module_src, f.span.line) {
                    docs.insert(AstSymbol::intern(&key), d);
                }
            }
            Item::Class(c) => {
                let key = format!("{prefix}.{}", c.name);
                let bases = render_class_bases(c.parent.as_ref(), &c.interfaces);
                let attrs = render_user_attrs(&c.attrs);
                out.insert(AstSymbol::intern(&key), format!("{attrs}class {key}{bases}"));
                track(&key, c.span, c.name.as_str().len() as u32, sources, &module_path);
                if let Some(d) = text::extract_doc_above(&module_src, c.span.line) {
                    docs.insert(AstSymbol::intern(&key), d);
                }
            }
            Item::Enum(e) => {
                let key = format!("{prefix}.{}", e.name);
                let repr = e
                    .repr_ty
                    .as_ref()
                    .map(|t| format!(": {t}"))
                    .unwrap_or_default();
                let flags_prefix = if e.flags { "@flags\n" } else { "" };
                out.insert(AstSymbol::intern(&key), format!("{flags_prefix}enum {key}{repr}"));
                track(&key, e.span, e.name.as_str().len() as u32, sources, &module_path);
                if let Some(d) = text::extract_doc_above(&module_src, e.span.line) {
                    docs.insert(AstSymbol::intern(&key), d);
                }
                register_enum_variants_with_sources(e, &key, out, sources, &module_path, &module_src);
            }
            Item::ExternC(b) => {
                for inner in &b.items {
                    // Skip module-private items — only `pub` inner
                    // FnDecls / FnDefs / Structs / Unions / Classes
                    // should surface in another file's `M.`
                    // completion. The `_autoreleasepool_pop` /
                    // `_make_obj_block` family live in foundation's
                    // ObjC runtime block without `pub` precisely so
                    // they stay internal.
                    if !is_extern_c_item_pub(inner) {
                        continue;
                    }
                    let (n, span, sig): (AstSymbol, Span, String) = match inner {
                        ilang_ast::ExternCItem::FnDecl {
                            name, span, params, ret, libs, ..
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
                            (
                                *name,
                                *span,
                                format!("{libs_prefix}fn {prefix}.{name}({ps}){r}"),
                            )
                        }
                        ilang_ast::ExternCItem::FnDef(f) => {
                            // Mirror the FnDecl arm above
                            // (`fn {prefix}.name(params): ret`).
                            // Previously the format string used
                            // `fn_body(f)` (which already renders
                            // `name(params): ret`) *and* prepended
                            // `{prefix}.{name}` — producing
                            // `cocoa.sharedApplication sharedApplication(): NSApplication`
                            // in hover.
                            let ps = f
                                .params
                                .iter()
                                .map(|p| format!("{}: {}", p.name, p.ty))
                                .collect::<Vec<_>>()
                                .join(", ");
                            let r = match &f.ret {
                                Some(t) => format!(": {t}"),
                                None => String::new(),
                            };
                            (
                                f.name.into(),
                                f.span,
                                format!("fn {prefix}.{}({ps}){r}", f.name),
                            )
                        }
                        ilang_ast::ExternCItem::Struct {
                            name, span, is_packed, is_handle, ..
                        } => {
                            let attrs = render_struct_attrs(*is_packed, *is_handle);
                            (
                                *name,
                                *span,
                                format!("{attrs}struct {prefix}.{name}"),
                            )
                        }
                        ilang_ast::ExternCItem::Union { name, span, .. } => (
                            *name,
                            *span,
                            format!("union {prefix}.{name}"),
                        ),
                        ilang_ast::ExternCItem::Class(c) => {
                            let bases = render_class_bases(c.parent.as_ref(), &c.interfaces);
                            let attrs = render_user_attrs(&c.attrs);
                            (
                                c.name.into(),
                                c.span,
                                format!("{attrs}class {prefix}.{}{bases}", c.name),
                            )
                        }
                    };
                    let key = format!("{prefix}.{n}");
                    out.insert(AstSymbol::intern(&key), sig);
                    track(&key, span, n.as_str().len() as u32, sources, &module_path);
                    if let Some(d) = text::extract_doc_above(&module_src, span.line) {
                        docs.insert(AstSymbol::intern(&key), d);
                    }
                }
                // @objc interfaces declared alongside the C / @objc
                // items in the same block. Surface them in cross-module
                // completion so `use cocoa { NSApplicationDelegate }`
                // hovers and other-file references find the signature.
                for iface in b.interfaces.iter() {
                    if !iface.is_pub {
                        continue;
                    }
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
                    let sig = if methods.is_empty() {
                        format!("{header} {prefix}.{}{parent} {{}}", iface.name)
                    } else {
                        format!(
                            "{header} {prefix}.{}{parent} {{\n{}\n}}",
                            iface.name,
                            methods.join("\n")
                        )
                    };
                    let key = format!("{prefix}.{}", iface.name);
                    out.insert(AstSymbol::intern(&key), sig);
                    track(
                        &key,
                        iface.span,
                        iface.name.as_str().len() as u32,
                        sources,
                        &module_path,
                    );
                    if let Some(d) = text::extract_doc_above(&module_src, iface.span.line) {
                        docs.insert(AstSymbol::intern(&key), d);
                    }
                }
                // `pub const` declarations hoisted into the block
                // (e.g. `windows.NULL = 0 as *void` in `winnull.il`).
                // They aren't part of `b.items` — the AST keeps them
                // on `b.consts` so the loader can lift them out as
                // top-level consts with raw-pointer types still
                // legal. Mirror the same harvest as a top-level
                // `Item::Const` so hover finds them.
                for c in b.consts.iter() {
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
                    let value = render_const_value_with_src(&c.value, Some(&module_src))
                        .map(|v| format!(" = {v}"))
                        .unwrap_or_default();
                    let key = format!("{prefix}.{}", c.name);
                    out.insert(AstSymbol::intern(&key), format!("const {key}{ty}{value}"));
                    if let Some(t) = resolved_ty {
                        const_types.insert(AstSymbol::intern(&key), t);
                    }
                    track(&key, c.span, c.name.as_str().len() as u32, sources, &module_path);
                    if let Some(d) = text::extract_doc_above(&module_src, c.span.line) {
                        docs.insert(AstSymbol::intern(&key), d);
                    }
                }
            }
            // Follow `pub use` chains so umbrella modules
            // (e.g. `sdl.il` re-exporting `sdl_renderer.il`) flow the
            // prefix through to the file that actually declares the
            // class.
            Item::Use(u) if u.re_export && u.selective.is_none() => {
                walk_module(
                    &format!("{prefix}.{}", u.module),
                    &mod_dir,
                    extra,
                    visited,
                    out,
                    sources,
                    docs,
                    const_types,
                );
                // Loader collapses one-deep umbrella prefixes so the
                // entry sees `sdl.X` (not `sdl.sdl_renderer.X`). Mirror
                // that: also record the umbrella's own prefix.
                walk_module_aliased(
                    prefix,
                    u.module.as_str(),
                    &mod_dir,
                    extra,
                    visited,
                    out,
                    sources,
                    docs,
                    const_types,
                );
            }
            _ => {}
        }
    }
}

pub(crate) fn walk_module_aliased(
    alias_prefix: &str,
    actual: &str,
    entry_dir: &Path,
    extra: &[PathBuf],
    visited: &mut HashSet<PathBuf>,
    out: &mut HashMap<AstSymbol, String>,
    sources: &mut ExternalSources,
    docs: &mut HashMap<AstSymbol, String>,
    const_types: &mut HashMap<AstSymbol, Type>,
) {
    let (module_path, module_src) =
        if let Some(s) = ilang_parser::loader::builtin_module_source(actual) {
            (
                PathBuf::from(format!("<builtin>/{actual}.il")),
                s.to_string(),
            )
        } else {
            // Same `<dir>/M.il` → `<dir>/M/mod.il` fallback as
            // `walk_module` — alias chasing must follow the loader's
            // subfolder resolution rule, otherwise F12 on a name
            // re-exported through `pub use core.*` lands nowhere.
            let mut candidates = vec![entry_dir.to_path_buf()];
            candidates.extend(extra.iter().cloned());
            let Some((p, s)) = candidates.into_iter().find_map(|d| {
                let direct = d.join(format!("{actual}.il"));
                if let Ok(src) = std::fs::read_to_string(&direct) {
                    return Some((direct, src));
                }
                let nested = d.join(actual).join("mod.il");
                std::fs::read_to_string(&nested).ok().map(|src| (nested, src))
            }) else {
                return;
            };
            (p, s)
        };
    let Ok(tokens) = tokenize(&module_src) else { return };
    let Ok(mod_prog) = parse(&tokens) else { return };
    let put = |key: &str, span: Span, name_len: u32, sources: &mut ExternalSources| {
        sources.insert(
            key.into(),
            ExternalLoc {
                path: module_path.clone(),
                span,
                name_len,
            },
        );
    };
    for it in &mod_prog.items {
        match it {
            Item::Const(c) => {
                let key = format!("{alias_prefix}.{}", c.name);
                let resolved_ty = c
                    .ty
                    .clone()
                    .or_else(|| infer_expr_type_with_scope(&c.value, &[]));
                let ty = match &resolved_ty {
                    Some(t) => format!(": {t}"),
                    None => String::new(),
                };
                let value = render_const_value_with_src(&c.value, Some(&module_src))
                    .map(|v| format!(" = {v}"))
                    .unwrap_or_default();
                out.insert(AstSymbol::intern(&key), format!("const {key}{ty}{value}"));
                if let Some(t) = resolved_ty {
                    const_types.insert(AstSymbol::intern(&key), t);
                }
                put(&key, c.span, c.name.as_str().len() as u32, sources);
                if let Some(d) = text::extract_doc_above(&module_src, c.span.line) {
                    docs.insert(AstSymbol::intern(&key), d);
                }
            }
            Item::Fn(f) => {
                let key = format!("{alias_prefix}.{}", f.name);
                let sig = format!("fn {}", fn_body(f));
                out.insert(
                    AstSymbol::intern(&key),
                    format!("fn {}", sig.trim_start_matches("fn ")),
                );
                put(&key, f.span, f.name.as_str().len() as u32, sources);
                if let Some(d) = text::extract_doc_above(&module_src, f.span.line) {
                    docs.insert(AstSymbol::intern(&key), d);
                }
            }
            Item::Class(c) => {
                let key = format!("{alias_prefix}.{}", c.name);
                let bases = render_class_bases(c.parent.as_ref(), &c.interfaces);
                let attrs = render_user_attrs(&c.attrs);
                out.insert(AstSymbol::intern(&key), format!("{attrs}class {key}{bases}"));
                put(&key, c.span, c.name.as_str().len() as u32, sources);
                if let Some(d) = text::extract_doc_above(&module_src, c.span.line) {
                    docs.insert(AstSymbol::intern(&key), d);
                }
            }
            Item::Enum(e) => {
                let key = format!("{alias_prefix}.{}", e.name);
                let repr = e
                    .repr_ty
                    .as_ref()
                    .map(|t| format!(": {t}"))
                    .unwrap_or_default();
                let flags_prefix = if e.flags { "@flags\n" } else { "" };
                out.insert(AstSymbol::intern(&key), format!("{flags_prefix}enum {key}{repr}"));
                put(&key, e.span, e.name.as_str().len() as u32, sources);
                if let Some(d) = text::extract_doc_above(&module_src, e.span.line) {
                    docs.insert(AstSymbol::intern(&key), d);
                }
                register_enum_variants_with_sources(e, &key, out, sources, &module_path, &module_src);
            }
            Item::ExternC(b) => {
                for inner in &b.items {
                    // See `walk_module`'s ExternC arm — same rule
                    // applies to umbrella re-exports.
                    if !is_extern_c_item_pub(inner) {
                        continue;
                    }
                    let entry: Option<(AstSymbol, Span, String)> = match inner {
                        ilang_ast::ExternCItem::FnDecl {
                            name, span, params, ret, libs, ..
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
                            Some((
                                (*name).into(),
                                *span,
                                format!("{libs_prefix}fn {alias_prefix}.{name}({ps}){r}"),
                            ))
                        }
                        ilang_ast::ExternCItem::FnDef(f) => {
                            // See `walk_module`'s same arm — the
                            // double-name bug applies here too.
                            let ps = f
                                .params
                                .iter()
                                .map(|p| format!("{}: {}", p.name, p.ty))
                                .collect::<Vec<_>>()
                                .join(", ");
                            let r = match &f.ret {
                                Some(t) => format!(": {t}"),
                                None => String::new(),
                            };
                            Some((
                                f.name.into(),
                                f.span,
                                format!("fn {alias_prefix}.{}({ps}){r}", f.name),
                            ))
                        }
                        ilang_ast::ExternCItem::Struct {
                            name, span, is_packed, is_handle, ..
                        } => {
                            let attrs = render_struct_attrs(*is_packed, *is_handle);
                            Some((
                                (*name).into(),
                                *span,
                                format!("{attrs}struct {alias_prefix}.{name}"),
                            ))
                        }
                        ilang_ast::ExternCItem::Union { name, span, .. } => Some((
                            (*name).into(),
                            *span,
                            format!("union {alias_prefix}.{name}"),
                        )),
                        ilang_ast::ExternCItem::Class(c) => {
                            let bases = render_class_bases(c.parent.as_ref(), &c.interfaces);
                            let attrs = render_user_attrs(&c.attrs);
                            Some((
                                c.name.into(),
                                c.span,
                                format!("{attrs}class {alias_prefix}.{}{bases}", c.name),
                            ))
                        }
                    };
                    if let Some((n, span, sig)) = entry {
                        let len = n.as_str().len() as u32;
                        let key = format!("{alias_prefix}.{n}");
                        out.insert(AstSymbol::intern(&key), sig);
                        put(&key, span, len, sources);
                        if let Some(d) = text::extract_doc_above(&module_src, span.line) {
                            docs.insert(AstSymbol::intern(&key), d);
                        }
                    }
                }
                // Aliased re-export side: same enumeration for
                // @objc interfaces declared in the same block.
                for iface in b.interfaces.iter() {
                    if !iface.is_pub {
                        continue;
                    }
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
                    let sig = if methods.is_empty() {
                        format!("{header} {alias_prefix}.{}{parent} {{}}", iface.name)
                    } else {
                        format!(
                            "{header} {alias_prefix}.{}{parent} {{\n{}\n}}",
                            iface.name,
                            methods.join("\n")
                        )
                    };
                    let key = format!("{alias_prefix}.{}", iface.name);
                    let len = iface.name.as_str().len() as u32;
                    out.insert(AstSymbol::intern(&key), sig);
                    put(&key, iface.span, len, sources);
                    if let Some(d) = text::extract_doc_above(&module_src, iface.span.line) {
                        docs.insert(AstSymbol::intern(&key), d);
                    }
                }
                // `pub const` declarations on `b.consts` (e.g.
                // `windows.NULL`). See `walk_module`'s matching arm
                // — keep the alias-prefix in the key so the
                // umbrella's `windows.NULL` hover still works.
                for c in b.consts.iter() {
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
                    let value = render_const_value_with_src(&c.value, Some(&module_src))
                        .map(|v| format!(" = {v}"))
                        .unwrap_or_default();
                    let key = format!("{alias_prefix}.{}", c.name);
                    out.insert(AstSymbol::intern(&key), format!("const {key}{ty}{value}"));
                    if let Some(t) = resolved_ty {
                        const_types.insert(AstSymbol::intern(&key), t);
                    }
                    put(&key, c.span, c.name.as_str().len() as u32, sources);
                    if let Some(d) = text::extract_doc_above(&module_src, c.span.line) {
                        docs.insert(AstSymbol::intern(&key), d);
                    }
                }
            }
            Item::Use(u) if u.re_export && u.selective.is_none() => {
                let mod_dir = module_path
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."));
                walk_module_aliased(
                    alias_prefix,
                    u.module.as_str(),
                    &mod_dir,
                    extra,
                    visited,
                    out,
                    sources,
                    docs,
                    const_types,
                );
            }
            _ => {}
        }
    }
}
