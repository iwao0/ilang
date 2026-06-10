//! Integration-test harness driven by `.il` fixture files under
//! `crates/ilang-cli/tests/programs/`.
//!
//! Each `.il` file declares its expected behaviour via magic comments
//! at the top of the file:
//!
//! ```text
//! // expect: 42
//! // expect: hello
//! // jit: skip          (optional — skip running through mir-jit)
//! // aot: skip          (optional — skip the AOT build / run arm)
//! // expect-error: division by zero   (failure case)
//! ```
//!
//! - `expect:` lines accumulate; the program's stdout (line-split) must
//!   match exactly, in order.
//! - `expect-error:` declares that the run must FAIL and the substring
//!   must appear somewhere in stderr.
//! - When the AOT arm is enabled (`ILANG_TEST_AOT=1`) its stdout is
//!   compared against the JIT stdout so divergence is caught even if
//!   both happen to pass `expect:` individually.
//!
//! Adding a new test = dropping a new `.il` file in `tests/programs/`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn ilang_bin() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_BIN_EXE_ilang"));
    p.pop();
    p.push("ilang");
    p
}

#[derive(Debug, Default)]
struct Spec {
    expect_lines: Vec<String>,
    expect_error: Option<String>,
    skip_jit: bool,
    skip_aot: bool,
}

fn parse_spec(src: &str) -> Spec {
    let mut spec = Spec::default();
    for line in src.lines() {
        let l = line.trim_start();
        let Some(body) = l.strip_prefix("//") else { continue };
        let body = body.trim_start();
        if let Some(rest) = body.strip_prefix("expect:") {
            spec.expect_lines.push(rest.trim().to_string());
        } else if let Some(rest) = body.strip_prefix("expect-error:") {
            spec.expect_error = Some(rest.trim().to_string());
        } else if body == "jit: skip" {
            spec.skip_jit = true;
        } else if body == "aot: skip" {
            spec.skip_aot = true;
        } else if body.is_empty() {
            // Blank comment line — keep scanning, the spec block can be
            // separated from the program by blank `//` lines.
        }
        // Everything else (regular doc comments, in-line notes) is
        // ignored. This keeps `// just describing what's tested` free.
    }
    spec
}

fn run(path: &Path) -> Output {
    let mut cmd = Command::new(ilang_bin());
    cmd.arg("run").arg("--mir-jit").arg(path);
    // Always opt children into the signal/exception crash reporter and
    // a forced Rust backtrace. When a fixture flakes under parallel
    // spawn, the parent harness needs every byte of diagnostic — both
    // are cheap when the child exits cleanly.
    cmd.env("ILANG_TRACE_CRASH", "1");
    cmd.env("RUST_BACKTRACE", "full");
    cmd.output().expect("failed to spawn ilang")
}

/// Compile via `ilang build` to a native executable in a temp dir,
/// then run the executable. Combines the two stages so the harness
/// can compare AOT stdout / stderr / exit-status against the other
/// backends. Returns an `Output` whose status / stderr reflects the
/// build step if it failed, or the program's run otherwise.
fn run_aot(path: &Path) -> Output {
    use std::sync::atomic::{AtomicU64, Ordering};
    static AOT_CTR: AtomicU64 = AtomicU64::new(0);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("ilang_fixture");
    let id = AOT_CTR.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let out_path = std::env::temp_dir()
        .join(format!("ilang_aot_test_{pid}_{id}_{stem}"));
    // Build stage. On failure, surface its Output as-is.
    let mut build = Command::new(ilang_bin());
    build.arg("build").arg(path).arg("-o").arg(&out_path);
    let build_out = build.output().expect("failed to spawn ilang build");
    if !build_out.status.success() {
        return build_out;
    }
    // Run the produced binary.
    let run_out = Command::new(&out_path)
        .output()
        .expect("failed to spawn AOT binary");
    // Best-effort cleanup; the linker drops a `<stem>.o` alongside.
    let _ = std::fs::remove_file(&out_path);
    let _ = std::fs::remove_file(out_path.with_extension("o"));
    run_out
}

fn check(spec: &Spec, out: &Output) -> Result<String, String> {
    if let Some(pat) = &spec.expect_error {
        if out.status.success() {
            return Err(format!("expected error containing {pat:?}, but command succeeded"));
        }
        let stderr = String::from_utf8_lossy(&out.stderr);
        if !stderr.contains(pat) {
            return Err(format!(
                "expected stderr to contain {pat:?}, got:\n{stderr}"
            ));
        }
        return Ok(String::new());
    }
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let code = out.status.code();
        #[cfg(unix)]
        let sig = {
            use std::os::unix::process::ExitStatusExt;
            out.status.signal()
        };
        #[cfg(not(unix))]
        let sig: Option<i32> = None;
        return Err(format!(
            "command failed (exit={code:?} signal={sig:?})\n\
             ---- stdout ({stdout_len} bytes) ----\n{stdout}\n\
             ---- stderr ({stderr_len} bytes) ----\n{stderr}",
            stdout_len = out.stdout.len(),
            stderr_len = out.stderr.len(),
        ));
    }
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let actual: Vec<&str> = stdout.lines().collect();
    let expected: Vec<&str> = spec.expect_lines.iter().map(|s| s.as_str()).collect();
    if actual != expected {
        return Err(format!(
            "output mismatch:\n  expected ({} line(s)):\n{}\n  actual ({} line(s)):\n{}",
            expected.len(),
            expected.iter().map(|l| format!("    {l}")).collect::<Vec<_>>().join("\n"),
            actual.len(),
            actual.iter().map(|l| format!("    {l}")).collect::<Vec<_>>().join("\n"),
        ));
    }
    Ok(stdout)
}

