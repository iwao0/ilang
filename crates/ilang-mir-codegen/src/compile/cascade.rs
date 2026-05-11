//! ARC retain / release for the heap-tagged containers (Object,
//! Array, Optional, Map, Tuple, Closure, Str, Enum) and the kind-
//! dispatched release / retain cascade that drains nested
//! containers when one drops.
//!
//! `release_by_kind` / `retain_by_kind` accept a runtime `KIND_*`
//! tag pulled from a container's header; `release_value_by_kind`
//! accepts the JIT-side recursive [`PrintKind`] and walks deeper
//! into nested kinds (Array<Optional<Tuple<...>>> etc.) for the
//! object-field cascade.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use super::print_kind::{
    PrintKind, KIND_ARRAY, KIND_CLOSURE, KIND_ENUM, KIND_MAP, KIND_NONE, KIND_OBJECT,
    KIND_OPTIONAL, KIND_STR, KIND_TUPLE,
};
use super::{host_release_map, host_retain_map};

// Per-class table of object fields whose runtime values need a
// cascade-release on object drop. Populated at compile time by the
// `compile_with_builtins` registration loop; consumed by
// `host_release_object_fields` at rc = 0.
static OBJECT_FIELD_TABLE: OnceLock<Mutex<HashMap<u32, Vec<(i64, PrintKind)>>>> = OnceLock::new();

pub(super) fn object_field_table_lock() -> &'static Mutex<HashMap<u32, Vec<(i64, PrintKind)>>> {
    OBJECT_FIELD_TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) extern "C" fn host_release_object(obj_ptr: i64) {
    release_object(obj_ptr);
}

pub(super) extern "C" fn host_release_optional(opt_ptr: i64) {
    if opt_ptr == 0 {
        return;
    }
    let rc_ptr = (opt_ptr + 8) as *mut i64;
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
    let tag = unsafe { *((opt_ptr + 16) as *const i64) };
    if tag != KIND_NONE {
        let inner = unsafe { *(opt_ptr as *const i64) };
        release_by_kind(inner, tag);
    }
    // Free the 24-byte Optional cell. The earlier some(_) over-
    // release concern was resolved by 192b91d (Some/Break/EnumCtor
    // retain on aliased heap inner) so freeing the cell on rc=0
    // is now safe — fresh `some(new T())` transfers the inner's
    // +1 to the Optional, aliased `some(x)` bumps rc on the inner.
    ilang_runtime::__mir_free(opt_ptr, 24);
}

pub(super) extern "C" fn host_retain_optional(opt_ptr: i64) {
    if opt_ptr == 0 {
        return;
    }
    let rc_ptr = (opt_ptr + 8) as *mut i64;
    let rc = unsafe { *rc_ptr };
    if rc <= 0 {
        return;
    }
    unsafe {
        *rc_ptr = rc + 1;
    }
}

pub(super) extern "C" fn host_release_array(arr_ptr: i64) {
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
    // Cascade-release each stored element by its kind. The tag was
    // 0/1 ("Object or not") under the old scheme; it now carries
    // the full KIND_* discriminant so Array<Array<...>> /
    // Array<Optional<...>> / Array<Str> reclaim their inner cells
    // too. KIND_NONE skips the loop (primitive elements).
    if tag != KIND_NONE {
        for i in 0..len {
            let elem = unsafe { *((data_ptr + i * 8) as *const i64) };
            release_by_kind(elem, tag);
        }
    }
    // Free the data buffer + the 48-byte header. Both came from
    // host_mir_alloc in NewArray / NewArrayEmpty / build_array /
    // host_array_push grow path, so reconstructing the same byte
    // counts via host_mir_free drops the underlying Vec.
    if data_ptr != 0 {
        ilang_runtime::__mir_free(data_ptr, cap.max(1) * stride);
    }
    ilang_runtime::__mir_free(arr_ptr, 48);
}

