//! Array host trampolines.
//!
//! ilang arrays use a 48-byte header (`[ len | cap | data_ptr | rc |
//! elem_kind | stride ]`) followed by `cap × stride` bytes of packed
//! element storage. The helpers here implement the user-callable
//! operations the JIT registers directly (`push` / `pop` / `indexOf`
//! / `includes` / `dataPtr`), plus the `build_array` constructor used
//! by `host_map_keys` / `host_map_values`, and the C-array bridge
//! used at FFI call sites.

pub(super) extern "C" fn host_array_index_of(arr: i64, value: i64) -> i64 {
    if arr == 0 {
        return -1;
    }
    let len = unsafe { *(arr as *const i64) };
    let data = unsafe { *((arr + 16) as *const i64) };
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
