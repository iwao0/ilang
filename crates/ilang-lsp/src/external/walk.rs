//! Module walker — load a target `.il` file from disk, parse it, and
//! register every public item it defines into the cross-doc
//! `external_signatures` / `external_sources` / `external_docs` maps.
//! Follows `pub use` chains so umbrella modules reach the file that
//! actually declares each class.
//!
//! The single public entry point `walk_module(prefix)` is invoked for
//! each direct `use M` import. Internally it dispatches through
//! `walk_module_inner`, which also handles the "alias" recursion
//! path (`pub use core.*` umbrella collapse, where a re-exported
//! source is re-walked keyed under the umbrella's own prefix without
//! re-registering visited / the module name).

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

/// Per-walk knobs that select between the "primary import" and "alias
/// re-export" personalities — the only points where the two callers
/// genuinely diverge.
#[derive(Clone, Copy)]
struct WalkOpts {
    /// Insert the module's path into `visited` and skip when already
    /// present. Aliasing the same source under multiple prefixes
    /// requires re-walking, so the alias path leaves this off.
    track_visited: bool,
    /// Register the module's own name (`prefix`) in `sources` /
    /// `out` / `docs` so F12 on `use foundation` lands at the top of
    /// `foundation.il`. Only the primary path does this — the alias
    /// path is invoked from a recursive `pub use` step where the
    /// outer call has already registered the umbrella.
    register_self: bool,
    /// Prefer the real on-disk `libs/std/<name>.il` over the synthetic
    /// `<builtin>/<name>.il` key when both exist. Primary uses this
    /// so F12 navigates to an actual file; the alias path preserves
    /// the synthetic key (matches the loader behaviour).
    prefer_real_builtin_path: bool,
}

impl WalkOpts {
    const fn primary() -> Self {
        Self {
            track_visited: true,
            register_self: true,
            prefer_real_builtin_path: true,
        }
    }
    const fn alias() -> Self {
        Self {
            track_visited: false,
            register_self: false,
            prefer_real_builtin_path: false,
        }
    }
}

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
    walk_module_inner(
        prefix,
        prefix,
        entry_dir,
        extra,
        visited,
        out,
        sources,
        docs,
        const_types,
        WalkOpts::primary(),
    );
}

/// Same as `walk_module`, but lets the caller use a different
/// on-disk source name than the registry key prefix. `use std.math`
/// (bare path-style) keys items under `std.math` while still
/// resolving the file via the leaf `math`.
pub(crate) fn walk_module_as(
    prefix: &str,
    source_name: &str,
    entry_dir: &Path,
    extra: &[PathBuf],
    visited: &mut HashSet<PathBuf>,
    out: &mut HashMap<AstSymbol, String>,
    sources: &mut ExternalSources,
    docs: &mut HashMap<AstSymbol, String>,
    const_types: &mut HashMap<AstSymbol, Type>,
) {
    walk_module_inner(
        prefix,
        source_name,
        entry_dir,
        extra,
        visited,
        out,
        sources,
        docs,
        const_types,
        WalkOpts::primary(),
    );
}