pub(super) extern "C" fn host_retain_array(arr_ptr: i64) {
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

pub(super) extern "C" fn host_retain_object(obj_ptr: i64) {
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

pub(super) extern "C" fn host_release_object_fields(class_id: i64, obj_ptr: i64) {
    if obj_ptr == 0 {
        return;
    }
    let entries = {
        let m = object_field_table_lock()
            .lock()
            .expect("field table poisoned");
        m.get(&(class_id as u32)).cloned()
    };
    let entries = match entries {
        Some(e) if !e.is_empty() => e,
        _ => return,
    };
    for (off, kind) in entries.iter() {
        let raw = unsafe { *((obj_ptr + *off) as *const i64) };
        release_value_by_kind(raw, kind);
    }
}

pub(super) fn release_value_by_kind(raw: i64, kind: &PrintKind) {
    match kind {
        PrintKind::Object => {
            release_object(raw);
        }
        PrintKind::Optional(inner) => {
            if raw != 0 {
                let payload = unsafe { *(raw as *const i64) };
                release_value_by_kind(payload, inner);
            }
        }
        PrintKind::Array(inner) => {
            if raw != 0
                && matches!(
                    **inner,
                    PrintKind::Object
                        | PrintKind::Optional(_)
                        | PrintKind::Array(_)
                        | PrintKind::Tuple(_)
                )
            {
                let len = unsafe { *(raw as *const i64) };
                let data_ptr = unsafe { *((raw + 16) as *const i64) };
                for i in 0..len {
                    let elem_raw =
                        unsafe { *((data_ptr + (i * 8)) as *const i64) };
                    release_value_by_kind(elem_raw, inner);
                }
            }
        }
        PrintKind::Tuple(items) => {
            if raw != 0 {
                for (i, k) in items.iter().enumerate() {
                    let elem_raw =
                        unsafe { *((raw + (i as i64) * 8) as *const i64) };
                    release_value_by_kind(elem_raw, k);
                }
            }
        }
        _ => {}
    }
}

/// Dispatch release on a runtime value given its static kind.
/// Recurses through nested containers (Array of Array, Optional
/// of Array, etc.) so deep cascades reclaim every level.
pub(super) fn release_by_kind(ptr: i64, kind: i64) {
    if ptr == 0 {
        return;
    }
    match kind {
        KIND_OBJECT => release_object(ptr),
        KIND_ARRAY => host_release_array(ptr),
        KIND_OPTIONAL => host_release_optional(ptr),
        KIND_TUPLE => ilang_runtime::__release_tuple(ptr),
        KIND_MAP => host_release_map(ptr),
        KIND_CLOSURE => ilang_runtime::__release_closure(ptr),
        KIND_STR => ilang_runtime::__release_string(ptr),
        KIND_ENUM => ilang_runtime::__release_enum(ptr),
        _ => {} // KIND_NONE / unknown — primitive, no cascade.
    }
}

/// Mirror of `release_by_kind` for retain. Used when one container
/// hands an element pointer to another (e.g. `arr.filter(...)`
/// keeps a subset of the source array's elements; both arrays then
/// own the kept elements at +1 each).
pub(super) fn retain_by_kind(ptr: i64, kind: i64) {
    if ptr == 0 {
        return;
    }
    match kind {
        KIND_OBJECT => host_retain_object(ptr),
        KIND_ARRAY => host_retain_array(ptr),
        KIND_OPTIONAL => host_retain_optional(ptr),
        KIND_TUPLE => ilang_runtime::__retain_tuple(ptr),
        KIND_MAP => host_retain_map(ptr),
        KIND_CLOSURE => ilang_runtime::__retain_closure(ptr),
        KIND_STR => ilang_runtime::__retain_string(ptr),
        KIND_ENUM => ilang_runtime::__retain_enum(ptr),
        _ => {}
    }
}

pub(super) fn release_object(obj_ptr: i64) {
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
    // Call user deinit if registered.
    let user_drop = ilang_runtime::__drop_dispatch(class_id);
    if user_drop != 0 {
        let f: extern "C" fn(i64, i64) = unsafe { std::mem::transmute(user_drop) };
        f(obj_ptr, 0);
    }
    host_release_object_fields(class_id, obj_ptr);
    if let Some(sz) = ilang_runtime::class_size_for(class_id) {
        ilang_runtime::__mir_free(obj_ptr, sz);
    }
}

