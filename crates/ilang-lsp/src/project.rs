//! Project / module file discovery: `ilang.toml` deps and umbrella
//! sub-module detection.
//!
//! The dep-resolution side mirrors `crates/ilang-cli/src/project.rs`
//! verbatim — accepted entry shapes (`name = "path"` / single table /
//! `[[deps.name]]` array-of-tables with optional `target` filter) and
//! transitive walk are kept in sync. The duplication is deliberate
//! for now: lsp doesn't depend on ilang-cli, so the two crates copy
//! the same logic until one of them is promoted to a shared crate.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use ilang_ast::{Item, Program};
use ilang_lexer::tokenize;
use ilang_parser::parse;

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

/// Mirror of the CLI's `ilang.toml` discovery. Walks up from the entry
/// file's directory looking for the closest `ilang.toml`; missing file
/// is not an error. Follows the manifest's deps transitively so a
/// consumer that depends on `gui-core` automatically picks up
/// gui-core's `gui_impl` without re-listing it in its own manifest.
pub(crate) fn collect_dep_paths(entry: &Path) -> Result<Vec<PathBuf>, String> {
    let entry_dir = entry
        .canonicalize()
        .map_err(|e| format!("cannot resolve entry path: {e}"))?
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let Some(project_file) = find_project_file(&entry_dir) else {
        return Ok(Vec::new());
    };
    let host = current_os();
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut out = Vec::new();
    walk_project(&project_file, host, &mut visited, &mut out)?;
    Ok(out)
}

fn walk_project(
    project_file: &Path,
    host: &str,
    visited: &mut HashSet<PathBuf>,
    out: &mut Vec<PathBuf>,
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
        if !out.iter().any(|q| q == &canon) {
            out.push(canon.clone());
        }
        let nested = canon.join("ilang.toml");
        if nested.exists() {
            walk_project(&nested, host, visited, out)?;
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
pub(crate) fn find_umbrella(path: &Path) -> Option<PathBuf> {
    let basename = path.file_stem()?.to_str()?;
    let dir = path.parent()?;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        if p == path || p.extension().and_then(|e| e.to_str()) != Some("il") {
            continue;
        }
        let Ok(src) = std::fs::read_to_string(&p) else {
            continue;
        };
        let Ok(tokens) = tokenize(&src) else { continue };
        let Ok(prog) = parse(&tokens) else { continue };
        let mut visited: HashSet<PathBuf> = HashSet::new();
        if umbrella_re_exports(&prog, dir, basename, &mut visited) {
            return Some(p);
        }
    }
    None
}

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
        if u.module == target {
            return true;
        }
        let nested = dir.join(format!("{}.il", u.module));
        if !visited.insert(nested.clone()) {
            continue;
        }
        if let Ok(src) = std::fs::read_to_string(&nested) {
            if let Ok(tokens) = tokenize(&src) {
                if let Ok(p) = parse(&tokens) {
                    if umbrella_re_exports(&p, dir, target, visited) {
                        return true;
                    }
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
