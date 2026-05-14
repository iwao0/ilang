//! Build a `libilang_runtime.a` static archive next to the ilang
//! binary so the `ilang build` subcommand can link AOT objects
//! against it without the user having to run `cargo build -p
//! ilang-runtime` themselves.
//!
//! Internally we invoke `cargo build -p ilang-runtime` into a
//! private target directory under `OUT_DIR`, then copy the
//! resulting staticlib next to `ilang`. The private target-dir
//! avoids fighting the outer cargo invocation for the workspace
//! lock and side-steps re-linking concerns. The runtime crate's
//! `Cargo.toml` already declares `crate-type = ["rlib", "staticlib"]`,
//! so cargo emits the archive directly.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let runtime_dir = manifest_dir.join("..").join("ilang-runtime");
    let runtime_manifest = runtime_dir.join("Cargo.toml");

    // OUT_DIR is `target/<profile>/build/ilang-cli-<hash>/out`. Walk
    // up three levels to get to `target/<profile>/`, where the
    // eventual `ilang` binary will land and where
    // `locate_runtime_lib()` looks for the archive.
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let profile_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("OUT_DIR shape (target/<profile>/build/<crate>/out)")
        .to_path_buf();
    let lib_filename = if cfg!(windows) { "ilang_runtime.lib" } else { "libilang_runtime.a" };
    let dest = profile_dir.join(lib_filename);

    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".into());

    // Build into a sub-target dir so the recursive cargo invocation
    // doesn't trip on the outer workspace's target-dir lock.
    let sub_target = out_dir.join("runtime-target");

    let mut cmd = Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    cmd.arg("build")
        .arg("--manifest-path")
        .arg(&runtime_manifest)
        .arg("--target-dir")
        .arg(&sub_target)
        .args(["-p", "ilang-runtime"]);
    if profile == "release" {
        cmd.arg("--release");
    }
    // Quiet the recursive cargo output so the outer build log stays
    // readable; errors still surface via the non-zero status.
    cmd.arg("--quiet");
    let status = cmd
        .status()
        .expect("invoking cargo for ilang-runtime staticlib");
    if !status.success() {
        panic!("cargo build of ilang-runtime failed (status: {status:?})");
    }

    let source = sub_target.join(&profile).join(lib_filename);
    fs::copy(&source, &dest).unwrap_or_else(|e| {
        panic!(
            "copying {:?} → {:?} failed: {e}",
            source, dest
        )
    });

    // Watch the entire `ilang-runtime/src/` tree + its Cargo.toml so
    // edits to helper files or deps trigger a rebuild.
    let runtime_src_dir = runtime_dir.join("src");
    println!("cargo:rerun-if-changed={}", runtime_src_dir.display());
    println!("cargo:rerun-if-changed={}", runtime_manifest.display());
}
