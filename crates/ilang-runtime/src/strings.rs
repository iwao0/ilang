//! String layout helpers + heap allocator + `__str_*` operations +
//! C-string interop. Every ilang string — whether it came out of
//! `leak_cstring` or was emitted by the codegen into `.rodata` —
//! lives as
//!
//! ```text
//! [ i64 cap | i64 rc | i64 len | bytes... | \0 ]
//! ```
//!
//! with the body pointer sitting 24 bytes past the allocation base.
//! `cap` is the total allocation size (so `__release_string` can
//! `dealloc` with the right `Layout` and `__str_concat_inplace` can
//! check spare capacity). Runtime-allocated strings start at `rc=1`;
//! codegen-emitted literals use `cap=0, rc=-1` as a sentinel pair so
//! `atomic_retain` / `atomic_release` (which both skip on `rc <= 0`)
//! treat them as permanent and never try to free them. That removes
//! the need for a registry side-table — every retain / release is a
//! plain atomic on `body - 16`.

use std::sync::atomic::{AtomicI64, Ordering};

use crate::arrays::{build_i64_array, __c_array_to_array};
use crate::kind::{KIND_NONE, KIND_STR};

// --------------------------------------------------------------------
// String layout helpers
// --------------------------------------------------------------------

/// Read an ilang string's bytes. `p` is the body pointer; the i64
/// length prefix sits at `p - 8`. A null `p` or non-positive length
/// returns an empty slice.
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
// String heap allocator
// --------------------------------------------------------------------

/// Live runtime-allocated string count — diagnostics only. Static
/// literals from `.rodata` are not counted (they never pass through
/// `leak_cstring`).
static LIVE_STRING_COUNT: AtomicI64 = AtomicI64::new(0);

const HEADER_BYTES: usize = 24;

#[inline]
fn layout_for_total(total: usize) -> std::alloc::Layout {
    std::alloc::Layout::from_size_align(total.max(HEADER_BYTES), 8).unwrap()
}

#[inline]
unsafe fn write_header(base: *mut u8, cap: usize, rc: i64, len: i64) {
    unsafe {
        std::ptr::copy_nonoverlapping((cap as i64).to_le_bytes().as_ptr(), base, 8);
        std::ptr::copy_nonoverlapping(rc.to_le_bytes().as_ptr(), base.add(8), 8);
        std::ptr::copy_nonoverlapping(len.to_le_bytes().as_ptr(), base.add(16), 8);
    }
}

/// Allocate a heap string with `rc = 1` from raw bytes. Returns the
/// body pointer (the user-visible string pointer). The bytes are copied
/// into a fresh layout buffer, so the caller keeps ownership of `body`.
///
/// This is the core builder: callers that already hold (or can cheaply
/// borrow) a `&[u8]` / `&str` should use this directly rather than
/// minting a throwaway `String` just to satisfy `leak_cstring` — the
/// `String`'s own heap allocation would be discarded here anyway.
pub fn leak_cstr_bytes(body: &[u8]) -> i64 {
    let len = body.len() as i64;
    let total = HEADER_BYTES + body.len() + 1;
    let layout = layout_for_total(total);
    let base = unsafe { std::alloc::alloc(layout) };
    if base.is_null() {
        std::alloc::handle_alloc_error(layout);
    }
    unsafe {
        write_header(base, total, 1, len);
        if !body.is_empty() {
            std::ptr::copy_nonoverlapping(body.as_ptr(), base.add(HEADER_BYTES), body.len());
        }
        *base.add(HEADER_BYTES + body.len()) = 0;
    }
    LIVE_STRING_COUNT.fetch_add(1, Ordering::Relaxed);
    unsafe { base.add(HEADER_BYTES) as i64 }
}

/// Allocate a heap string with `rc = 1` from an owned `String`. Thin
/// wrapper over `leak_cstr_bytes`; the bytes are copied into the layout
/// buffer and the `String`'s own allocation is freed on drop.
pub fn leak_cstring(s: String) -> i64 {
    leak_cstr_bytes(s.as_bytes())
}

/// Number of live heap-allocated strings (excludes static literals).
pub fn live_string_count() -> i64 {
    LIVE_STRING_COUNT.load(Ordering::Relaxed)
}

