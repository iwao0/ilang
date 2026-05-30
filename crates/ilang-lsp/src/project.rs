//! Project / module file discovery: `ilang.toml` deps and umbrella
//! sub-module detection.
//!
//! The dep-resolution side mirrors `crates/ilang-cli/src/project.rs`
//! verbatim — accepted entry shapes (`name = "path"` / single table /
//! `[[deps.name]]` array-of-tables with optional `target` filter) and
//! transitive walk are kept in sync. The duplication is deliberate
//! for now: lsp doesn't depend on ilang-cli, so the two crates copy
//! the same logic until one of them is promoted to a shared crate.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::SystemTime;

/// Set by the backend once `workspace/didChangeWatchedFiles` is
/// successfully registered with the client. While true, the umbrella
/// cache trusts watcher events to invalidate it: cached entries
/// stay valid across refreshes without re-statting every sibling
/// `.il`, and `clear_umbrella_cache()` (called from the
/// `did_change_watched_files` handler) drops them on any `.il`
/// create / change / delete. While false, every lookup falls back
/// to an mtime snapshot of the directory.
pub(crate) static UMBRELLA_WATCH_TRUSTED: AtomicBool = AtomicBool::new(false);

/// Drop every umbrella-resolution cache entry. Called from the LSP's
/// `did_change_watched_files` handler so a `pub use` edit in a
/// sibling file shows up on the next refresh even when the cache
/// has been skipping mtime snapshots under a trusted watcher.
pub(crate) fn clear_umbrella_cache() {
    UMBRELLA_CACHE.lock().unwrap().clear();
}

use ilang_ast::{Item, Program};

#[derive(Debug, serde::Deserialize)]
struct ProjectFile {
    #[serde(default)]
    deps: BTreeMap<String, DepEntry>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum DepEntry {
    /// `name = "path"`
    Bare(String),
    /// `name = { path = "...", target = "macos" }` or
    /// `[[deps.name]]` array (each element parsed as `Detailed`).
    Detailed(DetailedDep),
    /// `[[deps.name]] path = "..." target = "..."`
    Multi(Vec<DetailedDep>),
}

#[derive(Debug, serde::Deserialize)]
struct DetailedDep {
    path: String,
    #[serde(default)]
    target: Option<TargetSpec>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum TargetSpec {
    One(String),
    Many(Vec<String>),
}

impl TargetSpec {
    fn matches(&self, host: &str) -> bool {
        match self {
            TargetSpec::One(s) => s == host,
            TargetSpec::Many(xs) => xs.iter().any(|s| s == host),
        }
    }
}

/// Build-time host OS name, matching the loader's `@target("...")`
/// strings and `os.platform` runtime values.
const fn current_os() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        "other"
    }
}

/// Aggregated dep info — mirror of `ilang-cli::project::DepTree`.
/// `dirs` is the flat search-path list; `parents` maps each
/// child package's directory to its parent's directory in the
/// dep DAG and backs `use super.X` resolution. `names_to_dirs`
/// records each `[deps]` entry's user-chosen name → resolved
/// directory so `use <dep_name>` can route to the dep's `mod.il`.
#[derive(Debug, Default, Clone)]
pub(crate) struct DepTree {
    pub dirs:    Vec<PathBuf>,
    pub parents: std::collections::HashMap<PathBuf, PathBuf>,
    pub names_to_dirs: std::collections::HashMap<String, PathBuf>,
}

/// Mirror of the CLI's `ilang.toml` discovery. Walks up from the entry
/// file's directory looking for the closest `ilang.toml`; missing file
/// is not an error. Follows the manifest's deps transitively so a
/// consumer that depends on `gui-core` automatically picks up
/// gui-core's `gui_impl` without re-listing it in its own manifest.
pub(crate) fn collect_dep_paths(entry: &Path) -> Result<Vec<PathBuf>, String> {
    Ok(collect_dep_tree(entry)?.dirs)
}

