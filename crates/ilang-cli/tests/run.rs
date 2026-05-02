use std::io::Write;
use std::process::Command;

fn ilang_bin() -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_BIN_EXE_ilang"));
    p.pop();
    p.push("ilang");
    p
}

fn write_tmp(name: &str, content: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("ilang_test_{}_{name}", std::process::id()));
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(content.as_bytes()).unwrap();
    p
}

#[test]
fn run_simple_int() {
    let p = write_tmp("simple.il", "1 + 2 * 3\n");
    let out = Command::new(ilang_bin()).arg("run").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "7");
}

#[test]
fn run_float_promotion() {
    let p = write_tmp("float.il", "7.0 / 2");
    let out = Command::new(ilang_bin()).arg("run").arg(&p).output().unwrap();
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "3.5");
}

#[test]
fn run_division_by_zero_fails() {
    let p = write_tmp("div0.il", "1 / 0");
    let out = Command::new(ilang_bin()).arg("run").arg(&p).output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("division by zero"));
}

fn write_module(dir: &std::path::Path, name: &str, content: &str) -> std::path::PathBuf {
    let p = dir.join(format!("{name}.il"));
    std::fs::write(&p, content).unwrap();
    p
}

#[test]
fn use_whole_module_namespace() {
    let dir = std::env::temp_dir().join(format!("ilang_use_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    write_module(&dir, "utils", "fn double(n: i64): i64 { n * 2 }");
    let main = write_module(&dir, "main", "use utils\nutils.double(21)");
    let out = Command::new(ilang_bin()).arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "42");
}

#[test]
fn use_selective_import() {
    let dir = std::env::temp_dir().join(format!("ilang_sel_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    write_module(
        &dir,
        "math",
        "fn double(n: i64): i64 { n * 2 }\nfn triple(n: i64): i64 { n * 3 }\nfn quad(n: i64): i64 { n * 4 }",
    );
    let main = write_module(
        &dir,
        "main",
        "use math { double, triple }\ndouble(5) + triple(5)",
    );
    let out = Command::new(ilang_bin()).arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "25");
}

#[test]
fn use_circular_is_error() {
    let dir = std::env::temp_dir().join(format!("ilang_cyc_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    write_module(&dir, "a", "use b\nfn from_a(): i64 { 1 }");
    write_module(&dir, "b", "use a\nfn from_b(): i64 { 2 }");
    let main = write_module(&dir, "main", "use a\na.from_a()");
    let out = Command::new(ilang_bin()).arg("run").arg(&main).output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("circular import"));
}

#[test]
fn use_unknown_selective_name_is_error() {
    let dir = std::env::temp_dir().join(format!("ilang_bad_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    write_module(&dir, "utils", "fn double(n: i64): i64 { n * 2 }");
    let main = write_module(&dir, "main", "use utils { nope }\nnope()");
    let out = Command::new(ilang_bin()).arg("run").arg(&main).output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("nope"), "stderr: {stderr}");
}

#[test]
fn use_class_via_namespace() {
    let dir = std::env::temp_dir().join(format!("ilang_cls_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    write_module(
        &dir,
        "lib",
        "class Counter {\n  n: i64\n  init(start: i64) { this.n = start }\n  bump() { this.n = this.n + 1 }\n  get(): i64 { this.n }\n}",
    );
    let main = write_module(
        &dir,
        "main",
        "use lib\nlet c = new lib.Counter(10)\nc.bump()\nc.bump()\nc.get()",
    );
    let out = Command::new(ilang_bin()).arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "12");
}
