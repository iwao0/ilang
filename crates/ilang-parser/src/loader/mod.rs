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

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use ilang_ast::{ExternCItem, InterfaceDecl, Item, Program, Symbol};

use crate::ParseError;

// Per-phase counters populated by `load_recursive` when `ILANG_TIMING`
// is set. Process-wide atomics keep the load function signatures
// unchanged. `take_loader_timing` reads + resets them so a caller can
// emit a one-line summary per compile.
#[derive(Debug, Default, Clone, Copy)]
pub struct LoaderTiming {
    pub files: u64,
    pub read_ns: u64,
    pub tokenize_ns: u64,
    pub prescan_use_ns: u64,
    pub dir_objc_scan_ns: u64,
    pub parse_ns: u64,
    pub target_filter_ns: u64,
    pub expand_embeds_ns: u64,
    pub collect_objc_ns: u64,
    pub tag_spans_ns: u64,
    pub load_recursive_ns: u64,
    pub visibility_ns: u64,
    pub apply_use_ns: u64,
    pub renormalize_ns: u64,
    pub dup_pub_ns: u64,
    pub auto_lift_ns: u64,
    pub derive_ns: u64,
    pub inline_const_ns: u64,
    pub async_lower_ns: u64,
    pub au_qualify_ns: u64,
    pub au_rename_ns: u64,
    pub au_prefix_ns: u64,
    pub au_stmts_ns: u64,
    pub au_clone_ns: u64,
    pub au_named_globals_ns: u64,
    pub au_prologue_ns: u64,
    pub au_toposort_ns: u64,
    pub au_nested_ns: u64,
    pub au_selective_ns: u64,
    pub au_wildcard_ns: u64,
    pub precomp_exports_ns: u64,
}

static T_FILES: AtomicU64 = AtomicU64::new(0);
static T_READ: AtomicU64 = AtomicU64::new(0);
static T_TOK: AtomicU64 = AtomicU64::new(0);
static T_PRESCAN_USE: AtomicU64 = AtomicU64::new(0);
static T_DIR_OBJC: AtomicU64 = AtomicU64::new(0);
static T_PARSE: AtomicU64 = AtomicU64::new(0);
static T_TARGET: AtomicU64 = AtomicU64::new(0);
static T_EMBED: AtomicU64 = AtomicU64::new(0);
static T_COLLECT_OBJC: AtomicU64 = AtomicU64::new(0);
static T_TAG: AtomicU64 = AtomicU64::new(0);
// Post-`load_recursive` phases inside `load_program_full`. Each one
// runs once per compile (not per file).
static T_VISIBILITY: AtomicU64 = AtomicU64::new(0);
static T_APPLY_USE: AtomicU64 = AtomicU64::new(0);
static T_RENORM: AtomicU64 = AtomicU64::new(0);
static T_DUP_PUB: AtomicU64 = AtomicU64::new(0);
static T_AUTO_LIFT: AtomicU64 = AtomicU64::new(0);
static T_DERIVE: AtomicU64 = AtomicU64::new(0);
static T_INLINE_CONST: AtomicU64 = AtomicU64::new(0);
static T_ASYNC: AtomicU64 = AtomicU64::new(0);
static T_LOAD_RECURSIVE: AtomicU64 = AtomicU64::new(0);
// Sub-buckets inside `apply_use` so the 186ms hotspot can be broken
// out further.
pub(crate) static T_AU_QUALIFY: AtomicU64 = AtomicU64::new(0);
pub(crate) static T_AU_RENAME: AtomicU64 = AtomicU64::new(0);
pub(crate) static T_AU_PREFIX: AtomicU64 = AtomicU64::new(0);
pub(crate) static T_AU_STMTS: AtomicU64 = AtomicU64::new(0);
pub(crate) static T_AU_CLONE: AtomicU64 = AtomicU64::new(0);
pub(crate) static T_AU_NAMED_GLOBALS: AtomicU64 = AtomicU64::new(0);
pub(crate) static T_AU_PROLOGUE: AtomicU64 = AtomicU64::new(0);
pub(crate) static T_AU_TOPOSORT: AtomicU64 = AtomicU64::new(0);
pub(crate) static T_AU_NESTED: AtomicU64 = AtomicU64::new(0);
pub(crate) static T_AU_SELECTIVE: AtomicU64 = AtomicU64::new(0);
pub(crate) static T_AU_WILDCARD: AtomicU64 = AtomicU64::new(0);
static T_PRECOMP_EXPORTS: AtomicU64 = AtomicU64::new(0);

