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
    write_module(&dir, "utils", "pub fn double(n: i64): i64 { n * 2 }");
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
        "nums",
        "pub fn double(n: i64): i64 { n * 2 }\npub fn triple(n: i64): i64 { n * 3 }\npub fn quad(n: i64): i64 { n * 4 }",
    );
    let main = write_module(
        &dir,
        "main",
        "use nums { double, triple }\ndouble(5) + triple(5)",
    );
    let out = Command::new(ilang_bin()).arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "25");
}

#[test]
fn use_selective_through_export_chain() {
    // Selective import (`use M { X }`) walks `pub use` chains so
    // an umbrella module that re-exports an inner module's names can
    // still be selectively imported. The bare `Color` and the
    // umbrella-prefixed `umbrella.Color` reference the same enum, so
    // `paint` (typed `umbrella.Color`) accepts a bare `Color.red`.
    let dir = std::env::temp_dir().join(format!("ilang_sel_chain_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    write_module(
        &dir,
        "lib_inner",
        "pub enum Color: i32 { red = 1, green = 2, blue = 3 }",
    );
    write_module(
        &dir,
        "umbrella",
        // Flatten the re-export (`pub use M as _ { * }`) so the
        // inner module's `Color` lives at `umbrella.Color` — the
        // selective `use umbrella { Color }` below depends on that.
        // The plain `pub use lib_inner` form is namespaced and would
        // put `Color` at `umbrella.lib_inner.Color`.
        "pub use lib_inner as _ { * }\n\
         pub fn paint(c: Color): i32 { c as i32 }",
    );
    let main = write_module(
        &dir,
        "main",
        "use umbrella\n\
         use umbrella { Color }\n\
         umbrella.paint(Color.green)",
    );
    let out = Command::new(ilang_bin()).arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "2");
}

#[test]
fn use_selective_struct_inside_extern_c() {
    // `@extern(C) { struct S {} }` items count as exports too —
    // selective import should accept the struct, and references to
    // the bare name inside the importer's own `@extern(C)` block
    // should be rewritten to the prefixed `a.S` form so the type
    // checker resolves them.
    let dir = std::env::temp_dir().join(format!(
        "ilang_sel_extern_test_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    write_module(
        &dir,
        "a",
        "@extern(C) {\n\
             pub struct Pt {\n\
                 x: i32\n\
                 y: i32\n\
             }\n\
         }",
    );
    write_module(
        &dir,
        "b",
        "use a { Pt }\n\
         @extern(C) {\n\
             struct Pair {\n\
                 a: Pt\n\
                 b: Pt\n\
             }\n\
             fn make(): Pair {\n\
                 let p = new Pair()\n\
                 p\n\
             }\n\
         }\n\
         pub fn report(): i32 { make().a.x + make().b.y }",
    );
    let main = write_module(
        &dir,
        "main",
        "use b\nb.report()",
    );
    let out = Command::new(ilang_bin()).arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "0");
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
        "pub class Counter {\n  pub n: i64\n  pub init(start: i64) { this.n = start }\n  pub bump() { this.n = this.n + 1 }\n  pub get(): i64 { this.n }\n}",
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

#[test]
fn inherit_class_via_namespace() {
    let dir = std::env::temp_dir().join(format!("ilang_inherit_ns_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    write_module(&dir, "lib", "pub class Class3 {}");
    let main = write_module(
        &dir,
        "main",
        "use lib\nclass Class4: lib.Class3 {}\n0",
    );
    let out = Command::new(ilang_bin()).arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "0");
}

#[test]
fn use_builtin_math_module() {
    // No `math.il` written to disk — the loader should pick up the
    // shipped stdlib version.
    let dir = std::env::temp_dir().join(format!("ilang_math_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let main = write_module(&dir, "main", "use math\nmath.sqrt(16.0)");
    let out = Command::new(ilang_bin()).arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "4.0");
}

#[test]
fn use_builtin_math_jit() {
    let dir = std::env::temp_dir().join(format!("ilang_math_jit_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let main = write_module(
        &dir,
        "main",
        "use math\nlet p = math.pi\nmath.sin(p / 2.0)",
    );
    let out = Command::new(ilang_bin())
        .arg("run")
        .arg("--mir-jit")
        .arg(&main)
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "1.0");
}

#[test]
fn const_top_level_inlines() {
    let p = write_tmp(
        "const.il",
        "const TWO: i64 = 2\nfn double(n: i64): i64 { n * TWO }\ndouble(21)",
    );
    let out = Command::new(ilang_bin()).arg("run").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "42");
}

#[test]
fn const_module_qualified() {
    // `math.pi` resolves to the embedded `const pi: f64 = ...` and
    // is inlined at the use site — no parens needed.
    let dir = std::env::temp_dir().join(format!("ilang_const_pi_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let main = write_module(&dir, "main", "use math\nmath.pi");
    let out = Command::new(ilang_bin()).arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).trim().starts_with("3.14"));
}

#[test]
fn generic_fn_identity() {
    let p = write_tmp(
        "gen_id.il",
        "fn id<T>(x: T): T { x }\nid(42) + id(8)",
    );
    let out = Command::new(ilang_bin()).arg("run").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "50");
}

#[test]
fn generic_fn_infers_from_arg_type() {
    // The same generic fn binds T differently per call site.
    let p = write_tmp(
        "gen_str.il",
        "fn id<T>(x: T): T { x }\nid(\"hello\")",
    );
    let out = Command::new(ilang_bin()).arg("run").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hello");
}

#[test]
fn generic_fn_array_param() {
    let p = write_tmp(
        "gen_arr.il",
        "fn first<T>(xs: T[]): T { xs[0] }\nfirst([10, 20, 30])",
    );
    let out = Command::new(ilang_bin()).arg("run").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "10");
}

