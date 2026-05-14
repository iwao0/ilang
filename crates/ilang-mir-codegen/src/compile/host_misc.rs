//! Small leaf host helpers that don't belong in any larger group:
//! the `@extern(C) @optional` missing-symbol stub and the dlsym
//! probe used to decide whether a JIT-side native fn resolves.

#[cfg(not(windows))]
unsafe extern "C" {
    fn dlsym(handle: *mut u8, name: *const u8) -> *mut u8;
}

// `RTLD_DEFAULT` differs by platform: macOS uses (-2 as *mut u8),
// Linux uses NULL. Each target picks the right sentinel.
#[cfg(target_os = "macos")]
const RTLD_DEFAULT: *mut u8 = -2isize as *mut u8;
#[cfg(all(not(target_os = "macos"), not(windows)))]
const RTLD_DEFAULT: *mut u8 = std::ptr::null_mut();

// Windows: EnumProcessModules + GetProcAddress replaces dlsym(RTLD_DEFAULT).
// EnumProcessModules is in Kernel32.dll on Windows Vista+.
#[cfg(windows)]
unsafe extern "system" {
    fn GetCurrentProcess() -> *mut u8;
    fn GetProcAddress(hModule: *mut u8, lpProcName: *const u8) -> *mut u8;
    fn EnumProcessModules(
        hProcess: *mut u8,
        lphModule: *mut *mut u8,
        cb: u32,
        lpcbNeeded: *mut u32,
    ) -> i32;
}

pub(super) fn process_symbol_exists(name: &str) -> bool {
    let mut nul = name.as_bytes().to_vec();
    nul.push(0);
    #[cfg(not(windows))]
    {
        let p = unsafe { dlsym(RTLD_DEFAULT, nul.as_ptr()) };
        !p.is_null()
    }
    #[cfg(windows)]
    unsafe {
        let proc = GetCurrentProcess();
        let mut modules = vec![std::ptr::null_mut::<u8>(); 1024];
        let mut needed: u32 = 0;
        let ok = EnumProcessModules(
            proc,
            modules.as_mut_ptr(),
            (modules.len() * std::mem::size_of::<*mut u8>()) as u32,
            &mut needed,
        );
        if ok == 0 {
            return false;
        }
        let count = (needed as usize) / std::mem::size_of::<*mut u8>();
        modules[..count]
            .iter()
            .any(|&m| !m.is_null() && !GetProcAddress(m, nul.as_ptr()).is_null())
    }
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
