//! Build a `libilang_runtime.a` static archive next to the ilang
//! binary so the `ilang build` subcommand can link AOT objects
//! against it without the user having to run `cargo build -p
//! ilang-runtime` themselves.
//!
//! Cargo only emits a staticlib facet when something asks for it,
//! and ordinary rlib deps (which is what `ilang-cli`'s
//! `ilang-runtime` dependency resolves to) don't. We side-step the
//! whole crate-type negotiation by invoking `rustc` directly on
//! `ilang-runtime/src/lib.rs` here — the crate has no external deps
//! beyond `std`, so a single rustc call covers it.
//!
//! The archive lands at `target/<profile>/libilang_runtime.a` so
//! `locate_runtime_lib()` in `main.rs` (which looks next to
//! `current_exe()`) finds it.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let runtime_src = manifest_dir
        .join("..")
        .join("ilang-runtime")
        .join("src")
        .join("lib.rs");

    // OUT_DIR sits under `target/<profile>/build/ilang-cli-<hash>/out`.
    // Walk up to the `target/<profile>/` directory so the produced
    // archive ends up next to the eventual `ilang` binary, matching
    // what `locate_runtime_lib()` looks for.
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let profile_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("OUT_DIR shape (target/<profile>/build/<crate>/out)")
        .to_path_buf();
    let lib_path = profile_dir.join("libilang_runtime.a");

    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let edition = "2024";

    let mut cmd = Command::new(env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
    cmd.arg(&runtime_src)
        .args(["--crate-name", "ilang_runtime"])
        .args(["--crate-type", "staticlib"])
        .args(["--edition", edition])
        .args(["-o"])
        .arg(&lib_path);
    if profile == "release" {
        cmd.args(["-C", "opt-level=3"]);
    } else {
        cmd.args(["-C", "debuginfo=2"]);
    }
    // Suppress per-symbol warnings — the source already builds clean
    // through the normal `cargo build -p ilang-runtime` path, so
    // warnings here are noise.
    cmd.args(["--cap-lints", "allow"]);
    let status = cmd
        .status()
        .expect("invoking rustc for ilang-runtime staticlib");
    if !status.success() {
        panic!("rustc failed building libilang_runtime.a (status: {status:?})");
    }

    println!("cargo:rerun-if-changed={}", runtime_src.display());
}