pub fn take_loader_timing() -> LoaderTiming {
    LoaderTiming {
        files: T_FILES.swap(0, Ordering::Relaxed),
        read_ns: T_READ.swap(0, Ordering::Relaxed),
        tokenize_ns: T_TOK.swap(0, Ordering::Relaxed),
        prescan_use_ns: T_PRESCAN_USE.swap(0, Ordering::Relaxed),
        dir_objc_scan_ns: T_DIR_OBJC.swap(0, Ordering::Relaxed),
        parse_ns: T_PARSE.swap(0, Ordering::Relaxed),
        target_filter_ns: T_TARGET.swap(0, Ordering::Relaxed),
        expand_embeds_ns: T_EMBED.swap(0, Ordering::Relaxed),
        collect_objc_ns: T_COLLECT_OBJC.swap(0, Ordering::Relaxed),
        tag_spans_ns: T_TAG.swap(0, Ordering::Relaxed),
        load_recursive_ns: T_LOAD_RECURSIVE.swap(0, Ordering::Relaxed),
        visibility_ns: T_VISIBILITY.swap(0, Ordering::Relaxed),
        apply_use_ns: T_APPLY_USE.swap(0, Ordering::Relaxed),
        renormalize_ns: T_RENORM.swap(0, Ordering::Relaxed),
        dup_pub_ns: T_DUP_PUB.swap(0, Ordering::Relaxed),
        auto_lift_ns: T_AUTO_LIFT.swap(0, Ordering::Relaxed),
        derive_ns: T_DERIVE.swap(0, Ordering::Relaxed),
        inline_const_ns: T_INLINE_CONST.swap(0, Ordering::Relaxed),
        async_lower_ns: T_ASYNC.swap(0, Ordering::Relaxed),
        au_qualify_ns: T_AU_QUALIFY.swap(0, Ordering::Relaxed),
        au_rename_ns: T_AU_RENAME.swap(0, Ordering::Relaxed),
        au_prefix_ns: T_AU_PREFIX.swap(0, Ordering::Relaxed),
        au_stmts_ns: T_AU_STMTS.swap(0, Ordering::Relaxed),
        au_clone_ns: T_AU_CLONE.swap(0, Ordering::Relaxed),
        au_named_globals_ns: T_AU_NAMED_GLOBALS.swap(0, Ordering::Relaxed),
        au_prologue_ns: T_AU_PROLOGUE.swap(0, Ordering::Relaxed),
        au_toposort_ns: T_AU_TOPOSORT.swap(0, Ordering::Relaxed),
        au_nested_ns: T_AU_NESTED.swap(0, Ordering::Relaxed),
        au_selective_ns: T_AU_SELECTIVE.swap(0, Ordering::Relaxed),
        au_wildcard_ns: T_AU_WILDCARD.swap(0, Ordering::Relaxed),
        precomp_exports_ns: T_PRECOMP_EXPORTS.swap(0, Ordering::Relaxed),
    }
}

pub(crate) fn add_ns_pub(counter: &AtomicU64, t: Instant) {
    let ns = t.elapsed().as_nanos() as u64;
    counter.fetch_add(ns, Ordering::Relaxed);
}

fn add_ns(counter: &AtomicU64, t: Instant) {
    let ns = t.elapsed().as_nanos() as u64;
    counter.fetch_add(ns, Ordering::Relaxed);
}

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
    pre_scan_use_modules, scan_objc_classes_in_tokens, DirObjcScan};
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

