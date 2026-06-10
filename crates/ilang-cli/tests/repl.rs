//! Interactive-REPL integration tests: pipe a session into the
//! `ilang` binary (no args) and compare stdout after the banner.
//!
//! Heap-typed slot values must survive across chunks. __main's
//! epilogue used to release every heap slot at the end of EVERY
//! chunk (correct for file runs, where it makes deinits fire before
//! exit), so the next chunk read freed memory: `let arr = [1,2,3]`
//! then `arr[1]` on the following line was a SIGSEGV, `arr.length`
//! read 0, and an object's field read returned garbage. The REPL now
//! lowers chunks with `release_slots_at_exit = false`.

use std::io::Write;
use std::process::{Command, Stdio};

fn ilang_bin() -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_BIN_EXE_ilang"));
    p.pop();
    p.push("ilang");
    p
}

/// Run `lines` through the interactive REPL; return stdout lines
/// after the version banner.
fn repl(lines: &[&str]) -> Vec<String> {
    let mut child = Command::new(ilang_bin())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ilang repl");
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        for l in lines {
            writeln!(stdin, "{l}").expect("write line");
        }
    }
    let out = child.wait_with_output().expect("wait repl");
    assert!(
        out.status.success(),
        "repl exited with {:?}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .skip(1) // version banner
        .map(|s| s.to_string())
        .collect()
}

#[test]
fn repl_primitive_slots_round_trip() {
    let out = repl(&["let x = 41", "console.log(x + 1)"]);
    assert_eq!(out, vec!["42"]);
}

#[test]
fn repl_array_slot_survives_chunks() {
    let out = repl(&[
        "let arr: i64[] = [1, 2, 3]",
        "console.log(arr[1])",
        "console.log(arr.length)",
        "arr.push(9)",
        "console.log(arr[3])",
    ]);
    assert_eq!(out, vec!["2", "3", "9"]);
}

#[test]
fn repl_object_slot_survives_chunks() {
    let out = repl(&[
        "class Box { n: i64; init(x: i64) { this.n = x } }",
        "let b = new Box(5)",
        "console.log(b.n)",
        "let c = new Box(7)",
        "console.log(b.n + c.n)",
    ]);
    assert_eq!(out, vec!["5", "12"]);
}

#[test]
fn repl_map_slot_survives_chunks() {
    let out = repl(&[
        "let m = new Map<string, i64>()",
        "m[\"k\"] = 42",
        "console.log(m[\"k\"])",
        "m[\"k\"] = 43",
        "console.log(m[\"k\"])",
    ]);
    assert_eq!(out, vec!["42", "43"]);
}

#[test]
fn repl_string_slot_survives_chunks() {
    let out = repl(&[
        "let s = \"a\" + \"b\"",
        "console.log(s.length)",
        "console.log(s)",
    ]);
    assert_eq!(out, vec!["2", "ab"]);
}
