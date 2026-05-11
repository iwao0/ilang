//! Small leaf host helpers that don't belong in any larger group:
//! the `@extern(C) @optional` missing-symbol stub and the dlsym
//! probe used to decide whether a JIT-side native fn resolves.

unsafe extern "C" {
    fn dlsym(handle: *mut u8, name: *const u8) -> *mut u8;
}

// `RTLD_DEFAULT` differs by platform: macOS uses (-2 as *mut u8),
// Linux uses NULL. Each target picks the right sentinel.
#[cfg(target_os = "macos")]
const RTLD_DEFAULT: *mut u8 = -2isize as *mut u8;
#[cfg(not(target_os = "macos"))]
const RTLD_DEFAULT: *mut u8 = std::ptr::null_mut();

pub(super) fn process_symbol_exists(name: &str) -> bool {
    let mut nul = name.as_bytes().to_vec();
    nul.push(0);
    let p = unsafe { dlsym(RTLD_DEFAULT, nul.as_ptr()) };
    !p.is_null()
}

/// Stub for `@extern(C) @optional` fns whose lib / symbol couldn't
/// be resolved. Aborts if called; user code is expected to gate
/// via `os.libLoaded(...)`.
pub(super) extern "C" fn host_optional_missing_stub() -> ! {
    eprintln!(
        "panic: invoked an `@extern(C) @optional` fn whose library was not loaded"
    );
    std::process::exit(1);
}
