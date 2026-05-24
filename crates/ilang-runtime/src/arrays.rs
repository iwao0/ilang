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
use crate::strings::{cstr_to_str, leak_cstring};

#[inline]
pub(crate) unsafe fn array_header(arr: i64) -> (i64, i64, i64) {
    unsafe {
        let p = arr as *const i64;
        (*p, *p.add(1), *p.add(2))
    }
}

#[unsafe(export_name = "$array.indexOf")]
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

#[unsafe(export_name = "$array.includes")]
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

#[unsafe(export_name = "$array.push")]
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
#[unsafe(export_name = "$array.pop")]
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

/// Shift cells [from..len) down by one slot in-place. Caller is
/// responsible for the length decrement and for releasing the
/// element that lived at `from-1` before the shift.
#[inline]
unsafe fn shift_left_one(data: i64, from: i64, len: i64, stride: i64) {
    unsafe {
        if from >= len {
            return;
        }
        let dst = (data + (from - 1) * stride) as *mut u8;
        let src = (data + from * stride) as *const u8;
        let bytes = ((len - from) * stride) as usize;
        std::ptr::copy(src, dst, bytes);
    }
}

/// Remove the cell at `index` and return it as `Optional<T>`
/// (3-cell heap box `[value | rc | kind_tag]`, or 0 for none when
/// `index` is out of `[0, len)`). The Optional inherits the array's
/// per-cell refcount — no retain on the removed value, no release;
/// the array's length drops by one so the slot's old reference is
/// effectively handed to the returned Optional.
#[unsafe(export_name = "$array.removeAt")]
pub extern "C" fn __array_remove_at(arr: i64, index: i64) -> i64 {
    if arr == 0 {
        return 0;
    }
    unsafe {
        let h = arr as *mut i64;
        let len = *h;
        if index < 0 || index >= len {
            return 0;
        }
        let data = *h.add(2);
        let stride = *h.add(5);
        let addr = (data + index * stride) as *const u8;
        let value: i64 = match stride {
            1 => *(addr as *const u8) as i64,
            2 => *(addr as *const u16) as i64,
            4 => *(addr as *const u32) as i64,
            _ => *(addr as *const i64),
        };
        shift_left_one(data, index + 1, len, stride);
        *h = len - 1;
        let elem_tag = *h.add(4);
        let cell = __mir_alloc(24) as *mut i64;
        *cell = value;
        *cell.add(1) = 1;
        *cell.add(2) = elem_tag;
        cell as i64
    }
}

/// Remove the first cell whose stored value equals `value` and
/// return `1` on success, `0` when no element matched. The array
/// drops its reference to the matched cell (release on heap kinds)
/// since the value isn't handed back to the caller.
#[unsafe(export_name = "$array.remove")]
pub extern "C" fn __array_remove(arr: i64, value: i64) -> i64 {
    if arr == 0 {
        return 0;
    }
    unsafe {
        let h = arr as *mut i64;
        let len = *h;
        let data = *h.add(2);
        let stride = *h.add(5);
        let mut idx: i64 = -1;
        for i in 0..len {
            let addr = (data + i * stride) as *const u8;
            let cell: i64 = match stride {
                1 => *(addr as *const u8) as i64,
                2 => *(addr as *const u16) as i64,
                4 => *(addr as *const u32) as i64,
                _ => *(addr as *const i64),
            };
            if cell == value {
                idx = i;
                break;
            }
        }
        if idx < 0 {
            return 0;
        }
        let elem_tag = *h.add(4);
        if elem_tag != KIND_NONE {
            release_field_by_kind(value, elem_tag);
        }
        shift_left_one(data, idx + 1, len, stride);
        *h = len - 1;
        1
    }
}

#[unsafe(export_name = "$array.dataPtr")]
pub extern "C" fn __array_data_ptr(arr: i64) -> i64 {
    if arr == 0 {
        return 0;
    }
    unsafe { *((arr + 16) as *const i64) }
}

/// `bytesFromBuffer(p, n)` — copy `n` bytes from `p` into a fresh
/// `u8[]`. Thin wrapper over `__c_array_to_array` with stride=1
/// and kind_tag=0 (PK_INT/byte). Exposed as an ilang-callable FFI
/// helper so `@extern(C)` bindings can build an owned byte array
/// from a C function's `(const char *, size_t)` output without
/// hand-rolling the array header.
#[unsafe(export_name = "bytesFromBuffer")]
pub extern "C" fn bytes_from_buffer(p: i64, n: i64) -> i64 {
    __c_array_to_array(p, n, 1, 0)
}