#[unsafe(export_name = "$string.retain")]
pub extern "C" fn __retain_string(p: i64) {
    if p == 0 {
        return;
    }
    let rc_ptr = (p - 16) as *mut i64;
    unsafe { crate::refcount::atomic_retain(rc_ptr) };
}

#[unsafe(export_name = "$string.release")]
pub extern "C" fn __release_string(p: i64) {
    if p == 0 {
        return;
    }
    let rc_ptr = (p - 16) as *mut i64;
    match unsafe { crate::refcount::atomic_release(rc_ptr) } {
        Some(0) => {}
        _ => return,
    }
    // Last reference — free the whole `[cap | rc | len | bytes | \0]`
    // block, sized by the inline `cap` field.
    let cap = unsafe { *((p - 24) as *const i64) } as usize;
    let layout = layout_for_total(cap);
    let base = (p - HEADER_BYTES as i64) as *mut u8;
    unsafe { std::alloc::dealloc(base, layout) };
    LIVE_STRING_COUNT.fetch_sub(1, Ordering::Relaxed);
}

// --------------------------------------------------------------------
// String operations
// --------------------------------------------------------------------

#[unsafe(export_name = "$string.length")]
pub extern "C" fn __str_length(p: i64) -> i64 {
    let bytes = unsafe { cstr_bytes(p) };
    std::str::from_utf8(bytes)
        .map(|s| s.chars().count() as i64)
        .unwrap_or(bytes.len() as i64)
}

#[unsafe(export_name = "$string.concat")]
pub extern "C" fn __str_concat(a: i64, b: i64) -> i64 {
    let sa = unsafe { cstr_bytes(a) };
    let sb = unsafe { cstr_bytes(b) };
    let mut out = Vec::with_capacity(sa.len() + sb.len());
    out.extend_from_slice(sa);
    out.extend_from_slice(sb);
    // Both inputs are valid UTF-8 ilang strings, so `out` is too — the
    // `Ok` path reuses the buffer with no extra copy or validation scan.
    // Fall back to lossy replacement (matching the old behaviour) only
    // on the unreachable invalid-bytes case, never panicking.
    let s = String::from_utf8(out)
        .unwrap_or_else(|e| String::from_utf8_lossy(&e.into_bytes()).into_owned());
    leak_cstring(s)
}

/// In-place variant of `__str_concat` used by the MIR for the
/// `s = s + expr` pattern, where the LHS is provably the only
/// holder of its string and is about to be reassigned. Grows the
/// LHS buffer via doubling `realloc` so the amortised cost per
/// append is O(1) instead of the O(n) the plain `__str_concat`
/// pays when each step allocates a fresh buffer.
///
/// Contract: caller must guarantee `a.rc == 1` (no aliases) AND
/// that the original `a` pointer will be retired immediately
/// (replaced by the returned pointer). Both hold for the
/// `s = s + expr` rewrite emitted by the MIR — never call this
/// from user code or from contexts where another binding still
/// reads `a`.
#[unsafe(export_name = "$string.concatInplace")]
pub extern "C" fn __str_concat_inplace(a: i64, b: i64) -> i64 {
    if a == 0 {
        // No backing yet — fall back to the regular path.
        return __str_concat(a, b);
    }
    // Defensive `rc == 1` check: the MIR pattern matcher only fires
    // for Locals (closure-captured strings stay on the regular path),
    // but a user-side `let t = s; s = s + "x"` keeps a's rc at 2 and
    // a static literal has rc=-1. In either case the in-place rewrite
    // is unsafe — fall back to the allocating `__str_concat`.
    let rc = unsafe { (*((a - 16) as *const AtomicI64)).load(Ordering::Acquire) };
    if rc != 1 {
        return __str_concat(a, b);
    }
    let sb = unsafe { cstr_bytes(b) };
    let a_len = unsafe { *((a - 8) as *const i64) } as usize;
    let new_len = a_len + sb.len();
    let needed_total = HEADER_BYTES + new_len + 1;
    let cur_total = unsafe { *((a - 24) as *const i64) } as usize;

    // Fast path: spare capacity already covers the result. Copy
    // bytes in, bump the length prefix. Returned pointer unchanged.
    if cur_total >= needed_total {
        unsafe {
            std::ptr::copy_nonoverlapping(
                sb.as_ptr(),
                (a as *mut u8).add(a_len),
                sb.len(),
            );
            *((a as *mut u8).add(a_len + sb.len())) = 0;
            *((a - 8) as *mut i64) = new_len as i64;
        }
        return a;
    }
    // Grow with doubling — guarantees amortised O(1) appends.
    let new_total = needed_total.max(cur_total.saturating_mul(2));
    let old_layout = layout_for_total(cur_total);
    let cur_base = (a - HEADER_BYTES as i64) as *mut u8;
    let new_base = unsafe { std::alloc::realloc(cur_base, old_layout, new_total) };
    if new_base.is_null() {
        std::alloc::handle_alloc_error(layout_for_total(new_total));
    }
    let new_body = unsafe { new_base.add(HEADER_BYTES) } as i64;
    unsafe {
        write_header(new_base, new_total, 1, new_len as i64);
        std::ptr::copy_nonoverlapping(
            sb.as_ptr(),
            (new_body as *mut u8).add(a_len),
            sb.len(),
        );
        *((new_body as *mut u8).add(a_len + sb.len())) = 0;
    }
    new_body
}

