//! Pre-parse passes that run before — or right after — each file's
//! full parse. `pre_scan_use_modules` peeks at the token stream so
//! the loader can pull in `use` deps before the file itself is
//! parsed; `prescan_sibling_objc_classes` and
//! `collect_objc_class_names` keep the cross-file `@objc class`
//! registry current so subsequent `@extern(ObjC)` desugars see them;
//! `expand_embeds` resolves `@embed("path")` const RHSs against the
//! declaring source file's directory.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use ilang_ast::{ExternCItem, Item, Program, Symbol};
use ilang_lexer::TokenKind;

use super::LoadError;

/// Cheap pre-scan: pluck `<module>` out of every `use <module>` (and
/// `pub use <module>`) at the token level so the loader can resolve
/// dependencies before doing the full parse of this file. Only the
/// module identifier matters — selective imports / aliases are
/// resolved later by `apply_use`. Spurious `use` tokens inside e.g.
/// match arms or argument lists are not a concern: `use` is a keyword
/// reserved for the import form.
pub(super) fn pre_scan_use_modules(
    tokens: &[ilang_lexer::Token],
) -> Vec<(u32, String, Vec<String>)> {
    let mut deps = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        if !matches!(t.kind, TokenKind::Use) { continue }
        // Count leading `super.` prefixes — same shape the parser
        // walks in `parse_use_decl`.
        let mut j = i + 1;
        let mut super_count: u32 = 0;
        while let Some(tok) = tokens.get(j) {
            if matches!(tok.kind, TokenKind::Super) {
                j += 1;
                if let Some(dot) = tokens.get(j) {
                    if matches!(dot.kind, TokenKind::Dot) {
                        j += 1;
                        super_count += 1;
                        continue;
                    }
                }
                break;
            }
            break;
        }
        let module = match tokens.get(j) {
            Some(tok) => match &tok.kind {
                TokenKind::Ident(n) => n.clone(),
                _ => continue,
            },
            None => continue,
        };
        j += 1;
        // Mirror parse_use_decl's dot walk:
        // - `.Ident` followed by `.` is an intermediate path step.
        // - `.Ident` followed by `{` is the deepest file in a
        //   long-form selective (`use a.b { X }`) — push to
        //   subpath because the file is `b`, not `a`.
        // - `.Ident` followed by anything else is the selective
        //   shorthand (`use a.X` → take `X` from `a`); leave
        //   subpath as-accumulated.
        // - `.*` is wildcard; subpath already holds the path.
        let mut subpath: Vec<String> = Vec::new();
        while let Some(dot) = tokens.get(j) {
            if !matches!(dot.kind, TokenKind::Dot) {
                break;
            }
            j += 1;
            match tokens.get(j) {
                Some(tok) => match &tok.kind {
                    TokenKind::Ident(name) => {
                        j += 1;
                        match tokens.get(j).map(|t| &t.kind) {
                            Some(TokenKind::Dot) => {
                                subpath.push(name.clone());
                                continue;
                            }
                            Some(TokenKind::LBrace) => {
                                subpath.push(name.clone());
                                break;
                            }
                            _ => {
                                // `use a.b` bare — `b` is the
                                // deepest file, mirroring
                                // parse_use_decl's path-style import.
                                subpath.push(name.clone());
                                break;
                            }
                        }
                    }
                    TokenKind::Star => break,
                    _ => break,
                },
                None => break,
            }
        }
        deps.push((super_count, module, subpath));
    }
    deps
}

/// Per-sibling result of a folder-module `@objc class` pre-scan.
pub(super) struct SiblingObjcEntry {
    /// Canonicalized sibling path; used to exclude the file currently
    /// being parsed (which harvests its own @objc classes post-parse).
    /// `None` when canonicalize failed (the file is then never treated
    /// as "current", matching the old per-sibling comparison).
    pub(super) canon: Option<PathBuf>,
    /// File stem, registered as an implicit `use <stem>` so the
    /// auto-lift's synthetic cross-sibling refs pass normalize's
    /// dotted-ref check without a (circular) explicit import.
    pub(super) stem: Symbol,
    /// `@objc [pub] class <Name>` names harvested from this sibling.
    pub(super) classes: Vec<Symbol>,
}

/// Whole-directory `@objc class` pre-scan, computed once per folder
/// module and cached. Each sibling `*.il` (skipping `mod.il`) is read
/// and tokenized exactly once here, rather than re-read for every
/// other sibling that gets parsed — an N-file folder used to do
/// O(N²) reads + tokenizations.
pub(super) struct DirObjcScan {
    pub(super) entries: Vec<SiblingObjcEntry>,
}