#[test]
fn generic_fn_two_params() {
    let p = write_tmp(
        "gen_pair.il",
        "fn first<A, B>(a: A, b: B): A { a }\nfirst(\"x\", 99)",
    );
    let out = Command::new(ilang_bin()).arg("run").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "x");
}

#[test]
fn generic_fn_type_mismatch_caught() {
    // `id` infers T = string, so binding to i64 must fail at type check.
    let p = write_tmp(
        "gen_mis.il",
        "fn id<T>(x: T): T { x }\nlet n: i64 = id(\"nope\")\nn",
    );
    let out = Command::new(ilang_bin()).arg("run").arg(&p).output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("type mismatch"), "stderr: {stderr}");
}

#[test]
fn generic_fn_jit_identity() {
    let p = write_tmp(
        "gen_jit.il",
        "fn id<T>(x: T): T { x }\nid(40) + id(2)",
    );
    let out = Command::new(ilang_bin())
        .arg("run")
        .arg("--mir-jit")
        .arg(&p)
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "42");
}

#[test]
fn generic_fn_jit_two_instantiations() {
    // The same `id<T>` is called with two different concrete types,
    // which must produce two distinct monomorphized fn bodies.
    let p = write_tmp(
        "gen_jit_two.il",
        "fn id<T>(x: T): T { x }\nlet n = id(7)\nconsole.log(id(\"hi\"))\nn",
    );
    let out = Command::new(ilang_bin())
        .arg("run")
        .arg("--mir-jit")
        .arg(&p)
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("hi"), "stdout: {stdout}");
    assert!(stdout.trim_end().ends_with("7"), "stdout: {stdout}");
}

#[test]
fn generic_fn_jit_nested_generic_call() {
    // `doit` (generic) calls `first` (generic) — the inner instantiation
    // is resolved against doit's bound T at monomorphization time.
    let p = write_tmp(
        "gen_jit_nest.il",
        "fn first<T>(xs: T[]): T { xs[0] }\nfn doit<T>(seed: T, ys: T[]): T { first(ys) }\ndoit(0, [10, 20, 30])",
    );
    let out = Command::new(ilang_bin())
        .arg("run")
        .arg("--mir-jit")
        .arg(&p)
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "10");
}

#[test]
fn jit_let_unit_loop() {
    // `let x = loop {...}` binds Unit. Used to error out with
    // "let value produces no value"; should now match the interpreter.
    let p = write_tmp(
        "unit_loop.il",
        "let x = loop { break }\n0",
    );
    let out = Command::new(ilang_bin())
        .arg("run")
        .arg("--mir-jit")
        .arg(&p)
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "0");
}

