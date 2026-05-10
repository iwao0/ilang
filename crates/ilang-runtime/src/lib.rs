//! Runtime support library linked into ilang AOT executables.
//!
//! Each `extern "C"` here is the AOT-side body for a host symbol the
//! generated `.o` references — for now, just the print helpers used by
//! `console.log(...)`. The set will grow alongside the AOT lowering.

use std::io::Write;

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
    let _ = write!(out, "{x}");
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

/// Runtime panic for AOT-emitted code. `msg` points at a
/// NUL-terminated C string laid out by the codegen as a data symbol.
/// Prints to stderr and aborts. Used for integer divide-by-zero (and,
/// later, out-of-bounds / unwrap-of-none).
#[unsafe(no_mangle)]
pub extern "C" fn __ilang_panic(msg: *const u8) -> ! {
    use std::io::Write;
    // Walk to NUL. Safe to read up to the first 0 byte since the
    // codegen always emits a terminating NUL after the message body.
    let mut len = 0usize;
    if !msg.is_null() {
        while unsafe { *msg.add(len) } != 0 {
            len += 1;
        }
    }
    let bytes = if msg.is_null() {
        b"panic"[..].to_vec()
    } else {
        unsafe { std::slice::from_raw_parts(msg, len) }.to_vec()
    };
    let mut err = std::io::stderr().lock();
    let _ = err.write_all(&bytes);
    let _ = err.write_all(b"\n");
    std::process::abort()
}