pub(crate) fn collect_dep_tree(entry: &Path) -> Result<DepTree, String> {
    let entry_canon = entry
        .canonicalize()
        .map_err(|e| format!("cannot resolve entry path: {e}"))?;
    let entry_dir = entry_canon
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    // For a sub-package file (e.g. `libs/gui/cocoa/core.il`),
    // the closest `ilang.toml` only describes the sub-package's
    // own deps. Walk further up looking for an outer
    // `ilang.toml` whose tree transitively includes the current
    // file's package — that's the project root the dep DAG
    // (and `super.M` resolution) should be built from.
    let host = current_os();
    let mut best_tree: Option<DepTree> = None;
    let mut search = entry_dir.clone();
    loop {
        if let Some(pf) = find_project_file(&search) {
            let mut visited: HashSet<PathBuf> = HashSet::new();
            let mut out = DepTree::default();
            let pf_dir = pf
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."))
                .canonicalize()
                .unwrap_or_else(|_| PathBuf::from("."));
            if walk_project(&pf, &pf_dir, host, &mut visited, &mut out).is_ok() {
                // The manifest "covers" the entry when the entry
                // file sits inside the manifest's own package
                // directory or inside one of its (transitive)
                // dep directories.
                //
                // The "under pf_dir" arm is only safe for the
                // FIRST (innermost) match: an outer manifest is
                // also an ancestor of every sub-package directory,
                // so accepting it via the pf_dir arm would
                // overwrite a real sub-package match (libs/gui/
                // would clobber libs/gui/win32/ on macOS because
                // libs/gui/ilang.toml's macOS-resolved `gui_impl`
                // points at cocoa, not win32). Once we already
                // have a best_tree, an outer manifest may only
                // override if its tree TRANSITIVELY includes the
                // entry as a dep — which is exactly the
                // consumer-pulls-in-inner-package case the outer
                // walk was added for.
                let covers = if best_tree.is_none() {
                    entry_canon.starts_with(&pf_dir)
                        || out.dirs.iter().any(|d| entry_canon.starts_with(d))
                } else {
                    out.dirs.iter().any(|d| entry_canon.starts_with(d))
                };
                if covers {
                    best_tree = Some(out);
                }
            }
            // Move search one directory above this manifest to
            // look for a still-outer one.
            let parent = pf
                .parent()
                .and_then(|d| d.parent())
                .map(|p| p.to_path_buf());
            match parent {
                Some(p) => search = p,
                None => break,
            }
        } else {
            break;
        }
    }
    Ok(best_tree.unwrap_or_default())
}

fn walk_project(
    project_file: &Path,
    parent_pkg_dir: &Path,
    host: &str,
    visited: &mut HashSet<PathBuf>,
    out: &mut DepTree,
) -> Result<(), String> {
    let canon_pf = project_file
        .canonicalize()
        .map_err(|e| format!("cannot resolve {}: {e}", project_file.display()))?;
    if !visited.insert(canon_pf) {
        return Ok(());
    }
    let project_dir = project_file
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let src = std::fs::read_to_string(project_file)
        .map_err(|e| format!("cannot read {}: {e}", project_file.display()))?;
    let parsed: ProjectFile = toml::from_str(&src)
        .map_err(|e| format!("invalid {}: {e}", project_file.display()))?;
    for (name, dep) in parsed.deps {
        let chosen = select_for_host(&name, dep, host, project_file)?;
        let Some(path_str) = chosen else { continue };
        let p = project_dir.join(&path_str);
        let canon = p.canonicalize().map_err(|e| {
            format!(
                "{}: dep path {:?} doesn't exist: {e}",
                project_file.display(),
                path_str
            )
        })?;
        if !out.dirs.iter().any(|q| q == &canon) {
            out.dirs.push(canon.clone());
        }
        out.parents
            .entry(canon.clone())
            .or_insert_with(|| parent_pkg_dir.to_path_buf());
        out.names_to_dirs
            .entry(name.clone())
            .or_insert_with(|| canon.clone());
        let nested = canon.join("ilang.toml");
        if nested.exists() {
            walk_project(&nested, &canon, host, visited, out)?;
        }
    }
    Ok(())
}

