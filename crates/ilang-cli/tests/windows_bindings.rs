//! Windows-only integration harness for the Win32 bindings under
//! `bindings/windows/test/`. Each `.il` fixture exits non-zero on
//! assertion failure (the `test.expect*` helpers abort the process);
//! we just check the exit status.
//!
//! Coverage report (printed at the end of the run): for every
//! `@lib pub fn <name>` declared in a `bindings/windows/*.il` file,
//! check whether any test fixture contains a `<name>(` call. Group
//! by DLL (one binding file = one DLL).  Read via the standard
//! `cargo test ... -- --nocapture` flag.
//!
//! Non-Windows hosts skip the entire suite — the Win32 import libs
//! aren't available off-Windows.
//!
//! Each fixture is launched from `bindings/windows/test/` so the
//! `ilang.toml` deps alias (`windows = ".."`) resolves correctly.
//! The harness sets `current_dir(test_dir())` to enforce that.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn ilang_bin() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_BIN_EXE_ilang"));
    p.pop();
    p.push("ilang");
    p
}

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = crates/ilang-cli — pop twice for the
    // repo root.
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p
}

fn windows_dir() -> PathBuf {
    repo_root().join("bindings/windows")
}

fn test_dir() -> PathBuf {
    windows_dir().join("test")
}

/// All `.il` files in `test_dir`, sorted for stable failure order.
fn collect_test_fixtures() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let Ok(entries) = fs::read_dir(test_dir()) else {
        return out;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) == Some("il") {
            out.push(p);
        }
    }
    out.sort();
    out
}

/// Parses every `bindings/windows/*.il` file (except `windows.il` and
/// `mod.il`, which are pure re-exports) and harvests every
/// `@lib pub fn <name>` declaration along with the DLL it belongs
/// to (one DLL per file — `kernel32.il` → "kernel32").
fn parse_binding_fns() -> BTreeMap<String, Vec<String>> {
    let mut by_dll: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let Ok(entries) = fs::read_dir(windows_dir()) else {
        return by_dll;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("il") {
            continue;
        }
        let stem = match p.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        if stem == "windows" || stem == "mod" {
            continue;
        }
        let Ok(src) = fs::read_to_string(&p) else { continue };
        let mut fns: Vec<String> = Vec::new();
        for line in src.lines() {
            // Strip leading whitespace + optional doc comments.
            let trimmed = line.trim_start();
            // We only care about `@lib pub fn <name>(...)` lines.
            // Some functions break the signature across lines; the
            // declarator is always on the same line as `@lib pub fn`.
            let rest = match trimmed.strip_prefix("@lib pub fn ") {
                Some(r) => r,
                None => continue,
            };
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                fns.push(name);
            }
        }
        if !fns.is_empty() {
            by_dll.insert(stem, fns);
        }
    }
    by_dll
}

/// Extract every identifier that appears in call position
/// (`name(` or `name<`) inside the test fixtures. Catches both bare
/// calls (`GetTickCount()`) and method-style calls (`obj.foo(`).
fn parse_test_call_usage() -> BTreeSet<String> {
    let mut used: BTreeSet<String> = BTreeSet::new();
    for path in collect_test_fixtures() {
        let Ok(src) = fs::read_to_string(&path) else { continue };
        for line in src.lines() {
            // Strip line comments — function names don't appear inside.
            let line = match line.find("//") {
                Some(i) => &line[..i],
                None => line,
            };
            let bytes: Vec<char> = line.chars().collect();
            let mut i = 0;
            while i < bytes.len() {
                let c = bytes[i];
                if c.is_alphabetic() || c == '_' {
                    let start = i;
                    while i < bytes.len()
                        && (bytes[i].is_alphanumeric() || bytes[i] == '_')
                    {
                        i += 1;
                    }
                    let ident: String = bytes[start..i].iter().collect();
                    let next = bytes.get(i).copied().unwrap_or(' ');
                    // `ident(` or `ident<` count as call sites.
                    if next == '(' || next == '<' {
                        used.insert(ident);
                    }
                } else {
                    i += 1;
                }
            }
        }
    }
    used
}

fn coverage_report() -> String {
    let by_dll = parse_binding_fns();
    let used = parse_test_call_usage();

    let mut out = String::new();
    out.push_str("\n=== Win32 binding coverage ===\n");

    let mut total_fns = 0usize;
    let mut covered_fns = 0usize;
    let mut total_dlls = 0usize;
    let mut covered_dlls = 0usize;

    for (dll, fns) in &by_dll {
        total_dlls += 1;
        let mut hit = 0usize;
        for fn_name in fns {
            if used.contains(fn_name) {
                hit += 1;
            }
        }
        total_fns += fns.len();
        covered_fns += hit;
        if hit > 0 {
            covered_dlls += 1;
        }
        let pct = if fns.is_empty() {
            0
        } else {
            hit * 100 / fns.len()
        };
        out.push_str(&format!(
            "  {:<14} {:>3}/{:<3} ({:>3}%)\n",
            dll,
            hit,
            fns.len(),
            pct
        ));
    }
    let fn_pct = if total_fns == 0 {
        0
    } else {
        covered_fns * 100 / total_fns
    };
    let dll_pct = if total_dlls == 0 {
        0
    } else {
        covered_dlls * 100 / total_dlls
    };
    out.push_str(&format!(
        "  ----- \n  functions: {}/{} ({}%)\n  DLLs     : {}/{} ({}%)\n",
        covered_fns, total_fns, fn_pct, covered_dlls, total_dlls, dll_pct
    ));
    out
}

#[test]
fn run_windows_fixtures() {
    if !cfg!(target_os = "windows") {
        eprintln!("skipping: windows bindings tests only run on Windows");
        return;
    }

    let bin = ilang_bin();
    let fixtures = collect_test_fixtures();
    assert!(
        !fixtures.is_empty(),
        "no test fixtures discovered in {}",
        test_dir().display()
    );

    let mut failures: Vec<String> = Vec::new();
    for path in &fixtures {
        let out = Command::new(&bin)
            .arg("run")
            .arg(path)
            .current_dir(test_dir())
            .output()
            .unwrap_or_else(|e| panic!("failed to spawn ilang: {e}"));
        if !out.status.success() {
            failures.push(format!(
                "FAIL {}\n  stdout: {}\n  stderr: {}",
                path.file_name().unwrap().to_string_lossy(),
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr),
            ));
        } else {
            eprintln!(
                "pass: {}",
                path.file_name().unwrap().to_string_lossy()
            );
        }
    }

    eprintln!("{}", coverage_report());

    if !failures.is_empty() {
        panic!(
            "{} fixture(s) failed:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}
