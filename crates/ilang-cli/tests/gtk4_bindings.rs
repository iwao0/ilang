//! Integration harness for the GTK 4 bindings under
//! `bindings/gtk4/test/`. Mirrors directx12_bindings.rs: spawn each
//! `.il` fixture, treat non-zero exit as failure, and print a
//! per-file coverage report counting `@lib pub fn` declarations
//! that show up in fixture call sites.
//!
//! Some fixtures call into `libgtk-4` (display-free GMenu / GAction
//! manipulation only), so the harness only runs when `pkg-config`
//! reports `gtk4` available — on macOS with Homebrew's `gtk4`
//! formula or any Linux box with the distro's `libgtk-4-dev` (or
//! equivalent) installed. Windows is gated out entirely; GTK 4 on
//! Windows isn't part of the libs/gui surface.

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

fn gtk4_dir() -> PathBuf {
    repo_root().join("bindings/gtk4")
}

fn test_dir() -> PathBuf {
    gtk4_dir().join("test")
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

/// Parses every `bindings/gtk4/*.il` file (except the umbrella
/// `gtk.il`) and groups `@lib pub fn` declarations by source file.
fn parse_binding_fns() -> BTreeMap<String, Vec<String>> {
    let mut by_file: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let Ok(entries) = fs::read_dir(gtk4_dir()) else {
        return by_file;
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
        if stem == "gtk" {
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
            by_file.insert(stem, fns);
        }
    }
    by_file
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
    let by_file = parse_binding_fns();
    let used = parse_test_call_usage();

    let mut out = String::new();
    out.push_str("\n=== GTK 4 binding coverage ===\n");

    let mut total_fns = 0usize;
    let mut covered_fns = 0usize;

    for (file, fns) in &by_file {
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
            file,
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

fn gtk4_available() -> bool {
    if cfg!(target_os = "windows") {
        return false;
    }
    // Prefer pkg-config when present (it follows the distro's
    // configured search paths); fall back to probing the standard
    // install locations so machines without pkg-config still work
    // when libgtk-4 is dropped in by Homebrew / apt.
    let pc_ok = Command::new("pkg-config")
        .args(["--exists", "gtk4"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if pc_ok {
        return true;
    }
    let candidates: &[&str] = if cfg!(target_os = "macos") {
        &[
            "/opt/homebrew/lib/libgtk-4.dylib",
            "/opt/homebrew/lib/libgtk-4.1.dylib",
            "/usr/local/lib/libgtk-4.dylib",
            "/usr/local/lib/libgtk-4.1.dylib",
        ]
    } else {
        &[
            "/usr/lib/x86_64-linux-gnu/libgtk-4.so",
            "/usr/lib/x86_64-linux-gnu/libgtk-4.so.1",
            "/usr/lib/aarch64-linux-gnu/libgtk-4.so",
            "/usr/lib/aarch64-linux-gnu/libgtk-4.so.1",
            "/usr/lib64/libgtk-4.so",
            "/usr/lib64/libgtk-4.so.1",
            "/usr/lib/libgtk-4.so",
            "/usr/lib/libgtk-4.so.1",
        ]
    };
    candidates.iter().any(|p| std::path::Path::new(p).exists())
}

#[test]
fn run_gtk4_fixtures() {
    if !gtk4_available() {
        eprintln!(
            "skipping: pkg-config reports gtk4 unavailable on this host"
        );
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
