//! Runtime support library linked into ilang AOT executables and
//! also called by the JIT (via `JITBuilder::symbol` taking the same
//! function pointers). Every `extern "C"` symbol here is the canonical
//! body for one ilang runtime helper; the two compile backends share
//! it bit-for-bit.

use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};

// --------------------------------------------------------------------
// Heap allocator + introspection
// --------------------------------------------------------------------

static ALLOC_BYTES: AtomicI64 = AtomicI64::new(0);
static FREE_BYTES: AtomicI64 = AtomicI64::new(0);
static ALLOC_COUNT: AtomicI64 = AtomicI64::new(0);
static FREE_COUNT: AtomicI64 = AtomicI64::new(0);

/// Allocate `size` zero-initialised bytes via Rust's global allocator
/// and leak the `Vec<u8>`'s data pointer. Mirrored by `__mir_free`,
/// which reconstructs the same `Vec` to drop. Tracked in the live-
/// alloc counters so `test.liveAlloc*()` can detect leaks.
#[unsafe(no_mangle)]
pub extern "C" fn __mir_alloc(size: i64) -> i64 {
    let n = size as usize;
    let mut v: Vec<u8> = vec![0; n];
    let ptr = v.as_mut_ptr() as i64;
    std::mem::forget(v);
    ALLOC_BYTES.fetch_add(size, Ordering::Relaxed);
    ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
    ptr
}

