//! Path resolution + on-disk source reading. Turns the
//! `(super_count, module, subpath)` triples that `prescan` extracts
//! from a file's tokens into canonical disk paths, then reads each
//! resolved file (consulting the overlay map first so unsaved LSP
//! buffers drive diagnostics instead of the on-disk snapshot).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::LoadError;
use super::builtin::{builtin_module_source, builtin_path, is_builtin_path};

pub(super) fn canonicalize(p: &Path) -> Result<PathBuf, LoadError> {
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
pub(super) fn resolve_module(
    module: &str,
    subpath: &[String],
    dir: &Path,
    extra_paths: &[PathBuf],
    super_count: u32,
    parents: &HashMap<PathBuf, PathBuf>,
    dep_names_to_dirs: &HashMap<String, PathBuf>,
) -> Result<PathBuf, LoadError> {
    // Multi-segment imports (`use a.b.c.*` or `use a.b.X`): resolve
    // `a` to a directory first, walk subpath[..-1] as subdirectories,
    // and treat subpath[-1] as the file basename under the deepest
    // subdir. Falls through to single-segment resolution when subpath
    // is empty.
    if !subpath.is_empty() {
        // `use std.X` — the stdlib lives inside the compiler as
        // embedded source (see `builtin_module_source`). Route the
        // path-style import here instead of through the disk so
        // released binaries shipped without a checked-out source
        // tree keep working. Only flat `std.<X>` is honoured; deeper
        // paths (`std.X.Y`) have no built-in mapping.
        if module == "std" && super_count == 0 {
            if subpath.len() == 1 {
                let leaf = subpath[0].as_str();
                if builtin_module_source(leaf).is_some() {
                    return Ok(builtin_path(leaf));
                }
            }
            return Err(LoadError::ReadError {
                path: PathBuf::from(format!(
                    "<builtin>/std/{}.il",
                    subpath
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join("/")
                )),
                message: format!(
                    "`use std.{}`: no such built-in module under `std`",
                    subpath
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(".")
                ),
            });
        }
        let mut base = resolve_base_directory(
            module, dir, extra_paths, super_count, parents, dep_names_to_dirs,
        )?;
        // The last subpath entry names the actual `.il` file (or
        // folder-module umbrella). Everything before it is a chain
        // of subdirectories.
        let (last, mids) = subpath.split_last().unwrap();
        for seg in mids {
            base = base.join(seg);
            if !base.exists() {
                return Err(LoadError::ReadError {
                    path: base.clone(),
                    message: format!(
                        "`use {module}.{}.{last}.*`: subdirectory not found",
                        mids.join(".")
                    ),
                });
            }
        }
        let candidate = base.join(format!("{last}.il"));
        if candidate.exists() {
            return canonicalize(&candidate);
        }
        let candidate_mod = base.join(last).join("mod.il");
        if candidate_mod.exists() {
            return canonicalize(&candidate_mod);
        }
        return Err(LoadError::ReadError {
            path: candidate,
            message: format!(
                "`use {module}.{}.{last}`: no matching file or `mod.il`",
                subpath[..subpath.len() - 1].join(".")
            ),
        });
    }
    // `use super.M`: skip the sibling / extra_paths fallback and
    // anchor the search to the importer's package's parent in the
    // dep tree (built by `project.rs::collect_dep_tree`). Each
    // additional `super.` walks one more edge up.
    if super_count > 0 {
        let pkg = find_owning_package(dir, extra_paths)
            .unwrap_or_else(|| dir.to_path_buf());
        let mut cur = pkg;
        for _ in 0..super_count {
            cur = match parents.get(&cur) {
                Some(p) => p.clone(),
                None => return Err(LoadError::ReadError {
                    path: cur,
                    message: format!(
                        "`use super.{module}`: no parent package in the dep tree"
                    ),
                }),
            };
        }
        let filename = format!("{module}.il");
        let primary = cur.join(&filename);
        if primary.exists() {
            return canonicalize(&primary);
        }
        let mod_il = cur.join(module).join("mod.il");
        if mod_il.exists() {
            return canonicalize(&mod_il);
        }
        return canonicalize(&primary);
    }
    // Resolution order: sibling file → sibling subfolder →
    // `ilang.toml [deps]` name-keyed mod.il → each explicit dep dir
    // (file-name fallback) → stdlib builtin. The dep-name lookup
    // sits between sibling and extra_paths so the user-chosen name
    // from `[deps] X = "/path"` resolves `use X` to `/path/mod.il`
    // regardless of how the directory or umbrella file is spelled.
    // Stdlib comes LAST so a sibling file with the same name (e.g.
    // `appkit/events.il` next to `libs/std/events.il`) wins —
    // otherwise the loader would dlopen the stdlib file under that
    // bare module name and the visibility catalog would only see
    // the stdlib's pubs.
    let filename = format!("{module}.il");
    // `<dir>/<module>.il` — sibling file. Highest priority.
    let primary = dir.join(&filename);
    if primary.exists() {
        return canonicalize(&primary);
    }
    // `<dir>/<module>/mod.il` — Rust-style subfolder umbrella. Lets
    // a binding grow into a `<module>/` folder of category files
    // (`<module>/mod.il` re-exporting the siblings) without breaking
    // existing `use <module>` callers.
    let mod_il = dir.join(module).join("mod.il");
    if mod_il.exists() {
        return canonicalize(&mod_il);
    }
    // `[deps] <module> = "<dir>"` — load `<dir>/mod.il`. The dep
    // name is decoupled from the on-disk file structure: the
    // consumer writes `use <name>` and the loader picks the dep
    // directory by name, then loads its `mod.il` umbrella.
    if let Some(dep_dir) = dep_names_to_dirs.get(module) {
        let candidate_mod = dep_dir.join("mod.il");
        if candidate_mod.exists() {
            return canonicalize(&candidate_mod);
        }
    }
    for extra in extra_paths {
        let candidate = extra.join(&filename);
        if candidate.exists() {
            return canonicalize(&candidate);
        }
        let candidate_mod = extra.join(module).join("mod.il");
        if candidate_mod.exists() {
            return canonicalize(&candidate_mod);
        }
    }
    // Stdlib builtins (`math` / `os` / `events` / `regex` / …) are
    // only reachable through the `std.X` namespace — a bare
    // `use math` is deprecated, point the user at the new form.
    if builtin_module_source(module).is_some() {
        return Err(LoadError::ReadError {
            path: primary,
            message: format!(
                "`use {module}` is no longer supported — write `use std.{module}` instead",
            ),
        });
    }
    // Fall back to the primary path so the resulting "not found"
    // error mentions the importer-local location (most actionable).
    canonicalize(&primary)
}

/// Resolve `module` to a directory (not a file) — the starting
/// point for a multi-segment `use a.b.c` walk. Looks at the same
/// candidates as `resolve_module` but accepts only the directory
/// forms (`<dir>/<module>/` or `<extra>/<module>/`).
fn resolve_base_directory(
    module: &str,
    dir: &Path,
    extra_paths: &[PathBuf],
    super_count: u32,
    parents: &HashMap<PathBuf, PathBuf>,
    dep_names_to_dirs: &HashMap<String, PathBuf>,
) -> Result<PathBuf, LoadError> {
    if super_count > 0 {
        let pkg = find_owning_package(dir, extra_paths)
            .unwrap_or_else(|| dir.to_path_buf());
        let mut cur = pkg;
        for _ in 0..super_count {
            cur = match parents.get(&cur) {
                Some(p) => p.clone(),
                None => return Err(LoadError::ReadError {
                    path: cur,
                    message: format!(
                        "`use super.{module}.*`: no parent package in the dep tree"
                    ),
                }),
            };
        }
        let candidate = cur.join(module);
        if candidate.is_dir() {
            return Ok(candidate);
        }
        return Err(LoadError::ReadError {
            path: candidate,
            message: format!(
                "`use super.{module}.<...>`: directory not found"
            ),
        });
    }
    let local = dir.join(module);
    if local.is_dir() {
        return Ok(local);
    }
    // `[deps] <module> = "<dir>"` — use the dep's resolved
    // directory as the base for the subpath walk.
    if let Some(dep_dir) = dep_names_to_dirs.get(module) {
        if dep_dir.is_dir() {
            return Ok(dep_dir.clone());
        }
    }
    for extra in extra_paths {
        let candidate = extra.join(module);
        if candidate.is_dir() {
            return Ok(candidate);
        }
    }
    Err(LoadError::ReadError {
        path: dir.join(module),
        message: format!(
            "`use {module}.<...>`: no directory named `{module}` next to importer or under any dep path"
        ),
    })
}

/// Find the package directory `dir` (the importer's file's
/// directory) belongs to: the closest ancestor that appears in
/// the dep-tree's `extra_paths` list. Returns `None` when the
/// importer lives outside any registered package — e.g. the
/// entry file itself, whose package is the entry project's root.
fn find_owning_package(dir: &Path, extra_paths: &[PathBuf]) -> Option<PathBuf> {
    let canon = dir.canonicalize().ok()?;
    let mut best: Option<&Path> = None;
    for p in extra_paths {
        if canon.starts_with(p) {
            // Prefer the deepest ancestor — a package nested
            // inside another's directory wins.
            if best.map(|b| p.starts_with(b)).unwrap_or(true) {
                best = Some(p);
            }
        }
    }
    best.map(|p| p.to_path_buf())
}

pub(super) fn read_source(
    file: &Path,
    overlay: &HashMap<PathBuf, String>,
) -> Result<String, LoadError> {
    if let Some(s) = overlay.get(file) {
        Ok(s.clone())
    } else if let Some(name) = is_builtin_path(file) {
        Ok(builtin_module_source(name)
            .expect("builtin path checked")
            .to_string())
    } else {
        std::fs::read_to_string(file).map_err(|e| LoadError::ReadError {
            path: file.to_path_buf(),
            message: e.to_string(),
        })
    }
}
