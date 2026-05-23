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

mod collect;
mod enums;
mod harvest;
mod walk;
pub(crate) use collect::{
    collect_external_classes, collect_external_interfaces, collect_external_signatures,
};
pub(crate) use enums::{
    discriminant_literal_text, register_builtin_enums, register_enum_variants,
    register_enum_variants_with_sources,
};
pub(crate) use harvest::{harvest_from_program, harvest_imported_consts};
pub(crate) use walk::walk_module;

/// `true` if `inner` is exposed via `pub` and should appear in
/// another module's `use M.` completion. Used by `walk_module` /
/// `walk_module_aliased` to skip the module's internal helpers
/// (dlsym'd C-runtime hooks like `_autoreleasepool_pop`, the
/// `_make_*_block` thunks etc.) that live in the same `@extern(C)`
/// block as the user-facing wrappers but aren't intended as
/// surface API.
fn is_extern_c_item_pub(inner: &ilang_ast::ExternCItem) -> bool {
    use ilang_ast::ExternCItem;
    // Treat parser-synthesised @objc desugar helpers as not-pub
    // even though the parser marks them with `is_pub: true` (the
    // per-block `<tag>_sel_cache` class etc. need to be reachable
    // by sibling-file dispatch wrappers, but they're not user-
    // facing names).
    let name = match inner {
        ExternCItem::FnDecl { name, .. } => name.as_str(),
        ExternCItem::FnDef(f) => f.name.as_str(),
        ExternCItem::Struct { name, .. } => name.as_str(),
        ExternCItem::Union { name, .. } => name.as_str(),
        ExternCItem::Class(c) => c.name.as_str(),
    };
    if crate::symbols::is_synthesized_objc_helper(name) {
        return false;
    }
    match inner {
        ExternCItem::FnDecl { is_pub, .. } => *is_pub,
        ExternCItem::FnDef(f) => f.is_pub,
        ExternCItem::Struct { is_pub, .. } => *is_pub,
        ExternCItem::Union { is_pub, .. } => *is_pub,
        ExternCItem::Class(c) => c.is_pub,
    }
}

/// Pull top-level names with prefix-style identifiers (e.g.
/// `math.sqrt`, `math.pi`) out of a loader-merged program so the LSP
/// can answer hover queries on imported names. Plain (un-dotted) names
/// are skipped — they're already covered by the buffer-only index when
/// declared in the open file.
/// Per-decl source location for `module.<decl>` references — used by
/// cross-file F12 to land on the actual declaration line.
#[derive(Clone, Debug)]
pub(crate) struct ExternalLoc {
    pub(crate) path: PathBuf,
    pub(crate) span: Span,
    pub(crate) name_len: u32,
}

/// Push the parent of every umbrella-folder ancestor (a directory
/// that holds a `mod.il`) of `entry_dir` onto `extra`. Lets the LSP
/// resolve a `use foundation` from a deep category file like
/// `bindings/cocoa/spritekit/actions.il` against its sibling
/// `bindings/cocoa/foundation/mod.il` even when no `ilang.toml`
/// wires the dep path up — the editor opens binding files directly,
/// without the example project that normally supplies the path.
pub(crate) fn augment_with_sibling_module_roots(entry_dir: &Path, extra: &mut Vec<PathBuf>) {
    let mut dir = entry_dir.to_path_buf();
    while dir.join("mod.il").exists() {
        let Some(parent) = dir.parent() else { break };
        let parent_buf = parent.to_path_buf();
        if !extra.iter().any(|p| p == &parent_buf) {
            extra.push(parent_buf.clone());
        }
        dir = parent_buf;
    }
}

/// Walk the buffer's `use module` items and parse each module's source
/// (built-in or on-disk) to extract `Item::Const` declarations. Insert
/// them into `out` keyed by `module.const_name` so the buffer-only
/// walker can still resolve `math.pi` etc. — the main loader pass
/// would have inlined them. Also returns a `module.ClassName` → file
/// path map so cross-file F12 can navigate to the actual definition.

/// Walk a loader-merged program for dotted-name classes (e.g.
/// `sdl.Window`) so the hover walker can resolve method / field
/// accesses on imported types. `sources` carries each prefixed
/// name's file path so we can read the source and lift field doc
/// comments — the merged Program itself doesn't carry source
/// strings.
#[cfg(test)]
mod tests {
    use super::*;

    /// Opening a deep category file in a folder-binding
    /// (`bindings/cocoa/spritekit/actions.il`) should still resolve
    /// `use foundation { NSObject }` against its sibling
    /// `bindings/cocoa/foundation/` even with no `ilang.toml` in any
    /// ancestor — `augment_with_sibling_module_roots` adds the
    /// umbrella-folder parents as search roots so the harvest finds
    /// the sibling's exports.
    #[test]
    fn harvest_walks_up_to_sibling_module() {
        // Resolve repo-relative path from this crate's manifest dir.
        let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let actions_il = manifest
            .join("../../bindings/cocoa/spritekit/actions.il");
        if !actions_il.exists() {
            // Test only meaningful inside the ilang repo layout.
            return;
        }
        let src = std::fs::read_to_string(&actions_il).unwrap();
        let mut out: HashMap<AstSymbol, String> = HashMap::new();
        let mut sources: ExternalSources = HashMap::new();
        let mut docs: HashMap<AstSymbol, String> = HashMap::new();
        let mut const_types: HashMap<AstSymbol, Type> = HashMap::new();
        harvest_imported_consts(
            &actions_il, &src, &mut out, &mut sources, &mut docs, &mut const_types,
        );
        // `NSObject` is imported via `use foundation { NSObject, ... }`
        // — without the sibling-root augmentation the harvest can't
        // find foundation/mod.il and the symbol stays unresolved.
        assert!(
            sources.contains_key(&AstSymbol::intern("NSObject")),
            "NSObject must resolve through sibling foundation/ module"
        );
    }
}
