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
    // Match the JIT's display rule: append `.0` when the value has no
    // fractional part (so `3.0` doesn't print as the integer-looking
    // `3`). NaN / ±∞ go through Display unchanged.
    if x.fract() == 0.0 && x.is_finite() {
        let _ = write!(out, "{x:.1}");
    } else {
        let _ = write!(out, "{x}");
    }
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

/// Runtime panic shared by JIT and AOT. `msg` points at the start of
/// the user-visible bytes of an ilang string laid out as
/// `[ i64 length | bytes | \0 ]`. The 8-byte length prefix sits at
/// `msg - 8`, matching the format the codegen emits for string
/// literals and panic messages.
///
/// Prints to stderr (with a trailing newline) and exits the process.
/// Used for integer divide-by-zero / modulo-by-zero today; other
/// halting checks (OOB, unwrap-None) will reuse this entry point.
#[unsafe(no_mangle)]
pub extern "C" fn __ilang_panic(msg: i64) -> ! {
    use std::io::Write;
    let bytes: &[u8] = if msg == 0 {
        b"panic"
    } else {
        // SAFETY: callers (both JIT host wiring and AOT codegen) emit
        // the `[len | bytes | NUL]` layout described above. `msg` is
        // the address of the first byte after the length prefix.
        unsafe {
            let len_ptr = (msg - 8) as *const i64;
            let len = *len_ptr;
            if len <= 0 {
                &[]
            } else {
                std::slice::from_raw_parts(msg as *const u8, len as usize)
            }
        }
    };
    let mut err = std::io::stderr().lock();
    let _ = err.write_all(bytes);
    let _ = err.write_all(b"\n");
    std::process::exit(1)
}