#[test]
fn jit_let_unit_call_and_chain() {
    // Unit-RHS via a Unit-returning fn call, plus `let y = x` to
    // exercise the unit-binding lookup path.
    let p = write_tmp(
        "unit_call.il",
        "let x = console.log(\"hi\")\nlet y = x\n0",
    );
    let out = Command::new(ilang_bin())
        .arg("run")
        .arg("--mir-jit")
        .arg(&p)
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("hi"), "stdout: {stdout}");
    assert!(stdout.trim_end().ends_with('0'), "stdout: {stdout}");
}

#[test]
fn jit_let_unit_if_for_while() {
    // Cover the remaining Unit-yielding shapes in one program.
    let p = write_tmp(
        "unit_misc.il",
        "let a = if true {} else {}\nlet b = while false {}\nlet c = for i in [1,2,3] {}\n0",
    );
    let out = Command::new(ilang_bin())
        .arg("run")
        .arg("--mir-jit")
        .arg(&p)
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "0");
}

#[test]
fn jit_map_basic_string_int() {
    let p = write_tmp(
        "map_basic.il",
        "let m = new Map<string, i64>()\nm.set(\"a\", 1)\nm.set(\"b\", 2)\nm[\"a\"] + m[\"b\"]",
    );
    let out = Command::new(ilang_bin())
        .arg("run")
        .arg("--mir-jit")
        .arg(&p)
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "3");
}

#[test]
fn jit_map_index_assign_and_overwrite() {
    let p = write_tmp(
        "map_assign.il",
        "let m = new Map<string, i64>()\nm[\"x\"] = 10\nm[\"x\"] = 100\nm[\"x\"] + m.size()",
    );
    let out = Command::new(ilang_bin())
        .arg("run")
        .arg("--mir-jit")
        .arg(&p)
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "101");
}

#[test]
fn jit_map_has_and_delete() {
    let p = write_tmp(
        "map_has.il",
        "let m = new Map<i64, i64>()\nm.set(1, 100)\nm.set(2, 200)\nm.delete(1)\nlet n = if m.has(2) { 1 } else { 0 }\nn + m.size()",
    );
    let out = Command::new(ilang_bin())
        .arg("run")
        .arg("--mir-jit")
        .arg(&p)
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "2");
}

#[test]
fn jit_map_string_value_arc() {
    // V = string exercises the per-Map value drop path: overwriting an
    // entry must release the previous string without leaking or
    // double-freeing.
    let p = write_tmp(
        "map_strv.il",
        "let m = new Map<string, string>()\nm.set(\"k\", \"first\")\nm.set(\"k\", \"second\")\nm[\"k\"]",
    );
    let out = Command::new(ilang_bin())
        .arg("run")
        .arg("--mir-jit")
        .arg(&p)
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "second");
}

#[test]
fn jit_map_object_value() {
    let p = write_tmp(
        "map_obj.il",
        "class C {\n  n: i64\n  init(v: i64) { this.n = v }\n  get(): i64 { this.n }\n}\nlet m = new Map<string, C>()\nm.set(\"x\", new C(42))\nm[\"x\"].get()",
    );
    let out = Command::new(ilang_bin())
        .arg("run")
        .arg("--mir-jit")
        .arg(&p)
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "42");
}

#[test]
fn jit_map_get_primitive_v_present() {
    let p = write_tmp(
        "map_get_prim.il",
        "let m = new Map<string, i64>()\nm.set(\"a\", 42)\nlet r = m.get(\"a\")\nif r.isSome { r.unwrap() } else { -1 }",
    );
    let out = Command::new(ilang_bin())
        .arg("run")
        .arg("--mir-jit")
        .arg(&p)
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "42");
}

#[test]
fn jit_map_get_primitive_v_missing() {
    let p = write_tmp(
        "map_get_prim_miss.il",
        "let m = new Map<string, i64>()\nm.set(\"a\", 1)\nlet r = m.get(\"z\")\nif r.isNone { -99 } else { r.unwrap() }",
    );
    let out = Command::new(ilang_bin())
        .arg("run")
        .arg("--mir-jit")
        .arg(&p)
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "-99");
}