fn select_for_host(
    name: &str,
    dep: DepEntry,
    host: &str,
    project_file: &Path,
) -> Result<Option<String>, String> {
    let candidates: Vec<DetailedDep> = match dep {
        DepEntry::Bare(p) => vec![DetailedDep {
            path: p,
            target: None,
        }],
        DepEntry::Detailed(d) => vec![d],
        DepEntry::Multi(xs) => xs,
    };
    let mut kept: Vec<String> = candidates
        .into_iter()
        .filter(|d| d.target.as_ref().map_or(true, |t| t.matches(host)))
        .map(|d| d.path)
        .collect();
    if kept.len() > 1 {
        return Err(format!(
            "{}: dep `{}` has more than one entry matching host `{}`",
            project_file.display(),
            name,
            host
        ));
    }
    Ok(kept.pop())
}

/// If `path` is a sub-module re-exported from an umbrella file in the
/// same directory (i.e. some sibling has `pub use <basename>`),
/// return the umbrella's path. Used by the LSP so opening a sub-module
/// alone still type-checks under its umbrella's namespace.
///
/// Two cache modes share one `(dir, basename)`-keyed map:
/// * with a trusted file watcher (`UMBRELLA_WATCH_TRUSTED == true`),
///   any cached entry is reused as-is; invalidation happens through
///   `clear_umbrella_cache()` when the watcher fires.
/// * without one, every lookup builds an mtime snapshot of the
///   directory's `.il` siblings and reuses the cached entry only
///   when the snapshot matches.
/// Hot on binding directories with many sub-modules.
pub(crate) fn find_umbrella(path: &Path) -> Option<PathBuf> {
    let basename = path.file_stem()?.to_str()?.to_string();
    let dir = path.parent()?;
    let dir_key = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let cache_key = (dir_key, basename.clone());
    let watch_trusted = UMBRELLA_WATCH_TRUSTED.load(Ordering::Relaxed);

    if watch_trusted {
        if let Some(hit) = UMBRELLA_CACHE.lock().unwrap().get(&cache_key) {
            return hit.umbrella.clone();
        }
        let umbrella = resolve_umbrella(path, dir, &basename);
        UMBRELLA_CACHE.lock().unwrap().insert(
            cache_key,
            UmbrellaCacheEntry { snapshot: None, umbrella: umbrella.clone() },
        );
        return umbrella;
    }

    let snapshot = sibling_snapshot(dir);
    if let Some(hit) = UMBRELLA_CACHE.lock().unwrap().get(&cache_key) {
        if hit.snapshot.as_ref() == Some(&snapshot) {
            return hit.umbrella.clone();
        }
    }
    let umbrella = resolve_umbrella(path, dir, &basename);
    UMBRELLA_CACHE.lock().unwrap().insert(
        cache_key,
        UmbrellaCacheEntry {
            snapshot: Some(snapshot),
            umbrella: umbrella.clone(),
        },
    );
    umbrella
}

fn resolve_umbrella(path: &Path, dir: &Path, basename: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        if p == path || p.extension().and_then(|e| e.to_str()) != Some("il") {
            continue;
        }
        let Ok(src) = std::fs::read_to_string(&p) else {
            continue;
        };
        let Some(prog) = crate::text::try_parse(&src) else { continue };
        let mut visited: HashSet<PathBuf> = HashSet::new();
        if umbrella_re_exports(&prog, dir, basename, &mut visited) {
            return Some(p);
        }
    }
    None
}

/// Sorted `(sibling_path, mtime)` snapshot of every `.il` file in `dir`.
/// Equal snapshots mean nothing in the directory has been added,
/// removed, or modified since the cache entry was produced.
fn sibling_snapshot(dir: &Path) -> Vec<(PathBuf, Option<SystemTime>)> {
    let mut out: Vec<(PathBuf, Option<SystemTime>)> = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return out };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("il") {
            continue;
        }
        let mtime = std::fs::metadata(&p)
            .and_then(|m| m.modified())
            .ok();
        out.push((p, mtime));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

struct UmbrellaCacheEntry {
    /// `Some(snapshot)` when populated without a trusted watcher;
    /// the next lookup re-stats the dir and compares before reusing.
    /// `None` when populated under a trusted watcher; the entry is
    /// kept until `clear_umbrella_cache()` drops it on a watcher
    /// event.
    snapshot: Option<Vec<(PathBuf, Option<SystemTime>)>>,
    umbrella: Option<PathBuf>,
}

