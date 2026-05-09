//! Integration-test harness driven by `.il` fixture files under
//! `crates/ilang-cli/tests/programs/`.
//!
//! Each `.il` file declares its expected behaviour via magic comments
//! at the top of the file:
//!
//! ```text
//! // expect: 42
//! // expect: hello
//! // jit: skip          (optional — skip running through the JIT)
//! // interp: skip       (optional — skip running through the interpreter)
//! // expect-error: division by zero   (failure case)
//! ```
//!
//! - `expect:` lines accumulate; the program's stdout (line-split) must
//!   match exactly, in order.
//! - `expect-error:` declares that the run must FAIL and the substring
//!   must appear somewhere in stderr.
//! - When both interpreter and JIT run, their stdouts are also compared
//!   to each other so divergence is caught even if both happen to pass
//!   the `expect:` block.
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
    skip_interp: bool,
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
        } else if body == "interp: skip" {
            spec.skip_interp = true;
        } else if body.is_empty() {
            // Blank comment line — keep scanning, the spec block can be
            // separated from the program by blank `//` lines.
        }
        // Everything else (regular doc comments, in-line notes) is
        // ignored. This keeps `// just describing what's tested` free.
    }
    spec
}

fn run(jit: bool, path: &Path) -> Output {
    let mut cmd = Command::new(ilang_bin());
    cmd.arg("run");
    if jit {
        // Legacy ilang-codegen path — kept as a parity reference
        // until ilang-eval is retired.
        cmd.arg("--jit");
    } else {
        // Default arm: the mir-jit pipeline. (Used to be the
        // tree-walking interpreter; switched here as part of M1
        // Step 6, leaving `ilang-eval` reachable only via the
        // explicit `--interp` flag.)
        cmd.arg("--mir-jit");
    }
    cmd.arg(path);
    cmd.output().expect("failed to spawn ilang")
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
        return Err(format!("command failed:\n{stderr}"));
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

    let mut failures: Vec<String> = Vec::new();
    let total = paths.len();
    for path in &paths {
        let src = fs::read_to_string(path).unwrap();
        let spec = parse_spec(&src);
        let rel = path
            .strip_prefix(&dir)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| path.to_string_lossy().to_string());

        // Default arm — mir-jit. (The harness directive
        // `// interp: skip` historically guarded the interpreter
        // arm; now it gates the default mir-jit run too.)
        let mut mir_stdout: Option<String> = None;
        if !spec.skip_interp {
            match check(&spec, &run(false, path)) {
                Ok(s) => mir_stdout = Some(s),
                Err(msg) => failures.push(format!("{rel} [mir-jit]: {msg}")),
            }
        }
        let mut jit_stdout: Option<String> = None;
        if !spec.skip_jit {
            match check(&spec, &run(true, path)) {
                Ok(s) => jit_stdout = Some(s),
                Err(msg) => failures.push(format!("{rel} [jit]: {msg}")),
            }
        }
        // Cross-check: when both backends ran successfully, their
        // stdouts must agree. Catches divergence even if both
        // happen to satisfy the spec individually.
        if let (Some(i), Some(j)) = (&mir_stdout, &jit_stdout) {
            if i != j {
                failures.push(format!(
                    "{rel} [parity]: mir-jit and legacy JIT diverge\n  mir-jit:\n{}\n  jit:\n{}",
                    i.lines().map(|l| format!("    {l}")).collect::<Vec<_>>().join("\n"),
                    j.lines().map(|l| format!("    {l}")).collect::<Vec<_>>().join("\n"),
                ));
            }
        }
    }

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
    assert!(!spec.skip_jit && !spec.skip_interp);
}

#[test]
fn parse_spec_recognizes_skip_directives() {
    let src = "// jit: skip\n// interp: skip\n// expect: x\n";
    let spec = parse_spec(src);
    assert!(spec.skip_jit);
    assert!(spec.skip_interp);
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