#[test]
fn jit_map_get_heap_v_present() {
    let p = write_tmp(
        "map_get_h.il",
        "let m = new Map<string, string>()\nm.set(\"k\", \"v\")\nlet r = m.get(\"k\")\nif r.isSome { r.unwrap() } else { \"?\" }",
    );
    let out = Command::new(ilang_bin())
        .arg("run")
        .arg("--mir-jit")
        .arg(&p)
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "v");
}

#[test]
fn jit_map_get_heap_v_missing() {
    let p = write_tmp(
        "map_get_miss.il",
        "let m = new Map<string, string>()\nm.set(\"a\", \"1\")\nlet r = m.get(\"z\")\nif r.isNone { \"missing\" } else { r.unwrap() }",
    );
    let out = Command::new(ilang_bin())
        .arg("run")
        .arg("--mir-jit")
        .arg(&p)
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "missing");
}

#[test]
fn jit_map_keys_length() {
    let p = write_tmp(
        "map_keys.il",
        "let m = new Map<string, i64>()\nm.set(\"a\", 1)\nm.set(\"b\", 2)\nm.set(\"c\", 3)\nlet ks = m.keys()\nks.length",
    );
    let out = Command::new(ilang_bin())
        .arg("run")
        .arg("--mir-jit")
        .arg(&p)
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "3");
}

#[test]
fn jit_map_values_sum_i64() {
    let p = write_tmp(
        "map_values.il",
        "let m = new Map<string, i64>()\nm.set(\"a\", 10)\nm.set(\"b\", 20)\nm.set(\"c\", 30)\nlet vs = m.values()\nvs[0] + vs[1] + vs[2]",
    );
    let out = Command::new(ilang_bin())
        .arg("run")
        .arg("--mir-jit")
        .arg(&p)
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "60");
}

#[test]
fn jit_map_values_heap_v_length() {
    // Exercises the per-V retain helper: each string value gets an
    // extra +1 when copied into the result array.
    let p = write_tmp(
        "map_values_h.il",
        "let m = new Map<string, string>()\nm.set(\"a\", \"alpha\")\nm.set(\"b\", \"beta\")\nlet vs = m.values()\nvs.length",
    );
    let out = Command::new(ilang_bin())
        .arg("run")
        .arg("--mir-jit")
        .arg(&p)
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "2");
}

#[test]
fn jit_map_literal() {
    let p = write_tmp(
        "map_lit.il",
        "let m = { \"a\": 1, \"b\": 2, \"c\": 3 }\nm[\"a\"] + m[\"b\"] + m[\"c\"]",
    );
    let out = Command::new(ilang_bin())
        .arg("run")
        .arg("--mir-jit")
        .arg(&p)
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "6");
}

#[test]
fn jit_optional_primitive_i64_some() {
    let p = write_tmp(
        "opt_i64.il",
        "let x: i64? = some(42)\nif x.isSome { x.unwrap() } else { -1 }",
    );
    let out = Command::new(ilang_bin())
        .arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "42");
}

#[test]
fn jit_optional_primitive_i64_none() {
    let p = write_tmp(
        "opt_i64_n.il",
        "let x: i64? = none\nif x.isNone { 99 } else { x.unwrap() }",
    );
    let out = Command::new(ilang_bin())
        .arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "99");
}

#[test]
fn jit_optional_primitive_bool() {
    let p = write_tmp(
        "opt_bool.il",
        "let x: bool? = some(true)\nif x.isSome { x.unwrap() } else { false }",
    );
    let out = Command::new(ilang_bin())
        .arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "true");
}

#[test]
fn jit_optional_primitive_f64() {
    let p = write_tmp(
        "opt_f64.il",
        "let x: f64? = some(3.14)\nif x.isSome { x.unwrap() } else { 0.0 }",
    );
    let out = Command::new(ilang_bin())
        .arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "3.14");
}

