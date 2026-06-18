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

// A bare trailing expression echoes its value for ANY type — not just
// i64. The old REPL printed only `__main`'s i64 return, so `"hello"` /
// `true` / `3.14` / arrays silently produced nothing.
#[test]
fn repl_bare_expr_echoes_all_types() {
    let out = repl(&[
        "42",
        "\"hello\"",
        "true",
        "3.14",
        "5 > 3",
        "[1, 2, 3]",
        "(1, 2)",
        "some(7)",
    ]);
    assert_eq!(
        out,
        vec!["42", "hello", "true", "3.14", "true", "[1, 2, 3]", "(1, 2)", "some(7)"]
    );
}

// A `console.log(x)` tail must not double-print (the wrap is
// `console.log(console.log(x))`, but the inner returns Unit and
// `console.log(())` prints nothing), and a statement-only chunk echoes
// nothing.
#[test]
fn repl_console_log_tail_not_doubled() {
    let out = repl(&["let x = 41", "console.log(x + 1)", "x + 1"]);
    assert_eq!(out, vec!["42", "42"]);
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

// ── Round 15: the REPL now runs the loader-equivalent normalize
// chain (enum-ref renormalize / @derive / const inlining / async
// desugar) and a fresh per-chunk TypeChecker over the merged
// program, with slot types seeded into both monomorphize passes.
// Before that: enums were unusable across chunks, `async fn` hit
// the legacy pre-state-machine error, `const` leaked Item::Const
// into MIR, generic-typed slots (Result / Box<i64>) silently failed
// to persist, and `use` died with a raw "unexpected Item::Use
// post-loader".

#[test]
fn repl_enum_across_chunks() {
    let out = repl(&[
        "enum E { a, b }",
        "let x = E.a",
        "console.log(match x { a { 10 }, b { 20 } })",
    ]);
    assert_eq!(out, vec!["10"]);
}

#[test]
fn repl_async_fn_across_chunks() {
    let out = repl(&[
        "async fn af(q: Promise<i64>): i64 { (await q) + 1 }",
        "let _ = af(Promise.resolve(9)).then(fn(v: i64) { console.log(v) })",
    ]);
    assert_eq!(out, vec!["10"]);
}

#[test]
fn repl_const_across_chunks() {
    let out = repl(&["const K: i64 = 40", "console.log(K + 2)"]);
    assert_eq!(out, vec!["42"]);
}

#[test]
fn repl_generic_enum_slot_persists() {
    let out = repl(&[
        "let r: Result<i64, string> = Result.ok(3)",
        "console.log(match r { ok(v) { v * 7 }, err(e) { 0 - 1 } })",
    ]);
    assert_eq!(out, vec!["21"]);
}

#[test]
fn repl_generic_class_slot_persists() {
    let out = repl(&[
        "class Box<T> { x: T; init(v: T) { this.x = v } }",
        "let bx = new Box<i64>(5)",
        "console.log(bx.x + 1)",
    ]);
    assert_eq!(out, vec!["6"]);
}

#[test]
fn repl_use_gets_friendly_error() {
    let out = repl_with_stderr(&["use std.time as time", "console.log(1)"]);
    assert!(
        out.1.contains("isn't supported in the REPL yet"),
        "stderr: {}",
        out.1
    );
    assert_eq!(out.0, vec!["1"]);
}

/// Like `repl`, but also returns stderr (for diagnostics checks).
fn repl_with_stderr(lines: &[&str]) -> (Vec<String>, String) {
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
    let stdout: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .skip(1)
        .map(|s| s.to_string())
        .collect();
    (stdout, String::from_utf8_lossy(&out.stderr).to_string())
}

// ── Round 16: re-`let` semantics. Same-type re-let overwrites the
// slot; a TYPE-CHANGING re-let is refused — accepting it stored the
// new value's bits into a slot the table still typed as the old
// one, and the next read reinterpreted them (a string re-let over
// an i64 slot printed its raw pointer).

#[test]
fn repl_relet_same_type_overwrites() {
    let out = repl(&[
        "let x = 1",
        "let x = 2",
        "console.log(x)",
        "let s = \"a\"",
        "let s = \"bb\"",
        "console.log(s.length)",
    ]);
    assert_eq!(out, vec!["2", "2"]);
}

#[test]
fn repl_relet_type_change_rejected() {
    let (out, err) = repl_with_stderr(&[
        "let x = 41",
        "let x = \"now-a-string\"",
        "console.log(x)",
    ]);
    assert!(
        err.contains("already bound with a different type"),
        "stderr: {err}"
    );
    // The slot keeps its original value — no raw-pointer print.
    assert_eq!(out, vec!["41"]);
}

#[test]
fn repl_derive_eq_hash_works() {
    let out = repl(&[
        "@derive(Eq, Hash) class P { x: i64; init(v: i64) { this.x = v } }",
        "let s = new Set<P>()",
        "s.add(new P(1))",
        "s.add(new P(1))",
        "console.log(s.size())",
    ]);
    assert_eq!(out, vec!["1"]);
}
