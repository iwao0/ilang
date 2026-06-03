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
//!
//! This module is the orchestrator. The mechanical passes live in
//! siblings — `builtin` (the embedded stdlib registry + allow-lists),
//! `resolve` (path → canonical-PathBuf), `prescan` (token-level peek +
//! sibling @objc class harvest + `@embed` expansion), `apply_use`
//! (the per-`Item::Use` walk), `qualify` (bare top-level-name
//! qualification), `prefix` (module-prefix walk over items / stmts /
//! exprs / types), plus the older passes already split off (`consts`,
//! `dup_pub`, `rename`, `spans`, `target_filter`).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use ilang_ast::{ExternCItem, InterfaceDecl, Item, Program, Symbol};

use crate::ParseError;

mod apply_use;
mod builtin;
mod consts;
mod derive;
mod dup_pub;
mod prefix;
mod prescan;
mod qualify;
mod rename;
mod resolve;
mod spans;
mod target_filter;

use apply_use::apply_use;
use consts::inline_constants;
use prescan::{build_dir_objc_scan, collect_objc_class_names, expand_embeds,
    pre_scan_use_modules, DirObjcScan};
use rename::rename_in_program;
use resolve::{canonicalize, read_source, resolve_module};
use spans::tag_program_spans;

pub use builtin::{builtin_module_path, builtin_module_source};

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
    /// `async fn` body has a shape the current state-machine
    /// lowering can't handle (await in a sub-expression, await
    /// inside a loop / branch, etc.). The reason carries the
    /// specific limitation; users get an actionable message.
    AsyncLowerError {
        reason: String,
        span: ilang_ast::Span,
    },
    /// Two `pub` declarations share the same name in the merged
    /// program. Triggered when an umbrella binding re-exports two
    /// siblings that both declare the same `pub class` / `pub
    /// interface` / `pub enum` / `pub struct` / `pub union` /
    /// `pub const` etc., or when the same kind of decl is repeated
    /// in a single file. `pub fn` overloads are allowed when their
    /// parameter-type lists differ; identical-signature duplicates
    /// still error.
    DuplicatePubDeclaration {
        kind: &'static str,
        name: Symbol,
        first_span: ilang_ast::Span,
        second_span: ilang_ast::Span,
    },
}