#[test]
fn jit_optional_primitive_aliased_let() {
    // `let y = x` where x is a primitive Optional must retain the
    // box (each binding has its own +1).
    let p = write_tmp(
        "opt_alias.il",
        "let x: i64? = some(7)\nlet y = x\nif y.isSome { y.unwrap() } else { 0 }",
    );
    let out = Command::new(ilang_bin())
        .arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "7");
}

#[test]
fn jit_generic_enum_user_defined() {
    let p = write_tmp(
        "gen_enum.il",
        "enum Box<T> {\n  full: (T)\n  empty\n}\nlet b: Box<i64> = Box.full(42)\nmatch b {\n  full(v) { v }\n  empty { 0 }\n}",
    );
    let out = Command::new(ilang_bin())
        .arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "42");
}

#[test]
fn jit_result_ok() {
    let p = write_tmp(
        "result_ok.il",
        "let r: Result<i64, string> = Result.ok(42)\nmatch r {\n  ok(v) { v }\n  err(_) { -1 }\n}",
    );
    let out = Command::new(ilang_bin())
        .arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "42");
}

#[test]
fn jit_result_err() {
    let p = write_tmp(
        "result_err.il",
        "let r: Result<i64, string> = Result.err(\"boom\")\nmatch r {\n  ok(v) { v }\n  err(_) { -1 }\n}",
    );
    let out = Command::new(ilang_bin())
        .arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "-1");
}

#[test]
fn jit_result_via_function() {
    // Function returns Result; refinement pass propagates the
    // declared return type into both branches' enum-ctor sites.
    let p = write_tmp(
        "result_fn.il",
        "fn parse(s: string): Result<i64, string> {\n    if s == \"42\" { Result.ok(42) } else { Result.err(\"nope\") }\n}\nmatch parse(\"42\") {\n  ok(v) { v }\n  err(_) { -1 }\n}",
    );
    let out = Command::new(ilang_bin())
        .arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "42");
}

#[test]
fn jit_generic_enum_heap_payload() {
    let p = write_tmp(
        "either.il",
        "enum Either<L, R> {\n  left: (L)\n  right: (R)\n}\nlet e: Either<i64, string> = Either.right(\"hi\")\nmatch e {\n  left(_) { \"L\" }\n  right(s) { s }\n}",
    );
    let out = Command::new(ilang_bin())
        .arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hi");
}

#[test]
fn jit_array_pop_primitive() {
    let p = write_tmp(
        "pop_prim.il",
        "let xs: i64[] = [1, 2, 3]\nlet r = xs.pop()\nif r.isSome { r.unwrap() } else { -1 }",
    );
    let out = Command::new(ilang_bin()).arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "3");
}

#[test]
fn jit_array_pop_string() {
    let p = write_tmp(
        "pop_str.il",
        "let xs: string[] = [\"a\", \"b\"]\nlet r = xs.pop()\nif r.isSome { r.unwrap() } else { \"?\" }",
    );
    let out = Command::new(ilang_bin()).arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "b");
}

#[test]
fn jit_array_pop_empty_returns_none() {
    let p = write_tmp(
        "pop_empty.il",
        "let xs: i64[] = []\nlet r = xs.pop()\nif r.isNone { -1 } else { r.unwrap() }",
    );
    let out = Command::new(ilang_bin()).arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "-1");
}

#[test]
fn jit_array_remove_at_returns_element_and_shifts() {
    // removeAt drops the slot at the given index, shifts the tail
    // left by one, and returns the popped value wrapped in Optional.
    let p = write_tmp(
        "remove_at.il",
        "let xs: i64[] = [10, 20, 30, 40]\n\
         let r = xs.removeAt(1)\n\
         if r.isSome { r.unwrap() + xs.length * 1000 } else { -1 }",
    );
    let out = Command::new(ilang_bin()).arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    // Removed value (20) + 3 remaining * 1000 = 3020.
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "3020");
}

#[test]
fn jit_array_remove_at_out_of_range_returns_none() {
    let p = write_tmp(
        "remove_at_oob.il",
        "let xs: i64[] = [1, 2]\n\
         let r = xs.removeAt(99)\n\
         if r.isNone { xs.length } else { -1 }",
    );
    let out = Command::new(ilang_bin()).arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    // OOB index leaves the array untouched (length still 2).
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "2");
}

