//! `os.*` and FFI-side helpers: errno introspection, library-load
//! probing, optional-i32 / -i64 wrappers used by `@extern(C)` call
//! sites, and the no-op `freeCstr` identity (kept so existing ilang
//! bindings can call it like any other helper).

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use crate::alloc::__mir_alloc;
use crate::strings::{cstr_bytes, leak_cstring};

// --------------------------------------------------------------------
// `errnoCheck` family — wrap a syscall return into Optional<T>
// --------------------------------------------------------------------

/// Returns Optional<i32> as a heap cell: 0 = none, ptr = some(rc).
#[unsafe(export_name = "errnoCheck")]
pub extern "C" fn errno_check_i32(rc: i32) -> i64 {
    if rc < 0 {
        return 0;
    }
    let cell = __mir_alloc(8) as *mut i32;
    unsafe { *cell = rc; }
    cell as i64
}

#[unsafe(export_name = "errnoCheckI64")]
pub extern "C" fn errno_check_i64(rc: i64) -> i64 {
    if rc < 0 {
        return 0;
    }
    let cell = __mir_alloc(8) as *mut i64;
    unsafe { *cell = rc; }
    cell as i64
}

#[unsafe(export_name = "freeCstr")]
pub extern "C" fn free_cstr(_p: i64) {
    // No-op identity: ilang strings are registry-tracked and the
    // C-side never owns the buffer.
}

// --------------------------------------------------------------------
// `os.errno` / `os.setErrno`
// --------------------------------------------------------------------

#[unsafe(export_name = "$os.errno")]
pub extern "C" fn os_errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

#[unsafe(export_name = "$os.setErrno")]
pub extern "C" fn os_set_errno(code: i32) {
    #[cfg(target_os = "macos")]
    unsafe extern "C" {
        fn __error() -> *mut i32;
    }
    #[cfg(target_os = "linux")]
    unsafe extern "C" {
        fn __errno_location() -> *mut i32;
    }
    #[cfg(target_os = "macos")]
    unsafe { *__error() = code; }
    #[cfg(target_os = "linux")]
    unsafe { *__errno_location() = code; }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    { let _ = code; }
}

// --------------------------------------------------------------------
// `os.libLoaded` / `os.libLoadError` — `@lib(...)` fallback groups
// --------------------------------------------------------------------

#[cfg(not(windows))]
unsafe extern "C" {
    fn dlopen(path: *const u8, flags: i32) -> *mut u8;
    fn dlerror() -> *const u8;
}
#[cfg(not(windows))]
const RTLD_LAZY: i32 = 1;

#[cfg(windows)]
unsafe extern "system" {
    fn LoadLibraryA(lpFileName: *const u8) -> *mut u8;
    fn GetLastError() -> u32;
}

/// `@lib("primary", "fallback")` fallback groups. Each group is a
/// vector of library names declared on the same fn. Registered at
/// compile time so `os.libLoaded(name)` can fall through to
/// alternates declared on the same fn.
static LIB_GROUPS: OnceLock<Mutex<Vec<Vec<String>>>> = OnceLock::new();

fn lib_groups() -> &'static Mutex<Vec<Vec<String>>> {
    LIB_GROUPS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Append one `@lib(...)` group. The codegen calls this once per
/// extern fn whose lib list has more than one entry.
#[unsafe(no_mangle)]
pub extern "C" fn __register_lib_group_begin() -> i64 {
    let mut g = lib_groups().lock().expect("lib groups poisoned");
    g.push(Vec::new());
    (g.len() - 1) as i64
}

/// Append one name to the group started by `__register_lib_group_begin`.
#[unsafe(no_mangle)]
pub extern "C" fn __register_lib_group_member(group_idx: i64, name_str_ptr: i64) {
    let bytes = unsafe { cstr_bytes(name_str_ptr) };
    let name = String::from_utf8_lossy(bytes).into_owned();
    let mut g = lib_groups().lock().expect("lib groups poisoned");
    if let Some(grp) = g.get_mut(group_idx as usize) {
        grp.push(name);
    }
}

/// Best-effort cache of names we've successfully opened — avoids
/// repeated dlopen for the same library across many calls.
fn opened_cache() -> &'static Mutex<HashSet<String>> {
    static OPENED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    OPENED.get_or_init(|| Mutex::new(HashSet::new()))
}

fn try_open_lib(name: &str) -> bool {
    if opened_cache().lock().expect("opened cache poisoned").contains(name) {
        return true;
    }
    let try_one = |n: &str| -> bool {
        let mut nul = n.as_bytes().to_vec();
        nul.push(0);
        #[cfg(not(windows))]
        let h = unsafe { dlopen(nul.as_ptr(), RTLD_LAZY) };
        #[cfg(windows)]
        let h = unsafe { LoadLibraryA(nul.as_ptr()) };
        !h.is_null()
    };
    if try_one(name) {
        opened_cache()
            .lock()
            .expect("opened cache poisoned")
            .insert(name.to_string());
        return true;
    }
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
            if try_one(&cand) {
                opened_cache()
                    .lock()
                    .expect("opened cache poisoned")
                    .insert(name.to_string());
                return true;
            }
        }
    }
    false
}