/// Recursively collect every `*.il` file under `dir`. Sorted to give
/// stable failure ordering across runs.
fn collect_il_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_il_files(&p, out);
        } else if p.extension().and_then(|s| s.to_str()) == Some("il") {
            out.push(p);
        }
    }
}

#[test]
fn run_all_program_fixtures() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs");
    let mut paths: Vec<PathBuf> = Vec::new();
    collect_il_files(&dir, &mut paths);
    paths.sort();

    if paths.is_empty() {
        // No fixtures yet — harness boot test still passes.
        return;
    }

    // AOT is gated behind `ILANG_TEST_AOT=1` — the per-fixture
    // build + link + run round-trip is ~50ms each, so leaving it on
    // by default adds tens of seconds to the harness. CI flips the
    // env var on; local runs stay snappy.
    let run_aot_arm = std::env::var_os("ILANG_TEST_AOT").is_some();

    let total = paths.len();
    let n_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let next_idx = std::sync::atomic::AtomicUsize::new(0);
    let failures: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

    std::thread::scope(|s| {
        for _ in 0..n_threads {
            let paths = &paths;
            let dir = &dir;
            let failures = &failures;
            let next_idx = &next_idx;
            s.spawn(move || {
                loop {
                    let i = next_idx.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if i >= paths.len() {
                        break;
                    }
                    let path = &paths[i];
                    let src = match fs::read_to_string(path) {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    let spec = parse_spec(&src);
                    let rel = path
                        .strip_prefix(dir)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|_| path.to_string_lossy().to_string());

                    let mut local_failures: Vec<String> = Vec::new();
                    let mut mir_stdout: Option<String> = None;
                    if !spec.skip_jit {
                        match check(&spec, &run(path)) {
                            Ok(out) => mir_stdout = Some(out),
                            Err(msg) => local_failures.push(format!("{rel} [mir-jit]: {msg}")),
                        }
                    }
                    let mut aot_stdout: Option<String> = None;
                    if run_aot_arm && !spec.skip_aot {
                        match check(&spec, &run_aot(path)) {
                            Ok(out) => aot_stdout = Some(out),
                            Err(msg) => local_failures.push(format!("{rel} [aot]: {msg}")),
                        }
                    }
                    if let (Some(i), Some(a)) = (&mir_stdout, &aot_stdout) {
                        if i != a {
                            local_failures.push(format!(
                                "{rel} [parity]: mir-jit and AOT diverge\n  mir-jit:\n{}\n  aot:\n{}",
                                i.lines().map(|l| format!("    {l}")).collect::<Vec<_>>().join("\n"),
                                a.lines().map(|l| format!("    {l}")).collect::<Vec<_>>().join("\n"),
                            ));
                        }
                    }
                    if !local_failures.is_empty() {
                        let mut g = failures.lock().expect("failures poisoned");
                        g.extend(local_failures);
                    }
                }
            });
        }
    });

    let failures = failures.into_inner().expect("failures poisoned");

    if !failures.is_empty() {
        panic!(
            "\n{}/{} fixture run(s) failed:\n\n{}\n",
            failures.len(),
            total * 2, // counted as interp + jit per file (rough)
            failures.join("\n\n")
        );
    }
}

// ─── unit tests for the harness itself ───────────────────────────────

#[test]
fn parse_spec_collects_expect_lines() {
    let src = "// expect: foo\n// expect: bar\n1 + 2\n";
    let spec = parse_spec(src);
    assert_eq!(spec.expect_lines, vec!["foo".to_string(), "bar".to_string()]);
    assert_eq!(spec.expect_error, None);
    assert!(!spec.skip_jit && !spec.skip_aot);
}

#[test]
fn parse_spec_recognizes_skip_directives() {
    let src = "// jit: skip\n// aot: skip\n// expect: x\n";
    let spec = parse_spec(src);
    assert!(spec.skip_jit);
    assert!(spec.skip_aot);
    assert_eq!(spec.expect_lines, vec!["x".to_string()]);
}

#[test]
fn parse_spec_recognizes_expect_error() {
    let src = "// expect-error: divide by zero\n1 / 0\n";
    let spec = parse_spec(src);
    assert_eq!(spec.expect_error.as_deref(), Some("divide by zero"));
    assert!(spec.expect_lines.is_empty());
}

#[test]
fn parse_spec_ignores_unrelated_comments() {
    let src = "// just a comment\n// expect: 7\n// another note\n3 + 4\n";
    let spec = parse_spec(src);
    assert_eq!(spec.expect_lines, vec!["7".to_string()]);
}

#[test]
fn parse_spec_strips_leading_whitespace() {
    let src = "    // expect: indented\n";
    let spec = parse_spec(src);
    assert_eq!(spec.expect_lines, vec!["indented".to_string()]);
}