#[test]
fn jit_array_remove_first_match_only() {
    // remove drops the first equal cell and returns true; duplicates
    // further down stay in place.
    let p = write_tmp(
        "remove_first.il",
        "let xs: i64[] = [1, 2, 3, 2, 1]\n\
         let ok = xs.remove(2)\n\
         if ok { xs.indexOf(2) } else { -99 }",
    );
    let out = Command::new(ilang_bin()).arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    // After removing the first `2` (at index 1), the second `2` is
    // at index 2.
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "2");
}

#[test]
fn jit_array_remove_missing_returns_false() {
    let p = write_tmp(
        "remove_miss.il",
        "let xs: i64[] = [1, 2, 3]\n\
         if xs.remove(99) { -1 } else { xs.length }",
    );
    let out = Command::new(ilang_bin()).arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "3");
}

#[test]
fn jit_array_remove_string_releases_cell() {
    // Heap (string) elements: remove must release the dropped cell
    // so the program runs leak-free. We can't assert leak-free
    // directly from the CLI, but at minimum the program must succeed.
    let p = write_tmp(
        "remove_str.il",
        "let xs: string[] = [\"a\", \"b\", \"c\"]\n\
         let ok = xs.remove(\"b\")\n\
         if ok { xs.length } else { -1 }",
    );
    let out = Command::new(ilang_bin()).arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "2");
}

#[test]
fn jit_array_find_returns_some_on_match() {
    let p = write_tmp(
        "arr_find_hit.il",
        "let xs: i64[] = [1, 2, 3, 4]\n\
         let r = xs.find(fn(x: i64): bool { x == 3 })\n\
         if r.isSome { r.unwrap() } else { -1 }",
    );
    let out = Command::new(ilang_bin()).arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "3");
}

#[test]
fn jit_array_find_index_misses_with_negative_one() {
    let p = write_tmp(
        "arr_findidx_miss.il",
        "let xs: i64[] = [1, 2, 3]\n\
         xs.findIndex(fn(x: i64): bool { x == 99 })",
    );
    let out = Command::new(ilang_bin()).arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "-1");
}

#[test]
fn jit_array_every_some_combinations() {
    // `every` is vacuously true on empty arrays; `some` is false.
    // Mix both on a non-empty array to cover the typical branches.
    let p = write_tmp(
        "arr_every_some.il",
        "let xs: i64[] = [2, 4, 6]\n\
         let allEven = xs.every(fn(x: i64): bool { x % 2 == 0 })\n\
         let anyOdd = xs.some(fn(x: i64): bool { x % 2 == 1 })\n\
         if allEven && !anyOdd { 1 } else { 0 }",
    );
    let out = Command::new(ilang_bin()).arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "1");
}

#[test]
fn jit_array_concat_preserves_sources() {
    // `concat` must build a fresh array without disturbing the
    // two source arrays.
    let p = write_tmp(
        "arr_concat.il",
        "let a: i64[] = [1, 2]\n\
         let b: i64[] = [3, 4]\n\
         let c = a.concat(b)\n\
         c.length * 1000 + a.length * 100 + b.length * 10",
    );
    let out = Command::new(ilang_bin()).arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    // 4 in c, 2 in a, 2 in b → 4220.
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "4220");
}

#[test]
fn jit_array_reverse_returns_copy() {
    let p = write_tmp(
        "arr_reverse.il",
        "let a: i64[] = [1, 2, 3]\n\
         let r = a.reverse()\n\
         r.indexOf(1) * 100 + a.indexOf(1)",
    );
    let out = Command::new(ilang_bin()).arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    // 1 lands at index 2 in r, still at 0 in a → 200.
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "200");
}

#[test]
fn jit_array_join_strings() {
    let p = write_tmp(
        "arr_join.il",
        "let xs: string[] = [\"foo\", \"bar\", \"baz\"]\n\
         xs.join(\", \")",
    );
    let out = Command::new(ilang_bin()).arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "foo, bar, baz");
}

