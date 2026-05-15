//! Module loader: resolve `use module` and `use module { name1, name2 }`
//! by reading `<module>.il` adjacent to the importing file, parsing
//! it, and merging its top-level items into the entry program.
//!
//! Loading is recursive (a module's `use` items are followed too),
//! with cycle detection. Items get mangled as follows:
//!   - whole-module import (`use utils`):
//!       - `fn foo` in utils.il      → `utils.foo` in the merged program
//!       - `class Counter`           → `utils.Counter`
//!       - `enum Color`              → `utils.Color`
//!     Callers reference them as `utils.foo(args)`, `new utils.Counter()`,
//!     `utils.Color.red`, etc. The normalize pass + parser already
//!     understand these dotted forms.
//!   - selective import (`use utils { foo, bar }`):
//!       - imported items keep their bare names (`foo`, `bar`).
//!     Anything in utils.il that isn't named in the selective list is
//!     not visible.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use ilang_ast::{
    Block, ClassDecl, Expr, ExprKind, Item, MatchArm, Program, Stmt, StmtKind, Symbol, Type,
    UseDecl,
};

use crate::ParseError;

mod consts;
mod rename;

use consts::inline_constants;
use rename::{rename_in_item, rename_in_program, rename_in_stmt};

/// Modules whose source is shipped inside the compiler. `use math`
/// resolves here before consulting the filesystem.
pub fn builtin_module_source(name: &str) -> Option<&'static str> {
    match name {
        "math" => Some(include_str!("../stdlib/math.il")),
        "test" => Some(include_str!("../stdlib/test.il")),
        "os" => Some(include_str!("../stdlib/os.il")),
        "events" => Some(include_str!("../stdlib/events.il")),
        "fs" => Some(include_str!("../stdlib/fs.il")),
        "path" => Some(include_str!("../stdlib/path.il")),
        "regex" => Some(include_str!("../stdlib/regex.il")),
        _ => None,
    }
}

/// A path-shaped key for built-in modules so the rest of the loader
/// can treat them uniformly with on-disk files.
fn builtin_path(name: &str) -> PathBuf {
    PathBuf::from(format!("<builtin>/{name}.il"))
}

fn is_builtin_path(p: &Path) -> Option<&str> {
    let s = p.to_str()?;
    s.strip_prefix("<builtin>/")
        .and_then(|rest| rest.strip_suffix(".il"))
}

#[derive(Debug)]
pub enum LoadError {
    ReadError {
        path: PathBuf,
        message: String,
    },
    LexError(String),
    ParseError(ParseError),
    CircularImport {
        chain: Vec<Symbol>,
    },
    UnknownImport {
        module: Symbol,
        name: Symbol,
    },
    /// `const X = expr` where `expr` couldn't be folded to a literal.
    /// Carries a human-readable reason and the offending span.
    BadConst {
        name: Symbol,
        reason: String,
        span: ilang_ast::Span,
    },
    /// Cross-module reference to an item that isn't `pub` in its
    /// declaring module. Default visibility is module-private; the
    /// declaring module must mark items `pub` to opt them in.
    PrivateItemRef {
        module: Symbol,
        name: Symbol,
        span: ilang_ast::Span,
    },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::ReadError { path, message } => {
                write!(f, "cannot read {path:?}: {message}")
            }
            LoadError::LexError(s) => write!(f, "lex error: {s}"),
            LoadError::ParseError(e) => write!(f, "parse error: {e}"),
            LoadError::CircularImport { chain } => {
                write!(f, "circular import: {}", chain.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" → "))
            }
            LoadError::UnknownImport { module, name } => {
                write!(f, "module `{module}` doesn't export `{name}`")
            }
            LoadError::BadConst { name, reason, span } => {
                write!(f, "{span}: `const {name}` is not a constant expression: {reason}")
            }
            LoadError::PrivateItemRef { module, name, span } => {
                write!(
                    f,
                    "{span}: `{module}.{name}` is not `pub` in module `{module}` — mark the declaration with `pub` to expose it"
                )
            }
        }
    }
}

/// Load `entry`, recursively resolve every `use`, merge all items
/// into one Program, and return it. Removes all `Item::Use` from the
/// final program.
pub fn load_program(entry: &Path) -> Result<Program, LoadError> {
    load_program_with_paths(entry, &[])
}

/// Variant that accepts additional search paths for `use module`
/// resolution. The importer's own directory is always tried first;
/// each entry in `extra_paths` is then tried in order. Used by the
/// CLI when the project's `ilang.toml` declares dep paths.
pub fn load_program_with_paths(
    entry: &Path,
    extra_paths: &[PathBuf],
) -> Result<Program, LoadError> {
    load_program_with_overlay(entry, extra_paths, &HashMap::new())
}

/// Same as `load_program_with_paths` but lets the caller supply an
/// in-memory source for one or more files. Each `(canonical path,
/// source)` entry overrides the on-disk content during parsing —
/// used by the LSP so unsaved buffer edits drive diagnostics.
pub fn load_program_with_overlay(
    entry: &Path,
    extra_paths: &[PathBuf],
    overlay: &HashMap<PathBuf, String>,
) -> Result<Program, LoadError> {
    let mut visiting: HashSet<PathBuf> = HashSet::new();
    let mut chain: Vec<Symbol> = Vec::new();
    let mut loaded: HashMap<PathBuf, Program> = HashMap::new();
    let entry_dir = entry.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
    let entry_canon = canonicalize(entry)?;
    let extra_paths: Vec<PathBuf> = extra_paths.to_vec();

    load_recursive(
        &entry_canon, &entry_dir, &extra_paths,
        &mut visiting, &mut chain, &mut loaded, overlay,
    )?;

    // Cross-module visibility check before merging: every `M.X`
    // qualified reference and every selective `use M { X }` must
    // target a `pub` item in M. Walks every loaded file (entry
    // included) using the catalog of `pub` items per module.
    crate::visibility::validate_visibility(&loaded, &entry_canon)?;

    let entry_prog = loaded.remove(&entry_canon).expect("entry just loaded");
    // Process the entry's use items into actual merged content.
    // Sub-module top-level stmts are appended to `merged.stmts` first
    // (in dependency order) by `apply_use`, then the entry's own
    // top-level stmts are appended last so they run after every
    // imported module's initialization code (Python-style import
    // semantics).
    let entry_stmts = entry_prog.stmts;
    let mut merged = Program {
        items: Vec::new(),
        stmts: Vec::new(),
        tail: entry_prog.tail,
    };
    let mut whole_imports: HashSet<Symbol> = HashSet::new();
    // Tracks every (module-canonical-path, effective-prefix) pair
    // that's already been merged into `merged`. Stops `use math`
    // appearing in two import paths from registering math's items
    // twice (which would surface as "duplicate overload" later).
    let mut applied: HashSet<(PathBuf, String)> = HashSet::new();
    let mut rename_rules: HashMap<Symbol, Symbol> = HashMap::new();
    for item in entry_prog.items {
        match item {
            Item::Use(u) => apply_use(
                u,
                None,
                &entry_canon,
                &extra_paths,
                &mut loaded,
                &mut merged,
                &mut whole_imports,
                &mut applied,
                &mut rename_rules,
            )?,
            other => merged.items.push(other),
        }
    }
    // Entry's own top-level stmts run after all imported modules'
    // init stmts. Per Python semantics, `import M` runs M's top-level
    // exactly once; `apply_use` enforces the once-only via `applied`.
    merged.stmts.extend(entry_stmts);
    // Apply rename rules accumulated from selective imports that
    // resolved through `pub use` chains. Each rule maps a bare
    // imported name (e.g. `InitFlag` from `use sdl { InitFlag }`)
    // to its umbrella-qualified form (`sdl.InitFlag`), which the
    // umbrella's nested `pub use` already merged into the
    // program. Without the rewrite, bare refs in the entry would
    // resolve to a separate enum / class declaration that the type
    // checker treats as distinct.
    if !rename_rules.is_empty() {
        rename_in_program(&mut merged, &rename_rules);
    }
    // Re-normalize the merged program. Each file was normalized in
    // isolation, so an entry-file reference like `lib.Color.green`
    // collapses to `Field(Var("lib.Color"), "green")` — at parse time
    // `lib.Color` wasn't a known enum (it lives in another file). Now
    // that the loader has merged the prefixed `lib.Color` enum decl
    // into `merged.items`, a second normalize pass picks it up and
    // converts the field-access into an `EnumCtor`.
    // Module-prefix authorization was checked per file at parse
    // time; the merged Program has no `Item::Use`s, so use the
    // validation-skipping entry point here.
    let merged = crate::normalize::renormalize_merged(merged);
    // Inline `const` declarations: collect every Item::Const in the
    // merged Program, then walk all expressions replacing
    // `Var(const_name)` with the literal value. Item::Const entries
    // are removed afterwards. Downstream stages (type checker /
    // interpreter / JIT) never see consts.
    inline_constants(merged)
}

