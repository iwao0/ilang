//! Windows-only integration harness for the DirectX 12 bindings
//! under `bindings/directx12/test/`. Mirrors the windows_bindings.rs
//! pattern: spawn each `.il` fixture, treat non-zero exit as failure,
//! print a per-DLL coverage report.
//!
//! Tests on machines without a D3D12 GPU may still pass: each
//! fixture explicitly accepts the "feature not available" HRESULT
//! and exits 0.

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
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p
}

fn directx12_dir() -> PathBuf {
    repo_root().join("bindings/directx12")
}

fn test_dir() -> PathBuf {
    directx12_dir().join("test")
}

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

/// Parses every `bindings/directx12/*.il` file (except `directx12.il`,
/// the umbrella) and groups `@lib pub fn` declarations by source file.
fn parse_binding_fns() -> BTreeMap<String, Vec<String>> {
    let mut by_dll: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let Ok(entries) = fs::read_dir(directx12_dir()) else {
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
        if stem == "directx12" || stem == "mod" {
            continue;
        }
        let Ok(src) = fs::read_to_string(&p) else { continue };
        let mut fns: Vec<String> = Vec::new();
        for line in src.lines() {
            let trimmed = line.trim_start();
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

fn parse_test_call_usage() -> BTreeSet<String> {
    let mut used: BTreeSet<String> = BTreeSet::new();
    for path in collect_test_fixtures() {
        let Ok(src) = fs::read_to_string(&path) else { continue };
        for line in src.lines() {
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
    out.push_str("\n=== DirectX 12 binding coverage ===\n");

    let mut total_fns = 0usize;
    let mut covered_fns = 0usize;

    for (dll, fns) in &by_dll {
        let mut hit = 0usize;
        for fn_name in fns {
            if used.contains(fn_name) {
                hit += 1;
            }
        }
        total_fns += fns.len();
        covered_fns += hit;
        let pct = if fns.is_empty() {
            0
        } else {
            hit * 100 / fns.len()
        };
        out.push_str(&format!(
            "  {:<16} {:>3}/{:<3} ({:>3}%)\n",
            dll,
            hit,
            fns.len(),
            pct
        ));
    }
    let pct = if total_fns == 0 {
        0
    } else {
        covered_fns * 100 / total_fns
    };
    out.push_str(&format!(
        "  ----- \n  functions: {}/{} ({}%)\n",
        covered_fns, total_fns, pct
    ));
    out
}

#[test]
fn run_directx12_fixtures() {
    if !cfg!(target_os = "windows") {
        eprintln!("skipping: DirectX 12 bindings only run on Windows");
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