/// Tokenize each sibling `*.il` in `dir` (skipping `mod.il`) and
/// harvest `@objc pub class <Name>` / `@objc class <Name>` names plus
/// the file stem. The caller merges these (excluding the file it's
/// about to parse) into the cross-file `@objc` registry so a file's
/// auto-lift sees @objc class types declared in sibling category files
/// — without requiring an actual `use sibling { … }`, which would
/// create a circular import.
///
/// Cheap pre-pass: just tokenize and look for the three-token sequence
/// `@objc [pub] class <Ident>`. Avoids a full parse; missed classes
/// (typo'd attribute, etc.) just fall through to the existing
/// post-parse `collect_objc_class_names` pass.
pub(super) fn build_dir_objc_scan(dir: &Path) -> DirObjcScan {
    let mut entries = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return DirObjcScan { entries };
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("il") {
            continue;
        }
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == "mod.il" {
            continue;
        }
        let Some(stem) = p.file_stem().and_then(|s| s.to_str()).map(Symbol::intern) else {
            continue;
        };
        let canon = p.canonicalize().ok();
        let mut classes = Vec::new();
        if let Ok(src) = std::fs::read_to_string(&p) {
            if let Ok(toks) = ilang_lexer::tokenize(&src) {
                // Scan for `@objc ... class <Ident>` patterns. The `...`
                // is either nothing or `pub` (the only modifier the
                // parser accepts between `@objc` and `class` here).
                let mut i = 0;
                while i + 2 < toks.len() {
                    // `@` (At) + `objc` (Ident "objc")
                    if matches!(toks[i].kind, TokenKind::At) {
                        if let TokenKind::Ident(n) = &toks[i + 1].kind {
                            if n.as_str() == "objc" {
                                let mut j = i + 2;
                                if matches!(toks.get(j).map(|t| &t.kind), Some(TokenKind::Pub)) {
                                    j += 1;
                                }
                                if matches!(toks.get(j).map(|t| &t.kind), Some(TokenKind::Class)) {
                                    if let Some(TokenKind::Ident(cls)) =
                                        toks.get(j + 1).map(|t| &t.kind)
                                    {
                                        classes.push(Symbol::intern(cls.as_str()));
                                    }
                                }
                            }
                        }
                    }
                    i += 1;
                }
            }
        }
        entries.push(SiblingObjcEntry { canon, stem, classes });
    }
    DirObjcScan { entries }
}

/// Collect every `@objc class` name declared in `prog` (i.e. classes
/// the `@extern(ObjC)` desugar tagged with the `__objc_wrapper`
/// attribute) and add them to `registry`. Called after each file's
/// parse so that subsequently-loaded sibling modules see this file's
/// @objc classes during their own `@extern(ObjC)` desugar.
pub(super) fn collect_objc_class_names(prog: &Program, registry: &mut HashSet<Symbol>) {
    for item in &prog.items {
        let Item::ExternC(blk) = item else { continue };
        for it in blk.items.iter() {
            if let ExternCItem::Class(cd) = it {
                if cd.attrs.iter().any(|a| a.name.as_str() == "objc") {
                    registry.insert(cd.name);
                }
            }
        }
    }
}

/// Resolve `@embed("path/to/file") const X: T` declarations. The
/// path is taken relative to the **declaring source file** (Zig's
/// `@embedFile` rule). On the entry side we accept two type shapes:
///
/// - `: string` — the file is read as UTF-8 (invalid UTF-8 is a
///   `BadConst` error) and the const's value is replaced with a
///   `Str` literal.
/// - `: u8[]` — the file is read as raw bytes and the value becomes
///   an `Array` literal of `Int(byte)` elements. Large embeds keep
///   their array shape; the const-folder leaves array initialisers
///   as runtime one-shot inits so the AST stays cheap.
pub(super) fn expand_embeds(prog: &mut Program, source_file: &Path) -> Result<(), LoadError> {
    let source_dir = source_file
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    // Canonicalize once so per-embed escape checks only pay for the
    // child canonicalize. Falls back to the literal dir when
    // canonicalize fails (rare — happens when the source file's
    // parent disappeared mid-build); the per-embed check below
    // gracefully degrades by skipping the boundary check in that
    // case, letting the read error surface normally.
    let canonical_base = source_dir.canonicalize().ok();
    for item in prog.items.iter_mut() {
        let Item::Const(c) = item else { continue };
        let Some(rel) = c.embed_path.clone() else { continue };
        let path = source_dir.join(rel.as_str());
        // Containment check: an `@embed("../../etc/passwd")` or an
        // absolute path would otherwise let a built ilang program
        // read arbitrary files from the compiling machine. The
        // canonical form follows symlinks, so a symlink pointing
        // outside the source tree is caught too.
        if let (Some(base), Ok(canonical_path)) =
            (&canonical_base, path.canonicalize())
        {
            if !canonical_path.starts_with(base) {
                return Err(LoadError::BadConst {
                    name: c.name.clone(),
                    reason: format!(
                        "@embed({:?}): path escapes the source file's directory ({}). \
                         Embeds must resolve inside the source tree.",
                        rel.as_str(),
                        base.display(),
                    ),
                    span: c.value.span,
                });
            }
        }
        let bytes = std::fs::read(&path).map_err(|e| LoadError::ReadError {
            path: path.clone(),
            message: format!("@embed({:?}): {e}", rel.as_str()),
        })?;
        let span = c.value.span;
        match &c.ty {
            Some(ilang_ast::Type::Str) => {
                let s = std::str::from_utf8(&bytes).map_err(|e| LoadError::BadConst {
                    name: c.name.clone(),
                    reason: format!(
                        "@embed({:?}): file is not valid UTF-8 ({e}). Declare the const as `u8[]` to read raw bytes.",
                        rel.as_str()
                    ),
                    span,
                })?;
                c.value = ilang_ast::Expr {
                    kind: ilang_ast::ExprKind::Str(s.to_string()),
                    span,
                };
            }
            Some(ilang_ast::Type::Array { elem, .. })
                if matches!(**elem, ilang_ast::Type::U8) =>
            {
                let elems: Vec<ilang_ast::Expr> = bytes
                    .iter()
                    .map(|b| ilang_ast::Expr {
                        kind: ilang_ast::ExprKind::Int(*b as i64),
                        span,
                    })
                    .collect();
                c.value = ilang_ast::Expr {
                    kind: ilang_ast::ExprKind::Array(elems.into_boxed_slice()),
                    span,
                };
            }
            other => {
                return Err(LoadError::BadConst {
                    name: c.name.clone(),
                    reason: format!(
                        "@embed only supports `: string` or `: u8[]` (got {:?})",
                        other
                    ),
                    span,
                });
            }
        }
    }
    Ok(())
}