fn canonicalize(p: &Path) -> Result<PathBuf, LoadError> {
    p.canonicalize().map_err(|e| LoadError::ReadError {
        path: p.to_path_buf(),
        message: e.to_string(),
    })
}

/// Resolve a `use module` to either an on-disk canonicalized path
/// or a virtual `<builtin>/module.il` path for shipped stdlib
/// modules. The importer's own directory is searched first; if the
/// file isn't there, each entry in `extra_paths` (from the
/// project's `ilang.toml [deps]` section) is tried in order.
fn resolve_module(
    module: &str,
    dir: &Path,
    extra_paths: &[PathBuf],
) -> Result<PathBuf, LoadError> {
    if builtin_module_source(module).is_some() {
        return Ok(builtin_path(module));
    }
    let primary = dir.join(format!("{module}.il"));
    if primary.exists() {
        return canonicalize(&primary);
    }
    for extra in extra_paths {
        let candidate = extra.join(format!("{module}.il"));
        if candidate.exists() {
            return canonicalize(&candidate);
        }
    }
    // Fall back to the primary path so the resulting "not found"
    // error mentions the importer-local location (most actionable).
    canonicalize(&primary)
}

fn load_recursive(
    file: &Path,
    base_dir: &Path,
    extra_paths: &[PathBuf],
    visiting: &mut HashSet<PathBuf>,
    chain: &mut Vec<Symbol>,
    loaded: &mut HashMap<PathBuf, Program>,
    overlay: &HashMap<PathBuf, String>,
) -> Result<(), LoadError> {
    if loaded.contains_key(file) {
        return Ok(());
    }
    if !visiting.insert(file.to_path_buf()) {
        chain.push(file.display().to_string().into());
        return Err(LoadError::CircularImport { chain: chain.clone() });
    }
    chain.push(file.display().to_string().into());
    let prog = parse_file(file, overlay)?;
    let dir = file.parent().unwrap_or(base_dir).to_path_buf();
    for item in &prog.items {
        if let Item::Use(u) = item {
            let canon = resolve_module(u.module.as_str(), &dir, extra_paths)?;
            load_recursive(&canon, &dir, extra_paths, visiting, chain, loaded, overlay)?;
        }
    }
    loaded.insert(file.to_path_buf(), prog);
    visiting.remove(file);
    chain.pop();
    Ok(())
}

