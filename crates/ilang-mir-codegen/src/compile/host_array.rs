//! Array host runtime.
//!
//! ilang arrays use a 48-byte header (`[ len | cap | data_ptr | rc |
//! elem_kind | stride ]`) followed by `cap × stride` bytes of packed
//! element storage. The helpers here implement the user-callable
//! operations (push / pop / map / filter / slice / split / indexOf /
//! ...), the `build_array` constructor used by other host paths
//! (split, map values), and the C-array / fixed-length-array bridge
//! helpers used at FFI call sites.

use ilang_runtime::{cstr_to_str, leak_cstring};

use super::{retain_by_kind, KIND_NONE, KIND_STR};

/// Raw C-string scanner — for pointers crossing the FFI boundary
/// from C land (e.g. `getenv()`, char** array elements). These have
/// no length prefix; we walk to the first NUL.
pub(super) unsafe fn raw_cstr_bytes<'a>(p: i64) -> &'a [u8] { unsafe {
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

/// Read back the front of the dyn-array header.
pub(super) unsafe fn array_header(arr: i64) -> (i64, i64, i64) { unsafe {
    let p = arr as *const i64;
    (*p, *p.add(1), *p.add(2))
}}

pub(super) extern "C" fn host_array_index_of(arr: i64, value: i64) -> i64 {
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

pub(super) extern "C" fn host_array_includes(arr: i64, value: i64) -> i64 {
    if host_array_index_of(arr, value) >= 0 { 1 } else { 0 }
}

/// Write a value into a packed array slot at `data + idx*stride`,
/// truncating the i64 source down to the stride width.
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

pub(super) extern "C" fn host_array_push(arr: i64, value: i64) {
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
            let new_data = ilang_runtime::__mir_alloc(new_cap * stride);
            std::ptr::copy_nonoverlapping(
                data as *const u8,
                new_data as *mut u8,
                (len * stride) as usize,
            );
            store_packed(new_data, len, stride, value);
            // Free the old data buffer — without this, every grow
            // leaks the previous backing store. log2(N) grows for
            // N pushes, so a long loop accumulates ~2*N*stride
            // unreachable bytes.
            if data != 0 && cap > 0 {
                ilang_runtime::__mir_free(data, cap * stride);
            }
            *h = len + 1;
            *h.add(1) = new_cap;
            *h.add(2) = new_data;
        }
    }
}

/// Construct a new i64-cell array (48-byte header, stride 8) from an
/// i64 slice. Used by helpers that produce string[] / i64[] results.
/// `elem_kind` should be the KIND_* tag for the element type so
/// `host_release_array` can cascade-release the contents on drop
/// (e.g. KIND_STR for split() results, KIND_OBJECT for
/// Map<_, ClassT>.values()).
pub(super) fn build_array(items: &[i64], elem_kind: i64) -> i64 {
    let cap = items.len().max(4);
    let header = ilang_runtime::__mir_alloc(48);
    let data = ilang_runtime::__mir_alloc((cap * 8) as i64);
    unsafe {
        let h = header as *mut i64;
        *h = items.len() as i64;
        *h.add(1) = cap as i64;
        *h.add(2) = data;
        *h.add(3) = 1; // rc
        *h.add(4) = elem_kind;
        *h.add(5) = 8; // stride
        for (i, v) in items.iter().enumerate() {
            *((data + (i as i64) * 8) as *mut i64) = *v;
        }
    }
    header
}

/// Invoke a closure (`[fn_ptr | captures...]` block pointer) with one
/// arg and the trailing env pointer. The fn signature follows the
/// unified ABI: `extern "C" fn(arg, env_ptr) -> i64`.
unsafe fn call_closure_1(closure: i64, arg: i64) -> i64 { unsafe {
    let fn_ptr = *(closure as *const i64);
    let f: extern "C" fn(i64, i64) -> i64 = std::mem::transmute(fn_ptr);
    f(arg, closure)
}}

/// `arrayFromCArray<T>(src, n, stride, kind_tag)` — copy `n × stride`
/// bytes from a C-side array into a fresh ilang dyn-array `T[]`.
/// The lower side picks `stride` from T's MirTy so the host doesn't
/// need to know T.
pub(super) extern "C" fn host_c_array_to_array(src: i64, n: i64, stride: i64, kind_tag: i64) -> i64 {
    let n_safe = if n < 0 { 0 } else { n };
    let bytes = n_safe * stride;
    let header = ilang_runtime::__mir_alloc(48);
    let data = ilang_runtime::__mir_alloc(bytes.max(stride));
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

/// Wrap an inline fixed-length array (a bare `ptr` to `len` elements
/// of `stride` bytes each) into a dynamic-array header that the
/// host_array_* helpers expect. Used at builtin call sites where the
/// MIR arg type is `Array { len: Some(n), .. }` — those have no
/// header, just the raw element block. The wrapper is freshly heap-
/// allocated and considered owned by the caller (release rules apply
/// as for any other freshly-built array).
pub(super) extern "C" fn host_fixed_to_dyn(ptr: i64, len: i64, stride: i64, kind_tag: i64) -> i64 {
    let header = ilang_runtime::__mir_alloc(48);
    unsafe {
        let h = header as *mut i64;
        *h = len;            // len
        *h.add(1) = len;     // cap
        *h.add(2) = ptr;     // data_ptr (alias — no copy)
        *h.add(3) = 1;       // rc
        *h.add(4) = kind_tag;
        *h.add(5) = stride;
    }
    header
}

pub(super) extern "C" fn host_array_map(arr: i64, closure: i64, result_kind: i64) -> i64 {
    if arr == 0 || closure == 0 {
        return build_array(&[], result_kind);
    }
    let (len, _cap, data) = unsafe { array_header(arr) };
    let mut out = Vec::with_capacity(len as usize);
    for i in 0..len {
        let cell = unsafe { *((data + i * 8) as *const i64) };
        let v = unsafe { call_closure_1(closure, cell) };
        out.push(v);
    }
    // result_kind is the closure's return MirTy's KIND_* tag,
    // threaded in by the lower side. Lets the result array's
    // drop cascade-release each closure-produced value.
    build_array(&out, result_kind)
}

pub(super) extern "C" fn host_array_filter(arr: i64, closure: i64) -> i64 {
    if arr == 0 || closure == 0 {
        return build_array(&[], KIND_NONE);
    }
    let (len, _cap, data) = unsafe { array_header(arr) };
    let elem_kind = unsafe { *((arr + 32) as *const i64) };
    let mut out = Vec::new();
    for i in 0..len {
        let cell = unsafe { *((data + i * 8) as *const i64) };
        let keep = unsafe { call_closure_1(closure, cell) };
        if keep != 0 {
            // Filter passes through source elements unchanged —
            // share their +1 by retaining the kept ones so both
            // the source array (when it drops) and the result
            // array (when it drops) account for the reference.
            if elem_kind != KIND_NONE {
                retain_by_kind(cell, elem_kind);
            }
            out.push(cell);
        }
    }
    build_array(&out, elem_kind)
}

pub(super) extern "C" fn host_array_slice(arr: i64, start: i64, end: i64) -> i64 {
    if arr == 0 {
        return build_array(&[], KIND_NONE);
    }
    let (len, _cap, data) = unsafe { array_header(arr) };
    let elem_kind = unsafe { *((arr + 32) as *const i64) };
    let lo = start.max(0).min(len) as usize;
    let hi = end.max(0).min(len) as usize;
    let lo = lo.min(hi);
    let mut out: Vec<i64> = Vec::with_capacity(hi - lo);
    for i in lo..hi {
        let cell = unsafe { *((data + (i as i64) * 8) as *const i64) };
        // Slice copies element references — retain so both arrays
        // own the reference (mirrors filter).
        if elem_kind != KIND_NONE {
            retain_by_kind(cell, elem_kind);
        }
        out.push(cell);
    }
    build_array(&out, elem_kind)
}

pub(super) extern "C" fn host_array_for_each(arr: i64, closure: i64) {
    if arr == 0 || closure == 0 {
        return;
    }
    let (len, _cap, data) = unsafe { array_header(arr) };
    for i in 0..len {
        let cell = unsafe { *((data + i * 8) as *const i64) };
        unsafe { call_closure_1(closure, cell) };
    }
}

pub(super) extern "C" fn host_str_split(p: i64, sep: i64) -> i64 {
    let s = cstr_to_str(p);
    let sp = cstr_to_str(sep);
    let parts: Vec<i64> = if sp.is_empty() {
        // Empty separator → split per character (matching syntax.md).
        s.chars().map(|c| leak_cstring(c.to_string())).collect()
    } else {
        s.split(sp).map(|t| leak_cstring(t.to_string())).collect()
    };
    // Each part is a fresh leak_cstring entry — tag the array as
    // KIND_STR so dropping it cascades release_string and reclaims
    // every part.
    build_array(&parts, KIND_STR)
}

pub(super) extern "C" fn host_array_pop(arr: i64) -> i64 {
    // Returns the popped value as Optional<T>: a 3-cell heap
    // [value | rc | kind_tag], or 0 (none). Inherits the array's
    // elem kind tag so cascade deinit works on Optional drop.
    if arr == 0 {
        return 0;
    }
    unsafe {
        let h = arr as *mut i64;
        let len = *h;
        if len == 0 {
            return 0;
        }
        let data = *h.add(2);
        let stride = *h.add(5);
        let addr = (data + (len - 1) * stride) as *const u8;
        let v: i64 = match stride {
            1 => *(addr as *const u8) as i64,
            2 => *(addr as *const u16) as i64,
            4 => *(addr as *const u32) as i64,
            _ => *(addr as *const i64),
        };
        *h = len - 1;
        let elem_tag = *h.add(4);
        let cell = ilang_runtime::__mir_alloc(24) as *mut i64;
        *cell = v;
        *cell.add(1) = 1;
        *cell.add(2) = elem_tag;
        cell as i64
    }
}

/// `__array_data_ptr(arr)` — return the i64 byte address of the
/// array's data buffer (header offset 16 holds it).
pub(super) extern "C" fn host_array_data_ptr(arr: i64) -> i64 {
    if arr == 0 {
        return 0;
    }
    unsafe { *((arr + 16) as *const i64) }
}