#[test]
fn jit_array_shift_unshift_round_trip() {
    // shift then unshift the same value should reproduce the
    // original layout.
    let p = write_tmp(
        "arr_shift_unshift.il",
        "let q: i64[] = [10, 20, 30]\n\
         let head = q.shift()\n\
         if head.isNone { -1 } else {\n\
             q.unshift(head.unwrap())\n\
             q.length * 100 + q.indexOf(10)\n\
         }",
    );
    let out = Command::new(ilang_bin()).arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    // length still 3, 10 back at index 0 → 300.
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "300");
}

#[test]
fn jit_array_fill_overwrites_all_cells() {
    let p = write_tmp(
        "arr_fill.il",
        "let xs: i64[] = [1, 2, 3, 4]\n\
         xs.fill(7)\n\
         xs.indexOf(1) + xs.indexOf(7)",
    );
    let out = Command::new(ilang_bin()).arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    // 1 is gone (-1), 7 is at 0 → -1 + 0 = -1.
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "-1");
}

#[test]
fn jit_array_sort_ascending() {
    let p = write_tmp(
        "arr_sort.il",
        "let xs: i64[] = [5, 2, 8, 1, 9, 3]\n\
         let s = xs.sort(fn(a: i64, b: i64): i64 { a - b })\n\
         s.indexOf(1) * 1000 + s.indexOf(9)",
    );
    let out = Command::new(ilang_bin()).arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    // 1 at index 0, 9 at index 5 → 5.
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "5");
}

#[test]
fn jit_array_indexof_string() {
    let p = write_tmp(
        "indexof_str.il",
        "let xs: string[] = [\"a\", \"b\", \"c\"]\nxs.indexOf(\"b\")",
    );
    let out = Command::new(ilang_bin()).arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "1");
}

#[test]
fn jit_array_includes_string_missing() {
    let p = write_tmp(
        "incl_str.il",
        "let xs: string[] = [\"a\", \"b\"]\nif xs.includes(\"z\") { 1 } else { 0 }",
    );
    let out = Command::new(ilang_bin()).arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "0");
}

#[test]
fn jit_for_in_string_array() {
    let p = write_tmp(
        "forin_str.il",
        "let xs: string[] = [\"a\", \"b\", \"c\"]\nfor x in xs { console.log(x) }\n0",
    );
    let out = Command::new(ilang_bin()).arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("a") && s.contains("b") && s.contains("c"), "stdout: {s}");
}

#[test]
fn jit_for_in_object_array() {
    let p = write_tmp(
        "forin_obj.il",
        "class C {\n  n: i64\n  init(v: i64) { this.n = v }\n}\nlet xs: C[] = [new C(10), new C(20), new C(30)]\nlet total = 0\nfor c in xs { total = total + c.n }\ntotal",
    );
    let out = Command::new(ilang_bin()).arg("run").arg("--mir-jit").arg(&p).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "60");
}

#[test]
fn array_map_filter_slice_interp_and_jit() {
    let src = "fn double(n: i64): i64 { n * 2 }\nfn isEven(n: i64): bool { n % 2 == 0 }\nlet xs: i64[] = [1, 2, 3, 4, 5]\nlet d = xs.map(double)\nlet e = xs.filter(isEven)\nlet s = xs.slice(1, 4)\nd[0] + d[4] + e[0] + e[1] + s[0] + s[1] + s[2]";
    for jit in [false, true] {
        let p = write_tmp(&format!("am_{jit}.il"), src);
        let mut cmd = Command::new(ilang_bin());
        cmd.arg("run");
        if jit {
            cmd.arg("--mir-jit");
        }
        let out = cmd.arg(&p).output().unwrap();
        assert!(out.status.success(), "jit={jit} stderr: {}", String::from_utf8_lossy(&out.stderr));
        // 2 + 10 + 2 + 4 + 2 + 3 + 4 = 27
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "27", "jit={jit}");
    }
}