/// REPL support: run the loader's post-merge normalize chain on an
/// in-memory program (the REPL's accumulated items + the new
/// chunk). Mirrors the tail of `load_program_full` — enum-ref
/// renormalize (`E.a` Field → EnumCtor), `@derive` expansion,
/// `const` inlining, async desugar — which the REPL previously
/// skipped entirely, so enums were unusable across chunks, `const`
/// items leaked through to MIR, and `async fn` hit the legacy
/// "multi-state synthesis" diagnostic. The program must contain no
/// `Item::Use` (the caller rejects those up front with a friendly
/// message; `use` needs the file loader's module resolution).
/// `auto_lift_objc_subclasses` and the dup-pub validation are
/// intentionally omitted: the former needs the cross-file @objc
/// registry, and chunk-over-chunk redefinition is REPL-normal.
pub fn normalize_repl_chunk(prog: Program) -> Result<Program, LoadError> {
    let merged = crate::normalize::renormalize_merged(prog);
    let merged = derive::expand_derives(merged)?;
    let prog = inline_constants(merged)?;
    crate::normalize::async_desugar::lower_async(prog).map_err(|e| {
        LoadError::AsyncLowerError {
            reason: e.reason,
            span: e.span,
        }
    })
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
    let mut loaded: HashMap<PathBuf, Program> = HashMap::new();
    let mut objc_registry: HashSet<Symbol> = HashSet::new();
    let mut objc_class_modules: HashMap<Symbol, Symbol> = HashMap::new();
    // Per-source-file sibling-class map. Populated during the
    // discover_phase prescan; consumed by `apply_use` (via the
    // helper `qualify_sibling_class_refs_in_item`) after rename and
    // before prefix to qualify bare @objc class refs that target a
    // sibling category file the source didn't (and can't) `use`.
    let mut sibling_class_maps: HashMap<PathBuf, HashMap<Symbol, Symbol>> = HashMap::new();
    // Per-folder `@objc class` pre-scan cache. A folder module's
    // siblings are read + tokenized once here instead of once per
    // sibling that gets parsed (which was O(N²) in folder size).
    let mut objc_dir_cache: HashMap<PathBuf, DirObjcScan> = HashMap::new();
    // Token cache from Phase 1 → Phase 2. Each file is tokenized once
    // in discover_phase, then handed to parse_phase verbatim.
    let mut tokens: HashMap<PathBuf, Vec<ilang_lexer::Token>> = HashMap::new();
    // BFS visited set — replaces the old `visiting` stack guard.
    // Cycles are not an error: a file already in `seen` simply
    // doesn't get re-enqueued.
    let mut seen: HashSet<PathBuf> = HashSet::new();
    // Per-file implicit_modules list (sibling stems for folder
    // bindings). Carried into parse_phase so each file is parsed
    // with the same implicit-import set the old load_recursive
    // computed on the spot.
    let mut implicit_modules_by_file: HashMap<PathBuf, Vec<Symbol>> = HashMap::new();
    let entry_dir = entry.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
    let entry_canon = canonicalize(entry)?;
    let extra_paths: Vec<PathBuf> = extra_paths.to_vec();

    // Phase 1 + Phase 2 run inside a single timing bucket so the
    // `T_LOAD_RECURSIVE` LoaderTiming field carries the same
    // wall-clock semantics consumers used to see (it's now the
    // sum of discover + parse). Sub-buckets (`T_READ`, `T_TOK`,
    // `T_PARSE`, …) are filled by the phase bodies below.
    let t_lr = Instant::now();
    discover_phase(
        &entry_canon, &entry_dir, &extra_paths, parents, dep_names_to_dirs, overlay,
        &mut tokens, &mut seen, &mut objc_registry, &mut objc_class_modules,
        &mut sibling_class_maps, &mut objc_dir_cache, &mut implicit_modules_by_file,
    )?;
    parse_phase(
        &tokens, &mut objc_registry, &implicit_modules_by_file, &mut loaded,
    )?;
    add_ns(&T_LOAD_RECURSIVE, t_lr);

    // Cross-module visibility check before merging: every `M.X`
    // qualified reference and every selective `use M { X }` must
    // target a `pub` item in M. Walks every loaded file (entry
    // included) using the catalog of `pub` items per module.
    let t = Instant::now();
    crate::visibility::validate_visibility(&loaded, &entry_canon, dep_names_to_dirs)?;
    add_ns(&T_VISIBILITY, t);

    // Precompute, for every loaded module, the union of names it
    // exports through `find_in_export_chain` semantics — its own
    // items (regardless of `pub`) plus the transitive closure
    // through `pub use` re-exports. Turns the per-name existence
    // check inside `apply_use`'s selective branch from a chain
    // walk (O(chain length) per name) into a single HashSet
    // lookup. Built once; immutable for the rest of the load.
    let t = Instant::now();
    let exported_names = apply_use::precompute_exported_names(
        &loaded, &extra_paths, parents, dep_names_to_dirs,
    );
    add_ns(&T_PRECOMP_EXPORTS, t);

    // Keep the entry in `loaded` (clone instead of remove) so any
    // circular `use <entry>` from a sibling resolves to the entry's
    // items here. `apply_use` is given `entry_canon` so when a dep's
    // `use <entry>` lands back on this file, it still prefix-merges
    // items but skips the stmt-merge — the outer loop below runs
    // `entry_stmts` exactly once, unprefixed, and we don't want
    // those side-effect statements to re-execute under a prefixed
    // alias.
    let entry_prog = loaded
        .get(&entry_canon)
        .cloned()
        .expect("entry just loaded");
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
    let t = Instant::now();
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
                &exported_names,
                &entry_canon,
            )?,
            other => merged.items.push(other),
        }
    }
    add_ns(&T_APPLY_USE, t);
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
        let t = Instant::now();
        rename_in_program(&mut merged, &rename_rules);
        // bucket renames under apply_use for now — they only run
        // when selective imports chain through `pub use` re-exports.
        add_ns(&T_APPLY_USE, t);
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
    let t = Instant::now();
    let merged = crate::normalize::renormalize_merged(merged);
    add_ns(&T_RENORM, t);
    // Reject duplicate `pub` declarations across the merged
    // program. Sibling modules re-exported through a single
    // umbrella can otherwise land the same bare name twice
    // (the original case was `pub struct ID3DBlob {}` +
    // `@com pub interface ID3DBlob`); without this check the
    // type system silently keeps whichever the walker hit first.
    let t = Instant::now();
    dup_pub::validate_unique_pub(&merged)?;
    add_ns(&T_DUP_PUB, t);
    // Auto-lift top-level `class C: SomeObjcInterface { … }` into
    // a synthesised `@extern(ObjC) { @objc class C : NSObject, … }`
    // block so users can write Cocoa delegates without dropping into
    // the FFI block themselves. Detection runs against the merged
    // Items, so cross-module `@objc interface` references work.
    let t = Instant::now();
    let merged = auto_lift_objc_subclasses(merged);
    add_ns(&T_AUTO_LIFT, t);
    // Auto-synthesise `equals` / `hashCode` for classes that carry
    // `@derive(Eq, Hash)`. Runs after objc auto-lift so the lifted
    // classes' fields are visible to the derive walk.
    let t = Instant::now();
    let merged = derive::expand_derives(merged)?;
    add_ns(&T_DERIVE, t);
    // Inline `const` declarations: collect every Item::Const in the
    // merged Program, then walk all expressions replacing
    // `Var(const_name)` with the literal value. Item::Const entries
    // are removed afterwards. Downstream stages (type checker /
    // interpreter / JIT) never see consts.
    let t = Instant::now();
    let prog = inline_constants(merged)?;
    add_ns(&T_INLINE_CONST, t);
    // Lower `async fn` bodies into Promise-returning state-machine
    // form. Trivial (zero-await) bodies become
    // `Promise.resolve(...)`-wrapping fns; bodies with awaits are
    // not yet supported and fail here with an actionable error.
    let t = Instant::now();
    let res = crate::normalize::async_desugar::lower_async(prog).map_err(|e| {
        LoadError::AsyncLowerError {
            reason: e.reason,
            span: e.span,
        }
    });
    add_ns(&T_ASYNC, t);
    res
}

