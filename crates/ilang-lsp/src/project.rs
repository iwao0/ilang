//! Project / module file discovery: `ilang.toml` deps and umbrella
//! sub-module detection.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use ilang_ast::{Item, Program};
use ilang_lexer::tokenize;
use ilang_parser::parse;

#[derive(Debug, serde::Deserialize)]
struct ProjectFile {
    #[serde(default)]
    deps: BTreeMap<String, DepSpec>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum DepSpec {
    Path(String),
    Detailed { path: String },
}

impl DepSpec {
    fn path(&self) -> &str {
        match self {
            DepSpec::Path(p) => p,
            DepSpec::Detailed { path } => path,
        }
    }
}

/// Mirror of the CLI's `ilang.toml` discovery. Walks up from the entry
/// file's directory looking for the closest `ilang.toml`; missing file
/// is not an error.
pub(crate) fn collect_dep_paths(entry: &Path) -> Result<Vec<PathBuf>, String> {
    let entry_dir = entry
        .canonicalize()
        .map_err(|e| format!("cannot resolve entry path: {e}"))?
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let project_file = find_project_file(&entry_dir);
    let Some(project_file) = project_file else {
        return Ok(Vec::new());
    };
    let project_dir = project_file
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let src = std::fs::read_to_string(&project_file)
        .map_err(|e| format!("cannot read {}: {e}", project_file.display()))?;
    let parsed: ProjectFile = toml::from_str(&src)
        .map_err(|e| format!("invalid {}: {e}", project_file.display()))?;
    let mut out = Vec::new();
    for (_name, dep) in parsed.deps {
        let p = project_dir.join(dep.path());
        if let Ok(canon) = p.canonicalize() {
            out.push(canon);
        }
    }
    Ok(out)
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