/// Free a previously `__mir_alloc`'d block. The caller passes the
/// original `size` so we can rebuild the matching `Vec<u8>` and drop
/// it. A null pointer or non-positive size is a no-op.
#[unsafe(no_mangle)]
pub extern "C" fn __mir_free(ptr: i64, size: i64) {
    if ptr == 0 || size <= 0 {
        return;
    }
    unsafe {
        let _ = Vec::from_raw_parts(ptr as *mut u8, size as usize, size as usize);
    }
    FREE_BYTES.fetch_add(size, Ordering::Relaxed);
    FREE_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Bytes currently outstanding via `__mir_alloc`. Used by the
/// `test.liveAllocBytes()` JIT builtin to detect leaks.
pub fn live_alloc_bytes() -> i64 {
    ALLOC_BYTES.load(Ordering::Relaxed) - FREE_BYTES.load(Ordering::Relaxed)
}

/// Allocations currently outstanding via `__mir_alloc`. Used by
/// `test.liveAllocCount()`.
pub fn live_alloc_count() -> i64 {
    ALLOC_COUNT.load(Ordering::Relaxed) - FREE_COUNT.load(Ordering::Relaxed)
}

// --------------------------------------------------------------------
// Print helpers
// --------------------------------------------------------------------

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

/// Print an ilang string. `p` is the address of the first byte of the
/// user-visible payload; the byte length sits as an `i64` 8 bytes
/// *before* `p`, matching the codegen's `[ i64 length | bytes | \0 ]`
/// data layout. A null `p` (or non-positive length) prints nothing.
#[unsafe(no_mangle)]
pub extern "C" fn __print_str(p: i64) {
    let bytes = unsafe { cstr_bytes(p) };
    if bytes.is_empty() {
        return;
    }
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(bytes);
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

// --------------------------------------------------------------------
// Panic
// --------------------------------------------------------------------

/// Runtime panic shared by JIT and AOT. `msg` is the body pointer of
/// an ilang string (`[ i64 length | bytes | \0 ]` layout). Prints to
/// stderr with a trailing newline and exits the process.
#[unsafe(no_mangle)]
pub extern "C" fn __ilang_panic(msg: i64) -> ! {
    let bytes = if msg == 0 { b"panic" as &[u8] } else { unsafe { cstr_bytes(msg) } };
    let mut err = std::io::stderr().lock();
    let _ = err.write_all(bytes);
    let _ = err.write_all(b"\n");
    std::process::exit(1)
}

// --------------------------------------------------------------------
// String layout helpers
// --------------------------------------------------------------------

/// Read an ilang string's bytes. `p` is the body pointer; the i64
/// length prefix sits at `p - 8`. A null `p` or non-positive length
/// returns an empty slice. SAFETY: caller must ensure `p` was emitted
/// in the standard `[ i64 length | bytes | \0 ]` layout (every string
/// the codegen or `leak_cstring` produces).
///
/// Exposed `pub` so the JIT-side host functions in `ilang-mir-codegen`
/// can reuse the same decoder.
pub unsafe fn cstr_bytes<'a>(p: i64) -> &'a [u8] {
    if p == 0 {
        return &[];
    }
    unsafe {
        let len = *((p - 8) as *const i64);
        if len <= 0 {
            return &[];
        }
        std::slice::from_raw_parts(p as *const u8, len as usize)
    }
}

/// Convenience: decode `p` as a `&str`. Returns `""` on any UTF-8
/// error so callers can format unconditionally.
pub fn cstr_to_str<'a>(p: i64) -> &'a str {
    let bytes = unsafe { cstr_bytes(p) };
    std::str::from_utf8(bytes).unwrap_or("")
}

// --------------------------------------------------------------------
// String heap allocator and registry
// --------------------------------------------------------------------

/// Owned heap allocation for a `[ i64 len | bytes | \0 ]` string.
/// Aligned to 8 so the leading length prefix is reachable via
/// `*((body_ptr - 8) as *const i64)` without violating Rust's
/// pointer-alignment checks.
struct StringBacking {
    base: *mut u8,
    total: usize,
}
// SAFETY: the pointer is owned solely by this struct + the global
// registry (mutex-guarded). No interior mutability beyond what the
// registry already serializes.
unsafe impl Send for StringBacking {}

impl Drop for StringBacking {
    fn drop(&mut self) {
        if self.base.is_null() {
            return;
        }
        let layout = std::alloc::Layout::from_size_align(self.total, 8).unwrap();
        unsafe { std::alloc::dealloc(self.base, layout) };
    }
}

struct StringEntry {
    // Owns the buffer; freed via Drop.
    #[allow(dead_code)]
    backing: StringBacking,
    rc: i64,
}

static STRING_REGISTRY: OnceLock<Mutex<HashMap<i64, StringEntry>>> = OnceLock::new();

fn string_registry_lock() -> &'static Mutex<HashMap<i64, StringEntry>> {
    STRING_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Allocate a heap string with the `[ i64 len | bytes | \0 ]` layout
/// and register it at `rc = 1`. Returns the body pointer (the
/// user-visible string pointer). The matching `__release_string` call
/// drops the buffer when rc reaches 0.
pub fn leak_cstring(s: String) -> i64 {
    let body = s.into_bytes();
    let len = body.len() as i64;
    let total = 8 + body.len() + 1;
    let layout = std::alloc::Layout::from_size_align(total.max(8), 8).unwrap();
    let base = unsafe { std::alloc::alloc(layout) };
    if base.is_null() {
        std::alloc::handle_alloc_error(layout);
    }
    unsafe {
        std::ptr::copy_nonoverlapping(len.to_le_bytes().as_ptr(), base, 8);
        if !body.is_empty() {
            std::ptr::copy_nonoverlapping(body.as_ptr(), base.add(8), body.len());
        }
        *base.add(8 + body.len()) = 0;
    }
    let body_ptr = unsafe { base.add(8) } as i64;
    {
        let mut reg = string_registry_lock().lock().expect("string registry poisoned");
        reg.insert(
            body_ptr,
            StringEntry {
                backing: StringBacking { base, total },
                rc: 1,
            },
        );
    }
    body_ptr
}

/// Number of live entries in the string registry. Exposed for the
/// JIT-side `test.liveStringCount()` builtin so suites can assert
/// that temporaries got released. AOT programs don't currently use
/// this — it's a JIT-only diagnostic.
pub fn live_string_count() -> i64 {
    let reg = string_registry_lock().lock().expect("string registry poisoned");
    reg.len() as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __retain_string(p: i64) {
    if p == 0 {
        return;
    }
    let mut reg = string_registry_lock().lock().expect("string registry poisoned");
    if let Some(e) = reg.get_mut(&p) {
        e.rc += 1;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __release_string(p: i64) {
    if p == 0 {
        return;
    }
    let mut reg = string_registry_lock().lock().expect("string registry poisoned");
    let drop_it = if let Some(e) = reg.get_mut(&p) {
        e.rc -= 1;
        e.rc <= 0
    } else {
        false
    };
    if drop_it {
        reg.remove(&p);
    }
}

// --------------------------------------------------------------------
// String operations
// --------------------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn __str_length(p: i64) -> i64 {
    let bytes = unsafe { cstr_bytes(p) };
    // Unicode code-point count to match `String.length` semantics.
    std::str::from_utf8(bytes)
        .map(|s| s.chars().count() as i64)
        .unwrap_or(bytes.len() as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn __str_concat(a: i64, b: i64) -> i64 {
    let sa = unsafe { cstr_bytes(a) };
    let sb = unsafe { cstr_bytes(b) };
    let mut out = Vec::with_capacity(sa.len() + sb.len());
    out.extend_from_slice(sa);
    out.extend_from_slice(sb);
    leak_cstring(String::from_utf8_lossy(&out).into_owned())
}

#[unsafe(no_mangle)]
pub extern "C" fn __str_eq(a: i64, b: i64) -> i64 {
    if a == b {
        return 1;
    }
    let sa = unsafe { cstr_bytes(a) };
    let sb = unsafe { cstr_bytes(b) };
    if sa == sb { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn __int_to_string(n: i64) -> i64 {
    leak_cstring(n.to_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn __bool_to_string(b: i64) -> i64 {
    leak_cstring(if b != 0 { "true".to_string() } else { "false".to_string() })
}

#[unsafe(no_mangle)]
pub extern "C" fn __str_to_upper(p: i64) -> i64 {
    leak_cstring(cstr_to_str(p).to_uppercase())
}

#[unsafe(no_mangle)]
pub extern "C" fn __str_to_lower(p: i64) -> i64 {
    leak_cstring(cstr_to_str(p).to_lowercase())
}

#[unsafe(no_mangle)]
pub extern "C" fn __str_trim(p: i64) -> i64 {
    leak_cstring(cstr_to_str(p).trim().to_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn __str_includes(p: i64, q: i64) -> i64 {
    if cstr_to_str(p).contains(cstr_to_str(q)) { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn __str_starts_with(p: i64, q: i64) -> i64 {
    if cstr_to_str(p).starts_with(cstr_to_str(q)) { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn __str_ends_with(p: i64, q: i64) -> i64 {
    if cstr_to_str(p).ends_with(cstr_to_str(q)) { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn __str_char_at(p: i64, idx: i64) -> i64 {
    let s = cstr_to_str(p);
    let c = s.chars().nth(idx as usize);
    leak_cstring(c.map(|c| c.to_string()).unwrap_or_default())
}

#[unsafe(no_mangle)]
pub extern "C" fn __str_slice(p: i64, start: i64, end: i64) -> i64 {
    let s = cstr_to_str(p);
    let chars: Vec<char> = s.chars().collect();
    let lo = (start.max(0) as usize).min(chars.len());
    let hi = (end.max(0) as usize).min(chars.len());
    let lo = lo.min(hi);
    leak_cstring(chars[lo..hi].iter().collect::<String>())
}

#[unsafe(no_mangle)]
pub extern "C" fn __str_replace(p: i64, from: i64, to: i64) -> i64 {
    let s = cstr_to_str(p);
    let f = cstr_to_str(from);
    let t = cstr_to_str(to);
    leak_cstring(s.replace(f, t))
}

// --------------------------------------------------------------------
// Array layout + leaf operations
// --------------------------------------------------------------------
//
// Array header (48 bytes):
//   offset  field          notes
//   ------  -----          -----
//   +0      length (i64)
//   +8      capacity (i64)
//   +16     data pointer
//   +24     refcount
//   +32     element KIND_* tag (for `__release_array` cascade)
//   +40     stride bytes per cell

/// Element-type tags stored at header +32 so `__release_array` can
/// decide whether to cascade-release the cells. The JIT side mirrors
/// these constants in `compile.rs` and uses them for the broader
/// `release_by_kind` dispatcher.
pub const KIND_NONE: i64 = 0;
pub const KIND_STR: i64 = 7;

#[inline]
unsafe fn array_header(arr: i64) -> (i64, i64, i64) {
    unsafe {
        let p = arr as *const i64;
        (*p, *p.add(1), *p.add(2))
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __array_index_of(arr: i64, value: i64) -> i64 {
    if arr == 0 {
        return -1;
    }
    let (len, _cap, data) = unsafe { array_header(arr) };
    for i in 0..len {
        let cell = unsafe { *((data + i * 8) as *const i64) };
        if cell == value {
            return i;
        }
    }
    -1
}

#[unsafe(no_mangle)]
pub extern "C" fn __array_includes(arr: i64, value: i64) -> i64 {
    if __array_index_of(arr, value) >= 0 { 1 } else { 0 }
}

#[inline]
unsafe fn store_packed(data: i64, idx: i64, stride: i64, value: i64) {
    unsafe {
        let addr = (data + idx * stride) as *mut u8;
        match stride {
            1 => *(addr as *mut u8) = value as u8,
            2 => *(addr as *mut u16) = value as u16,
            4 => *(addr as *mut u32) = value as u32,
            _ => *(addr as *mut i64) = value,
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __array_push(arr: i64, value: i64) {
    if arr == 0 {
        return;
    }
    unsafe {
        let h = arr as *mut i64;
        let len = *h;
        let cap = *h.add(1);
        let data = *h.add(2);
        let stride = *h.add(5);
        if len < cap {
            store_packed(data, len, stride, value);
            *h = len + 1;
        } else {
            let new_cap = (cap * 2).max(4);
            let new_data = __mir_alloc(new_cap * stride);
            std::ptr::copy_nonoverlapping(
                data as *const u8,
                new_data as *mut u8,
                (len * stride) as usize,
            );
            store_packed(new_data, len, stride, value);
            if data != 0 && cap > 0 {
                __mir_free(data, cap * stride);
            }
            *h = len + 1;
            *h.add(1) = new_cap;
            *h.add(2) = new_data;
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __array_pop(arr: i64) -> i64 {
    if arr == 0 {
        return 0;
    }
    unsafe {
        let h = arr as *mut i64;
        let len = *h;
        if len <= 0 {
            return 0;
        }
        let data = *h.add(2);
        let stride = *h.add(5);
        let idx = len - 1;
        let addr = (data + idx * stride) as *const u8;
        let value = match stride {
            1 => *(addr as *const u8) as i64,
            2 => *(addr as *const u16) as i64,
            4 => *(addr as *const u32) as i64,
            _ => *(addr as *const i64),
        };
        *h = idx;
        value
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __array_data_ptr(arr: i64) -> i64 {
    if arr == 0 {
        return 0;
    }
    unsafe { *((arr + 16) as *const i64) }
}

/// `arrayFromCArray<T>(src, n, stride, kind_tag)` — copy `n × stride`
/// bytes from a C-side array into a fresh ilang dyn-array.
#[unsafe(no_mangle)]
pub extern "C" fn __c_array_to_array(src: i64, n: i64, stride: i64, kind_tag: i64) -> i64 {
    let n_safe = if n < 0 { 0 } else { n };
    let bytes = n_safe * stride;
    let header = __mir_alloc(48);
    let data = __mir_alloc(bytes.max(stride));
    unsafe {
        if bytes > 0 && src != 0 {
            std::ptr::copy_nonoverlapping(src as *const u8, data as *mut u8, bytes as usize);
        }
        let h = header as *mut i64;
        *h = n_safe;
        *h.add(1) = n_safe;
        *h.add(2) = data;
        *h.add(3) = 1;
        *h.add(4) = kind_tag;
        *h.add(5) = stride;
    }
    header
}

/// Retain an array (`++rc`). No-op on null or rc <= 0 entries.
#[unsafe(no_mangle)]
pub extern "C" fn __retain_array(arr_ptr: i64) {
    if arr_ptr == 0 {
        return;
    }
    let rc_ptr = (arr_ptr + 24) as *mut i64;
    let rc = unsafe { *rc_ptr };
    if rc <= 0 {
        return;
    }
    unsafe {
        *rc_ptr = rc + 1;
    }
}

/// Release an array (`--rc`); free header + data buffer at rc 0.
/// Cell cascade is limited to `KIND_NONE` (no-op) and `KIND_STR`
/// (release each cell via `__release_string`). Arrays of objects /
/// closures / etc. leak their inner items until the process exits —
/// the full cascade machinery lives in the JIT-side `compile.rs`
/// because its dependencies (per-class field tables, vtable
/// dispatch, etc.) haven't moved here yet.
#[unsafe(no_mangle)]
pub extern "C" fn __release_array(arr_ptr: i64) {
    if arr_ptr == 0 {
        return;
    }
    let rc_ptr = (arr_ptr + 24) as *mut i64;
    let rc = unsafe { *rc_ptr };
    if rc <= 0 {
        return;
    }
    let new_rc = rc - 1;
    unsafe {
        *rc_ptr = new_rc;
    }
    if new_rc != 0 {
        return;
    }
    let tag = unsafe { *((arr_ptr + 32) as *const i64) };
    let len = unsafe { *(arr_ptr as *const i64) };
    let cap = unsafe { *((arr_ptr + 8) as *const i64) };
    let data_ptr = unsafe { *((arr_ptr + 16) as *const i64) };
    let stride = unsafe { *((arr_ptr + 40) as *const i64) };
    match tag {
        KIND_NONE => {}
        KIND_STR => {
            for i in 0..len {
                let cell = unsafe { *((data_ptr + i * 8) as *const i64) };
                __release_string(cell);
            }
        }
        _ => {
            // Other cascade kinds aren't supported in this layer yet;
            // inner cells leak. The JIT side overrides this symbol via
            // `JITBuilder::symbol("__release_array", host_release_array)`
            // so JIT-run programs still get the full cascade.
        }
    }
    if data_ptr != 0 {
        __mir_free(data_ptr, cap.max(1) * stride);
    }
    __mir_free(arr_ptr, 48);
}

// --------------------------------------------------------------------
// Map runtime
// --------------------------------------------------------------------
//
// `ManagedMap` wraps Rust's `HashMap<MapKey, i64>` with a refcount,
// the per-value KIND_* tag (for cascade-release on drop), and per-
// side print-kind tags (so `__print_map` can stringify the cells).
//
// Cascade support in `__release_map` / `__map_set` is limited to the
// same kinds `__release_array` handles (`KIND_NONE`, `KIND_STR`,
// `KIND_ARRAY`). Maps whose values are objects / closures / enums
// leak their inner cells until process exit — the JIT side keeps its
// fully-cascading `host_map_*` for now.

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum MapKey {
    Int(i64),
    Str(String),
}

struct ManagedMap {
    rc: i64,
    val_kind: i64,
    key_print_kind: i64,
    val_print_kind: i64,
    inner: HashMap<MapKey, i64>,
    /// For string-keyed maps: canonical key → original C-string ptr
    /// the user inserted. Lets `keys()` return the original ptrs.
    str_key_origs: HashMap<MapKey, i64>,
}

pub const PK_I64_SIG: i64 = 0;
pub const PK_I64_UNS: i64 = 1;
pub const PK_I32_SIG: i64 = 2;
pub const PK_I32_UNS: i64 = 3;
pub const PK_I16_SIG: i64 = 4;
pub const PK_I16_UNS: i64 = 5;
pub const PK_I8_SIG: i64 = 6;
pub const PK_I8_UNS: i64 = 7;
pub const PK_BOOL: i64 = 8;
pub const PK_F64: i64 = 9;
pub const PK_F32: i64 = 10;
pub const PK_STR: i64 = 11;
pub const PK_OBJECT: i64 = 12;
pub const PK_ARRAY_I64_SIG: i64 = 100;
pub const PK_OTHER: i64 = -1;

const KIND_ARRAY: i64 = 2;

fn raw_to_map_key(raw: i64, key_print_kind: i64) -> MapKey {
    if key_print_kind == PK_STR {
        if raw == 0 {
            MapKey::Str(String::new())
        } else {
            let bytes = unsafe { cstr_bytes(raw) };
            MapKey::Str(String::from_utf8_lossy(bytes).into_owned())
        }
    } else {
        MapKey::Int(raw)
    }
}

fn map_key_to_raw(k: &MapKey) -> i64 {
    match k {
        MapKey::Int(n) => *n,
        MapKey::Str(s) => leak_cstring(s.clone()),
    }
}

/// Limited retain dispatcher for map values. Mirrors the JIT-side
/// `retain_by_kind` for the kinds whose retain implementations live
/// in this crate. Unknown kinds silently leak.
fn map_retain_by_kind(ptr: i64, kind: i64) {
    if ptr == 0 {
        return;
    }
    match kind {
        KIND_STR => __retain_string(ptr),
        KIND_ARRAY => __retain_array(ptr),
        _ => {}
    }
}

fn map_release_by_kind(ptr: i64, kind: i64) {
    if ptr == 0 {
        return;
    }
    match kind {
        KIND_STR => __release_string(ptr),
        KIND_ARRAY => __release_array(ptr),
        _ => {}
    }
}

fn format_f64_like_jit(f: f64) -> String {
    if f.is_nan() {
        "NaN".to_string()
    } else if f.is_infinite() {
        if f > 0.0 { "Infinity".to_string() } else { "-Infinity".to_string() }
    } else if f == f.trunc() && f.abs() < 1e16 {
        format!("{}.0", f as i64)
    } else {
        format!("{f}")
    }
}

fn format_kind_id(out: &mut String, kind: i64, raw: i64) {
    use std::fmt::Write;
    match kind {
        PK_I64_SIG => { let _ = write!(out, "{}", raw); }
        PK_I64_UNS => { let _ = write!(out, "{}", raw as u64); }
        PK_I32_SIG => { let _ = write!(out, "{}", raw as i32); }
        PK_I32_UNS => { let _ = write!(out, "{}", raw as u32); }
        PK_I16_SIG => { let _ = write!(out, "{}", raw as i16); }
        PK_I16_UNS => { let _ = write!(out, "{}", raw as u16); }
        PK_I8_SIG => { let _ = write!(out, "{}", raw as i8); }
        PK_I8_UNS => { let _ = write!(out, "{}", raw as u8); }
        PK_BOOL => { let _ = write!(out, "{}", raw != 0); }
        PK_F64 => {
            let f = f64::from_bits(raw as u64);
            let _ = write!(out, "{}", format_f64_like_jit(f));
        }
        PK_F32 => {
            let f = f32::from_bits((raw as i32) as u32);
            let _ = write!(out, "{}", format_f64_like_jit(f as f64));
        }
        PK_STR => {
            if raw != 0 {
                let bytes = unsafe { cstr_bytes(raw) };
                let _ = write!(out, "{}", String::from_utf8_lossy(bytes));
            }
        }
        PK_OBJECT => {
            // Full object formatting needs per-class field tables
            // that haven't moved here yet; fall back to a placeholder.
            let _ = write!(out, "<object#{}>", raw);
        }
        PK_ARRAY_I64_SIG => {
            out.push('[');
            if raw != 0 {
                let len = unsafe { *(raw as *const i64) };
                let data_ptr = unsafe { *((raw + 16) as *const i64) };
                for i in 0..len {
                    if i > 0 { out.push_str(", "); }
                    let elem = unsafe { *((data_ptr + i * 8) as *const i64) };
                    let _ = write!(out, "{}", elem);
                }
            }
            out.push(']');
        }
        _ => { let _ = write!(out, "{}", raw); }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __map_new() -> i64 {
    let m = Box::new(ManagedMap {
        rc: 1,
        val_kind: 0,
        key_print_kind: PK_OTHER,
        val_print_kind: PK_OTHER,
        inner: HashMap::new(),
        str_key_origs: HashMap::new(),
    });
    Box::into_raw(m) as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __map_set_print_kinds(map: i64, key_kind: i64, val_kind: i64) {
    if map == 0 {
        return;
    }
    let m = unsafe { &mut *(map as *mut ManagedMap) };
    m.key_print_kind = key_kind;
    m.val_print_kind = val_kind;
}

#[unsafe(no_mangle)]
pub extern "C" fn __map_set_value_kind(map: i64, kind: i64) {
    if map == 0 {
        return;
    }
    let m = unsafe { &mut *(map as *mut ManagedMap) };
    m.val_kind = kind;
}

#[unsafe(no_mangle)]
pub extern "C" fn __map_get(map: i64, key: i64) -> i64 {
    if map == 0 {
        return 0;
    }
    let m = unsafe { &*(map as *const ManagedMap) };
    let mk = raw_to_map_key(key, m.key_print_kind);
    *m.inner.get(&mk).unwrap_or(&0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __map_get_optional(map: i64, key: i64) -> i64 {
    if map == 0 {
        return 0;
    }
    let m = unsafe { &*(map as *const ManagedMap) };
    let mk = raw_to_map_key(key, m.key_print_kind);
    match m.inner.get(&mk) {
        Some(&v) => {
            let cell = __mir_alloc(24) as *mut i64;
            unsafe {
                *cell = v;
                *cell.add(1) = 1;
                *cell.add(2) = m.val_kind;
            }
            cell as i64
        }
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __map_set(map: i64, key: i64, value: i64) {
    if map == 0 {
        return;
    }
    let m = unsafe { &mut *(map as *mut ManagedMap) };
    let mk = raw_to_map_key(key, m.key_print_kind);
    if m.key_print_kind == PK_STR && key != 0 {
        m.str_key_origs.entry(mk.clone()).or_insert(key);
    }
    let val_kind = m.val_kind;
    if val_kind != KIND_NONE {
        map_retain_by_kind(value, val_kind);
    }
    let prev = m.inner.insert(mk, value);
    if let Some(old) = prev {
        if val_kind != KIND_NONE {
            map_release_by_kind(old, val_kind);
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __map_has(map: i64, key: i64) -> i64 {
    if map == 0 {
        return 0;
    }
    let m = unsafe { &*(map as *const ManagedMap) };
    let mk = raw_to_map_key(key, m.key_print_kind);
    if m.inner.contains_key(&mk) { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn __map_size(map: i64) -> i64 {
    if map == 0 {
        return 0;
    }
    let m = unsafe { &*(map as *const ManagedMap) };
    m.inner.len() as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __map_delete(map: i64, key: i64) -> i64 {
    if map == 0 {
        return 0;
    }
    let m = unsafe { &mut *(map as *mut ManagedMap) };
    let mk = raw_to_map_key(key, m.key_print_kind);
    let val_kind = m.val_kind;
    match m.inner.remove(&mk) {
        Some(old) => {
            if val_kind != KIND_NONE {
                map_release_by_kind(old, val_kind);
            }
            1
        }
        None => 0,
    }
}

/// Build an i64[] (KIND_STR-tagged for string keys, KIND_NONE
/// otherwise) populated with every key in the map. Used by
/// `Map.keys`. The order matches Rust's HashMap iteration (non-
/// deterministic across runs) — same as the JIT.
fn build_i64_array(items: &[i64], elem_kind: i64) -> i64 {
    let cap = items.len().max(4);
    let header = __mir_alloc(48);
    let data = __mir_alloc((cap * 8) as i64);
    unsafe {
        let h = header as *mut i64;
        *h = items.len() as i64;
        *h.add(1) = cap as i64;
        *h.add(2) = data;
        *h.add(3) = 1;
        *h.add(4) = elem_kind;
        *h.add(5) = 8;
        for (i, v) in items.iter().enumerate() {
            *((data + (i as i64) * 8) as *mut i64) = *v;
        }
    }
    header
}

#[unsafe(no_mangle)]
pub extern "C" fn __map_keys(map: i64) -> i64 {
    if map == 0 {
        return build_i64_array(&[], KIND_NONE);
    }
    let m = unsafe { &*(map as *const ManagedMap) };
    let elem_kind = if m.key_print_kind == PK_STR { KIND_STR } else { KIND_NONE };
    let keys: Vec<i64> = if m.key_print_kind == PK_STR {
        // Prefer the original literal pointer so `keys().includes(orig)`
        // works without a content compare.
        m.inner
            .keys()
            .map(|k| m.str_key_origs.get(k).copied().unwrap_or_else(|| map_key_to_raw(k)))
            .collect()
    } else {
        m.inner.keys().map(map_key_to_raw).collect()
    };
    if elem_kind == KIND_STR {
        for k in &keys {
            __retain_string(*k);
        }
    }
    build_i64_array(&keys, elem_kind)
}

#[unsafe(no_mangle)]
pub extern "C" fn __map_values(map: i64) -> i64 {
    if map == 0 {
        return build_i64_array(&[], KIND_NONE);
    }
    let m = unsafe { &*(map as *const ManagedMap) };
    let val_kind = m.val_kind;
    let values: Vec<i64> = m.inner.values().copied().collect();
    if val_kind != KIND_NONE {
        for v in &values {
            map_retain_by_kind(*v, val_kind);
        }
    }
    build_i64_array(&values, val_kind)
}

#[unsafe(no_mangle)]
pub extern "C" fn __print_map(map_ptr: i64) {
    use std::io::Write;
    let mut out = String::new();
    if map_ptr == 0 {
        out.push_str("{}");
        let mut o = std::io::stdout().lock();
        let _ = o.write_all(out.as_bytes());
        return;
    }
    let m = unsafe { &*(map_ptr as *const ManagedMap) };
    let mut entries: Vec<(i64, i64)> =
        m.inner.iter().map(|(k, &v)| (map_key_to_raw(k), v)).collect();
    let kk = m.key_print_kind;
    let vk = m.val_print_kind;
    entries.sort_by(|a, b| {
        let mut sa = String::new();
        let mut sb = String::new();
        format_kind_id(&mut sa, kk, a.0);
        format_kind_id(&mut sb, kk, b.0);
        sa.cmp(&sb)
    });
    out.push('{');
    for (i, (k, v)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        format_kind_id(&mut out, kk, *k);
        out.push_str(": ");
        format_kind_id(&mut out, vk, *v);
    }
    out.push('}');
    let mut o = std::io::stdout().lock();
    let _ = o.write_all(out.as_bytes());
}

#[unsafe(no_mangle)]
pub extern "C" fn __retain_map(map: i64) {
    if map == 0 {
        return;
    }
    let m = unsafe { &mut *(map as *mut ManagedMap) };
    if m.rc <= 0 {
        return;
    }
    m.rc += 1;
}

#[unsafe(no_mangle)]
pub extern "C" fn __release_map(map: i64) {
    if map == 0 {
        return;
    }
    let m_mut = unsafe { &mut *(map as *mut ManagedMap) };
    if m_mut.rc <= 0 {
        return;
    }
    m_mut.rc -= 1;
    if m_mut.rc != 0 {
        return;
    }
    let val_kind = m_mut.val_kind;
    if val_kind != KIND_NONE {
        let values: Vec<i64> = m_mut.inner.values().copied().collect();
        for v in values {
            map_release_by_kind(v, val_kind);
        }
    }
    unsafe {
        let _ = Box::from_raw(map as *mut ManagedMap);
    }
}

// --------------------------------------------------------------------
// Class objects (minimal)
// --------------------------------------------------------------------
//
// Object header: 16 bytes
//   +0  i64 class_id
//   +8  i64 refcount
//   +16 fields...
//
// The JIT side keeps a richer implementation that consults per-class
// drop / size / field tables for user `deinit` + cascade release. AOT
// links the simpler versions below: `__retain_object` bumps the rc
// inline; `__release_object` decrements it and leaks the buffer when
// rc reaches 0 (the OS reaps everything on process exit, which is
// fine for one-shot `ilang build` programs). Programs that need
// deinit / inheritance / virtual dispatch in AOT will eventually
// require the init-emit work that populates the JIT-side tables at
// process startup.

/// Total byte size of the heap allocation for each class id. AOT
/// populates via `__register_class_size` from `__ilang_aot_init`;
/// JIT populates the same map during its post-finalize step. Classes
/// whose memory is reclaimed via a different path (CRepr / packed /
/// union / weak-referenced) stay out of the table.
static CLASS_SIZE_TABLE: OnceLock<Mutex<HashMap<u32, i64>>> = OnceLock::new();

fn class_size_table() -> &'static Mutex<HashMap<u32, i64>> {
    CLASS_SIZE_TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __register_class_size(class_id: i64, size: i64) {
    let mut t = class_size_table().lock().expect("class size table poisoned");
    t.insert(class_id as u32, size);
}

/// Look up the registered byte size for a class. Returns `None` when
/// the class was deliberately left out of the table (CRepr / packed /
/// union, weak-referenced, etc.). Used by the JIT-side host helpers
/// that still own the field-cascade walk but need the runtime's size
/// registry for the trailing `__mir_free`.
pub fn class_size_for(class_id: i64) -> Option<i64> {
    let t = class_size_table().lock().expect("class size table poisoned");
    t.get(&(class_id as u32)).copied()
}

#[unsafe(no_mangle)]
pub extern "C" fn __retain_object(obj_ptr: i64) {
    if obj_ptr == 0 {
        return;
    }
    let rc_ptr = (obj_ptr + 8) as *mut i64;
    let rc = unsafe { *rc_ptr };
    if rc <= 0 {
        return;
    }
    unsafe {
        *rc_ptr = rc + 1;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __release_object(obj_ptr: i64) {
    if obj_ptr == 0 {
        return;
    }
    let rc_ptr = (obj_ptr + 8) as *mut i64;
    let rc = unsafe { *rc_ptr };
    if rc <= 0 {
        return;
    }
    let new_rc = rc - 1;
    unsafe {
        *rc_ptr = new_rc;
    }
    if new_rc != 0 {
        return;
    }
    let class_id = unsafe { *(obj_ptr as *const i64) };
    // Run user deinit if registered.
    let user_drop = __drop_dispatch(class_id);
    if user_drop != 0 {
        let f: extern "C" fn(i64, i64) = unsafe { std::mem::transmute(user_drop) };
        f(obj_ptr, 0);
    }
    // Heap-typed field cascade — still stubbed; once
    // OBJECT_FIELD_TABLE moves here the cascade walks the table and
    // calls per-kind release. For now object fields of heap types
    // leak their inner values.
    __release_object_fields(class_id, obj_ptr);
    // Free the backing buffer if a size was registered.
    let size = {
        let t = class_size_table().lock().expect("class size table poisoned");
        t.get(&(class_id as u32)).copied()
    };
    if let Some(sz) = size {
        __mir_free(obj_ptr, sz);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __release_object_fields(_class_id: i64, _obj_ptr: i64) {
    // Stub: full implementation needs the per-class field-offset table
    // that the JIT populates at codegen time. Until AOT learns to emit
    // that table at startup, programs with heap-typed object fields
    // leak the inner values.
}

/// Per-class virtual dispatch table. Keyed by `(global_class_id,
/// slot)`. Populated either by the JIT after `finalize_definitions`
/// or by the AOT-emitted `__ilang_aot_init` running at process
/// startup; both routes funnel through `__register_vtable_entry`.
static VTABLE: OnceLock<Mutex<HashMap<(u32, u32), i64>>> = OnceLock::new();

fn vtable() -> &'static Mutex<HashMap<(u32, u32), i64>> {
    VTABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __register_vtable_entry(class_id: i64, slot: i64, fn_addr: i64) {
    let mut t = vtable().lock().expect("vtable poisoned");
    t.insert((class_id as u32, slot as u32), fn_addr);
}

#[unsafe(no_mangle)]
pub extern "C" fn __virt_dispatch(class_id: i64, slot: i64) -> i64 {
    let t = vtable().lock().expect("vtable poisoned");
    *t.get(&(class_id as u32, slot as u32)).unwrap_or(&0)
}

/// Per-class user-defined `deinit` dispatch table. AOT keeps it empty
/// for now; JIT populates via `__register_drop`.
static DROP_TABLE: OnceLock<Mutex<HashMap<u32, i64>>> = OnceLock::new();

fn drop_table() -> &'static Mutex<HashMap<u32, i64>> {
    DROP_TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __register_drop(class_id: i64, fn_addr: i64) {
    let mut t = drop_table().lock().expect("drop table poisoned");
    t.insert(class_id as u32, fn_addr);
}

#[unsafe(no_mangle)]
pub extern "C" fn __drop_dispatch(class_id: i64) -> i64 {
    let t = drop_table().lock().expect("drop table poisoned");
    *t.get(&(class_id as u32)).unwrap_or(&0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __class_name(_class_id: i64) -> i64 {
    leak_cstring("<class>".to_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn __print_object(obj_ptr: i64) {
    let mut out = std::io::stdout().lock();
    let _ = if obj_ptr == 0 {
        out.write_all(b"<null>")
    } else {
        // Placeholder shape until the AOT side learns the per-class
        // field tables. JIT prints e.g. `Point { x: 1, y: 2 }`.
        let class_id = unsafe { *(obj_ptr as *const i64) };
        out.write_all(format!("<object class_id={class_id} @{obj_ptr:#x}>").as_bytes())
    };
}

// --------------------------------------------------------------------
// Raw memory FFI read / write helpers
// --------------------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn __read_i8(p: i64, off: i64) -> i64 {
    unsafe { *((p + off) as *const i8) as i64 }
}
#[unsafe(no_mangle)]
pub extern "C" fn __read_i16(p: i64, off: i64) -> i64 {
    unsafe { *((p + off) as *const i16) as i64 }
}
#[unsafe(no_mangle)]
pub extern "C" fn __read_i32(p: i64, off: i64) -> i64 {
    unsafe { *((p + off) as *const i32) as i64 }
}
#[unsafe(no_mangle)]
pub extern "C" fn __read_i64(p: i64, off: i64) -> i64 {
    unsafe { *((p + off) as *const i64) }
}
#[unsafe(no_mangle)]
pub extern "C" fn __read_u8(p: i64, off: i64) -> i64 {
    unsafe { *((p + off) as *const u8) as i64 }
}
#[unsafe(no_mangle)]
pub extern "C" fn __read_u16(p: i64, off: i64) -> i64 {
    unsafe { *((p + off) as *const u16) as i64 }
}
#[unsafe(no_mangle)]
pub extern "C" fn __read_u32(p: i64, off: i64) -> i64 {
    unsafe { *((p + off) as *const u32) as i64 }
}
#[unsafe(no_mangle)]
pub extern "C" fn __read_u64(p: i64, off: i64) -> i64 {
    unsafe { *((p + off) as *const u64) as i64 }
}
#[unsafe(no_mangle)]
pub extern "C" fn __read_f32(p: i64, off: i64) -> f32 {
    unsafe { *((p + off) as *const f32) }
}
#[unsafe(no_mangle)]
pub extern "C" fn __read_f64(p: i64, off: i64) -> f64 {
    unsafe { *((p + off) as *const f64) }
}

#[unsafe(no_mangle)]
pub extern "C" fn __write_i8(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut i8) = v as i8; }
}
#[unsafe(no_mangle)]
pub extern "C" fn __write_i16(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut i16) = v as i16; }
}
#[unsafe(no_mangle)]
pub extern "C" fn __write_i32(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut i32) = v as i32; }
}
#[unsafe(no_mangle)]
pub extern "C" fn __write_i64(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut i64) = v; }
}
#[unsafe(no_mangle)]
pub extern "C" fn __write_u8(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut u8) = v as u8; }
}
#[unsafe(no_mangle)]
pub extern "C" fn __write_u16(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut u16) = v as u16; }
}
#[unsafe(no_mangle)]
pub extern "C" fn __write_u32(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut u32) = v as u32; }
}
#[unsafe(no_mangle)]
pub extern "C" fn __write_u64(p: i64, off: i64, v: i64) {
    unsafe { *((p + off) as *mut u64) = v as u64; }
}
#[unsafe(no_mangle)]
pub extern "C" fn __write_f32(p: i64, off: i64, v: f32) {
    unsafe { *((p + off) as *mut f32) = v; }
}
#[unsafe(no_mangle)]
pub extern "C" fn __write_f64(p: i64, off: i64, v: f64) {
    unsafe { *((p + off) as *mut f64) = v; }
}
