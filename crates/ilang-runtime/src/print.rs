//! Print primitives and panic. Used by `console.log` and friends.
//! Also hosts the closure / weak printers and the fn-name registry
//! that backs `__print_fn`.

use std::collections::HashMap;
use std::io::Write;
use std::sync::{Mutex, OnceLock};

use crate::strings::{cstr_bytes, cstr_to_str};

#[unsafe(no_mangle)]
pub extern "C" fn __print_int(n: i64) {
    let mut out = std::io::stdout().lock();
    let _ = write!(out, "{n}");
}

#[unsafe(no_mangle)]
pub extern "C" fn __print_bool(b: i64) {
    let mut out = std::io::stdout().lock();
    let _ = if b != 0 {
        write!(out, "true")
    } else {
        write!(out, "false")
    };
}

#[unsafe(no_mangle)]
pub extern "C" fn __print_f64(x: f64) {
    let mut out = std::io::stdout().lock();
    if x.fract() == 0.0 && x.is_finite() {
        let _ = write!(out, "{x:.1}");
    } else {
        let _ = write!(out, "{x}");
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __print_str(p: i64) {
    let bytes = unsafe { cstr_bytes(p) };
    if bytes.is_empty() {
        return;
    }
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(bytes);
}

#[unsafe(no_mangle)]
pub extern "C" fn __print_space() {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(b" ");
}

#[unsafe(no_mangle)]
pub extern "C" fn __print_newline() {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(b"\n");
}

// --------------------------------------------------------------------
// Panic
// --------------------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn __ilang_panic(msg: i64) -> ! {
    let bytes = if msg == 0 {
        b"panic" as &[u8]
    } else {
        unsafe { cstr_bytes(msg) }
    };
    let mut err = std::io::stderr().lock();
    let _ = err.write_all(bytes);
    let _ = err.write_all(b"\n");
    std::process::exit(1)
}

// --------------------------------------------------------------------
// Weak / fn printers
// --------------------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn __print_weak(weak_ptr: i64) {
    let mut out = std::io::stdout().lock();
    if weak_ptr == 0 {
        let _ = out.write_all(b"weak(<dead>)");
        return;
    }
    let rc = unsafe { *((weak_ptr + 8) as *const i64) };
    if rc <= 0 {
        let _ = out.write_all(b"weak(<dead>)");
    } else {
        let _ = out.write_all(b"weak(<alive>)");
    }
}

static FN_NAME_TABLE: OnceLock<Mutex<HashMap<i64, String>>> = OnceLock::new();

fn fn_name_table() -> &'static Mutex<HashMap<i64, String>> {
    FN_NAME_TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __register_fn_name(fn_addr: i64, name_str_ptr: i64) {
    let name = cstr_to_str(name_str_ptr).to_string();
    fn_name_table()
        .lock()
        .expect("fn name table poisoned")
        .insert(fn_addr, name);
}

#[unsafe(no_mangle)]
pub extern "C" fn __print_fn(closure_ptr: i64) {
    let mut out = std::io::stdout().lock();
    if closure_ptr == 0 {
        let _ = out.write_all(b"<fn>");
        return;
    }
    let fn_addr = unsafe { *(closure_ptr as *const i64) };
    let t = fn_name_table().lock().expect("fn name table poisoned");
    match t.get(&fn_addr) {
        Some(name) => {
            let _ = write!(out, "<fn {name}>");
        }
        None => {
            let _ = out.write_all(b"<fn>");
        }
    }
}
