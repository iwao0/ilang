//! `ilang.toml` project-file parsing + `[deps]` resolution.
//!
//! Each `[deps]` entry maps a name to a directory; every `.il`
//! file under that directory becomes resolvable via `use <name>`
//! from the project (the dep name itself is informational, not
//! the module name — `use sdl` finds `sdl.il` under any
//! registered directory). Paths are interpreted relative to the
//! project file.

use std::path::PathBuf;

#[derive(Debug, serde::Deserialize)]
struct ProjectFile {
    #[serde(default)]
    deps: std::collections::BTreeMap<String, DepSpec>,
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

pub(crate) fn collect_dep_paths(entry: &PathBuf) -> Result<Vec<PathBuf>, String> {
    let entry_dir = entry
        .canonicalize()
        .map_err(|e| format!("cannot resolve entry path: {e}"))?
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    // Walk upward from the entry's directory looking for the
    // closest `ilang.toml`. Stops at the first hit; absent file is
    // not an error (project file is optional).
    let project_file = find_project_file(&entry_dir);
    let Some(project_file) = project_file else {
        return Ok(Vec::new());
    };
    let project_dir = project_file
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let src = std::fs::read_to_string(&project_file)
        .map_err(|e| format!("cannot read {}: {e}", project_file.display()))?;
    let parsed: ProjectFile = toml::from_str(&src)
        .map_err(|e| format!("invalid {}: {e}", project_file.display()))?;
    let mut out = Vec::new();
    for (_name, dep) in parsed.deps {
        let p = project_dir.join(dep.path());
        let canon = p.canonicalize().map_err(|e| {
            format!(
                "{}: dep path {:?} doesn't exist: {e}",
                project_file.display(),
                dep.path()
            )
        })?;
        out.push(canon);
    }
    Ok(out)
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