#[unsafe(export_name = "$string.eq")]
pub extern "C" fn __str_eq(a: i64, b: i64) -> i64 {
    if a == b {
        return 1;
    }
    let sa = unsafe { cstr_bytes(a) };
    let sb = unsafe { cstr_bytes(b) };
    if sa == sb { 1 } else { 0 }
}

#[unsafe(export_name = "$string.fromInt")]
pub extern "C" fn __int_to_string(n: i64) -> i64 {
    leak_cstring(n.to_string())
}

#[unsafe(export_name = "$string.fromBool")]
pub extern "C" fn __bool_to_string(b: i64) -> i64 {
    leak_cstr_bytes(if b != 0 { b"true" } else { b"false" })
}

/// `(f32).toString()` — per-width entry point because cranelift's
/// float-arg ABI distinguishes f32 from f64. f32 widens to f64 (exact
/// for every f32 bit pattern) and reuses the f64 formatting so the
/// output matches `console.log` / template-literal `${x}`.
#[unsafe(export_name = "$string.fromF32")]
pub extern "C" fn __float_to_string_f32(x: f32) -> i64 {
    __float_to_string_f64(x as f64)
}

/// `(f64).toString()` — JS-style: NaN / ±Infinity, `1.0` keeps the
/// trailing `.0`, otherwise Rust's default `{f64}` formatting. Mirrors
/// `$fmt.f64` so `x.toString()` and `` `${x}` `` produce the same string.
#[unsafe(export_name = "$string.fromF64")]
pub extern "C" fn __float_to_string_f64(x: f64) -> i64 {
    let s = if x.is_nan() {
        "NaN".to_string()
    } else if x.is_infinite() {
        if x > 0.0 { "Infinity".to_string() } else { "-Infinity".to_string() }
    } else if x == x.trunc() && x.abs() < 1e16 {
        format!("{}.0", x as i64)
    } else {
        format!("{x}")
    };
    leak_cstring(s)
}

#[unsafe(export_name = "$string.toUpper")]
pub extern "C" fn __str_to_upper(p: i64) -> i64 {
    leak_cstring(cstr_to_str(p).to_uppercase())
}

#[unsafe(export_name = "$string.toLower")]
pub extern "C" fn __str_to_lower(p: i64) -> i64 {
    leak_cstring(cstr_to_str(p).to_lowercase())
}

#[unsafe(export_name = "$string.trim")]
pub extern "C" fn __str_trim(p: i64) -> i64 {
    leak_cstr_bytes(cstr_to_str(p).trim().as_bytes())
}

#[unsafe(export_name = "$string.includes")]
pub extern "C" fn __str_includes(p: i64, q: i64) -> i64 {
    if cstr_to_str(p).contains(cstr_to_str(q)) { 1 } else { 0 }
}

#[unsafe(export_name = "$string.startsWith")]
pub extern "C" fn __str_starts_with(p: i64, q: i64) -> i64 {
    if cstr_to_str(p).starts_with(cstr_to_str(q)) { 1 } else { 0 }
}

