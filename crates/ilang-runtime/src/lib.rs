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
