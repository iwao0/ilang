//! Small leaf host trampolines that don't belong in any larger
//! group: closure / weak printers, the `typeof` / `enum-as-string`
//! cast helper, the `@extern(C) @optional` missing-symbol stub, and
//! the `cstrArrayToStrings` / `stringFromCstr` FFI bridges.
//!
//! Also hosts the small in-process registries those helpers consult
//! (`FN_NAME_TABLE`, `ENUM_INFO`).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use ilang_runtime::leak_cstring;

use super::{host_array::raw_cstr_bytes, PrintKind};

pub(super) static FN_NAME_TABLE: OnceLock<Mutex<HashMap<i64, String>>> = OnceLock::new();

pub(super) fn fn_name_lock() -> &'static Mutex<HashMap<i64, String>> {
    FN_NAME_TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) extern "C" fn host_print_fn(closure_ptr: i64) {
    if closure_ptr == 0 {
        print!("<fn>");
        return;
    }
    let fn_addr = unsafe { *(closure_ptr as *const i64) };
    let m = fn_name_lock().lock().expect("fn name table poisoned");
    if let Some(name) = m.get(&fn_addr) {
        print!("<fn {}>", name);
    } else {
        print!("<fn>");
    }
}

#[derive(Clone)]
pub(super) struct EnumPrintInfo {
    pub(super) name: String,
    /// discriminant → (variant_name, payload_kinds)
    pub(super) variants: HashMap<i64, (String, Vec<PrintKind>)>,
    /// Discriminant strings for `: string`-repr enums, keyed by
    /// the variant's integer tag (the declaration index used by
    /// `EnumTag`). `None` for the usual integer-repr enums; the
    /// tag → string lookup powers `enum-as-string` casts.
    pub(super) str_repr: Option<HashMap<i64, String>>,
}

pub(super) static ENUM_INFO: OnceLock<Mutex<HashMap<u32, EnumPrintInfo>>> = OnceLock::new();

pub(super) fn enum_info_lock() -> &'static Mutex<HashMap<u32, EnumPrintInfo>> {
    ENUM_INFO.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) extern "C" fn host_print_weak(weak_ptr: i64) {
    if weak_ptr == 0 {
        print!("weak(<dead>)");
        return;
    }
    let rc = unsafe { *((weak_ptr + 8) as *const i64) };
    if rc <= 0 {
        print!("weak(<dead>)");
    } else {
        print!("weak(<alive>)");
    }
}

/// The alloc trackers feeding `test.liveAlloc*()` introspection live
/// inside `ilang-runtime`; the test helpers below just forward.
pub(super) extern "C" fn host_test_live_alloc_bytes() -> i64 {
    ilang_runtime::live_alloc_bytes()
}

pub(super) extern "C" fn host_test_live_alloc_count() -> i64 {
    ilang_runtime::live_alloc_count()
}

/// `test.liveStringCount(): i64` — number of entries currently in
/// the rc-tracked string registry (leak_cstring buffers). Catches
/// `intToStr` / `str_concat` / `getError` etc. temps that should
/// have been released after their consumer ran.
pub(super) extern "C" fn host_test_live_string_count() -> i64 {
    ilang_runtime::live_string_count()
}

/// Cast helper for `enum-value as string` on `: string`-repr
/// enums. Look the enum up by global id, find the variant whose
/// integer tag matches `disc`, and return a fresh `StringRc *`
/// for that variant's declared discriminant string. The caller
/// owns the returned +1 ref (released the same way any other
/// string is). Aborts when called on an enum that isn't
/// string-repr or for an unknown discriminant — those should be
/// caught by the type checker / `Inst::EnumTag` registration but
/// the runtime check costs nothing and keeps a localised crash
/// instead of a delayed memory bug.
pub(super) extern "C" fn host_enum_disc_str(global_eid: i64, disc: i64) -> i64 {
    let m = enum_info_lock().lock().expect("enum info poisoned");
    let info = match m.get(&(global_eid as u32)) {
        Some(i) => i,
        None => {
            eprintln!(
                "ilang: cast to string on unregistered enum (global={global_eid})"
            );
            std::process::abort();
        }
    };
    let table = match info.str_repr.as_ref() {
        Some(t) => t,
        None => {
            eprintln!(
                "ilang: cast to string on enum `{}` which has no `: string` repr",
                info.name,
            );
            std::process::abort();
        }
    };
    match table.get(&disc) {
        Some(s) => leak_cstring(s.clone()),
        None => {
            eprintln!(
                "ilang: cast to string on enum `{}` with unknown discriminant {disc}",
                info.name,
            );
            std::process::abort();
        }
    }
}

pub(super) extern "C" fn host_identity(p: i64) -> i64 { p }

/// Stub for `@extern(C) @optional` fns whose lib / symbol couldn't
/// be resolved. Aborts if called; user code is expected to gate
/// via `os.libLoaded(...)`.
pub(super) extern "C" fn host_optional_missing_stub() -> ! {
    eprintln!(
        "panic: invoked an `@extern(C) @optional` fn whose library was not loaded"
    );
    std::process::exit(1);
}

unsafe extern "C" {
    fn dlsym(handle: *mut u8, name: *const u8) -> *mut u8;
}

// `RTLD_DEFAULT` differs by platform: macOS uses (-2 as *mut u8),
// Linux uses NULL. Use a const fn so each target picks the right
// sentinel.
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

/// `stringFromCstr(p)` — copy the bytes pointed to by `p` into a
/// fresh leaked NUL-terminated buffer so `free(p)` afterwards
/// doesn't invalidate the caller's string view.
pub(super) extern "C" fn host_string_from_cstr(p: i64) -> i64 {
    if p == 0 {
        return leak_cstring(String::new());
    }
    let bytes = unsafe { raw_cstr_bytes(p) };
    leak_cstring(String::from_utf8_lossy(bytes).into_owned())
}

pub(super) extern "C" fn host_noop(_: i64) {}

/// `cstrArrayToStrings(p: *const *const char): string[]` — walk a
/// NULL-terminated `char**` and copy each `char*` into a fresh
/// NUL-terminated buffer, packed into a 40-byte-header ilang array.
pub(super) extern "C" fn host_cstr_array_to_strings(ptrs: i64) -> i64 {
    let mut elems: Vec<i64> = Vec::new();
    if ptrs != 0 {
        unsafe {
            let mut p = ptrs as *const *const u8;
            while !(*p).is_null() {
                let raw = (*p) as i64;
                let bytes = raw_cstr_bytes(raw);
                let s = String::from_utf8_lossy(bytes).into_owned();
                elems.push(leak_cstring(s));
                p = p.add(1);
            }
        }
    }
    let n = elems.len() as i64;
    let header = ilang_runtime::__mir_alloc(48);
    let data = ilang_runtime::__mir_alloc(n.max(1) * 8);
    unsafe {
        let h = header as *mut i64;
        *h = n;
        *h.add(1) = n;
        *h.add(2) = data;
        *h.add(3) = 1;
        *h.add(4) = 0;
        *h.add(5) = 8; // stride
        let d = data as *mut i64;
        for (i, s) in elems.iter().enumerate() {
            *d.add(i) = *s;
        }
    }
    header
}