impl LoadError {
    /// Source file path that best identifies where this error
    /// occurred — used by the CLI to prefix the diagnostic. Returns
    /// `None` for module-level errors that don't tie to a single
    /// file (read failures, circular imports), in which case the
    /// caller should fall back to the entry path.
    pub fn source_file(&self) -> Option<&str> {
        let s = match self {
            LoadError::BadConst { span, .. }
            | LoadError::PrivateItemRef { span, .. }
            | LoadError::AsyncLowerError { span, .. } => span,
            LoadError::DuplicatePubDeclaration { second_span, .. } => second_span,
            // A parse error inside a `use`d module carries the real
            // source file on its span — surface it so the diagnostic
            // points at the offending module, not the entry file.
            LoadError::ParseError(e) => match e {
                ParseError::Unexpected { span, .. }
                | ParseError::InvalidAssignTarget { span }
                | ParseError::UnauthorizedModuleRef { span, .. }
                | ParseError::Generic { span, .. } => span,
            },
            LoadError::ReadError { .. }
            | LoadError::LexError(_)
            | LoadError::CircularImport { .. }
            | LoadError::UnknownImport { .. } => return None,
        };
        let s = s.source_file.as_str();
        if s.is_empty() { None } else { Some(s) }
    }
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
            LoadError::AsyncLowerError { reason, span } => {
                write!(f, "[{span}]: {reason}")
            }
            LoadError::PrivateItemRef { module, name, span } => {
                write!(
                    f,
                    "{span}: `{module}.{name}` is not `pub` in module `{module}` — mark the declaration with `pub` to expose it"
                )
            }
            LoadError::DuplicatePubDeclaration {
                kind,
                name,
                first_span,
                second_span,
            } => {
                write!(
                    f,
                    "{second_span}: duplicate `pub` {kind} `{name}` — already declared at {first_span}"
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
    load_program_with_overlay_and_parents(entry, extra_paths, &HashMap::new(), overlay)
}

/// Full-featured entry: the caller supplies both the flat list of
/// dep directories and the `child_pkg_dir → parent_pkg_dir` map
/// (produced by `ilang-cli`'s `project::collect_dep_tree`). The
/// `parents` map is what backs `use super.X` resolution; without
/// it, super-prefixed imports fail with "no parent package in
/// the dep tree".
pub fn load_program_with_overlay_and_parents(
    entry: &Path,
    extra_paths: &[PathBuf],
    parents: &HashMap<PathBuf, PathBuf>,
    overlay: &HashMap<PathBuf, String>,
) -> Result<Program, LoadError> {
    load_program_full(entry, extra_paths, parents, &HashMap::new(), overlay)
}

/// Full-featured entry: also accepts the `dep_name → dep_directory`
/// map from `ilang.toml [deps]`. Bare `use <dep_name>` resolves to
/// `<dep_directory>/mod.il` first, before falling back to the
/// sibling / extra_paths file-name search. Lets `[deps]
/// winapi = "../bindings/windows"` carry through as `use winapi`
/// regardless of what the umbrella file under `bindings/windows/`
/// is named.
pub fn load_program_full(
    entry: &Path,
    extra_paths: &[PathBuf],
    parents: &HashMap<PathBuf, PathBuf>,
    dep_names_to_dirs: &HashMap<String, PathBuf>,
    overlay: &HashMap<PathBuf, String>,
) -> Result<Program, LoadError> {
    let mut visiting: HashSet<PathBuf> = HashSet::new();
    let mut chain: Vec<Symbol> = Vec::new();
    let mut loaded: HashMap<PathBuf, Program> = HashMap::new();
    let mut objc_registry: HashSet<Symbol> = HashSet::new();
    let mut objc_class_modules: HashMap<Symbol, Symbol> = HashMap::new();
    // Per-source-file sibling-class map. Populated during the
    // load_recursive's prescan; consumed by `apply_use` (via the
    // helper `qualify_sibling_class_refs_in_item`) after rename and
    // before prefix to qualify bare @objc class refs that target a
    // sibling category file the source didn't (and can't) `use`.
    let mut sibling_class_maps: HashMap<PathBuf, HashMap<Symbol, Symbol>> = HashMap::new();
    // Per-folder `@objc class` pre-scan cache. A folder module's
    // siblings are read + tokenized once here instead of once per
    // sibling that gets parsed (which was O(N²) in folder size).
    let mut objc_dir_cache: HashMap<PathBuf, DirObjcScan> = HashMap::new();
    let entry_dir = entry.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
    let entry_canon = canonicalize(entry)?;
    let extra_paths: Vec<PathBuf> = extra_paths.to_vec();

    load_recursive(
        &entry_canon, &entry_dir, &extra_paths, parents, dep_names_to_dirs,
        &mut visiting, &mut chain, &mut loaded, overlay, &mut objc_registry,
        &mut objc_class_modules, &mut sibling_class_maps, &mut objc_dir_cache,
    )?;

    // Cross-module visibility check before merging: every `M.X`
    // qualified reference and every selective `use M { X }` must
    // target a `pub` item in M. Walks every loaded file (entry
    // included) using the catalog of `pub` items per module.
    crate::visibility::validate_visibility(&loaded, &entry_canon, dep_names_to_dirs)?;

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
                parents,
                dep_names_to_dirs,
                &mut loaded,
                &mut merged,
                &mut whole_imports,
                &mut applied,
                &mut rename_rules,
                &sibling_class_maps,
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
    // Reject duplicate `pub` declarations across the merged
    // program. Sibling modules re-exported through a single
    // umbrella can otherwise land the same bare name twice
    // (the original case was `pub struct ID3DBlob {}` +
    // `@com pub interface ID3DBlob`); without this check the
    // type system silently keeps whichever the walker hit first.
    dup_pub::validate_unique_pub(&merged)?;
    // Auto-lift top-level `class C: SomeObjcInterface { … }` into
    // a synthesised `@extern(ObjC) { @objc class C : NSObject, … }`
    // block so users can write Cocoa delegates without dropping into
    // the FFI block themselves. Detection runs against the merged
    // Items, so cross-module `@objc interface` references work.
    let merged = auto_lift_objc_subclasses(merged);
    // Auto-synthesise `equals` / `hashCode` for classes that carry
    // `@derive(Eq, Hash)`. Runs after objc auto-lift so the lifted
    // classes' fields are visible to the derive walk.
    let merged = derive::expand_derives(merged)?;
    // Inline `const` declarations: collect every Item::Const in the
    // merged Program, then walk all expressions replacing
    // `Var(const_name)` with the literal value. Item::Const entries
    // are removed afterwards. Downstream stages (type checker /
    // interpreter / JIT) never see consts.
    let prog = inline_constants(merged)?;
    // Lower `async fn` bodies into Promise-returning state-machine
    // form. Trivial (zero-await) bodies become
    // `Promise.resolve(...)`-wrapping fns; bodies with awaits are
    // not yet supported and fail here with an actionable error.
    crate::normalize::async_desugar::lower_async(prog).map_err(|e| {
        LoadError::AsyncLowerError {
            reason: e.reason,
            span: e.span,
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn load_recursive(
    file: &Path,
    base_dir: &Path,
    extra_paths: &[PathBuf],
    parents: &HashMap<PathBuf, PathBuf>,
    dep_names_to_dirs: &HashMap<String, PathBuf>,
    visiting: &mut HashSet<PathBuf>,
    chain: &mut Vec<Symbol>,
    loaded: &mut HashMap<PathBuf, Program>,
    overlay: &HashMap<PathBuf, String>,
    objc_registry: &mut HashSet<Symbol>,
    objc_class_modules: &mut HashMap<Symbol, Symbol>,
    sibling_class_maps: &mut HashMap<PathBuf, HashMap<Symbol, Symbol>>,
    objc_dir_cache: &mut HashMap<PathBuf, DirObjcScan>,
) -> Result<(), LoadError> {
    if loaded.contains_key(file) {
        return Ok(());
    }
    if !visiting.insert(file.to_path_buf()) {
        chain.push(file.display().to_string().into());
        return Err(LoadError::CircularImport { chain: chain.clone() });
    }
    chain.push(file.display().to_string().into());
    // Tokenize once. We need two passes over the same token stream:
    // a cheap scan to discover `use` deps (so we can load them before
    // parsing this file, populating `objc_registry`), then the full
    // parse that consults the registry inside `@extern(ObjC)` blocks.
    let src = read_source(file, overlay)?;
    let toks = ilang_lexer::tokenize(&src)
        .map_err(|e| LoadError::LexError(e.to_string()))?;
    let dir = file.parent().unwrap_or(base_dir).to_path_buf();
    for (super_count, dep_name, subpath) in pre_scan_use_modules(&toks) {
        let canon = resolve_module(
            &dep_name, &subpath, &dir, extra_paths, super_count, parents, dep_names_to_dirs,
        )?;
        load_recursive(
            &canon, &dir, extra_paths, parents, dep_names_to_dirs, visiting, chain, loaded, overlay,
            objc_registry, objc_class_modules, sibling_class_maps, objc_dir_cache,
        )?;
    }
    // Folder-module sibling pre-scan: when `<dir>/mod.il` exists, peek
    // at every sibling `<dir>/*.il` (without parsing) and harvest
    // `@objc pub class <Name>` declarations into the registry. Lets the
    // current file's auto-lift recognise an @objc class type declared
    // in a sibling category file even though the two don't `use` each
    // other (which would create a circular import). Without this, e.g.
    // `physicsWorld(): SKPhysicsWorld` in spritekit/node.il would fall
    // through `is_objc_class_ty` and the auto-lift would skip the
    // raw-pointer / wrap-handle dance, producing a garbage handle.
    //
    // Also records the sibling's basename in `objc_class_modules` so
    // the auto-lift can emit `new <module>.<Class>(...)` when wrapping
    // a sibling-file class — the loader's prefix pass would otherwise
    // re-tag the bare `<Class>` with the *importer's* module name and
    // the type checker would fail to resolve it.
    // Per-file sibling-class map. Built fresh per file (excluding the
    // file currently being parsed) from the cached directory scan, so a
    // class defined in the file currently being parsed doesn't carry
    // over a stale mapping.
    let mut file_class_modules: HashMap<Symbol, Symbol> = HashMap::new();
    let mut implicit_modules: Vec<Symbol> = Vec::new();
    if dir.join("mod.il").exists() {
        // Umbrella module name = containing directory's basename.
        // Folder-bindings flatten everything through `mod.il`'s
        // `pub use sibling.*` (wildcard re-export), which merges every
        // sibling's items under the umbrella's prefix, NOT under each
        // sibling's file stem (so `spritekit/actions.il`'s `SKAction`
        // becomes `spritekit.SKAction`). Pointing the qualified ref at
        // the umbrella gives a name the merged Program will carry.
        let umbrella = dir.file_name().and_then(|n| n.to_str()).map(Symbol::intern);
        // Scan the directory's siblings once (reads + tokenizes each
        // file a single time), then reuse the cache for every other
        // sibling in the same folder.
        let scan = objc_dir_cache
            .entry(dir.clone())
            .or_insert_with(|| build_dir_objc_scan(&dir));
        let current_canon = file.canonicalize().ok();
        for e in &scan.entries {
            // Skip the file currently being parsed — its own @objc
            // classes are harvested post-parse by
            // `collect_objc_class_names`; matching the old per-file
            // prescan which excluded `current`.
            let is_current = match (&current_canon, &e.canon) {
                (Some(a), Some(b)) => a == b,
                _ => false,
            };
            if is_current {
                continue;
            }
            if let Some(um) = umbrella {
                for &cls in &e.classes {
                    objc_registry.insert(cls);
                    file_class_modules.entry(cls).or_insert(um);
                }
            }
            // Every sibling stem becomes an implicit `use <stem>` for
            // normalize's dotted-ref validation, so the auto-lift's
            // synthetic `new physics.SKPhysicsWorld(...)` passes the
            // "this file does not `use physics`" check without an
            // actual (circular) import.
            implicit_modules.push(e.stem);
        }
        // Also seed the cumulative cross-file map for any downstream
        // consumers that still consult it.
        for (k, v) in &file_class_modules {
            objc_class_modules.entry(*k).or_insert(*v);
        }
    }
    let file_symbol = Symbol::intern(&file.display().to_string());
    let mut prog = crate::parse_with_implicit_modules(
        &toks,
        objc_registry,
        &implicit_modules,
    )
    .map_err(|mut e| {
        // The parser doesn't know paths; stamp the real file so the
        // diagnostic points at this module, not the entry file.
        e.set_source_file(file_symbol);
        LoadError::ParseError(e)
    })?;
    // Drop items annotated with `@target(...)` whose OS doesn't match
    // the build host. Runs before embed / objc-class harvesting so
    // those passes never see classes / fns that don't survive the
    // filter (and so per-OS same-name decls don't collide downstream).
    target_filter::filter_program(&mut prog)?;
    expand_embeds(&mut prog, file)?;
    collect_objc_class_names(&prog, objc_registry);
    // Stamp every span in this file's Program with the canonical
    // path so cross-module errors report the right source. Parser-
    // generated spans (which borrow line / col of the closest
    // user token) inherit the file too; that's accurate enough —
    // they always live in the same file as the trigger token.
    tag_program_spans(&mut prog, file_symbol);
    if !file_class_modules.is_empty() {
        sibling_class_maps.insert(file.to_path_buf(), file_class_modules);
    }
    loaded.insert(file.to_path_buf(), prog);
    visiting.remove(file);
    chain.pop();
    Ok(())
}

/// Walk the merged program and lift any top-level `Item::Class`
/// that names at least one `@objc interface` in its base list into
/// a synthesised `@extern(ObjC) { @objc class … { … } }` block.
/// The conversion (selector wiring from the interface metadata,
/// auto-injection of `alloc` / `init`, implicit NSObject parent)
/// lives in `crate::item::extern_objc::lift_class_to_objc_block`.
///
/// Detection runs against the post-merge Items, so cross-module
/// references to an `@objc interface` declared in a dependency
/// module are found correctly.
fn auto_lift_objc_subclasses(prog: Program) -> Program {
    auto_lift_objc_subclasses_with(prog, &HashMap::new(), &HashSet::new())
}

/// Like `auto_lift_objc_subclasses` but accepts extra `@objc
/// interface` and `@objc class` names harvested from somewhere
/// else (e.g. the LSP wants to lift its single-file parse using
/// names from the post-merge program — without those, references
/// like `class AppDelegate : NSApplicationDelegate` look like
/// they inherit from an unknown name and the lift bails).
pub fn auto_lift_objc_subclasses_with(
    mut prog: Program,
    extra_ifaces: &HashMap<Symbol, InterfaceDecl>,
    extra_class_names: &HashSet<Symbol>,
) -> Program {
    // 1. Collect every `@objc interface` declaration and every
    //    `@objc class` name across the merged program, seeded
    //    with whatever the caller already knows about.
    let mut objc_ifaces: HashMap<Symbol, InterfaceDecl> = extra_ifaces.clone();
    let mut objc_class_names: HashSet<Symbol> = extra_class_names.clone();
    for item in &prog.items {
        if let Item::ExternC(blk) = item {
            for iface in blk.interfaces.iter() {
                if iface.is_objc {
                    objc_ifaces.insert(iface.name, iface.clone());
                }
            }
            for inner in blk.items.iter() {
                if let ExternCItem::Class(cd) = inner {
                    if cd.attrs.iter().any(|a| a.name.as_str() == "objc") {
                        objc_class_names.insert(cd.name);
                    }
                }
            }
        }
    }
    if objc_ifaces.is_empty() && objc_class_names.is_empty() {
        return prog;
    }

    // 2. Partition top-level Items. Classes whose base list mentions
    //    any @objc interface are extracted and lifted; everything
    //    else stays put.
    let old_items = std::mem::take(&mut prog.items);
    let mut new_items: Vec<Item> = Vec::with_capacity(old_items.len());
    for item in old_items {
        match item {
            Item::Class(cd) => {
                // Lift when the class's base list mentions an
                // `@objc interface` (protocol implementation), or
                // when it directly inherits from an `@objc class`
                // like NSObject / NSView / NSWindow — anything
                // descended from an Objective-C class needs the
                // ObjC runtime registration that the @extern(ObjC)
                // desugar provides, so doing it automatically
                // saves the user from writing a wrapper block plus
                // `alloc` / `init` / `register` shims.
                let bases: Vec<Symbol> = cd
                    .parent
                    .iter()
                    .copied()
                    .chain(cd.interfaces.iter().copied())
                    .collect();
                let touches_objc_iface =
                    bases.iter().any(|b| objc_ifaces.contains_key(b));
                let touches_objc_class =
                    bases.iter().any(|b| objc_class_names.contains(b));
                if touches_objc_iface || touches_objc_class {
                    let block = crate::item::extern_objc::lift_class_to_objc_block(
                        cd,
                        &objc_ifaces,
                        &objc_class_names,
                    );
                    new_items.push(Item::ExternC(block));
                } else {
                    new_items.push(Item::Class(cd));
                }
            }
            other => new_items.push(other),
        }
    }
    prog.items = new_items;
    prog
}
