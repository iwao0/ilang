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
