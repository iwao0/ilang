//! Built-in `@extern fn` host implementations. The interpreter
//! routes any extern fn call through `invoke_extern(name, args)`,
//! which looks up the name and runs the corresponding Rust function.
//!
//! Names match the qualified form produced by the loader (e.g.
//! `math.sin`), so collisions across modules are impossible.

use crate::value::Value;

/// Try to invoke an extern fn by qualified name. Returns `None` if
/// the name isn't registered (caller surfaces this as a runtime
/// error — it usually means the extern attribute was on a fn the
/// runtime doesn't know about, which is a stdlib/runtime mismatch).
pub fn invoke_extern(name: &str, args: &[Value]) -> Option<Value> {
    match name {
        // math — single-arg f64 → f64
        "math.sin" => Some(Value::Float(f1(args)?.sin())),
        "math.cos" => Some(Value::Float(f1(args)?.cos())),
        "math.tan" => Some(Value::Float(f1(args)?.tan())),
        "math.asin" => Some(Value::Float(f1(args)?.asin())),
        "math.acos" => Some(Value::Float(f1(args)?.acos())),
        "math.atan" => Some(Value::Float(f1(args)?.atan())),
        "math.sqrt" => Some(Value::Float(f1(args)?.sqrt())),
        "math.exp" => Some(Value::Float(f1(args)?.exp())),
        "math.ln" => Some(Value::Float(f1(args)?.ln())),
        "math.log10" => Some(Value::Float(f1(args)?.log10())),
        "math.log2" => Some(Value::Float(f1(args)?.log2())),
        "math.floor" => Some(Value::Float(f1(args)?.floor())),
        "math.ceil" => Some(Value::Float(f1(args)?.ceil())),
        "math.round" => Some(Value::Float(f1(args)?.round())),
        "math.abs" => Some(Value::Float(f1(args)?.abs())),
        // math — two-arg f64
        "math.atan2" => {
            let (y, x) = f2(args)?;
            Some(Value::Float(y.atan2(x)))
        }
        "math.pow" => {
            let (base, exp) = f2(args)?;
            Some(Value::Float(base.powf(exp)))
        }
        // test — assertion helpers. On failure each prints to stderr
        // and aborts with exit code 2 so the harness sees a non-zero
        // status independently of any other panic mechanism.
        "test.expect" => {
            if args.len() != 2 { return None; }
            let a = as_i64(&args[0])?;
            let e = as_i64(&args[1])?;
            if a != e { test_fail(&format!("expected {e}, got {a}")); }
            Some(Value::Unit)
        }
        "test.expectStr" => {
            if args.len() != 2 { return None; }
            let a = match &args[0] { Value::Str(s) => s.clone(), _ => return None };
            let e = match &args[1] { Value::Str(s) => s.clone(), _ => return None };
            if *a != *e { test_fail(&format!("expected {:?}, got {:?}", *e, *a)); }
            Some(Value::Unit)
        }
        "test.expectBool" => {
            if args.len() != 2 { return None; }
            let a = match &args[0] { Value::Bool(b) => *b, _ => return None };
            let e = match &args[1] { Value::Bool(b) => *b, _ => return None };
            if a != e { test_fail(&format!("expected {e}, got {a}")); }
            Some(Value::Unit)
        }
        "test.expectF64" => {
            if args.len() != 2 { return None; }
            let a = as_f64(&args[0])?;
            let e = as_f64(&args[1])?;
            if a != e { test_fail(&format!("expected {e}, got {a}")); }
            Some(Value::Unit)
        }
        "test.expectTrue" => {
            if args.len() != 1 { return None; }
            let c = match &args[0] { Value::Bool(b) => *b, _ => return None };
            if !c { test_fail("expected true, got false"); }
            Some(Value::Unit)
        }
        "test.expectFalse" => {
            if args.len() != 1 { return None; }
            let c = match &args[0] { Value::Bool(b) => *b, _ => return None };
            if c { test_fail("expected false, got true"); }
            Some(Value::Unit)
        }
        "test.fail" => {
            if args.len() != 1 { return None; }
            let msg = match &args[0] { Value::Str(s) => s.to_string(), _ => return None };
            test_fail(&msg);
            #[allow(unreachable_code)] Some(Value::Unit)
        }
        // `os.errno()` — current thread's errno (or `GetLastError`
        // on Windows). Same Rust impl as the JIT side.
        "os.errno" => {
            if !args.is_empty() { return None; }
            let n = std::io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or(0);
            Some(Value::Int32(n))
        }
        "os.setErrno" => {
            if args.len() != 1 { return None; }
            // Accept any integer width — the type checker enforces
            // the declared `i32` param, but const-inlined values
            // can arrive as untyped i64 literals.
            let code = as_i64(&args[0])? as i32;
            set_os_errno(code);
            Some(Value::Unit)
        }
        "os.libLoaded" => {
            // The interpreter has no `@extern("lib")` machinery —
            // native libraries are JIT-only — so this always
            // returns false. Programs run under the interpreter
            // hit the fallback branch of any `if os.libLoaded(...)`
            // guard, which is the safe default.
            if args.len() != 1 { return None; }
            let _ = match &args[0] { Value::Str(s) => s.clone(), _ => return None };
            Some(Value::Bool(false))
        }
        _ => None,
    }
}

