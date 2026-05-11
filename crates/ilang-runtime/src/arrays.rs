//! Array layout + leaf operations + higher-order helpers.
//!
//! Array header (48 bytes):
//!   +0   length (i64)
//!   +8   capacity (i64)
//!   +16  data pointer
//!   +24  refcount
//!   +32  element KIND_* tag (for `__release_array` cascade)
//!   +40  stride bytes per cell

use crate::alloc::{__mir_alloc, __mir_free};
use crate::cascade::{release_field_by_kind, retain_field_by_kind};
use crate::kind::KIND_NONE;

#[inline]
pub(crate) unsafe fn array_header(arr: i64) -> (i64, i64, i64) {
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

/// Returns the popped value as Optional<T>: a 3-cell heap
/// `[value | rc | kind_tag]`, or 0 (none). Inherits the array's
/// elem kind tag so the Optional's cascade drops the cell properly.
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
        let value: i64 = match stride {
            1 => *(addr as *const u8) as i64,
            2 => *(addr as *const u16) as i64,
            4 => *(addr as *const u32) as i64,
            _ => *(addr as *const i64),
        };
        *h = idx;
        let elem_tag = *h.add(4);
        let cell = __mir_alloc(24) as *mut i64;
        *cell = value;
        *cell.add(1) = 1;
        *cell.add(2) = elem_tag;
        cell as i64
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

/// Retain an array (`++rc`).
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
/// Cascade-releases each stored element via `release_field_by_kind`.
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
    if tag != KIND_NONE {
        for i in 0..len {
            let cell = unsafe { *((data_ptr + i * 8) as *const i64) };
            release_field_by_kind(cell, tag);
        }
    }
    if data_ptr != 0 {
        __mir_free(data_ptr, cap.max(1) * stride);
    }
    __mir_free(arr_ptr, 48);
}

// --------------------------------------------------------------------
// Construction + higher-order helpers
// --------------------------------------------------------------------

/// Build an i64-cell array from a slice. `elem_kind` is the KIND_*
/// tag stored at +32 so `__release_array` cascades correctly.
pub(crate) fn build_i64_array(items: &[i64], elem_kind: i64) -> i64 {
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

/// Wrap an inline fixed-length array (a bare `ptr` to `len` elements
/// of `stride` bytes each) into a dynamic-array header.
#[unsafe(no_mangle)]
pub extern "C" fn __fixed_to_dyn(ptr: i64, len: i64, stride: i64, kind_tag: i64) -> i64 {
    let header = __mir_alloc(48);
    unsafe {
        let h = header as *mut i64;
        *h = len;
        *h.add(1) = len;
        *h.add(2) = ptr;
        *h.add(3) = 1;
        *h.add(4) = kind_tag;
        *h.add(5) = stride;
    }
    header
}

/// Invoke a closure (`[fn_ptr | rc | captures...]` block pointer)
/// with one arg and the trailing env pointer.
unsafe fn call_closure_1(closure: i64, arg: i64) -> i64 {
    unsafe {
        let fn_ptr = *(closure as *const i64);
        let f: extern "C" fn(i64, i64) -> i64 = std::mem::transmute(fn_ptr);
        f(arg, closure)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __array_map(arr: i64, closure: i64, result_kind: i64) -> i64 {
    if arr == 0 || closure == 0 {
        return build_i64_array(&[], result_kind);
    }
    let (len, _cap, data) = unsafe { array_header(arr) };
    let mut out: Vec<i64> = Vec::with_capacity(len as usize);
    for i in 0..len {
        let cell = unsafe { *((data + i * 8) as *const i64) };
        let v = unsafe { call_closure_1(closure, cell) };
        out.push(v);
    }
    build_i64_array(&out, result_kind)
}

#[unsafe(no_mangle)]
pub extern "C" fn __array_filter(arr: i64, closure: i64) -> i64 {
    if arr == 0 || closure == 0 {
        return build_i64_array(&[], KIND_NONE);
    }
    let (len, _cap, data) = unsafe { array_header(arr) };
    let elem_kind = unsafe { *((arr + 32) as *const i64) };
    let mut out: Vec<i64> = Vec::new();
    for i in 0..len {
        let cell = unsafe { *((data + i * 8) as *const i64) };
        let keep = unsafe { call_closure_1(closure, cell) };
        if keep != 0 {
            if elem_kind != KIND_NONE {
                retain_field_by_kind(cell, elem_kind);
            }
            out.push(cell);
        }
    }
    build_i64_array(&out, elem_kind)
}

#[unsafe(no_mangle)]
pub extern "C" fn __array_slice(arr: i64, start: i64, end: i64) -> i64 {
    if arr == 0 {
        return build_i64_array(&[], KIND_NONE);
    }
    let (len, _cap, data) = unsafe { array_header(arr) };
    let elem_kind = unsafe { *((arr + 32) as *const i64) };
    let lo = start.max(0).min(len) as usize;
    let hi = end.max(0).min(len) as usize;
    let lo = lo.min(hi);
    let mut out: Vec<i64> = Vec::with_capacity(hi - lo);
    for i in lo..hi {
        let cell = unsafe { *((data + (i as i64) * 8) as *const i64) };
        if elem_kind != KIND_NONE {
            retain_field_by_kind(cell, elem_kind);
        }
        out.push(cell);
    }
    build_i64_array(&out, elem_kind)
}

#[unsafe(no_mangle)]
pub extern "C" fn __array_for_each(arr: i64, closure: i64) {
    if arr == 0 || closure == 0 {
        return;
    }
    let (len, _cap, data) = unsafe { array_header(arr) };
    for i in 0..len {
        let cell = unsafe { *((data + i * 8) as *const i64) };
        unsafe { call_closure_1(closure, cell) };
    }
}