#[allow(clippy::too_many_arguments)]
fn walk_module_inner(
    prefix: &str,
    source_name: &str,
    entry_dir: &Path,
    extra: &[PathBuf],
    visited: &mut HashSet<PathBuf>,
    out: &mut HashMap<AstSymbol, String>,
    sources: &mut ExternalSources,
    docs: &mut HashMap<AstSymbol, String>,
    const_types: &mut HashMap<AstSymbol, Type>,
    opts: WalkOpts,
) {
    let Some((module_path, module_src)) = resolve_module_source(source_name, entry_dir, extra, opts)
    else {
        return;
    };
    if opts.track_visited && !visited.insert(module_path.clone()) {
        return;
    }
    if opts.register_self {
        // F12 on the module name itself (e.g. `sdl` in `use sdl` or
        // `new sdl.Window()`) navigates to the start of the module file.
        sources.entry(prefix.into()).or_insert(ExternalLoc {
            path: module_path.clone(),
            span: Span::new(1, 1),
            name_len: 0,
        });
        // Top-of-file `///` block — the module-level doc. Surfaces
        // on hover over `use foundation` etc. The signature line is a
        // simple `(module) {prefix}` placeholder so the hover renders
        // something even when the file has no top doc.
        out.entry(AstSymbol::intern(prefix))
            .or_insert_with(|| format!("(module) {prefix}"));
        if let Some(d) = text::extract_module_doc(&module_src) {
            docs.entry(AstSymbol::intern(prefix)).or_insert(d);
        }
    }
    let Some(mod_prog) = text::try_parse(&module_src) else { return };
    let mod_dir = module_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let track = |key: &str, span: Span, name_len: u32, sources: &mut ExternalSources| {
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
                track(&key, c.span, c.name.as_str().len() as u32, sources);
                if let Some(d) = text::extract_doc_above(&module_src, c.span.line) {
                    docs.insert(AstSymbol::intern(&key), d);
                }
            }
            Item::Fn(f) => {
                let key = format!("{prefix}.{}", f.name);
                let sig = format!("fn {}", fn_body(f));
                out.insert(
                    AstSymbol::intern(&key),
                    format!("fn {}", sig.trim_start_matches("fn ")),
                );
                track(&key, f.span, f.name.as_str().len() as u32, sources);
                if let Some(d) = text::extract_doc_above(&module_src, f.span.line) {
                    docs.insert(AstSymbol::intern(&key), d);
                }
            }
            Item::Class(c) => {
                let key = format!("{prefix}.{}", c.name);
                let bases = render_class_bases(c.parent.as_ref(), &c.interfaces);
                let attrs = render_user_attrs(&c.attrs);
                out.insert(AstSymbol::intern(&key), format!("{attrs}class {key}{bases}"));
                track(&key, c.span, c.name.as_str().len() as u32, sources);
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
                track(&key, e.span, e.name.as_str().len() as u32, sources);
                if let Some(d) = text::extract_doc_above(&module_src, e.span.line) {
                    docs.insert(AstSymbol::intern(&key), d);
                }
                register_enum_variants_with_sources(e, &key, out, sources, &module_path, &module_src);
            }
            Item::ExternC(b) => {
                for inner in &b.items {
                    // Skip module-private items — only `pub` inner
                    // FnDecls / FnDefs / Structs / Unions / Classes
                    // should surface in another file's `M.` completion.
                    // The `_autoreleasepool_pop` / `_make_obj_block`
                    // family live in foundation's ObjC runtime block
                    // without `pub` precisely so they stay internal.
                    if !is_extern_c_item_pub(inner) {
                        continue;
                    }
                    let (n, span, sig): (AstSymbol, Span, String) = match inner {
                        ilang_ast::ExternCItem::FnDecl {
                            name, type_params, span, params, ret, libs, ..
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
                            (
                                *name,
                                *span,
                                format!("{libs_prefix}fn {prefix}.{name}{tps}({ps}){r}"),
                            )
                        }
                        ilang_ast::ExternCItem::FnDef(f) => {
                            // Don't use fn_body(f) here — it already
                            // renders `name(params): ret` and prepending
                            // `{prefix}.{name}` would produce
                            // `cocoa.sharedApplication sharedApplication(): NSApplication`.
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
                    track(&key, span, n.as_str().len() as u32, sources);
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
                    track(&key, iface.span, iface.name.as_str().len() as u32, sources);
                    if let Some(d) = text::extract_doc_above(&module_src, iface.span.line) {
                        docs.insert(AstSymbol::intern(&key), d);
                    }
                }
                // `pub const` declarations hoisted into the block
                // (e.g. `windows.NULL = 0 as *void` in `winnull.il`).
                // They aren't part of `b.items` — the AST keeps them
                // on `b.consts` so the loader can lift them out as
                // top-level consts with raw-pointer types still legal.
                // Mirror the same harvest as a top-level `Item::Const`
                // so hover finds them.
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
                    track(&key, c.span, c.name.as_str().len() as u32, sources);
                    if let Some(d) = text::extract_doc_above(&module_src, c.span.line) {
                        docs.insert(AstSymbol::intern(&key), d);
                    }
                }
            }
            // Follow `pub use` chains so umbrella modules (e.g.
            // `sdl.il` re-exporting `sdl_renderer.il`) flow the prefix
            // through to the file that actually declares the class.
            //
            // Primary mode walks the re-exported module twice: once
            // under the nested `{prefix}.{u.module}` (so the chain is
            // visible by its real name) and once aliased under `prefix`
            // so the loader's umbrella-prefix collapse is mirrored
            // (`sdl.X` resolves even when the source lives in
            // `sdl_renderer.il`).
            //
            // Alias mode only recurses through the alias chain — its
            // outer call already represents the umbrella view.
            Item::Use(u) if u.re_export && u.selective.is_none() => {
                if opts.register_self {
                    walk_module_inner(
                        &format!("{prefix}.{}", u.module),
                        u.module.as_str(),
                        &mod_dir,
                        extra,
                        visited,
                        out,
                        sources,
                        docs,
                        const_types,
                        WalkOpts::primary(),
                    );
                }
                walk_module_inner(
                    prefix,
                    u.module.as_str(),
                    &mod_dir,
                    extra,
                    visited,
                    out,
                    sources,
                    docs,
                    const_types,
                    WalkOpts::alias(),
                );
            }
            _ => {}
        }
    }
}