/// Cross-platform errno write. Mirrors the JIT-side helper in
/// `crates/ilang-codegen/src/os_externs.rs` so interpreter and JIT
/// behave identically for `os.setErrno`.
#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn __errno_location() -> *mut i32;
}
#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn __error() -> *mut i32;
}
#[cfg(target_os = "windows")]
unsafe extern "system" {
    fn SetLastError(dwErrCode: u32);
}

fn set_os_errno(code: i32) {
    #[cfg(target_os = "linux")]
    unsafe {
        *__errno_location() = code;
    }
    #[cfg(target_os = "macos")]
    unsafe {
        *__error() = code;
    }
    #[cfg(target_os = "windows")]
    unsafe {
        SetLastError(code as u32);
    }
    // Other platforms: silent no-op (matches JIT side).
    let _ = code;
}

fn test_fail(msg: &str) -> ! {
    eprintln!("test assertion failed: {msg}");
    std::process::exit(2);
}

fn as_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Int(n) => Some(*n),
        Value::Int8(n) => Some(*n as i64),
        Value::Int16(n) => Some(*n as i64),
        Value::Int32(n) => Some(*n as i64),
        Value::UInt8(n) => Some(*n as i64),
        Value::UInt16(n) => Some(*n as i64),
        Value::UInt32(n) => Some(*n as i64),
        Value::UInt64(n) => Some(*n as i64),
        _ => None,
    }
}

fn f1(args: &[Value]) -> Option<f64> {
    if args.len() != 1 {
        return None;
    }
    as_f64(&args[0])
}

fn f2(args: &[Value]) -> Option<(f64, f64)> {
    if args.len() != 2 {
        return None;
    }
    Some((as_f64(&args[0])?, as_f64(&args[1])?))
}

fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Float(x) => Some(*x),
        Value::Float32(x) => Some(*x as f64),
        // Allow integer args to coerce — convenient for `math.sqrt(16)`
        // even though the declared signature is f64. The type checker
        // already accepts this via the int-literal-to-float rule.
        Value::Int(n) => Some(*n as f64),
        Value::Int8(n) => Some(*n as f64),
        Value::Int16(n) => Some(*n as f64),
        Value::Int32(n) => Some(*n as f64),
        Value::UInt8(n) => Some(*n as f64),
        Value::UInt16(n) => Some(*n as f64),
        Value::UInt32(n) => Some(*n as f64),
        Value::UInt64(n) => Some(*n as f64),
        _ => None,
    }
}

/// Names that the interpreter / JIT recognize as extern handlers.
/// Useful for the JIT side which needs to register Rust function
/// pointers during JIT module construction.
pub fn known_extern_names() -> &'static [&'static str] {
    &[
        "math.sin", "math.cos", "math.tan", "math.asin", "math.acos", "math.atan",
        "math.atan2", "math.sqrt", "math.pow", "math.exp", "math.ln", "math.log10",
        "math.log2", "math.floor", "math.ceil", "math.round", "math.abs",
        "test.expect", "test.expectStr", "test.expectBool", "test.expectF64",
        "test.expectTrue", "test.expectFalse", "test.fail",
        "os.errno", "os.setErrno", "os.libLoaded",
    ]
}
