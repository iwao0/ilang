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

// Cross-platform errno setter. On Unix we write through the C
// runtime's per-thread errno location; on Windows we call
// `SetLastError`. Both compile down to a single store/call.
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

extern "C" fn os_set_errno(code: i32) {
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
    // Other platforms: silently no-op. The interpreter side does the
    // same so behavior stays consistent.
}

pub(crate) fn register_os_symbols(builder: &mut JITBuilder) {
    builder.symbol("os.errno", os_errno as *const u8);
    builder.symbol("os.setErrno", os_set_errno as *const u8);
}