/// Phase 1 of the loader: BFS through every reachable source file,
/// tokenize it, and harvest cross-module info that the parser needs
/// in Phase 2. Parses no file. Tolerates circular `use` graphs by
/// keying the worklist on a `seen: HashSet<PathBuf>` rather than the
/// old "visiting" stack guard — a file already enqueued is simply
/// skipped, so A.il and B.il can `use` each other without erroring.
///
/// The cross-module info this phase fills in mirrors what the old
/// `load_recursive` collected before parse:
///   - `objc_registry`: every `@objc [pub] class <Name>` declared in
///     any reachable file (token-level scan via
///     `scan_objc_classes_in_tokens`). Parser-synthesised @objc
///     classes (the auto-lift on `class C : SomeObjcInterface`) are
///     picked up in Phase 2's post-parse `collect_objc_class_names`.
///   - `objc_class_modules` / `sibling_class_maps`: folder-binding
///     pre-scan via `build_dir_objc_scan` (cached in `objc_dir_cache`).
///   - `implicit_modules_by_file`: per-file sibling stems for
///     `parse_with_implicit_modules` in Phase 2.
///   - `tokens`: token streams stashed per file so Phase 2 doesn't
///     re-read / re-tokenize anything.
#[allow(clippy::too_many_arguments)]
fn discover_phase(
    entry: &Path,
    entry_dir: &Path,
    extra_paths: &[PathBuf],
    parents: &HashMap<PathBuf, PathBuf>,
    dep_names_to_dirs: &HashMap<String, PathBuf>,
    overlay: &HashMap<PathBuf, String>,
    tokens: &mut HashMap<PathBuf, Vec<ilang_lexer::Token>>,
    seen: &mut HashSet<PathBuf>,
    objc_registry: &mut HashSet<Symbol>,
    objc_class_modules: &mut HashMap<Symbol, Symbol>,
    sibling_class_maps: &mut HashMap<PathBuf, HashMap<Symbol, Symbol>>,
    objc_dir_cache: &mut HashMap<PathBuf, DirObjcScan>,
    implicit_modules_by_file: &mut HashMap<PathBuf, Vec<Symbol>>,
) -> Result<(), LoadError> {
    let mut wq: VecDeque<(PathBuf, PathBuf)> = VecDeque::new();
    if seen.insert(entry.to_path_buf()) {
        wq.push_back((entry.to_path_buf(), entry_dir.to_path_buf()));
    }
    while let Some((file, base_dir)) = wq.pop_front() {
        T_FILES.fetch_add(1, Ordering::Relaxed);
        let t = Instant::now();
        let src = read_source(&file, overlay)?;
        add_ns(&T_READ, t);
        let t = Instant::now();
        let toks = ilang_lexer::tokenize(&src)
            .map_err(|e| LoadError::LexError(e.to_string()))?;
        add_ns(&T_TOK, t);
        // Harvest @objc classes from THIS file's tokens. The old
        // load_recursive did the equivalent post-parse via
        // `collect_objc_class_names`; doing it up-front here means
        // the registry is complete before any file is parsed, so
        // parse order in Phase 2 doesn't matter for `@extern(ObjC)`
        // type recognition.
        for sym in scan_objc_classes_in_tokens(&toks) {
            objc_registry.insert(sym);
        }
        let dir = file.parent().unwrap_or(&base_dir).to_path_buf();
        // Folder-module sibling pre-scan: same as the old
        // load_recursive's `<dir>/mod.il` branch. Builds
        // per-file `file_class_modules` (→ `sibling_class_maps`) and
        // `implicit_modules`, plus extends the global
        // `objc_class_modules`. Cached at directory granularity in
        // `objc_dir_cache` to amortise across siblings.
        let mut file_class_modules: HashMap<Symbol, Symbol> = HashMap::new();
        let mut implicit_modules: Vec<Symbol> = Vec::new();
        if dir.join("mod.il").exists() {
            let umbrella = dir.file_name().and_then(|n| n.to_str()).map(Symbol::intern);
            let t = Instant::now();
            let scan = objc_dir_cache
                .entry(dir.clone())
                .or_insert_with(|| build_dir_objc_scan(&dir));
            add_ns(&T_DIR_OBJC, t);
            let current_canon = file.canonicalize().ok();
            for e in &scan.entries {
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
                implicit_modules.push(e.stem);
            }
            for (k, v) in &file_class_modules {
                objc_class_modules.entry(*k).or_insert(*v);
            }
        }
        if !file_class_modules.is_empty() {
            sibling_class_maps.insert(file.clone(), file_class_modules);
        }
        implicit_modules_by_file.insert(file.clone(), implicit_modules);
        // Discover deps from the same token stream and enqueue them.
        // Cycles are absorbed by the `seen.insert` guard: a file
        // already enqueued (mid-traversal or earlier) simply won't
        // be re-pushed, terminating the walk naturally.
        let t = Instant::now();
        let use_modules = pre_scan_use_modules(&toks);
        add_ns(&T_PRESCAN_USE, t);
        tokens.insert(file.clone(), toks);
        for (super_count, dep_name, subpath) in use_modules {
            let canon = resolve_module(
                &dep_name, &subpath, &dir, extra_paths, super_count, parents, dep_names_to_dirs,
            )?;
            if seen.insert(canon.clone()) {
                wq.push_back((canon, dir.clone()));
            }
        }
    }
    Ok(())
}