/// Locate the on-disk `.il` source for `source_name`. Tries the
/// `loader`-registered builtin first (with the real-file-preference
/// knob the alias path doesn't want), then falls back to
/// `<dir>/M.il` → `<dir>/M/mod.il` across `entry_dir` and `extra`,
/// mirroring `ilang_parser::loader::resolve_module`.
fn resolve_module_source(
    source_name: &str,
    entry_dir: &Path,
    extra: &[PathBuf],
    opts: WalkOpts,
) -> Option<(PathBuf, String)> {
    if let Some(s) = ilang_parser::loader::builtin_module_source(source_name) {
        let path = if opts.prefer_real_builtin_path {
            // Prefer the real on-disk `libs/std/<name>.il` so F12 lands
            // in an actual file. Falls back to the synthetic
            // `<builtin>/<name>.il` key in release-only installs
            // where the source tree isn't present (the rest of the
            // LSP — hover, completion — still works off the embedded
            // source string).
            ilang_parser::loader::builtin_module_path(source_name)
                .unwrap_or_else(|| PathBuf::from(format!("<builtin>/{source_name}.il")))
        } else {
            PathBuf::from(format!("<builtin>/{source_name}.il"))
        };
        return Some((path, s.to_string()));
    }
    // Mirror `loader::resolve_module`: try `<dir>/M.il` first, then
    // `<dir>/M/mod.il` (Rust-style subfolder umbrella), then
    // `<extra>/mod.il` when the extra dir's basename equals
    // `source_name` (the new `ilang.toml [deps]` lookup where the
    // dep umbrella is the dep directory's own `mod.il`).
    let mut candidates = vec![entry_dir.to_path_buf()];
    candidates.extend(extra.iter().cloned());
    candidates.into_iter().find_map(|d| {
        let direct = d.join(format!("{source_name}.il"));
        if let Ok(src) = std::fs::read_to_string(&direct) {
            return Some((direct, src));
        }
        let nested = d.join(source_name).join("mod.il");
        if let Ok(src) = std::fs::read_to_string(&nested) {
            return Some((nested, src));
        }
        // `<extra>/mod.il` — the dep was registered as
        // `[deps] <source_name> = "<extra>"` and the umbrella lives
        // at the dep dir's root. Only fire when the extra dir's
        // basename matches `source_name` so unrelated deps' mod.il
        // files don't bleed into the lookup.
        if d.file_name().and_then(|n| n.to_str()) == Some(source_name) {
            let umbrella = d.join("mod.il");
            if let Ok(src) = std::fs::read_to_string(&umbrella) {
                return Some((umbrella, src));
            }
        }
        None
    })
}
