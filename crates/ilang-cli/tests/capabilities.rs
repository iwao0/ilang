//! Capability enforcement (`ilang.toml` manifest).
//!
//! A program may use `std.fs` (`file`), `std.os` (`os`), or a user
//! `@extern(C)` block (`ffi`) only when the capability is granted by an
//! `ilang.toml` found by walking up from the entry file. A denied
//! capability aborts at runtime under `run` (JIT) and fails the build
//! under `build` (AOT). Trusted std infrastructure (`std.math` etc.)
//! needs no capability.

use std::path::PathBuf;
use std::process::Command;

fn ilang_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_ilang"))
}

/// A throwaway directory under the OS temp dir, unique per (pid, tag).
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ilang_cap_{}_{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &std::path::Path, name: &str, contents: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, contents).unwrap();
    p
}

const FS_PROG: &str = "use std.fs as fs\nlet _ = fs.exists(\"/tmp\")\n";

#[test]
fn fs_denied_without_file_cap_runtime() {
    let dir = scratch("fs_deny_run");
    write(&dir, "ilang.toml", "capabilities = [\"os\"]\n");
    let prog = write(&dir, "p.il", FS_PROG);
    let out = Command::new(ilang_bin()).arg("run").arg(&prog).output().unwrap();
    assert!(!out.status.success(), "expected a runtime capability error");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("capability 'file'"), "stderr: {err}");
}

#[test]
fn fs_denied_without_file_cap_aot() {
    let dir = scratch("fs_deny_build");
    write(&dir, "ilang.toml", "capabilities = [\"os\"]\n");
    let prog = write(&dir, "p.il", FS_PROG);
    let out = Command::new(ilang_bin())
        .arg("build")
        .arg(&prog)
        .arg("-o")
        .arg(dir.join("p.out"))
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected a build-time capability error");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("capability 'file'"), "stderr: {err}");
    // The compile error must fail the build before producing a binary.
    assert!(!dir.join("p.out").exists(), "binary should not be produced");
}

#[test]
fn fs_allowed_with_file_cap() {
    let dir = scratch("fs_grant");
    write(&dir, "ilang.toml", "capabilities = [\"file\"]\n");
    let prog = write(&dir, "p.il", FS_PROG);
    let out = Command::new(ilang_bin()).arg("run").arg(&prog).output().unwrap();
    assert!(
        out.status.success(),
        "granting file should run; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn no_manifest_denies() {
    // An entry under a directory with no ilang.toml anywhere above it
    // grants nothing. (The OS temp dir has no ilang.toml ancestor.)
    let dir = scratch("no_manifest");
    let prog = write(&dir, "p.il", FS_PROG);
    let out = Command::new(ilang_bin()).arg("run").arg(&prog).output().unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("capability 'file'"), "stderr: {err}");
}

#[test]
fn pure_program_needs_no_capability() {
    // No std sinks, no FFI — runs with no manifest at all.
    let dir = scratch("pure");
    let prog = write(&dir, "p.il", "console.log((1 + 2).toString())\n");
    let out = Command::new(ilang_bin()).arg("run").arg(&prog).output().unwrap();
    assert!(
        out.status.success(),
        "pure program should run with no manifest; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "3");
}

#[test]
fn unknown_capability_is_an_error() {
    let dir = scratch("unknown_cap");
    write(&dir, "ilang.toml", "capabilities = [\"bogus\"]\n");
    let prog = write(&dir, "p.il", "console.log(\"hi\")\n");
    let out = Command::new(ilang_bin()).arg("run").arg(&prog).output().unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("unknown capability"), "stderr: {err}");
}
