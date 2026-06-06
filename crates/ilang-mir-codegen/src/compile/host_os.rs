//! `try_open_lib`: the dlopen probe used by `compile_with_builtins`
//! when walking the `@lib(...)` groups of an `@extern(C)` block, so
//! the JIT side has a chance to load each declared shared library
//! up front.

#[cfg(not(windows))]
unsafe extern "C" {
    fn dlopen(path: *const u8, flags: i32) -> *mut u8;
}
#[cfg(not(windows))]
const RTLD_LAZY: i32 = 1;
// `RTLD_GLOBAL` differs per-platform. Required so the loaded library's
// symbols enter the global lookup scope and `dlsym(RTLD_DEFAULT, …)`
// — what `lookup_symbol_in_process` uses — can resolve them. Without
// it the dlopen succeeds but JIT-side symbol resolution still misses.
#[cfg(target_os = "linux")]
const RTLD_GLOBAL: i32 = 0x100;
#[cfg(target_os = "macos")]
const RTLD_GLOBAL: i32 = 0x8;
#[cfg(all(not(windows), not(target_os = "linux"), not(target_os = "macos")))]
const RTLD_GLOBAL: i32 = 0x100;

#[cfg(windows)]
unsafe extern "system" {
    fn LoadLibraryA(lpFileName: *const u8) -> *mut u8;
}

pub(super) fn try_open_lib(name: &str) -> Option<*mut u8> {
    let try_one = |n: &str| -> Option<*mut u8> {
        let mut nul = n.as_bytes().to_vec();
        nul.push(0);
        #[cfg(not(windows))]
        let h = unsafe { dlopen(nul.as_ptr(), RTLD_LAZY | RTLD_GLOBAL) };
        #[cfg(windows)]
        let h = unsafe { LoadLibraryA(nul.as_ptr()) };
        if h.is_null() { None } else { Some(h) }
    };
    if let Some(h) = try_one(name) {
        return Some(h);
    }
    // Bare name like "c" / "SDL2" / "webkitgtk-6.0" — try
    // OS-specific candidate filenames and Homebrew install dirs
    // (Apple Silicon `/opt/homebrew`, Intel `/usr/local`) so
    // user-installed libs resolve out of the box. Mirrors the
    // candidates the legacy `crates/ilang-codegen/src/native_extern.rs`
    // walks. Skip only when the name is already a full filename
    // (`.so`/`.dylib`/`.dll`) or path — a bare `.` in the name (e.g.
    // the `6.0` of a versioned soname stem) must still expand.
    let already_filename = name.contains('/')
        || name.contains(".so")
        || name.contains(".dylib")
        || name.contains(".dll");
    if !already_filename {
        let candidates: Vec<String> = if cfg!(target_os = "macos") {
            vec![
                format!("lib{name}.dylib"),
                format!("{name}.dylib"),
                format!("/opt/homebrew/lib/lib{name}.dylib"),
                format!("/opt/homebrew/lib/{name}.dylib"),
                format!("/usr/local/lib/lib{name}.dylib"),
                format!("/usr/local/lib/{name}.dylib"),
            ]
        } else if cfg!(target_os = "windows") {
            vec![format!("{name}.dll"), format!("lib{name}.dll")]
        } else {
            let mut out = vec![format!("lib{name}.so")];
            for n in [6, 5, 4, 3, 2, 1, 0] {
                out.push(format!("lib{name}.so.{n}"));
            }
            out
        };
        for cand in candidates {
            if let Some(h) = try_one(&cand) {
                return Some(h);
            }
        }
    }
    None
}