/// Phase 2 of the loader: parse every file Phase 1 collected. Each
/// parse sees the COMPLETE `objc_registry` (token-scan harvest from
/// Phase 1), so the order in which files are parsed doesn't affect
/// `@extern(ObjC)` type recognition. The per-file `target_filter` /
/// `expand_embeds` / `collect_objc_class_names` / `tag_program_spans`
/// passes are the same the old `load_recursive` ran inline after
/// each parse.
///
/// `tokens` is iterated in sorted-key order so a parse error in
/// either of two files surfaces deterministically (matches what the
/// old depth-first order achieved by accident).
fn parse_phase(
    tokens: &HashMap<PathBuf, Vec<ilang_lexer::Token>>,
    objc_registry: &mut HashSet<Symbol>,
    implicit_modules_by_file: &HashMap<PathBuf, Vec<Symbol>>,
    loaded: &mut HashMap<PathBuf, Program>,
) -> Result<(), LoadError> {
    let mut paths: Vec<&PathBuf> = tokens.keys().collect();
    paths.sort();
    let empty_implicits: Vec<Symbol> = Vec::new();
    for file in paths {
        let toks = &tokens[file];
        let implicit = implicit_modules_by_file
            .get(file)
            .unwrap_or(&empty_implicits);
        let file_symbol = Symbol::intern(&file.display().to_string());
        let t = Instant::now();
        let mut prog = crate::parse_with_implicit_modules(
            toks,
            objc_registry,
            implicit,
        )
        .map_err(|mut e| {
            e.set_source_file(file_symbol);
            LoadError::ParseError(e)
        })?;
        add_ns(&T_PARSE, t);
        let t = Instant::now();
        target_filter::filter_program(&mut prog)?;
        add_ns(&T_TARGET, t);
        let t = Instant::now();
        expand_embeds(&mut prog, file)?;
        add_ns(&T_EMBED, t);
        // After parse: register any @objc classes the parser
        // synthesised (auto-lift converts `class C : SomeObjcInterface`
        // into an `@extern(ObjC) { @objc class C ... }` block whose
        // class name isn't visible to the token scan). Keeps the
        // registry complete for later parses in this loop and for
        // the post-load `auto_lift_objc_subclasses` pass.
        let t = Instant::now();
        collect_objc_class_names(&prog, objc_registry);
        add_ns(&T_COLLECT_OBJC, t);
        let t = Instant::now();
        tag_program_spans(&mut prog, file_symbol);
        add_ns(&T_TAG, t);
        loaded.insert(file.clone(), prog);
    }
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
