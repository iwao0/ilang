//! String layout helpers + heap registry + `__str_*` operations +
//! C-string interop. Strings live as `[ i64 len | bytes | \0 ]` with
//! body pointer past the prefix; `leak_cstring` produces them, the
//! registry tracks the buffer for the matching `__release_string`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::arrays::build_i64_array;
use crate::kind::KIND_STR;

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
// String heap allocator and registry
// --------------------------------------------------------------------

struct StringBacking {
    base: *mut u8,
    total: usize,
}
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
    #[allow(dead_code)]
    backing: StringBacking,
    rc: i64,
}

static STRING_REGISTRY: OnceLock<Mutex<HashMap<i64, StringEntry>>> = OnceLock::new();

fn string_registry_lock() -> &'static Mutex<HashMap<i64, StringEntry>> {
    STRING_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Allocate a heap string and register it at `rc = 1`. Returns the
/// body pointer (the user-visible string pointer).
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

/// Number of live entries in the string registry.
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
#[unsafe(no_mangle)]
pub extern "C" fn __str_concat_inplace(a: i64, b: i64) -> i64 {
    if a == 0 {
        // No backing yet — fall back to the regular path.
        return __str_concat(a, b);
    }
    let sb = unsafe { cstr_bytes(b) };
    let a_len = unsafe { *((a - 8) as *const i64) } as usize;
    let new_len = a_len + sb.len();
    let needed_total = 8 + new_len + 1;
    let mut reg = string_registry_lock().lock().expect("string registry poisoned");
    // Snapshot the raw pointer + capacity without cloning the
    // entry (StringBacking owns the buffer through Drop — copying
    // it would dealloc twice). Defensive `rc == 1` re-check: the
    // MIR pattern matcher only fires for Locals (closure-captured
    // strings stay on the regular path), but a user-side
    // `let t = s; s = s + "x"` keeps a's rc at 2 — falling back to
    // the allocating `__str_concat` keeps the alias safe.
    let (cur_base, cur_total) = match reg.get(&a) {
        Some(e) if e.rc == 1 => (e.backing.base, e.backing.total),
        Some(_) => {
            drop(reg);
            return __str_concat(a, b);
        }
        None => {
            // Not in registry (e.g. a static literal pointer that
            // the codegen handed us). Fall back to a fresh
            // allocation.
            drop(reg);
            return __str_concat(a, b);
        }
    };
    // Fast path: spare capacity already covers the result. Copy
    // bytes in, bump the length prefix, leave the registry entry
    // alone. Returned pointer is the same body_ptr.
    if cur_total >= needed_total {
        unsafe {
            std::ptr::copy_nonoverlapping(
                sb.as_ptr(),
                (a as *mut u8).add(a_len),
                sb.len(),
            );
            *((a as *mut u8).add(a_len + sb.len())) = 0;
            *(cur_base as *mut i64) = new_len as i64;
        }
        return a;
    }
    // Grow with doubling — guarantees amortised O(1) appends. The
    // `realloc` may move the buffer; in that case the body pointer
    // changes, so the registry key has to move too. We pull the
    // entry out of the registry before reallocating so that
    // StringBacking's Drop won't fire on the old base when we
    // re-insert the new one.
    let old_entry = reg.remove(&a).expect("entry presence checked above");
    let new_total = needed_total.max(cur_total.saturating_mul(2));
    let old_layout = std::alloc::Layout::from_size_align(cur_total.max(8), 8).unwrap();
    // Forget the old StringBacking so its Drop doesn't deallocate
    // the buffer realloc is about to consume.
    std::mem::forget(old_entry.backing);
    let new_base = unsafe { std::alloc::realloc(cur_base, old_layout, new_total) };
    if new_base.is_null() {
        let layout = std::alloc::Layout::from_size_align(new_total, 8).unwrap();
        std::alloc::handle_alloc_error(layout);
    }
    let new_body = unsafe { new_base.add(8) } as i64;
    unsafe {
        std::ptr::copy_nonoverlapping(
            sb.as_ptr(),
            (new_body as *mut u8).add(a_len),
            sb.len(),
        );
        *((new_body as *mut u8).add(a_len + sb.len())) = 0;
        *(new_base as *mut i64) = new_len as i64;
    }
    reg.insert(
        new_body,
        StringEntry {
            backing: StringBacking { base: new_base, total: new_total },
            rc: 1,
        },
    );
    new_body
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

#[unsafe(no_mangle)]
pub extern "C" fn __str_split(p: i64, sep: i64) -> i64 {
    let s = cstr_to_str(p);
    let sp = cstr_to_str(sep);
    let parts: Vec<i64> = if sp.is_empty() {
        s.chars().map(|c| leak_cstring(c.to_string())).collect()
    } else {
        s.split(sp).map(|t| leak_cstring(t.to_string())).collect()
    };
    build_i64_array(&parts, KIND_STR)
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

#[unsafe(export_name = "cstrFromString")]
pub extern "C" fn cstr_from_string(p: i64) -> i64 { p }

#[unsafe(export_name = "stringFromCstr")]
pub extern "C" fn string_from_cstr(p: i64) -> i64 {
    if p == 0 {
        return leak_cstring(String::new());
    }
    let bytes = unsafe { raw_cstr_bytes(p) };
    leak_cstring(String::from_utf8_lossy(bytes).into_owned())
}

#[unsafe(export_name = "cstrArrayToStrings")]
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
