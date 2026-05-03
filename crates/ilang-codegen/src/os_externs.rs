//! Host-side implementations for the `os` stdlib module's `@extern fn`s.
//! Registered with the JITBuilder so JITed code can call them under
//! their qualified names (e.g. `os.errno`).

use cranelift_jit::JITBuilder;

extern "C" fn os_errno() -> i32 {
    // `std::io::Error::last_os_error` reads the current thread's
    // errno on Unix and `GetLastError()` on Windows. `raw_os_error`
    // returns `Option<i32>` — `None` would mean the platform didn't
    // expose an OS error code, which we surface as 0.
    std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or(0)
}

pub(crate) fn register_os_symbols(builder: &mut JITBuilder) {
    builder.symbol("os.errno", os_errno as *const u8);
}