#[unsafe(export_name = "$os.libLoaded")]
pub extern "C" fn os_lib_loaded(name: i64) -> i64 {
    if name == 0 {
        return 0;
    }
    let bytes = unsafe { cstr_bytes(name) };
    let n = String::from_utf8_lossy(bytes).into_owned();
    if try_open_lib(&n) {
        return 1;
    }
    // Check fallback groups containing `n`; any alternate that
    // opens counts as loaded.
    let groups = lib_groups().lock().expect("lib groups poisoned");
    for group in groups.iter() {
        if !group.iter().any(|s| s == &n) {
            continue;
        }
        for alt in group {
            if alt == &n {
                continue;
            }
            if try_open_lib(alt) {
                return 1;
            }
        }
    }
    0
}

/// Read a NUL-terminated C string into bytes — `dlerror` returns a
/// libc-owned `*const u8` without the ilang `[i64 len | …]` prefix,
/// so we walk for `\0` instead of peeking at offset -8.
#[cfg(not(windows))]
unsafe fn raw_c_str_bytes<'a>(p: *const u8) -> &'a [u8] {
    if p.is_null() {
        return &[];
    }
    unsafe {
        let mut len = 0usize;
        while *p.add(len) != 0 {
            len += 1;
        }
        std::slice::from_raw_parts(p, len)
    }
}

#[unsafe(export_name = "$os.libLoadError")]
pub extern "C" fn os_lib_load_error(name: i64) -> i64 {
    let n = if name == 0 {
        return leak_cstring(String::new());
    } else {
        let bytes = unsafe { cstr_bytes(name) };
        String::from_utf8_lossy(bytes).into_owned()
    };
    // Re-attempt open so we get a fresh error for this name.
    let mut nul = n.as_bytes().to_vec();
    nul.push(0);
    #[cfg(not(windows))]
    {
        let h = unsafe { dlopen(nul.as_ptr(), RTLD_LAZY) };
        if !h.is_null() {
            return leak_cstring(String::new());
        }
        unsafe {
            let p = dlerror();
            if p.is_null() {
                leak_cstring(format!("could not load `{n}`"))
            } else {
                let bytes = raw_c_str_bytes(p);
                leak_cstring(String::from_utf8_lossy(bytes).into_owned())
            }
        }
    }
    #[cfg(windows)]
    {
        let h = unsafe { LoadLibraryA(nul.as_ptr()) };
        if !h.is_null() {
            return leak_cstring(String::new());
        }
        let code = unsafe { GetLastError() };
        leak_cstring(format!("could not load `{n}` (error {code})"))
    }
}

// --------------------------------------------------------------------
// `os.platform`
// --------------------------------------------------------------------

/// Host OS name as an ilang `string`. One of `"macos"`, `"linux"`,
/// `"windows"`; for any other target Rust knows about we fall back
/// to `"other"` so user code can exhaustively branch with a single
/// catch-all arm. Resolved at compile time from `cfg(target_os)`,
/// so the cost is one allocated string per call (no syscall).
///
/// Exported under `os.__platform`; user code reaches the value
/// through the `pub let os.platform: string = __platform()`
/// binding declared in `stdlib/os.il`, so the call happens once
/// at program init and `os.platform` reads as a property.
// --------------------------------------------------------------------
// `@objc class : Parent` IMP lookup — bridges JIT-emitted methods to
// `class_addMethod`, which can't see JIT-compiled functions through
// the host dyld. The parser-generated `register()` body calls
// `__ilang_objc_imp_lookup(name)` instead of `dlsym(RTLD_DEFAULT)`;
// in JIT mode the entries are populated from `JITModule
// ::get_finalized_function`, and in AOT mode we fall back to dlsym so
// the exported `ilang_objc_imp__…` symbols already in the binary's
// symbol table still resolve.
// --------------------------------------------------------------------

fn imp_table() -> &'static Mutex<HashMap<String, usize>> {
    static T: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// JIT-side registration: called from `jit_setup` after
/// `finalize_definitions` once each IMP's address is known.
pub fn __register_objc_imp(name: String, addr: usize) {
    imp_table()
        .lock()
        .expect("imp table poisoned")
        .insert(name, addr);
}

#[cfg(not(windows))]
unsafe extern "C" {
    fn dlsym(handle: *mut u8, name: *const u8) -> *mut u8;
}
#[cfg(not(windows))]
const RTLD_DEFAULT: *mut u8 = -2isize as *mut u8;

/// Two-arg shape (handle, name) mirrors the `dlsym` signature the
/// parser-generated `register()` body uses; the handle is ignored —
/// we always search both the JIT-registered table and the host's
/// `RTLD_DEFAULT`.
#[unsafe(export_name = "__ilang_objc_imp_lookup")]
pub extern "C" fn __ilang_objc_imp_lookup(_handle: i64, name_ptr: i64) -> i64 {
    if name_ptr == 0 {
        return 0;
    }
    let bytes = unsafe { cstr_bytes(name_ptr) };
    let name = String::from_utf8_lossy(bytes).into_owned();
    if let Some(addr) = imp_table()
        .lock()
        .expect("imp table poisoned")
        .get(&name)
        .copied()
    {
        return addr as i64;
    }
    #[cfg(not(windows))]
    {
        let mut nul = name.as_bytes().to_vec();
        nul.push(0);
        let p = unsafe { dlsym(RTLD_DEFAULT, nul.as_ptr()) };
        p as i64
    }
    #[cfg(windows)]
    {
        let _ = name;
        0
    }
}

#[unsafe(export_name = "$os.platform")]
pub extern "C" fn os_platform() -> i64 {
    #[cfg(target_os = "macos")]
    let name = "macos";
    #[cfg(target_os = "linux")]
    let name = "linux";
    #[cfg(target_os = "windows")]
    let name = "windows";
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let name = "other";
    leak_cstring(name.to_string())
}