fn parse_file(file: &Path, overlay: &HashMap<PathBuf, String>) -> Result<Program, LoadError> {
    let src = if let Some(s) = overlay.get(file) {
        s.clone()
    } else if let Some(name) = is_builtin_path(file) {
        builtin_module_source(name)
            .expect("builtin path checked")
            .to_string()
    } else {
        std::fs::read_to_string(file).map_err(|e| LoadError::ReadError {
            path: file.to_path_buf(),
            message: e.to_string(),
        })?
    };
    let toks = ilang_lexer::tokenize(&src)
        .map_err(|e| LoadError::LexError(e.to_string()))?;
    let mut prog = crate::parse(&toks).map_err(LoadError::ParseError)?;
    expand_embeds(&mut prog, file)?;
    Ok(prog)
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
fn expand_embeds(prog: &mut Program, source_file: &Path) -> Result<(), LoadError> {
    let source_dir = source_file
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    for item in prog.items.iter_mut() {
        let Item::Const(c) = item else { continue };
        let Some(rel) = c.embed_path.clone() else { continue };
        let path = source_dir.join(rel.as_str());
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

fn apply_use(
    u: UseDecl,
    // When `Some(p)`, items from `u`'s module merge under prefix `p`
    // instead of `u.module`. Used by `pub use M` so M's items
    // appear under the re-exporting module's namespace. `None` at
    // the entry-point and on regular nested uses.
    prefix_override: Option<&str>,
    importer_canon: &Path,
    extra_paths: &[PathBuf],
    loaded: &mut HashMap<PathBuf, Program>,
    merged: &mut Program,
    _whole_imports: &mut HashSet<Symbol>,
    applied: &mut HashSet<(PathBuf, String)>,
    // Per-name rewrite rules accumulated by selective imports that
    // resolve through `pub use` chains. Bare-name `X` refs in
    // the entry's items / stmts / tail get rewritten to the prefixed
    // form `umbrella.X` after all imports are merged, so the bare
    // and prefixed views of the same enum / class / fn line up at
    // the type checker.
    rename_rules: &mut HashMap<Symbol, Symbol>,
) -> Result<(), LoadError> {
    let importer_dir = importer_canon
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let canon = resolve_module(u.module.as_str(), &importer_dir, extra_paths)?;
    // Clone instead of remove — the same module may legitimately be
    // applied multiple times (e.g. once via pub use to publish under
    // an umbrella prefix, and once directly so a sibling module that
    // `use`s it sees the items under the original prefix). Each
    // application targets a distinct effective prefix, so the
    // resulting items don't shadow each other.
    let mut module_prog = loaded
        .get(&canon)
        .cloned()
        .expect("loaded before via load_recursive");
    let nominal_prefix: String = prefix_override
        .map(str::to_string)
        .unwrap_or_else(|| u.module.as_str().to_string());
    // If this module's canon is already prefix-merged under some
    // other prefix (e.g. an umbrella's `pub use` ran first and
    // exposed the items under `sdl.X`), reuse that prefix for our
    // rename rules instead of producing a parallel `M.X` copy. The
    // umbrella's view and the explicit `use M { X }` view should
    // refer to the same merged item, otherwise the type checker
    // sees two distinct types with identical content.
    let existing_prefix: Option<String> = applied
        .iter()
        .find_map(|(p, pref)| (p == &canon && !pref.starts_with("@sel:")).then(|| pref.clone()));
    let effective_prefix: String =
        existing_prefix.clone().unwrap_or_else(|| nominal_prefix.clone());
    // Selective and whole imports both produce the same prefix-merged
    // view of the module — bare references in selective imports get
    // rewritten to the prefixed form by the rename pass at the end of
    // `load_program`, so the only thing that varies is whether we
    // also expose any names bare. Dedup the prefix-merge step on
    // (canon, prefix) so `use M` followed by `use M { X }` (or vice
    // versa) doesn't double-register every item; the per-selective
    // record below is gated by its own dedup key.
    let merge_key = (canon.clone(), effective_prefix.clone());
    let needs_merge = applied.insert(merge_key);
    // The selective branch (line ~517) writes rename rules into the
    // *caller's* `rename_rules` map, which is per-importer. Each
    // importer that does `use M { X }` needs that mapping recorded
    // into its own map, so this branch must run regardless of whether
    // some other importer already did the same selective import. If
    // there's nothing selective and no merge to do, only then can we
    // skip.
    if !needs_merge && u.selective.is_none() {
        return Ok(());
    }

    // Recursively expand the module's own use items first, into the
    // module_prog's namespace. `pub use N` propagates the
    // current module's effective prefix to N so its items also land
    // under the re-exporting namespace.
    let mut nested_uses = Vec::new();
    let mut local_items = Vec::new();
    for item in module_prog.items {
        match item {
            Item::Use(nu) => nested_uses.push(nu),
            other => local_items.push(other),
        }
    }
    module_prog.items = local_items.into();
    // Keep a copy for the selective branch's `pub use` chain
    // existence check — selective imports may resolve names declared
    // in chained modules rather than this module's own items.
    let nested_uses_for_search: Vec<UseDecl> = nested_uses.clone();

    if needs_merge {
        // Rename rules collected from THIS module's own selective
        // imports — applied to this module's items before
        // `prefix_item` so a `use N { Y }` inside M rewrites the
        // bare `Y` references in M's body to `N.Y`.
        let mut module_rename_rules: HashMap<Symbol, Symbol> = HashMap::new();
        for nu in nested_uses {
            // `pub use M as _ { * }` (wildcard): flatten M's items
            // into the umbrella's namespace — override = umbrella prefix.
            // `pub use M` (no wildcard): namespace under the umbrella —
            // override = `<umbrella>.<M>` so items land at
            // `<umbrella>.M.X` and callers reach them via that path.
            let nested_override_owned: Option<String> = if nu.re_export {
                if nu.wildcard {
                    Some(effective_prefix.clone())
                } else {
                    Some(format!("{}.{}", effective_prefix, nu.module.as_str()))
                }
            } else {
                None
            };
            let nested_override: Option<&str> = nested_override_owned.as_deref();
            apply_use(
                nu,
                nested_override,
                &canon,
                extra_paths,
                loaded,
                merged,
                _whole_imports,
                applied,
                &mut module_rename_rules,
            )?;
        }
        // Prefix-merge the module's own local items. Even for
        // selective imports we want the module's items present in
        // the merged Program (under their prefixed names) so a
        // selectively-imported class's internal references to other
        // module items resolve.
        let mut named_globals: HashSet<Symbol> = module_prog
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Const(c) => Some(c.name.clone()),
                Item::Class(c) => Some(c.name.clone()),
                // Top-level fns count too — `qualify_var_refs`
                // qualifies bare `Call(name, ...)` callees only
                // when the name is in this set, so the later
                // `prefix_*` walk doesn't accidentally qualify
                // local-closure callees (`let f = ...; f(v)` →
                // not `module.f(v)`).
                Item::Fn(f) => Some(f.name.clone()),
                _ => None,
            })
            .collect();
        for item in &module_prog.items {
            if let Item::ExternC(b) = item {
                for inner in &b.items {
                    match inner {
                        ilang_ast::ExternCItem::Class(c) => {
                            named_globals.insert(c.name.clone());
                        }
                        ilang_ast::ExternCItem::FnDef(f) => {
                            named_globals.insert(f.name.clone());
                        }
                        ilang_ast::ExternCItem::FnDecl { name, .. } => {
                            named_globals.insert(name.clone());
                        }
                        _ => {}
                    }
                }
            }
        }
        // Top-level `let X = ...` in this module — fn bodies (and
        // other top-level stmts) within the module reference X
        // bare; the qualify pass below rewrites those refs to
        // `prefix.X` so they line up with the prefixed `let`
        // binding that the stmt pass below emits.
        for s in &module_prog.stmts {
            if let StmtKind::Let { name, .. } = &s.kind {
                named_globals.insert(name.clone());
            }
        }
        // Fold the module's trailing expression into its stmt list
        // so it executes during import (e.g. a final `counter = 42`
        // tail expression). The entry's tail stays separate; only
        // sub-modules' tails get demoted.
        if let Some(tail) = module_prog.tail.take() {
            let span = tail.span;
            module_prog.stmts.push(Stmt {
                kind: StmtKind::Expr(tail),
                span, source_module: None });
        }
        for item in module_prog.items.iter_mut() {
            qualify_var_refs_in_item(item, &effective_prefix, &named_globals);
        }
        // Apply this module's own selective-import rename rules
        // BEFORE prefixing — `prefix_item` adds the module prefix to
        // every bare `Object`/`Var`/`Call`, which would turn
        // `NeonRenderer` (after `use neon { NeonRenderer }`) into
        // `M.NeonRenderer` instead of the intended `neon.NeonRenderer`.
        if !module_rename_rules.is_empty() {
            for item in module_prog.items.iter_mut() {
                rename_in_item(item, &module_rename_rules);
            }
        }
        for item in module_prog.items {
            merged.items.push(prefix_item(item, &effective_prefix));
        }
        // Forward this module's top-level stmts (Let bindings + side
        // effects) into the merged program so they execute when the
        // entry runs. `applied` guarantees a given (canon, prefix)
        // only goes through this branch once, so each module's
        // initialization runs exactly once even if multiple `use`
        // sites reach it.
        for stmt in module_prog.stmts {
            let mut s = stmt;
            qualify_var_refs_in_stmt(&mut s, &effective_prefix, &named_globals);
            if !module_rename_rules.is_empty() {
                rename_in_stmt(&mut s, &module_rename_rules);
            }
            let mut s = prefix_stmt(s, &effective_prefix);
            // Top-level `let X = ...` becomes `let prefix.X = ...`
            // so cross-module references (Var("prefix.X")) resolve
            // to the same global slot.
            if let StmtKind::Let { name, .. } = &mut s.kind {
                *name = Symbol::intern(&format!("{effective_prefix}.{name}")).into();
            }
            // Tag the merged stmt with its source module so the
            // type checker judges access from that module's
            // perspective. Without this, the module's own
            // top-level stmts (e.g. `let X = Class.c` referring
            // to a non-pub static of the SAME module) get
            // judged from the entry module and falsely fail the
            // cross-module visibility rule.
            s.source_module = Some(Symbol::intern(&effective_prefix));
            merged.stmts.push(s);
        }
    }

    // Selective imports record one rename rule per requested name so
    // the final pass rewrites bare references in the entry's content
    // to the prefixed form `effective_prefix.name`. We rely on the
    // prefix-merge above (or a sibling whole-import that ran first)
    // to make `effective_prefix.name` actually present in `merged`.
    if let Some(names) = u.selective {
        // Whether the requested names are visible in this module's
        // local items or any of its `pub use` chains. We need an
        // existence check to surface a load error for typos —
        // skipping the check would silently accept any bare name.
        let mut local_names: HashSet<&str> = HashSet::new();
        if let Some(p) = loaded.get(&canon) {
            for item in p.items.iter() {
                if let Some(n) = item_name_of_ref(item) {
                    local_names.insert(n);
                }
                // `@extern(C) { struct S {} fn f() {} ... }` items
                // count as exports too — selective import should be
                // able to pull `S` or `f` out of `a.il`'s extern
                // block.
                if let Item::ExternC(b) = item {
                    for inner in b.items.iter() {
                        match inner {
                            ilang_ast::ExternCItem::Struct { name, .. }
                            | ilang_ast::ExternCItem::Union { name, .. }
                            | ilang_ast::ExternCItem::FnDecl { name, .. } => {
                                local_names.insert(name.as_str());
                            }
                            ilang_ast::ExternCItem::FnDef(f) => {
                                local_names.insert(f.name.as_str());
                            }
                            ilang_ast::ExternCItem::Class(c) => {
                                local_names.insert(c.name.as_str());
                            }
                        }
                    }
                }
            }
        }
        let module_dir = canon
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        for name in &names {
            let exists = local_names.contains(name.as_str()) || {
                let mut visited: HashSet<PathBuf> = HashSet::new();
                visited.insert(canon.clone());
                let mut hit = false;
                for nu in &nested_uses_for_search {
                    if !nu.re_export {
                        continue;
                    }
                    if find_in_export_chain(
                        nu.module.as_str(),
                        name.as_str(),
                        &module_dir,
                        extra_paths,
                        loaded,
                        &mut visited,
                    )? {
                        hit = true;
                        break;
                    }
                }
                hit
            };
            if !exists {
                return Err(LoadError::UnknownImport {
                    module: u.module.clone(),
                    name: name.clone(),
                });
            }
            rename_rules.insert(
                name.clone(),
                Symbol::intern(&format!("{effective_prefix}.{name}")).into(),
            );
        }
    }
    Ok(())
}

fn item_name_of_ref(item: &Item) -> Option<&str> {
    match item {
        Item::Fn(f) => Some(f.name.as_str()),
        Item::Class(c) => Some(c.name.as_str()),
        Item::Enum(e) => Some(e.name.as_str()),
        Item::Const(c) => Some(c.name.as_str()),
        Item::ExternC(_) | Item::Use(_) => None,
        Item::Interface(i) => Some(i.name.as_str()),
    }
}

/// Rewrite bare `Var("X")` → `Var("prefix.X")` inside an item's
/// expression nodes, but only when `X` is in `consts`. Used as a
/// pre-pass before module prefixing so module-level const refs
/// from fn / method / `@extern(C)` bodies survive into
/// `inline_constants` with names that match the prefixed const
/// declaration.
fn qualify_var_refs_in_item(
    item: &mut Item,
    prefix: &str,
    consts: &HashSet<Symbol>,
) {
    match item {
        Item::Fn(f) => qualify_var_refs_in_block(&mut f.body, prefix, consts),
        Item::Class(c) => qualify_var_refs_in_class(c, prefix, consts),
        Item::ExternC(b) => {
            for inner in b.items.iter_mut() {
                match inner {
                    ilang_ast::ExternCItem::FnDef(f) => {
                        qualify_var_refs_in_block(&mut f.body, prefix, consts);
                    }
                    ilang_ast::ExternCItem::Class(c) => {
                        qualify_var_refs_in_class(c, prefix, consts);
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

fn qualify_var_refs_in_class(c: &mut ClassDecl, prefix: &str, consts: &HashSet<Symbol>) {
    for m in c.methods.iter_mut().chain(c.static_methods.iter_mut()) {
        qualify_var_refs_in_block(&mut m.body, prefix, consts);
    }
    for prop in c.properties.iter_mut() {
        if let Some(g) = prop.getter.as_mut() {
            qualify_var_refs_in_block(&mut g.body, prefix, consts);
        }
        if let Some(s) = prop.setter.as_mut() {
            qualify_var_refs_in_block(&mut s.body, prefix, consts);
        }
    }
    for sf in c.static_fields.iter_mut() {
        qualify_var_refs_in_expr(&mut sf.value, prefix, consts);
    }
}

fn qualify_var_refs_in_block(b: &mut Block, prefix: &str, consts: &HashSet<Symbol>) {
    for s in b.stmts.iter_mut() {
        qualify_var_refs_in_stmt(s, prefix, consts);
    }
    if let Some(t) = b.tail.as_mut() {
        qualify_var_refs_in_expr(t, prefix, consts);
    }
}

fn qualify_var_refs_in_stmt(s: &mut Stmt, prefix: &str, consts: &HashSet<Symbol>) {
    use ilang_ast::StmtKind;
    match &mut s.kind {
        StmtKind::Let { value, .. }
        | StmtKind::LetTuple { value, .. }
        | StmtKind::LetStruct { value, .. } => {
            qualify_var_refs_in_expr(value, prefix, consts)
        }
        StmtKind::Expr(e) => qualify_var_refs_in_expr(e, prefix, consts),
    }
}

fn qualify_var_refs_in_expr(e: &mut Expr, prefix: &str, consts: &HashSet<Symbol>) {
    match &mut e.kind {
        ExprKind::Var(name) => {
            if consts.contains(name) {
                *name = Symbol::intern(&format!("{prefix}.{name}")).into();
            }
        }
        ExprKind::Unary { expr, .. } => qualify_var_refs_in_expr(expr, prefix, consts),
        ExprKind::Binary { lhs, rhs, .. } => {
            qualify_var_refs_in_expr(lhs, prefix, consts);
            qualify_var_refs_in_expr(rhs, prefix, consts);
        }
        ExprKind::Logical { lhs, rhs, .. } => {
            qualify_var_refs_in_expr(lhs, prefix, consts);
            qualify_var_refs_in_expr(rhs, prefix, consts);
        }
        ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. } => {
            qualify_var_refs_in_expr(expr, prefix, consts)
        }
        ExprKind::Call { callee, args } => {
            // Qualify the callee here (not in the later
            // `prefix_*` walk) so locally-bound closures —
            // `let f = ...; f(v)` — don't get accidentally
            // rewritten to `module.f(v)`. `consts` lists every
            // top-level name (const / class / fn / `let`) the
            // module exposes; bare callee names not in there
            // are presumed local and left alone.
            if !is_builtin_callee(callee.as_str())
                && !callee.as_str().contains('.')
                && consts.contains(callee)
            {
                *callee = Symbol::intern(&format!("{prefix}.{callee}")).into();
            }
            for a in args.iter_mut() {
                qualify_var_refs_in_expr(a, prefix, consts);
            }
        }
        ExprKind::SuperCall { args, .. } => {
            for a in args.iter_mut() {
                qualify_var_refs_in_expr(a, prefix, consts);
            }
        }
        ExprKind::MethodCall { obj, args, .. } => {
            qualify_var_refs_in_expr(obj, prefix, consts);
            for a in args.iter_mut() {
                qualify_var_refs_in_expr(a, prefix, consts);
            }
        }
        ExprKind::Field { obj, .. } => qualify_var_refs_in_expr(obj, prefix, consts),
        ExprKind::AssignField { obj, value, .. } => {
            qualify_var_refs_in_expr(obj, prefix, consts);
            qualify_var_refs_in_expr(value, prefix, consts);
        }
        ExprKind::Index { obj, index } => {
            qualify_var_refs_in_expr(obj, prefix, consts);
            qualify_var_refs_in_expr(index, prefix, consts);
        }
        ExprKind::AssignIndex { obj, index, value } => {
            qualify_var_refs_in_expr(obj, prefix, consts);
            qualify_var_refs_in_expr(index, prefix, consts);
            qualify_var_refs_in_expr(value, prefix, consts);
        }
        ExprKind::Assign { target, value } => {
            // LHS: `state = ...` writing to a top-level let needs
            // the same qualification as a Var read.
            if consts.contains(target) {
                *target = Symbol::intern(&format!("{prefix}.{target}")).into();
            }
            qualify_var_refs_in_expr(value, prefix, consts);
        }
        ExprKind::New { args, .. } => {
            for a in args.iter_mut() {
                qualify_var_refs_in_expr(a, prefix, consts);
            }
        }
        ExprKind::EnumCtor { args, .. } => match args {
            ilang_ast::CtorArgs::Unit => {}
            ilang_ast::CtorArgs::Tuple(es) => {
                for a in es.iter_mut() {
                    qualify_var_refs_in_expr(a, prefix, consts);
                }
            }
            ilang_ast::CtorArgs::Struct(fs) => {
                for (_, e) in fs.iter_mut() {
                    qualify_var_refs_in_expr(e, prefix, consts);
                }
            }
        },
        ExprKind::If { cond, then_branch, else_branch } => {
            qualify_var_refs_in_expr(cond, prefix, consts);
            qualify_var_refs_in_block(then_branch, prefix, consts);
            if let Some(e) = else_branch.as_mut() {
                qualify_var_refs_in_expr(e, prefix, consts);
            }
        }
        ExprKind::While { cond, body } => {
            qualify_var_refs_in_expr(cond, prefix, consts);
            qualify_var_refs_in_block(body, prefix, consts);
        }
        ExprKind::Loop { body } => qualify_var_refs_in_block(body, prefix, consts),
        ExprKind::ForIn { iter, body, .. } => {
            qualify_var_refs_in_expr(iter, prefix, consts);
            qualify_var_refs_in_block(body, prefix, consts);
        }
        ExprKind::Block(b) => qualify_var_refs_in_block(b, prefix, consts),
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                qualify_var_refs_in_expr(s, prefix, consts);
            }
            if let Some(e) = end {
                qualify_var_refs_in_expr(e, prefix, consts);
            }
        }
        ExprKind::Array(es) => {
            for e in es.iter_mut() {
                qualify_var_refs_in_expr(e, prefix, consts);
            }
        }
        ExprKind::Tuple(es) => {
            for e in es.iter_mut() {
                qualify_var_refs_in_expr(e, prefix, consts);
            }
        }
        ExprKind::MapLit(pairs) => {
            for (k, v) in pairs.iter_mut() {
                qualify_var_refs_in_expr(k, prefix, consts);
                qualify_var_refs_in_expr(v, prefix, consts);
            }
        }
        ExprKind::FnExpr { body, .. } => qualify_var_refs_in_block(body, prefix, consts),
        ExprKind::Match { scrutinee, arms } => {
            qualify_var_refs_in_expr(scrutinee, prefix, consts);
            for arm in arms.iter_mut() {
                qualify_var_refs_in_expr(&mut arm.body, prefix, consts);
            }
        }
        ExprKind::Some(e) => qualify_var_refs_in_expr(e, prefix, consts),
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            qualify_var_refs_in_expr(expr, prefix, consts);
            qualify_var_refs_in_block(then_branch, prefix, consts);
            if let Some(e) = else_branch.as_mut() {
                qualify_var_refs_in_expr(e, prefix, consts);
            }
        }
        ExprKind::Return(Some(e)) => qualify_var_refs_in_expr(e, prefix, consts),
        ExprKind::Break(Some(e)) => qualify_var_refs_in_expr(e, prefix, consts),
        ExprKind::StructLit { fields, .. } => {
            for (_, e) in fields.iter_mut() {
                qualify_var_refs_in_expr(e, prefix, consts);
            }
        }
        // Leaf nodes — nothing to walk into.
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::This
        | ExprKind::None
        | ExprKind::Continue
        | ExprKind::Closure { .. }
        | ExprKind::Break(None)
        | ExprKind::Return(None) => {}
    }
}

/// Walk `pub use` chains starting at `module` (resolved relative
/// to `importer_dir`) and return the first item whose bare name
/// matches `name`. Used by selective import (`use M { X }`) so X can
/// be a name declared in a module that M re-exports via `pub use`
/// instead of being declared in M directly.
///
/// `visited` is shared across the walk to avoid revisiting modules in
/// diamond `pub use` graphs. The returned `Item` is cloned and
/// keeps its bare name (the caller pushes it under that name).
fn find_in_export_chain(
    module: &str,
    name: &str,
    importer_dir: &Path,
    extra_paths: &[PathBuf],
    loaded: &HashMap<PathBuf, Program>,
    visited: &mut HashSet<PathBuf>,
) -> Result<bool, LoadError> {
    let canon = resolve_module(module, importer_dir, extra_paths)?;
    if !visited.insert(canon.clone()) {
        return Ok(false);
    }
    let prog = loaded
        .get(&canon)
        .expect("module pre-loaded by load_recursive");
    // Local items first — including struct / fn / class / static
    // / fn-decl entries declared inside this module's own
    // `@extern(C) { ... }` block.
    for item in &prog.items {
        if let Some(item_name) = item_name_of(item) {
            if item_name.as_str() == name {
                return Ok(true);
            }
        }
        if let Item::ExternC(b) = item {
            for inner in &b.items {
                let n = match inner {
                    ilang_ast::ExternCItem::Struct { name, .. }
                    | ilang_ast::ExternCItem::Union { name, .. }
                    | ilang_ast::ExternCItem::FnDecl { name, .. } => name.as_str(),
                    ilang_ast::ExternCItem::FnDef(f) => f.name.as_str(),
                    ilang_ast::ExternCItem::Class(c) => c.name.as_str(),
                };
                if n == name {
                    return Ok(true);
                }
            }
        }
    }
    // Then follow `pub use` re-exports.
    let module_dir = canon
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    for item in &prog.items {
        if let Item::Use(nu) = item {
            if !nu.re_export {
                continue;
            }
            if find_in_export_chain(
                nu.module.as_str(),
                name,
                &module_dir,
                extra_paths,
                loaded,
                visited,
            )? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn item_name_of(item: &Item) -> Option<Symbol> {
    match item {
        Item::Fn(f) => Some(f.name.clone()),
        Item::Class(c) => Some(c.name.clone()),
        Item::Enum(e) => Some(e.name.clone()),
        Item::Const(c) => Some(c.name.clone()),
        Item::ExternC(_) => None,
        Item::Use(_) => None,
        Item::Interface(i) => Some(i.name.clone()),
    }
}

fn prefix_class_decl(c: &mut ilang_ast::ClassDecl, prefix: &str) {
    c.name = format!("{prefix}.{}", c.name).into();
    if let Some(parent) = c.parent.as_mut() {
        *parent = prefix_type_name(parent, prefix);
    }
    for ifn in c.interfaces.iter_mut() {
        *ifn = prefix_type_name(ifn, prefix);
    }
    for m in c.methods.iter_mut().chain(c.static_methods.iter_mut()) {
        let body = std::mem::replace(
            &mut m.body,
            Block { stmts: Vec::new(), tail: None },
        );
        m.body = prefix_block_calls(body, prefix);
        m.params = m
            .params
            .iter()
            .map(|p| ilang_ast::Param {
                name: p.name.clone(),
                ty: prefix_type(&p.ty, prefix),
                span: p.span,
                default: p.default.clone().map(|d| prefix_expr(d, prefix)),
            })
            .collect();
        m.ret = m.ret.as_ref().map(|t| prefix_type(t, prefix));
    }
    for f in &mut c.fields {
        f.ty = prefix_type(&f.ty, prefix);
    }
    for sf in &mut c.static_fields {
        sf.ty = prefix_type(&sf.ty, prefix);
        let value = std::mem::replace(
            &mut sf.value,
            Expr::new(ExprKind::None, sf.span),
        );
        sf.value = prefix_expr(value, prefix);
    }
    for prop in &mut c.properties {
        prop.ty = prefix_type(&prop.ty, prefix);
        if let Some(g) = prop.getter.as_mut() {
            let body = std::mem::replace(
                &mut g.body,
                Block { stmts: Vec::new(), tail: None },
            );
            g.body = prefix_block_calls(body, prefix);
            g.ret = g.ret.as_ref().map(|t| prefix_type(t, prefix));
        }
        if let Some(s) = prop.setter.as_mut() {
            let body = std::mem::replace(
                &mut s.body,
                Block { stmts: Vec::new(), tail: None },
            );
            s.body = prefix_block_calls(body, prefix);
            s.params = s
                .params
                .iter()
                .map(|p| ilang_ast::Param {
                    name: p.name.clone(),
                    ty: prefix_type(&p.ty, prefix),
                    span: p.span,
                    default: p.default.clone().map(|d| prefix_expr(d, prefix)),
                })
                .collect();
        }
    }
    // The class's own type parameters look like bare `Object`
    // names at parse time (the type checker is what later
    // distinguishes them as `TypeVar`s). The prefix walk above
    // accidentally turned them into `prefix.T`; sweep the body
    // and roll those back. Doing it as a post-pass avoids
    // threading an exclusion set through every recursive
    // `prefix_*` helper.
    if !c.type_params.is_empty() {
        let type_params: HashSet<Symbol> = c.type_params.iter().cloned().collect();
        unprefix_type_params_in_class(c, prefix, &type_params);
    }
}

fn unprefix_type_params_in_class(
    c: &mut ilang_ast::ClassDecl,
    prefix: &str,
    type_params: &HashSet<Symbol>,
) {
    for f in c.fields.iter_mut() {
        unprefix_type_params(&mut f.ty, prefix, type_params);
    }
    for sf in c.static_fields.iter_mut() {
        unprefix_type_params(&mut sf.ty, prefix, type_params);
    }
    for prop in c.properties.iter_mut() {
        unprefix_type_params(&mut prop.ty, prefix, type_params);
    }
    for m in c.methods.iter_mut().chain(c.static_methods.iter_mut()) {
        for p in m.params.iter_mut() {
            unprefix_type_params(&mut p.ty, prefix, type_params);
        }
        if let Some(t) = m.ret.as_mut() {
            unprefix_type_params(t, prefix, type_params);
        }
        unprefix_type_params_in_block(&mut m.body, prefix, type_params);
    }
}

fn unprefix_type_params_in_block(
    b: &mut Block,
    prefix: &str,
    type_params: &HashSet<Symbol>,
) {
    for s in b.stmts.iter_mut() {
        unprefix_type_params_in_stmt(s, prefix, type_params);
    }
    if let Some(t) = b.tail.as_mut() {
        unprefix_type_params_in_expr(t, prefix, type_params);
    }
}

fn unprefix_type_params_in_stmt(
    s: &mut Stmt,
    prefix: &str,
    type_params: &HashSet<Symbol>,
) {
    match &mut s.kind {
        StmtKind::Let { ty, value, .. } => {
            if let Some(t) = ty.as_mut() {
                unprefix_type_params(t, prefix, type_params);
            }
            unprefix_type_params_in_expr(value, prefix, type_params);
        }
        StmtKind::LetTuple { value, .. } | StmtKind::LetStruct { value, .. } => {
            unprefix_type_params_in_expr(value, prefix, type_params);
        }
        StmtKind::Expr(e) => unprefix_type_params_in_expr(e, prefix, type_params),
    }
}

fn unprefix_type_params_in_expr(
    e: &mut Expr,
    prefix: &str,
    type_params: &HashSet<Symbol>,
) {
    match &mut e.kind {
        ExprKind::Cast { expr, ty }
        | ExprKind::TypeTest { expr, ty }
        | ExprKind::TypeDowncast { expr, ty } => {
            unprefix_type_params(ty, prefix, type_params);
            unprefix_type_params_in_expr(expr, prefix, type_params);
        }
        ExprKind::FnExpr { params, ret, body } => {
            for p in params.iter_mut() {
                unprefix_type_params(&mut p.ty, prefix, type_params);
            }
            if let Some(t) = ret.as_mut() {
                unprefix_type_params(t, prefix, type_params);
            }
            unprefix_type_params_in_block(body, prefix, type_params);
        }
        ExprKind::New { type_args, args, .. } => {
            for t in type_args.iter_mut() {
                unprefix_type_params(t, prefix, type_params);
            }
            for a in args.iter_mut() {
                unprefix_type_params_in_expr(a, prefix, type_params);
            }
        }
        ExprKind::Block(b) => unprefix_type_params_in_block(b, prefix, type_params),
        ExprKind::If { cond, then_branch, else_branch } => {
            unprefix_type_params_in_expr(cond, prefix, type_params);
            unprefix_type_params_in_block(then_branch, prefix, type_params);
            if let Some(e2) = else_branch.as_mut() {
                unprefix_type_params_in_expr(e2, prefix, type_params);
            }
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            unprefix_type_params_in_expr(expr, prefix, type_params);
            unprefix_type_params_in_block(then_branch, prefix, type_params);
            if let Some(e2) = else_branch.as_mut() {
                unprefix_type_params_in_expr(e2, prefix, type_params);
            }
        }
        ExprKind::While { cond, body } => {
            unprefix_type_params_in_expr(cond, prefix, type_params);
            unprefix_type_params_in_block(body, prefix, type_params);
        }
        ExprKind::Loop { body } => unprefix_type_params_in_block(body, prefix, type_params),
        ExprKind::ForIn { iter, body, .. } => {
            unprefix_type_params_in_expr(iter, prefix, type_params);
            unprefix_type_params_in_block(body, prefix, type_params);
        }
        ExprKind::Match { scrutinee, arms } => {
            unprefix_type_params_in_expr(scrutinee, prefix, type_params);
            for arm in arms.iter_mut() {
                unprefix_type_params_in_expr(&mut arm.body, prefix, type_params);
            }
        }
        ExprKind::Call { args, .. } => {
            for a in args.iter_mut() {
                unprefix_type_params_in_expr(a, prefix, type_params);
            }
        }
        ExprKind::MethodCall { obj, args, .. } => {
            unprefix_type_params_in_expr(obj, prefix, type_params);
            for a in args.iter_mut() {
                unprefix_type_params_in_expr(a, prefix, type_params);
            }
        }
        ExprKind::SuperCall { args, .. } => {
            for a in args.iter_mut() {
                unprefix_type_params_in_expr(a, prefix, type_params);
            }
        }
        ExprKind::Field { obj, .. } => unprefix_type_params_in_expr(obj, prefix, type_params),
        ExprKind::AssignField { obj, value, .. } => {
            unprefix_type_params_in_expr(obj, prefix, type_params);
            unprefix_type_params_in_expr(value, prefix, type_params);
        }
        ExprKind::Index { obj, index } => {
            unprefix_type_params_in_expr(obj, prefix, type_params);
            unprefix_type_params_in_expr(index, prefix, type_params);
        }
        ExprKind::AssignIndex { obj, index, value } => {
            unprefix_type_params_in_expr(obj, prefix, type_params);
            unprefix_type_params_in_expr(index, prefix, type_params);
            unprefix_type_params_in_expr(value, prefix, type_params);
        }
        ExprKind::Unary { expr, .. } => unprefix_type_params_in_expr(expr, prefix, type_params),
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            unprefix_type_params_in_expr(lhs, prefix, type_params);
            unprefix_type_params_in_expr(rhs, prefix, type_params);
        }
        ExprKind::Assign { value, .. } => {
            unprefix_type_params_in_expr(value, prefix, type_params);
        }
        ExprKind::Return(v) | ExprKind::Break(v) => {
            if let Some(e2) = v.as_mut() {
                unprefix_type_params_in_expr(e2, prefix, type_params);
            }
        }
        ExprKind::Some(inner) => unprefix_type_params_in_expr(inner, prefix, type_params),
        ExprKind::Array(items) | ExprKind::Tuple(items) => {
            for item in items.iter_mut() {
                unprefix_type_params_in_expr(item, prefix, type_params);
            }
        }
        ExprKind::MapLit(entries) => {
            for (k, v) in entries.iter_mut() {
                unprefix_type_params_in_expr(k, prefix, type_params);
                unprefix_type_params_in_expr(v, prefix, type_params);
            }
        }
        ExprKind::EnumCtor { args, .. } => match args {
            ilang_ast::CtorArgs::Tuple(es) => {
                for e in es.iter_mut() {
                    unprefix_type_params_in_expr(e, prefix, type_params);
                }
            }
            ilang_ast::CtorArgs::Struct(fs) => {
                for (_, e) in fs.iter_mut() {
                    unprefix_type_params_in_expr(e, prefix, type_params);
                }
            }
            ilang_ast::CtorArgs::Unit => {}
        },
        _ => {}
    }
}

fn unprefix_type_params(
    t: &mut Type,
    prefix: &str,
    type_params: &HashSet<Symbol>,
) {
    let candidate = format!("{prefix}.");
    let unprefix_name = |name: &Symbol| -> Option<Symbol> {
        let s = name.as_str();
        let rest = s.strip_prefix(&candidate)?;
        let rest_sym: Symbol = Symbol::intern(rest);
        if type_params.contains(&rest_sym) {
            Some(rest_sym)
        } else {
            None
        }
    };
    match t {
        Type::Object(name) => {
            if let Some(orig) = unprefix_name(name) {
                *name = orig;
            }
        }
        Type::Array { elem, .. } => unprefix_type_params(elem, prefix, type_params),
        Type::Optional(inner) | Type::Weak(inner) => {
            unprefix_type_params(inner, prefix, type_params);
        }
        Type::Generic(g) => {
            if let Some(orig) = unprefix_name(&g.base) {
                g.base = orig;
            }
            for a in g.args.iter_mut() {
                unprefix_type_params(a, prefix, type_params);
            }
        }
        Type::Fn(ft) => {
            for p in ft.params.iter_mut() {
                unprefix_type_params(p, prefix, type_params);
            }
            unprefix_type_params(&mut ft.ret, prefix, type_params);
        }
        Type::RawPtr { inner, .. } => unprefix_type_params(inner, prefix, type_params),
        _ => {}
    }
}

fn prefix_type_name(name: &Symbol, prefix: &str) -> Symbol {
    if name.as_str().contains('.') {
        name.clone()
    } else {
        Symbol::intern(&format!("{prefix}.{name}"))
    }
}

fn prefix_item(item: Item, prefix: &str) -> Item {
    match item {
        Item::Fn(mut f) => {
            f.name = format!("{prefix}.{}", f.name).into();
            f.params = f
                .params
                .iter()
                .map(|p| ilang_ast::Param {
                    name: p.name.clone(),
                    ty: prefix_type(&p.ty, prefix),
                    span: p.span,
                    default: p.default.clone().map(|d| prefix_expr(d, prefix)),
                })
                .collect();
            f.ret = f.ret.as_ref().map(|t| prefix_type(t, prefix));
            f.body = prefix_block_calls(f.body, prefix);
            Item::Fn(f)
        }
        Item::Class(mut c) => {
            prefix_class_decl(&mut c, prefix);
            Item::Class(c)
        }
        Item::Enum(mut e) => {
            e.name = format!("{prefix}.{}", e.name).into();
            for v in &mut e.variants {
                v.payload = match std::mem::replace(&mut v.payload, ilang_ast::VariantPayload::Unit) {
                    ilang_ast::VariantPayload::Unit => ilang_ast::VariantPayload::Unit,
                    ilang_ast::VariantPayload::Tuple(tys) => ilang_ast::VariantPayload::Tuple(
                        Vec::from(tys).into_iter().map(|t| prefix_type(&t, prefix)).collect(),
                    ),
                    ilang_ast::VariantPayload::Struct(fs) => {
                        ilang_ast::VariantPayload::Struct(
                            Vec::from(fs).into_iter()
                                .map(|mut fd| {
                                    fd.ty = prefix_type(&fd.ty, prefix);
                                    fd
                                })
                                .collect(),
                        )
                    }
                };
            }
            Item::Enum(e)
        }
        Item::Use(u) => Item::Use(u),
        Item::Const(mut c) => {
            c.name = format!("{prefix}.{}", c.name).into();
            c.ty = c.ty.as_ref().map(|t| prefix_type(t, prefix));
            // RHS is folded to a literal later by `inline_constants`,
            // but it can still contain `ModuleEnum.Variant` /
            // `ClassName.staticField` / `Call(fn)` references that
            // need the same prefix rewrite as fn bodies before the
            // fold runs.
            let value = std::mem::replace(
                &mut c.value,
                Expr::new(ExprKind::None, c.span),
            );
            c.value = prefix_expr(value, prefix);
            Item::Const(c)
        }
        Item::ExternC(mut b) => {
            // Prefix the ilang-side names of the block's items so
            // callers can write `module.fn` etc. For library-form
            // (@lib) FnDecls, preserve the original C symbol name in
            // `c_symbol` so dlsym still finds it after the ilang name
            // has been rewritten to the prefixed form. Host-form fns
            // (no @lib) keep using the prefixed name as the symbol —
            // host registration code uses the prefixed name to match.
            //
            // Field / param / ret / static types also get prefixed so
            // intra-block references (e.g. `*SDL_Window` returning
            // from a fn that declared the struct) keep resolving.
            for inner in &mut b.items {
                match inner {
                    ilang_ast::ExternCItem::Struct { name, fields, .. }
                    | ilang_ast::ExternCItem::Union { name, fields, .. } => {
                        *name = Symbol::intern(&format!("{prefix}.{name}")).into();
                        for f in fields {
                            f.ty = prefix_type(&f.ty, prefix);
                        }
                    }
                    ilang_ast::ExternCItem::FnDecl {
                        name, libs, c_symbol, params, ret, ..
                    } => {
                        if !libs.is_empty() && c_symbol.is_none() {
                            *c_symbol = Some(name.clone());
                        }
                        *name = Symbol::intern(&format!("{prefix}.{name}")).into();
                        for p in params.iter_mut() {
                            p.ty = prefix_type(&p.ty, prefix);
                        }
                        if let Some(rt) = ret.as_mut() {
                            *rt = prefix_type(rt, prefix);
                        }
                    }
                    ilang_ast::ExternCItem::FnDef(f) => {
                        f.name = format!("{prefix}.{}", f.name).into();
                        for p in f.params.iter_mut() {
                            p.ty = prefix_type(&p.ty, prefix);
                        }
                        if let Some(rt) = f.ret.as_mut() {
                            *rt = prefix_type(rt, prefix);
                        }
                        let body = std::mem::replace(
                            &mut f.body,
                            Block { stmts: Vec::new(), tail: None },
                        );
                        f.body = prefix_block_calls(body, prefix);
                    }
                    ilang_ast::ExternCItem::Class(c) => {
                        prefix_class_decl(c, prefix);
                    }
                }
            }
            Item::ExternC(b)
        }
        Item::Interface(i) => Item::Interface(i),
    }
}

/// Within a prefixed item, references to other top-level items from
/// the same module should also resolve to their prefixed names. We
/// don't have full symbol info here, so we use a heuristic: rewrite
/// bare `Call { callee: name }` and bare `Type::Object(name)` /
/// `Type::Generic { base, .. }` only when the name is *not* already
/// in the prefixed form. This is intentionally conservative — for
/// MVP we only rewrite Calls. Other forms (class refs from inside)
/// stay bare and can be cross-resolved by the type checker.
fn prefix_block_calls(b: Block, prefix: &str) -> Block {
    Block {
        stmts: Vec::from(b.stmts).into_iter().map(|s| prefix_stmt(s, prefix)).collect(),
        tail: b.tail.map(|e| Box::new(prefix_expr(*e, prefix))),
    }
}

fn prefix_stmt(s: Stmt, prefix: &str) -> Stmt {
    let kind = match s.kind {
        StmtKind::Let { is_pub, is_const, name, ty, value } => StmtKind::Let {
            is_pub,
            is_const,
            name,
            ty: ty.map(|t| prefix_type(&t, prefix)),
            value: prefix_expr(value, prefix),
        },
        StmtKind::LetTuple { elems, value } => StmtKind::LetTuple {
            elems,
            value: prefix_expr(value, prefix),
        },
        StmtKind::LetStruct { class, fields, value } => StmtKind::LetStruct {
            class,
            fields,
            value: prefix_expr(value, prefix),
        },
        StmtKind::Expr(e) => StmtKind::Expr(prefix_expr(e, prefix)),
    };
    Stmt { kind, span: s.span, source_module: s.source_module.clone() }
}

fn prefix_expr(e: Expr, prefix: &str) -> Expr {
    let span = e.span;
    let kind = match e.kind {
        // Function calls: callee qualification has already been
        // done by the earlier `qualify_var_refs` pass (it has the
        // module's top-level fn-name set, so locally-bound
        // closure callees like `let f = ...; f(v)` stay bare and
        // don't get accidentally rewritten to `module.f(v)`).
        // Just recurse into the arguments here.
        ExprKind::Call { callee, args } => ExprKind::Call {
            callee,
            args: Vec::from(args).into_iter().map(|a| prefix_expr(a, prefix)).collect(),
        },
        ExprKind::New { class, type_args, args, init_method } => ExprKind::New {
            // `new module.Class(...)` already qualified — leave as
            // is; only re-prefix bare names so a second pass
            // doesn't produce `module.module.Class`. Builtin
            // types (`Map`, `Result`, …) are also left bare so
            // `new Map<...>()` inside a stdlib module doesn't
            // get rewritten to `new module.Map<...>()`.
            class: if class.as_str().contains('.') || is_builtin_type(class.as_str()) {
                class
            } else {
                format!("{prefix}.{}", class).into()
            },
            type_args: Vec::from(type_args).into_iter().map(|t| prefix_type(&t, prefix)).collect(),
            args: Vec::from(args).into_iter().map(|a| prefix_expr(a, prefix)).collect(),
            init_method,
        },
        ExprKind::EnumCtor {
            enum_name,
            variant,
            args,
        } => ExprKind::EnumCtor {
            enum_name: if enum_name.as_str().contains('.')
                || is_builtin_type(enum_name.as_str())
            {
                enum_name
            } else {
                format!("{prefix}.{}", enum_name).into()
            },
            variant,
            args: match args {
                ilang_ast::CtorArgs::Unit => ilang_ast::CtorArgs::Unit,
                ilang_ast::CtorArgs::Tuple(es) => ilang_ast::CtorArgs::Tuple(
                    Vec::from(es).into_iter().map(|e| prefix_expr(e, prefix)).collect(),
                ),
                ilang_ast::CtorArgs::Struct(fs) => ilang_ast::CtorArgs::Struct(
                    fs.into_iter()
                        .map(|(n, e)| (n, prefix_expr(e, prefix)))
                        .collect(),
                ),
            },
        },
        ExprKind::Cast { expr, ty } => ExprKind::Cast {
            expr: Box::new(prefix_expr(*expr, prefix)),
            ty: prefix_type(&ty, prefix),
        },
        ExprKind::TypeTest { expr, ty } => ExprKind::TypeTest {
            expr: Box::new(prefix_expr(*expr, prefix)),
            ty: prefix_type(&ty, prefix),
        },
        ExprKind::TypeDowncast { expr, ty } => ExprKind::TypeDowncast {
            expr: Box::new(prefix_expr(*expr, prefix)),
            ty: prefix_type(&ty, prefix),
        },
        ExprKind::FnExpr { params, ret, body } => ExprKind::FnExpr {
            params: params
                .into_iter()
                .map(|p| ilang_ast::Param {
                    name: p.name,
                    ty: prefix_type(&p.ty, prefix),
                    span: p.span,
                    default: p.default.map(|d| prefix_expr(d, prefix)),
                })
                .collect(),
            ret: ret.map(|t| prefix_type(&t, prefix)),
            body: prefix_block_calls(body, prefix),
        },
        // Recurse mechanically through everything else.
        ExprKind::Unary { op, expr } => ExprKind::Unary {
            op,
            expr: Box::new(prefix_expr(*expr, prefix)),
        },
        ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
            op,
            lhs: Box::new(prefix_expr(*lhs, prefix)),
            rhs: Box::new(prefix_expr(*rhs, prefix)),
        },
        ExprKind::Logical { op, lhs, rhs } => ExprKind::Logical {
            op,
            lhs: Box::new(prefix_expr(*lhs, prefix)),
            rhs: Box::new(prefix_expr(*rhs, prefix)),
        },
        ExprKind::Field { obj, name } => ExprKind::Field {
            obj: Box::new(prefix_expr(*obj, prefix)),
            name,
        },
        ExprKind::MethodCall { obj, method, args } => ExprKind::MethodCall {
            obj: Box::new(prefix_expr(*obj, prefix)),
            method,
            args: Vec::from(args).into_iter().map(|a| prefix_expr(a, prefix)).collect(),
        },
        ExprKind::Block(b) => ExprKind::Block(prefix_block_calls(b, prefix)),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => ExprKind::If {
            cond: Box::new(prefix_expr(*cond, prefix)),
            then_branch: prefix_block_calls(then_branch, prefix),
            else_branch: else_branch.map(|e| Box::new(prefix_expr(*e, prefix))),
        },
        ExprKind::IfLet {
            name,
            expr,
            then_branch,
            else_branch,
        } => ExprKind::IfLet {
            name,
            expr: Box::new(prefix_expr(*expr, prefix)),
            then_branch: prefix_block_calls(then_branch, prefix),
            else_branch: else_branch.map(|e| Box::new(prefix_expr(*e, prefix))),
        },
        ExprKind::While { cond, body } => ExprKind::While {
            cond: Box::new(prefix_expr(*cond, prefix)),
            body: prefix_block_calls(body, prefix),
        },
        ExprKind::Loop { body } => ExprKind::Loop {
            body: prefix_block_calls(body, prefix),
        },
        ExprKind::ForIn { var, iter, body } => ExprKind::ForIn {
            var,
            iter: Box::new(prefix_expr(*iter, prefix)),
            body: prefix_block_calls(body, prefix),
        },
        ExprKind::Range { start, end, inclusive } => ExprKind::Range {
            start: start.map(|s| Box::new(prefix_expr(*s, prefix))),
            end: end.map(|e| Box::new(prefix_expr(*e, prefix))),
            inclusive,
        },
        ExprKind::Closure { fn_name, captures } => {
            ExprKind::Closure { fn_name, captures }
        }
        ExprKind::SuperCall { method, args } => ExprKind::SuperCall {
            method,
            args: Vec::from(args).into_iter().map(|a| prefix_expr(a, prefix)).collect(),
        },
        ExprKind::Return(opt) => ExprKind::Return(opt.map(|e| Box::new(prefix_expr(*e, prefix)))),
        ExprKind::Break(opt) => ExprKind::Break(opt.map(|e| Box::new(prefix_expr(*e, prefix)))),
        ExprKind::Assign { target, value } => ExprKind::Assign {
            target,
            value: Box::new(prefix_expr(*value, prefix)),
        },
        ExprKind::AssignField { obj, field, value, is_init } => ExprKind::AssignField {
            obj: Box::new(prefix_expr(*obj, prefix)),
            field,
            value: Box::new(prefix_expr(*value, prefix)), is_init },
        ExprKind::AssignIndex { obj, index, value } => ExprKind::AssignIndex {
            obj: Box::new(prefix_expr(*obj, prefix)),
            index: Box::new(prefix_expr(*index, prefix)),
            value: Box::new(prefix_expr(*value, prefix)),
        },
        ExprKind::Array(items) => {
            ExprKind::Array(Vec::from(items).into_iter().map(|e| prefix_expr(e, prefix)).collect())
        }
        ExprKind::Tuple(items) => {
            ExprKind::Tuple(Vec::from(items).into_iter().map(|e| prefix_expr(e, prefix)).collect())
        }
        ExprKind::MapLit(entries) => ExprKind::MapLit(
            Vec::from(entries)
                .into_iter()
                .map(|(k, v)| (prefix_expr(k, prefix), prefix_expr(v, prefix)))
                .collect(),
        ),
        ExprKind::Index { obj, index } => ExprKind::Index {
            obj: Box::new(prefix_expr(*obj, prefix)),
            index: Box::new(prefix_expr(*index, prefix)),
        },
        ExprKind::Some(inner) => ExprKind::Some(Box::new(prefix_expr(*inner, prefix))),
        ExprKind::Match { scrutinee, arms } => ExprKind::Match {
            scrutinee: Box::new(prefix_expr(*scrutinee, prefix)),
            arms: arms
                .into_iter()
                .map(|arm: MatchArm| MatchArm {
                    pattern: arm.pattern,
                    body: prefix_expr(arm.body, prefix),
                    span: arm.span,
                })
                .collect(),
        },
        // Trivial nodes pass through.
        other @ (ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::Var(_)
        | ExprKind::This
        | ExprKind::None
        | ExprKind::Continue) => other,
        // Struct literals are desugared by `normalize` before the
        // loader walks anything; reaching this arm means a module
        // skipped that pass.
        ExprKind::StructLit { class, fields } => ExprKind::StructLit {
            class,
            fields: fields
                .into_iter()
                .map(|(n, e)| (n, prefix_expr(e, prefix)))
                .collect(),
        },
    };
    Expr { kind, span }
}

fn prefix_type(t: &Type, prefix: &str) -> Type {
    match t {
        Type::Object(name) if !name.as_str().contains('.') && !is_builtin_type(&name.as_str()) => {
            Type::Object(Symbol::intern(&format!("{prefix}.{name}")).into())
        }
        Type::Array { elem, fixed } => Type::Array {
            elem: Box::new(prefix_type(elem, prefix)),
            fixed: *fixed,
        },
        Type::Optional(inner) => Type::Optional(Box::new(prefix_type(inner, prefix))),
        Type::Weak(inner) => Type::Weak(Box::new(prefix_type(inner, prefix))),
        Type::Generic(g) => Type::generic(
            if !g.base.as_str().contains('.') && !is_builtin_type(g.base.as_str()) {
                Symbol::intern(&format!("{prefix}.{}", g.base))
            } else {
                g.base
            },
            g.args.iter().map(|a| prefix_type(a, prefix)).collect(),
        ),
        Type::Fn(ft) => Type::func(
            ft.params.iter().map(|p| prefix_type(p, prefix)).collect(),
            prefix_type(&ft.ret, prefix),
        ),
        Type::RawPtr { is_const, inner } => Type::RawPtr {
            is_const: *is_const,
            inner: Box::new(prefix_type(inner, prefix)),
        },
        _ => t.clone(),
    }
}

/// Names that should never get module-prefixed at Call sites — the
/// FFI marshalling helpers shipped by the type checker (mirrors the
/// `FFI_HELPERS` list in `ilang-types`).
fn is_builtin_callee(name: &str) -> bool {
    matches!(
        name,
        "stringFromCstr"
            | "cstrFromString"
            | "freeCstr"
            | "bytesFromBuffer"
            | "readI8"
            | "readI16"
            | "readI32"
            | "readI64"
            | "readU8"
            | "readU16"
            | "readU32"
            | "readU64"
            | "readF32"
            | "readF64"
            | "writeI8"
            | "writeI16"
            | "writeI32"
            | "writeI64"
            | "writeU8"
            | "writeU16"
            | "writeU32"
            | "writeU64"
            | "writeF32"
            | "writeF64"
            | "fnAddr"
            | "arrayFromCArray"
            | "cstrArrayToStrings"
            | "errnoCheck"
            | "errnoCheckI64"
    )
}

fn is_builtin_type(name: &str) -> bool {
    // Built-in classes/enums that should never get prefixed even
    // when referenced inside a module body.
    matches!(name, "Console" | "Map" | "Promise" | "Result")
}
