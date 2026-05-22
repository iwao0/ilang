//! `ilang.toml` project-file parsing + `[deps]` resolution.
//!
//! Each `[deps]` entry maps a name to a directory; every `.il`
//! file under that directory becomes resolvable via `use <name>`
//! from the project (the dep name itself is informational, not
//! the module name — `use sdl` finds `sdl.il` under any
//! registered directory). Paths are interpreted relative to the
//! project file.
//!
//! Three accepted shapes per entry:
//!
//!   gui   = "../libs/gui-core"                    # bare path
//!   gui   = { path = "../libs/gui-core" }         # single table
//!   [[deps.gui_impl]]                             # OS-multiplexed
//!   path   = "../libs/gui-cocoa"
//!   target = "macos"
//!   [[deps.gui_impl]]
//!   path   = "../libs/gui-win32"
//!   target = "windows"
//!
//! `target` accepts a single OS string (`"macos"`) or an array of
//! OS strings (`["macos", "linux"]`, OR-matched). Entries whose
//! `target` doesn't match the build host are silently dropped. If
//! more than one surviving entry shares the same dep name, the
//! load fails.

use std::path::PathBuf;

#[derive(Debug, serde::Deserialize)]
struct ProjectFile {
    #[serde(default)]
    deps: std::collections::BTreeMap<String, DepEntry>,
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

/// Aggregated dep info for the loader. `dirs` is the flat list
/// of every package directory reachable from the entry's
/// `ilang.toml`; `parents` maps each child package's directory to
/// the directory of the package that listed it as a dep. The
/// loader uses `parents` to resolve `use super.M`-style imports
/// without falling back to filesystem `../` lookups.
#[derive(Debug, Default, Clone)]
pub(crate) struct DepTree {
    pub dirs:    Vec<PathBuf>,
    pub parents: std::collections::HashMap<PathBuf, PathBuf>,
}

pub(crate) fn collect_dep_tree(entry: &PathBuf) -> Result<DepTree, String> {
    let entry_dir = entry
        .canonicalize()
        .map_err(|e| format!("cannot resolve entry path: {e}"))?
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    // Walk upward from the entry's directory looking for the
    // closest `ilang.toml`. Stops at the first hit; absent file is
    // not an error (project file is optional).
    let Some(project_file) = find_project_file(&entry_dir) else {
        return Ok(DepTree::default());
    };
    let host = current_os();
    let mut visited: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    let mut out = DepTree::default();
    let entry_pkg_dir = project_file
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    walk_project(&project_file, &entry_pkg_dir, host, &mut visited, &mut out)?;
    Ok(out)
}

/// Read one `ilang.toml`, resolve its `[deps]` entries against
/// the current host OS, push each dep directory into `out.dirs`,
/// record `child → parent_pkg_dir` in `out.parents`, and recurse
/// into the dep's own `ilang.toml` if it has one — so a consumer
/// that depends on `gui-core` automatically picks up gui-core's
/// `gui_impl` dep without having to mention the OS-specific
/// package in its own manifest. `visited` is keyed on canonical
/// project-file paths to break cycles (`A → B → A`).
fn walk_project(
    project_file: &std::path::Path,
    parent_pkg_dir: &std::path::Path,
    host: &str,
    visited: &mut std::collections::HashSet<PathBuf>,
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
        .unwrap_or_else(|| std::path::PathBuf::from("."));
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
        // Recurse: if the dep is itself an ilang package (has its
        // own `ilang.toml`), pull its deps into the search path
        // too.
        let nested = canon.join("ilang.toml");
        if nested.exists() {
            walk_project(&nested, &canon, host, visited, out)?;
        }
    }
    Ok(())
}

/// Resolve a `[deps]` entry to at most one path string for the
/// current host OS. Multiple surviving entries with the same name
/// are an error.
fn select_for_host(
    name: &str,
    dep: DepEntry,
    host: &str,
    project_file: &std::path::Path,
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

pub(crate) fn find_project_file(start: &std::path::Path) -> Option<PathBuf> {
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