/// `arrayFromCArray<T>(src, n, stride, kind_tag)` — copy `n × stride`
/// bytes from a C-side array into a fresh ilang dyn-array.
#[unsafe(export_name = "$array.fromCArray")]
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
#[unsafe(export_name = "$array.retain")]
pub extern "C" fn __retain_array(arr_ptr: i64) {
    if arr_ptr == 0 {
        return;
    }
    let rc_ptr = (arr_ptr + 24) as *mut i64;
    unsafe { crate::refcount::atomic_retain(rc_ptr) };
}

/// Release an array (`--rc`); free header + data buffer at rc 0.
/// Cascade-releases each stored element via `release_field_by_kind`.
#[unsafe(export_name = "$array.release")]
pub extern "C" fn __release_array(arr_ptr: i64) {
    if arr_ptr == 0 {
        return;
    }
    let rc_ptr = (arr_ptr + 24) as *mut i64;
    match unsafe { crate::refcount::atomic_release(rc_ptr) } {
        Some(0) => {}
        _ => return,
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
#[unsafe(export_name = "$array.fixedToDyn")]
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

/// Invoke a closure with two args plus the trailing env pointer.
/// Used by `__array_sort`'s comparator callback.
unsafe fn call_closure_2(closure: i64, a: i64, b: i64) -> i64 {
    unsafe {
        let fn_ptr = *(closure as *const i64);
        let f: extern "C" fn(i64, i64, i64) -> i64 = std::mem::transmute(fn_ptr);
        f(a, b, closure)
    }
}

/// Box an array cell into a fresh `Optional<T>` heap block
/// (`[value | rc | kind_tag]`). The cell counts as one new
/// reference: bump rc on heap kinds before boxing so the Optional
/// owns its own +1 alongside the array's existing one.
fn box_optional_cell(value: i64, elem_tag: i64) -> i64 {
    if elem_tag != KIND_NONE {
        retain_field_by_kind(value, elem_tag);
    }
    let cell = __mir_alloc(24) as *mut i64;
    unsafe {
        *cell = value;
        *cell.add(1) = 1;
        *cell.add(2) = elem_tag;
    }
    cell as i64
}

#[unsafe(export_name = "$array.map")]
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

#[unsafe(export_name = "$array.filter")]
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

#[unsafe(export_name = "$array.slice")]
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

#[unsafe(export_name = "$array.forEach")]
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

/// First cell for which `pred(cell)` returns non-zero, boxed as
/// `Optional<T>`. Empty / no-match → 0 (`none`). The array keeps
/// its own reference to the matched cell, so the Optional grabs
/// a fresh +1 via `retain_field_by_kind` before boxing.
#[unsafe(export_name = "$array.find")]
pub extern "C" fn __array_find(arr: i64, closure: i64) -> i64 {
    if arr == 0 || closure == 0 {
        return 0;
    }
    let (len, _cap, data) = unsafe { array_header(arr) };
    let elem_kind = unsafe { *((arr + 32) as *const i64) };
    for i in 0..len {
        let cell = unsafe { *((data + i * 8) as *const i64) };
        let hit = unsafe { call_closure_1(closure, cell) };
        if hit != 0 {
            return box_optional_cell(cell, elem_kind);
        }
    }
    0
}

/// Index of the first cell for which `pred(cell)` returns non-zero,
/// or `-1` when nothing matched.
#[unsafe(export_name = "$array.findIndex")]
pub extern "C" fn __array_find_index(arr: i64, closure: i64) -> i64 {
    if arr == 0 || closure == 0 {
        return -1;
    }
    let (len, _cap, data) = unsafe { array_header(arr) };
    for i in 0..len {
        let cell = unsafe { *((data + i * 8) as *const i64) };
        let hit = unsafe { call_closure_1(closure, cell) };
        if hit != 0 {
            return i;
        }
    }
    -1
}

/// `1` when `pred(cell)` returns non-zero for every cell (vacuously
/// true on an empty array). Short-circuits on the first `0`.
#[unsafe(export_name = "$array.every")]
pub extern "C" fn __array_every(arr: i64, closure: i64) -> i64 {
    if arr == 0 || closure == 0 {
        return 1;
    }
    let (len, _cap, data) = unsafe { array_header(arr) };
    for i in 0..len {
        let cell = unsafe { *((data + i * 8) as *const i64) };
        let hit = unsafe { call_closure_1(closure, cell) };
        if hit == 0 {
            return 0;
        }
    }
    1
}

/// `1` when `pred(cell)` returns non-zero for at least one cell;
/// `0` otherwise (including the empty array case).
#[unsafe(export_name = "$array.some")]
pub extern "C" fn __array_some(arr: i64, closure: i64) -> i64 {
    if arr == 0 || closure == 0 {
        return 0;
    }
    let (len, _cap, data) = unsafe { array_header(arr) };
    for i in 0..len {
        let cell = unsafe { *((data + i * 8) as *const i64) };
        let hit = unsafe { call_closure_1(closure, cell) };
        if hit != 0 {
            return 1;
        }
    }
    0
}

/// Build a fresh array containing every cell of `a` followed by
/// every cell of `b`. Both source arrays keep their own references
/// to the underlying heap cells; the new array bumps each cell's
/// refcount so all three holders are accounted for.
#[unsafe(export_name = "$array.concat")]
pub extern "C" fn __array_concat(a: i64, b: i64) -> i64 {
    let a_len = if a == 0 { 0 } else { unsafe { array_header(a).0 } };
    let b_len = if b == 0 { 0 } else { unsafe { array_header(b).0 } };
    let elem_kind = if a != 0 {
        unsafe { *((a + 32) as *const i64) }
    } else if b != 0 {
        unsafe { *((b + 32) as *const i64) }
    } else {
        KIND_NONE
    };
    let mut out: Vec<i64> = Vec::with_capacity((a_len + b_len) as usize);
    if a != 0 {
        let data = unsafe { *((a + 16) as *const i64) };
        for i in 0..a_len {
            let cell = unsafe { *((data + i * 8) as *const i64) };
            if elem_kind != KIND_NONE {
                retain_field_by_kind(cell, elem_kind);
            }
            out.push(cell);
        }
    }
    if b != 0 {
        let data = unsafe { *((b + 16) as *const i64) };
        for i in 0..b_len {
            let cell = unsafe { *((data + i * 8) as *const i64) };
            if elem_kind != KIND_NONE {
                retain_field_by_kind(cell, elem_kind);
            }
            out.push(cell);
        }
    }
    build_i64_array(&out, elem_kind)
}

/// Build a fresh array with the cells of `arr` in reverse order.
/// Each heap cell gains one extra reference (the new array holds
/// it alongside the original).
#[unsafe(export_name = "$array.reverse")]
pub extern "C" fn __array_reverse(arr: i64) -> i64 {
    if arr == 0 {
        return build_i64_array(&[], KIND_NONE);
    }
    let (len, _cap, data) = unsafe { array_header(arr) };
    let elem_kind = unsafe { *((arr + 32) as *const i64) };
    let mut out: Vec<i64> = Vec::with_capacity(len as usize);
    for i in (0..len).rev() {
        let cell = unsafe { *((data + i * 8) as *const i64) };
        if elem_kind != KIND_NONE {
            retain_field_by_kind(cell, elem_kind);
        }
        out.push(cell);
    }
    build_i64_array(&out, elem_kind)
}

/// Join a `string[]` into a single ilang string with `sep` between
/// each cell. The type checker restricts the receiver to `string[]`
/// so cells dereference cleanly via `cstr_to_str`.
#[unsafe(export_name = "$array.join")]
pub extern "C" fn __array_join(arr: i64, sep: i64) -> i64 {
    if arr == 0 {
        return leak_cstring(String::new());
    }
    let (len, _cap, data) = unsafe { array_header(arr) };
    let sep_str = cstr_to_str(sep);
    let mut out = String::new();
    for i in 0..len {
        if i > 0 {
            out.push_str(sep_str);
        }
        let cell = unsafe { *((data + i * 8) as *const i64) };
        out.push_str(cstr_to_str(cell));
    }
    leak_cstring(out)
}

/// Remove and return the first cell as `Optional<T>`. Empty
/// arrays return 0 (`none`). The Optional inherits the array's
/// reference (no retain) — the array's length drops by one.
#[unsafe(export_name = "$array.shift")]
pub extern "C" fn __array_shift(arr: i64) -> i64 {
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
        let addr = data as *const u8;
        let value: i64 = match stride {
            1 => *(addr as *const u8) as i64,
            2 => *(addr as *const u16) as i64,
            4 => *(addr as *const u32) as i64,
            _ => *(addr as *const i64),
        };
        shift_left_one(data, 1, len, stride);
        *h = len - 1;
        let elem_tag = *h.add(4);
        let cell = __mir_alloc(24) as *mut i64;
        *cell = value;
        *cell.add(1) = 1;
        *cell.add(2) = elem_tag;
        cell as i64
    }
}

/// Insert `value` at index 0, shifting the rest right by one and
/// growing the backing buffer if needed. The MIR side already
/// bumped the value's refcount on heap kinds, so we just store.
#[unsafe(export_name = "$array.unshift")]
pub extern "C" fn __array_unshift(arr: i64, value: i64) {
    if arr == 0 {
        return;
    }
    unsafe {
        let h = arr as *mut i64;
        let len = *h;
        let cap = *h.add(1);
        let stride = *h.add(5);
        let data = if len < cap {
            *h.add(2)
        } else {
            let new_cap = (cap * 2).max(4);
            let new_data = __mir_alloc(new_cap * stride);
            let old_data = *h.add(2);
            if len > 0 && old_data != 0 {
                std::ptr::copy_nonoverlapping(
                    old_data as *const u8,
                    new_data as *mut u8,
                    (len * stride) as usize,
                );
            }
            if old_data != 0 && cap > 0 {
                __mir_free(old_data, cap * stride);
            }
            *h.add(1) = new_cap;
            *h.add(2) = new_data;
            new_data
        };
        // Make room at index 0 — move cells [0..len) one slot right.
        if len > 0 {
            let src = data as *const u8;
            let dst = (data + stride) as *mut u8;
            std::ptr::copy(src, dst, (len * stride) as usize);
        }
        store_packed(data, 0, stride, value);
        *h = len + 1;
    }
}

/// Replace every cell with `value`. Releases the previously stored
/// cell on heap kinds and retains `value` for each slot it lands
/// in, so refcount stays balanced after the bulk overwrite.
#[unsafe(export_name = "$array.fill")]
pub extern "C" fn __array_fill(arr: i64, value: i64) {
    if arr == 0 {
        return;
    }
    unsafe {
        let h = arr as *const i64;
        let len = *h;
        let data = *h.add(2);
        let elem_tag = *h.add(4);
        let stride = *h.add(5);
        for i in 0..len {
            if elem_tag != KIND_NONE {
                let old = match stride {
                    1 => *((data + i * stride) as *const u8) as i64,
                    2 => *((data + i * stride) as *const u16) as i64,
                    4 => *((data + i * stride) as *const u32) as i64,
                    _ => *((data + i * stride) as *const i64),
                };
                release_field_by_kind(old, elem_tag);
                retain_field_by_kind(value, elem_tag);
            }
            store_packed(data, i, stride, value);
        }
    }
}

/// Comparator-based stable sort. Builds a new array containing
/// each source cell (with a fresh retain on heap kinds) and orders
/// them by `cmp(a, b)`. `cmp` returns negative / zero / positive
/// for less / equal / greater — same convention `qsort_r` /
/// `Array.prototype.sort` use.
#[unsafe(export_name = "$array.sort")]
pub extern "C" fn __array_sort(arr: i64, closure: i64) -> i64 {
    if arr == 0 {
        return build_i64_array(&[], KIND_NONE);
    }
    let (len, _cap, data) = unsafe { array_header(arr) };
    let elem_kind = unsafe { *((arr + 32) as *const i64) };
    let mut buf: Vec<i64> = Vec::with_capacity(len as usize);
    for i in 0..len {
        let cell = unsafe { *((data + i * 8) as *const i64) };
        if elem_kind != KIND_NONE {
            retain_field_by_kind(cell, elem_kind);
        }
        buf.push(cell);
    }
    if closure != 0 {
        buf.sort_by(|a, b| {
            let r = unsafe { call_closure_2(closure, *a, *b) };
            r.cmp(&0)
        });
    }
    build_i64_array(&buf, elem_kind)
}