#[test]
fn array_foreach_runs_callback() {
    // Use a string accumulator via a dedicated counter object. Side-
    // effects on a class field prove forEach iterates.
    // No closures — verify forEach iterates by mutating a per-element
    // counter object passed in by reference.
    let src = "class Counter {\n  n: i64\n  init() { this.n = 0 }\n  bump() { this.n = this.n + 1 }\n  get(): i64 { this.n }\n}\nfn bump(c: Counter) { c.bump() }\nlet cs: Counter[] = [new Counter(), new Counter(), new Counter()]\ncs.forEach(bump)\ncs[0].get() + cs[1].get() + cs[2].get()";
    for jit in [false, true] {
        let p = write_tmp(&format!("fe_{jit}.il"), src);
        let mut cmd = Command::new(ilang_bin());
        cmd.arg("run");
        if jit {
            cmd.arg("--mir-jit");
        }
        let out = cmd.arg(&p).output().unwrap();
        assert!(out.status.success(), "jit={jit} stderr: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "3", "jit={jit}");
    }
}

#[test]
fn array_string_map_filter_chain() {
    let src = "fn upper(s: string): string { s.toUpper() }\nfn nonempty(s: string): bool { s.length > 0 }\nlet xs: string[] = [\"\", \"hi\", \"\", \"yo\"]\nlet ys = xs.filter(nonempty).map(upper)\nys.length";
    for jit in [false, true] {
        let p = write_tmp(&format!("strchain_{jit}.il"), src);
        let mut cmd = Command::new(ilang_bin());
        cmd.arg("run");
        if jit {
            cmd.arg("--mir-jit");
        }
        let out = cmd.arg(&p).output().unwrap();
        assert!(out.status.success(), "jit={jit} stderr: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "2", "jit={jit}");
    }
}

#[test]
fn string_replace_all() {
    for jit in [false, true] {
        let p = write_tmp(&format!("rep_{jit}.il"), "\"hello hello\".replace(\"hello\", \"HI\")");
        let mut cmd = Command::new(ilang_bin());
        cmd.arg("run");
        if jit { cmd.arg("--mir-jit"); }
        let out = cmd.arg(&p).output().unwrap();
        assert!(out.status.success(), "jit={jit} stderr: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "HI HI", "jit={jit}");
    }
}

#[test]
fn string_split_then_index() {
    let src = "let parts = \"a,b,c,d\".split(\",\")\nparts[2]";
    for jit in [false, true] {
        let p = write_tmp(&format!("spl_{jit}.il"), src);
        let mut cmd = Command::new(ilang_bin());
        cmd.arg("run");
        if jit { cmd.arg("--mir-jit"); }
        let out = cmd.arg(&p).output().unwrap();
        assert!(out.status.success(), "jit={jit} stderr: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "c", "jit={jit}");
    }
}

#[test]
fn string_split_empty_separator() {
    let src = "\"abc\".split(\"\").length";
    for jit in [false, true] {
        let p = write_tmp(&format!("spe_{jit}.il"), src);
        let mut cmd = Command::new(ilang_bin());
        cmd.arg("run");
        if jit { cmd.arg("--mir-jit"); }
        let out = cmd.arg(&p).output().unwrap();
        assert!(out.status.success(), "jit={jit} stderr: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "3", "jit={jit}");
    }
}

#[test]
fn string_slice_basic() {
    let src = "\"hello world\".slice(6, 11)";
    for jit in [false, true] {
        let p = write_tmp(&format!("ssl_{jit}.il"), src);
        let mut cmd = Command::new(ilang_bin());
        cmd.arg("run");
        if jit { cmd.arg("--mir-jit"); }
        let out = cmd.arg(&p).output().unwrap();
        assert!(out.status.success(), "jit={jit} stderr: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "world", "jit={jit}");
    }
}

#[test]
fn string_slice_out_of_range_clamps() {
    let src = "\"hi\".slice(-5, 100)";
    for jit in [false, true] {
        let p = write_tmp(&format!("ssc_{jit}.il"), src);
        let mut cmd = Command::new(ilang_bin());
        cmd.arg("run");
        if jit { cmd.arg("--mir-jit"); }
        let out = cmd.arg(&p).output().unwrap();
        assert!(out.status.success(), "jit={jit} stderr: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hi", "jit={jit}");
    }
}
