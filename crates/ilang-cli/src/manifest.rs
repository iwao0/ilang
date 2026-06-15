//! `ilang.toml` capability manifest.
//!
//! Searched for by walking up from the entry `.il` file's directory
//! (Cargo-style), so one manifest at a project root covers every `.il`
//! under it. The `capabilities` array lists the granted capabilities:
//!
//! ```toml
//! capabilities = ["file", "os", "ffi", "net"]
//! ```
//!
//! A capability the manifest does NOT list is denied — `std.fs`
//! (`file`), `std.os` (`os`), and a user `@extern(C)` call (`ffi`) abort
//! at runtime (JIT) or fail the build (AOT) without their grant. An
//! absent / empty manifest grants nothing.

use std::path::Path;

use ilang_runtime::caps::{CAP_FFI, CAP_FILE, CAP_NET, CAP_OS};

/// Granted capabilities as a bitset, found by walking up from `entry`'s
/// directory. Absent manifest → 0 (deny all). An unknown capability name
/// in the manifest is a hard error so a typo can't silently under-grant.
pub fn granted_caps(entry: &Path) -> Result<u32, String> {
    let Some(path) = find_manifest(entry) else {
        return Ok(0);
    };
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("ilang.toml: {}: {e}", path.display()))?;
    let doc: toml::Value = text
        .parse()
        .map_err(|e| format!("ilang.toml: {}: {e}", path.display()))?;
    let mut bits = 0u32;
    if let Some(list) = doc.get("capabilities") {
        let arr = list.as_array().ok_or_else(|| {
            format!("ilang.toml: `capabilities` must be an array of strings")
        })?;
        for v in arr {
            let name = v.as_str().ok_or_else(|| {
                format!("ilang.toml: `capabilities` entries must be strings")
            })?;
            bits |= cap_bit(name).ok_or_else(|| {
                format!(
                    "ilang.toml: unknown capability {name:?} \
                     (known: file, os, ffi, net)"
                )
            })?;
        }
    }
    Ok(bits)
}

fn cap_bit(name: &str) -> Option<u32> {
    match name {
        "file" => Some(CAP_FILE),
        "os" => Some(CAP_OS),
        "ffi" => Some(CAP_FFI),
        "net" => Some(CAP_NET),
        _ => None,
    }
}

/// Walk up from `entry`'s directory looking for `ilang.toml`. The entry
/// is canonicalised first so the walk climbs the real filesystem to the
/// root regardless of the process's working directory or a relative
/// entry path.
fn find_manifest(entry: &Path) -> Option<std::path::PathBuf> {
    let abs = std::fs::canonicalize(entry).ok()?;
    let mut dir = abs.parent()?.to_path_buf();
    loop {
        let candidate = dir.join("ilang.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}