static UMBRELLA_CACHE: LazyLock<Mutex<HashMap<(PathBuf, String), UmbrellaCacheEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn umbrella_re_exports(
    prog: &Program,
    dir: &Path,
    target: &str,
    visited: &mut HashSet<PathBuf>,
) -> bool {
    for item in &prog.items {
        let Item::Use(u) = item else { continue };
        if !u.re_export || u.selective.is_some() {
            continue;
        }
        // For multi-segment `pub use a.b.c.*` the deepest segment
        // names the file we're re-exporting; the segments before it
        // are subdirectories.
        let (effective_target, nested) = if u.subpath.is_empty() {
            (u.module.as_str().to_string(), dir.join(format!("{}.il", u.module)))
        } else {
            let mut d = dir.join(u.module.as_str());
            let len = u.subpath.len();
            for seg in &u.subpath[..len - 1] {
                d = d.join(seg.as_str());
            }
            let last = u.subpath[len - 1].as_str();
            (last.to_string(), d.join(format!("{last}.il")))
        };
        if effective_target == target {
            return true;
        }
        if !visited.insert(nested.clone()) {
            continue;
        }
        if let Ok(src) = std::fs::read_to_string(&nested) {
            if let Some(p) = crate::text::try_parse(&src) {
                let nested_dir = nested.parent().unwrap_or(dir);
                if umbrella_re_exports(&p, nested_dir, target, visited) {
                    return true;
                }
            }
        }
    }
    false
}

pub(crate) fn find_project_file(start: &Path) -> Option<PathBuf> {
    let mut cur = Some(start.to_path_buf());
    while let Some(dir) = cur {
        let candidate = dir.join("ilang.toml");
        if candidate.exists() {
            return Some(candidate);
        }
        cur = dir.parent().map(|p| p.to_path_buf());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{collect_dep_tree, find_umbrella};
    use std::path::PathBuf;

    #[test]
    fn dep_tree_prefers_inner_manifest_for_sub_package_file() {
        // Regression for the bug where opening `libs/gui/win32/
        // button.il` on macOS resolved the dep tree against the
        // outer `libs/gui/ilang.toml` (whose macOS-resolved
        // `gui_impl` is cocoa) instead of the inner
        // `libs/gui/win32/ilang.toml` (which declares `windows`).
        // The collateral damage was the entire windows binding
        // disappearing from completion — `HeapFree`, `HMODULE`,
        // `CreateWindowExW`, every Win32 type — even though those
        // are exactly what the file uses.
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop();
        path.pop();
        path.push("libs/gui/win32/button.il");
        let dep_tree = collect_dep_tree(&path).expect("dep tree collection");
        let has_windows = dep_tree
            .dirs
            .iter()
            .any(|d| d.ends_with("bindings/windows"));
        assert!(
            has_windows,
            "expected dep tree for libs/gui/win32/button.il to include \
             bindings/windows; got {:#?}",
            dep_tree.dirs
        );
    }

    /// The umbrella cache must invalidate when a sibling's content
    /// changes — otherwise editing a `pub use` line in the umbrella
    /// would leave a stale "submodule of <X>" verdict for every
    /// keystroke until the LSP restarts.
    #[test]
    fn find_umbrella_invalidates_on_sibling_mtime_change() {
        let dir = std::env::temp_dir().join(format!(
            "ilang_umbrella_cache_{}",
            std::process::id(),
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let umbrella = dir.join("sdl.il");
        let leaf = dir.join("sdl_window.il");
        std::fs::write(&leaf, "pub fn open() {}\n").unwrap();
        std::fs::write(&umbrella, "pub use sdl_window\n").unwrap();
        assert_eq!(find_umbrella(&leaf).as_deref(), Some(umbrella.as_path()));

        // Drop the re-export — find_umbrella must observe the change
        // even though the file path is unchanged.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&umbrella, "pub fn unrelated() {}\n").unwrap();
        assert_eq!(find_umbrella(&leaf), None);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
