//! Host trampolines for the built-in `os` module:
//!
//! - `errno_check_i{32,64}`: turn a libc-style `rc < 0` failure
//!   indicator into an `Optional<i32/i64>` cell.
//! - `os.errno` / `os.setErrno`: read / write the process's libc
//!   errno slot.
//! - `os.libLoaded` / `os.libLoadError`: dlopen a named library on
//!   demand and report whether the open succeeded (plus the libdl
//!   error string on failure).
//!
//! `try_open_lib` / `try_open_lib_err` are the dlopen primitives
//! used by the `libLoaded` / `libLoadError` host helpers plus the
//! `@lib(...)` open pass in [`super::compile_with_builtins`].

use std::sync::{Mutex, OnceLock};

use ilang_runtime::{cstr_bytes, leak_cstring};

pub(super) extern "C" fn host_errno_check_i32(rc: i32) -> i64 {
    // Returns Optional<i32> as a heap cell: 0 = none, ptr = some(rc).
    if rc < 0 {
        return 0;
    }
    let cell = ilang_runtime::__mir_alloc(8) as *mut i32;
    unsafe {
        *cell = rc;
    }
    cell as i64
}

pub(super) extern "C" fn host_errno_check_i64(rc: i64) -> i64 {
    if rc < 0 {
        return 0;
    }
    let cell = ilang_runtime::__mir_alloc(8) as *mut i64;
    unsafe {
        *cell = rc;
    }
    cell as i64
}

/// `os.libLoaded(name)` — try to dlopen the library on demand and
/// remember whether it succeeded. The mir-codegen pipeline relies on
/// Cranelift JIT's process-wide symbol search, which always succeeds
/// for libc-provided names; for fallback libs declared via
/// `@lib("primary", "fallback")` we attempt each in turn so the
/// `os.libLoaded` query reflects reality.
pub(super) extern "C" fn host_os_lib_loaded(name: i64) -> i64 {
    let n = if name == 0 {
        return 0;
    } else {
        let bytes = unsafe { cstr_bytes(name) };
        String::from_utf8_lossy(bytes).into_owned()
    };
    // First, check whether `name` itself opens.
    if try_open_lib(&n).is_some() {
        return 1;
    }
    // Otherwise, check fallback groups: any `@lib(a, b, c)` group
    // containing `n` whose other entry opens counts as loaded.
    let registry = lib_groups_lock().lock().expect("lib groups poisoned");
    for group in registry.iter() {
        if !group.iter().any(|s| s.as_str() == n) {
            continue;
        }
        for alt in group {
            let s = alt.as_str();
            if s == n {
                continue;
            }
            if try_open_lib(s).is_some() {
                return 1;
            }
        }
    }
    0
}

pub(super) extern "C" fn host_os_lib_load_error(name: i64) -> i64 {
    let n = if name == 0 {
        return leak_cstring(String::new());
    } else {
        let bytes = unsafe { cstr_bytes(name) };
        String::from_utf8_lossy(bytes).into_owned()
    };
    match try_open_lib_err(&n) {
        Some(e) => leak_cstring(e),
        None => leak_cstring(String::new()),
    }
}

unsafe extern "C" {
    fn dlopen(path: *const u8, flags: i32) -> *mut u8;
    fn dlerror() -> *const u8;
}

static LIB_GROUPS: OnceLock<Mutex<Vec<Vec<ilang_ast::Symbol>>>> = OnceLock::new();

fn lib_groups_lock() -> &'static Mutex<Vec<Vec<ilang_ast::Symbol>>> {
    LIB_GROUPS.get_or_init(|| Mutex::new(Vec::new()))
}

const RTLD_LAZY: i32 = 1;

pub(super) fn try_open_lib(name: &str) -> Option<*mut u8> {
    let try_one = |n: &str| -> Option<*mut u8> {
        let mut nul = n.as_bytes().to_vec();
        nul.push(0);
        let h = unsafe { dlopen(nul.as_ptr(), RTLD_LAZY) };
        if h.is_null() { None } else { Some(h) }
    };
    if let Some(h) = try_one(name) {
        return Some(h);
    }
    // Bare name like "c" / "SDL2" — try OS-specific candidate
    // filenames and Homebrew install dirs (Apple Silicon
    // `/opt/homebrew`, Intel `/usr/local`) so user-installed libs
    // resolve out of the box. Mirrors the candidates the legacy
    // `crates/ilang-codegen/src/native_extern.rs` walks.
    if !name.contains('.') && !name.contains('/') {
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

fn try_open_lib_err(name: &str) -> Option<String> {
    let mut nul = name.as_bytes().to_vec();
    nul.push(0);
    let h = unsafe { dlopen(nul.as_ptr(), RTLD_LAZY) };
    if !h.is_null() {
        return None;
    }
    unsafe {
        let p = dlerror();
        if p.is_null() {
            return Some(format!("could not load `{name}`"));
        }
        let bytes = cstr_bytes(p as i64);
        Some(String::from_utf8_lossy(bytes).into_owned())
    }
}

pub(super) extern "C" fn host_os_errno() -> i32 {
    // Best-effort errno: read Rust's libc `errno`.
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

pub(super) extern "C" fn host_os_set_errno(code: i32) {
    #[cfg(target_os = "macos")]
    unsafe extern "C" {
        fn __error() -> *mut i32;
    }
    #[cfg(target_os = "linux")]
    unsafe extern "C" {
        fn __errno_location() -> *mut i32;
    }
    unsafe {
        #[cfg(target_os = "macos")]
        {
            *__error() = code;
        }
        #[cfg(target_os = "linux")]
        {
            *__errno_location() = code;
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            let _ = code;
        }
    }
}