#[unsafe(export_name = "$string.endsWith")]
pub extern "C" fn __str_ends_with(p: i64, q: i64) -> i64 {
    if cstr_to_str(p).ends_with(cstr_to_str(q)) { 1 } else { 0 }
}

#[unsafe(export_name = "$string.charAt")]
pub extern "C" fn __str_char_at(p: i64, idx: i64) -> i64 {
    let s = cstr_to_str(p);
    let c = s.chars().nth(idx as usize);
    leak_cstring(c.map(|c| c.to_string()).unwrap_or_default())
}

#[unsafe(export_name = "$string.slice")]
pub extern "C" fn __str_slice(p: i64, start: i64, end: i64) -> i64 {
    let s = cstr_to_str(p);
    let chars: Vec<char> = s.chars().collect();
    let lo = (start.max(0) as usize).min(chars.len());
    let hi = (end.max(0) as usize).min(chars.len());
    let lo = lo.min(hi);
    leak_cstring(chars[lo..hi].iter().collect::<String>())
}

/// `from_index == i64::MIN` is the "omitted" sentinel emitted by the
/// MIR when the caller did not pass a `fromIndex` argument. JS-style
/// clamp otherwise: negative values are treated as 0, values past the
/// string length fall through to the not-found / empty-needle cases.
#[unsafe(export_name = "$string.indexOf")]
pub extern "C" fn __str_index_of(p: i64, needle: i64, from_index: i64) -> i64 {
    let s = cstr_to_str(p);
    let n = cstr_to_str(needle);
    let chars: Vec<char> = s.chars().collect();
    let total = chars.len();
    let start = if from_index == i64::MIN || from_index < 0 {
        0
    } else if (from_index as usize) > total {
        return if n.is_empty() { total as i64 } else { -1 };
    } else {
        from_index as usize
    };
    if n.is_empty() {
        return start as i64;
    }
    let n_chars: Vec<char> = n.chars().collect();
    if n_chars.len() > total - start {
        return -1;
    }
    let last = total - n_chars.len();
    for i in start..=last {
        if chars[i..i + n_chars.len()] == n_chars[..] {
            return i as i64;
        }
    }
    -1
}

/// `from_index == i64::MIN` means "omitted" — search starts from the
/// end. Otherwise JS-style clamp: negative becomes 0, values past the
/// end clamp to the end.
#[unsafe(export_name = "$string.lastIndexOf")]
pub extern "C" fn __str_last_index_of(p: i64, needle: i64, from_index: i64) -> i64 {
    let s = cstr_to_str(p);
    let n = cstr_to_str(needle);
    let chars: Vec<char> = s.chars().collect();
    let total = chars.len();
    let n_chars: Vec<char> = n.chars().collect();
    let upper = if from_index == i64::MIN {
        total
    } else if from_index < 0 {
        0
    } else if (from_index as usize) > total {
        total
    } else {
        from_index as usize
    };
    if n_chars.is_empty() {
        return upper as i64;
    }
    if n_chars.len() > total {
        return -1;
    }
    let max_start = total - n_chars.len();
    let mut i = upper.min(max_start);
    loop {
        if chars[i..i + n_chars.len()] == n_chars[..] {
            return i as i64;
        }
        if i == 0 {
            return -1;
        }
        i -= 1;
    }
}

#[unsafe(export_name = "$string.replace")]
pub extern "C" fn __str_replace(p: i64, from: i64, to: i64) -> i64 {
    let s = cstr_to_str(p);
    let f = cstr_to_str(from);
    let t = cstr_to_str(to);
    leak_cstring(s.replace(f, t))
}

#[unsafe(export_name = "$string.split")]
pub extern "C" fn __str_split(p: i64, sep: i64) -> i64 {
    let s = cstr_to_str(p);
    let sp = cstr_to_str(sep);
    let parts: Vec<i64> = if sp.is_empty() {
        // Encode each char into a stack buffer instead of minting a
        // throwaway `String` per character.
        let mut buf = [0u8; 4];
        s.chars()
            .map(|c| leak_cstr_bytes(c.encode_utf8(&mut buf).as_bytes()))
            .collect()
    } else {
        // Each piece is already a `&str` slice into `s` — copy its bytes
        // straight into the ilang string, no intermediate `String`.
        s.split(sp).map(|t| leak_cstr_bytes(t.as_bytes())).collect()
    };
    build_i64_array(&parts, KIND_STR)
}

/// `string.fromUtf16(units)` — interpret `units: u16[]` as a
/// UTF-16 code-unit sequence and return a fresh UTF-8 string. The
/// whole buffer is consumed: a trailing `0x0000` (if present) is
/// included as a literal NUL character in the result, so callers
/// that want strict round-trip with `encodeUtf16()` (which
/// defaults to NUL-terminated) should pass `encodeUtf16(false)`.
/// Invalid UTF-16 (unpaired surrogates) is replaced with U+FFFD —
/// `from_utf16_lossy` matches what Win32 / WTF-16 sources can
/// legitimately produce.
#[unsafe(export_name = "$string.fromUtf16")]
pub extern "C" fn __str_from_utf16(arr: i64) -> i64 {
    if arr == 0 {
        return leak_cstring(String::new());
    }
    // u16[] layout: [ len | cap | data_ptr | rc | kind ] — same
    // header shape used by `fs_write_file_bytes` (see arrays.rs).
    let s = unsafe {
        let len = *(arr as *const i64) as usize;
        let data_ptr = *((arr + 16) as *const i64) as *const u16;
        if len == 0 || data_ptr.is_null() {
            String::new()
        } else {
            let slice = std::slice::from_raw_parts(data_ptr, len);
            String::from_utf16_lossy(slice)
        }
    };
    leak_cstring(s)
}

/// `s.encodeUtf16(nulTerminated = true)` — encode the UTF-8 string
/// as UTF-16 code units and return a fresh `u16[]`. When
/// `nul_terminated != 0` the buffer ends with an extra `0x0000`
/// so it can be passed straight to Win32 W-suffix APIs (or any
/// other API that wants an LPCWSTR). The pad is what makes the
/// common Win32 path one-liner:
///
///   SetWindowTextW(hwnd, text.encodeUtf16())   // *const u16 coerce
///
/// `__c_array_to_array` *copies* `v.as_ptr()` into a fresh ilang
/// array body, so `v` going out of scope at function return is
/// fine — the returned array owns its own storage.
#[unsafe(export_name = "$string.encodeUtf16")]
pub extern "C" fn __str_encode_utf16(p: i64, nul_terminated: i64) -> i64 {
    let s = cstr_to_str(p);
    let mut v: Vec<u16> = s.encode_utf16().collect();
    if nul_terminated != 0 {
        v.push(0);
    }
    __c_array_to_array(v.as_ptr() as i64, v.len() as i64, 2, KIND_NONE)
}

// --------------------------------------------------------------------
// C-string interop helpers
// --------------------------------------------------------------------
//
// ilang strings carry an `[ i64 len | bytes | \0 ]` body pointer; C
// strings are bare NUL-terminated buffers. The helpers below bridge
// the two formats so `@extern(C)` calls can pass / receive C strings
// without forcing every binding site to peek at the layout.

unsafe fn raw_cstr_bytes<'a>(p: i64) -> &'a [u8] { unsafe {
    if p == 0 {
        return &[];
    }
    let mut len = 0;
    let q = p as *const u8;
    while *q.add(len) != 0 {
        len += 1;
    }
    std::slice::from_raw_parts(q, len)
}}

#[unsafe(export_name = "$ffi.cstrFromString")]
pub extern "C" fn cstr_from_string(p: i64) -> i64 { p }

#[unsafe(export_name = "$ffi.stringFromCstr")]
pub extern "C" fn string_from_cstr(p: i64) -> i64 {
    if p == 0 {
        return leak_cstring(String::new());
    }
    let bytes = unsafe { raw_cstr_bytes(p) };
    leak_cstring(String::from_utf8_lossy(bytes).into_owned())
}

#[unsafe(export_name = "$ffi.cstrArrayToStrings")]
pub extern "C" fn cstr_array_to_strings(ptrs: i64) -> i64 {
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
    build_i64_array(&elems, KIND_STR)
}
