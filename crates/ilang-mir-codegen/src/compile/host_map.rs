//! `Map<K, V>` host runtime: a Rust `HashMap<MapKey, i64>` wrapped
//! with an rc and per-side print/cascade kind tags so the runtime
//! retain / release cascade reclaims heap-typed values when the
//! whole map drops, and so `console.log(map)` knows how to pretty-
//! print each side.

use ilang_runtime::{cstr_bytes, leak_cstring};

use super::{
    build_array, format_kind_id, release_by_kind, retain_by_kind, KIND_NONE, KIND_OBJECT,
    PK_OTHER, PK_STR,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum MapKey {
    Int(i64),
    Str(String),
}

pub(super) struct ManagedMap {
    pub(super) rc: i64,
    pub(super) val_kind: i64,
    pub(super) key_print_kind: i64,
    pub(super) val_print_kind: i64,
    pub(super) inner: std::collections::HashMap<MapKey, i64>,
    /// For string-keyed maps: canonical key → original C-string ptr
    /// the user inserted. Lets `keys()` return the original ptrs so
    /// downstream `arr.includes(orig)` works without content compare.
    pub(super) str_key_origs: std::collections::HashMap<MapKey, i64>,
}

pub(super) fn raw_to_map_key(raw: i64, key_print_kind: i64) -> MapKey {
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

pub(super) fn map_key_to_raw(k: &MapKey) -> i64 {
    match k {
        MapKey::Int(n) => *n,
        MapKey::Str(s) => {
            // Re-emit a leaked length-prefixed copy so callers reading
            // back keys via host_map_keys see a stable ilang-string
            // pointer (matches the `[ i64 len | bytes | \0 ]` layout
            // used everywhere else).
            leak_cstring(s.clone())
        }
    }
}

pub(super) extern "C" fn host_map_new() -> i64 {
    let m = Box::new(ManagedMap {
        rc: 1,
        val_kind: 0,
        key_print_kind: PK_OTHER,
        val_print_kind: PK_OTHER,
        inner: std::collections::HashMap::new(),
        str_key_origs: std::collections::HashMap::new(),
    });
    Box::into_raw(m) as i64
}

pub(super) extern "C" fn host_map_set_print_kinds(map: i64, key_kind: i64, val_kind: i64) {
    if map == 0 {
        return;
    }
    let m = unsafe { &mut *(map as *mut ManagedMap) };
    m.key_print_kind = key_kind;
    m.val_print_kind = val_kind;
}

pub(super) extern "C" fn host_print_map(map_ptr: i64) {
    let mut out = String::new();
    if map_ptr == 0 {
        out.push_str("{}");
        print!("{out}");
        return;
    }
    let m = unsafe { &*(map_ptr as *const ManagedMap) };
    let mut entries: Vec<(i64, i64)> = m
        .inner
        .iter()
        .map(|(k, &v)| (map_key_to_raw(k), v))
        .collect();
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
    print!("{out}");
}

pub(super) extern "C" fn host_map_get(map: i64, key: i64) -> i64 {
    if map == 0 {
        return 0;
    }
    let m = unsafe { &*(map as *const ManagedMap) };
    let mk = raw_to_map_key(key, m.key_print_kind);
    *m.inner.get(&mk).unwrap_or(&0)
}

pub(super) extern "C" fn host_map_get_optional(map: i64, key: i64) -> i64 {
    if map == 0 {
        return 0;
    }
    let m = unsafe { &*(map as *const ManagedMap) };
    let mk = raw_to_map_key(key, m.key_print_kind);
    match m.inner.get(&mk) {
        Some(&v) => {
            let cell = ilang_runtime::__mir_alloc(24) as *mut i64;
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

pub(super) extern "C" fn host_map_set(map: i64, key: i64, value: i64) {
    if map == 0 {
        return;
    }
    let m = unsafe { &mut *(map as *mut ManagedMap) };
    let mk = raw_to_map_key(key, m.key_print_kind);
    if m.key_print_kind == PK_STR && key != 0 {
        m.str_key_origs.entry(mk.clone()).or_insert(key);
    }
    let val_kind = m.val_kind;
    // Retain the new value so the map owns its own +1 share. Without
    // this, a let-bound value flowing in through use_local would have
    // only the caller's slot share — the slot's scope-exit release
    // then drops the rc to 0 and frees the registry entry, leaving
    // the map's stored pointer dangling for the next get.
    if val_kind != KIND_NONE {
        retain_by_kind(value, val_kind);
    }
    let prev = m.inner.insert(mk, value);
    // Release the displaced value (if any) so `m["k"] = v1; m["k"] =
    // v2` doesn't leak v1.
    if let Some(old) = prev {
        if val_kind != KIND_NONE {
            release_by_kind(old, val_kind);
        }
    }
}

pub(super) extern "C" fn host_map_set_value_kind(map: i64, kind: i64) {
    // Tags the map's value side with KIND_* so retain-on-insert and
    // cascade-release-on-drop know which per-type runtime helper to
    // dispatch through. Called by NewMap codegen for every heap-
    // typed value side.
    if map == 0 {
        return;
    }
    let m = unsafe { &mut *(map as *mut ManagedMap) };
    m.val_kind = kind;
}

pub(super) extern "C" fn host_release_map(map: i64) {
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
            release_by_kind(v, val_kind);
        }
    }
    // Reclaim the box so the HashMap drops its allocation.
    unsafe {
        let _ = Box::from_raw(map as *mut ManagedMap);
    }
}

pub(super) extern "C" fn host_retain_map(map: i64) {
    if map == 0 {
        return;
    }
    let m = unsafe { &mut *(map as *mut ManagedMap) };
    if m.rc <= 0 {
        return;
    }
    m.rc += 1;
}

pub(super) extern "C" fn host_map_has(map: i64, key: i64) -> i64 {
    if map == 0 {
        return 0;
    }
    let mm = unsafe { &*(map as *const ManagedMap) };
    let mk = raw_to_map_key(key, mm.key_print_kind);
    if mm.inner.contains_key(&mk) { 1 } else { 0 }
}

pub(super) extern "C" fn host_map_size(map: i64) -> i64 {
    if map == 0 {
        return 0;
    }
    let m = unsafe { &(*(map as *const ManagedMap)).inner };
    m.len() as i64
}

pub(super) extern "C" fn host_map_delete(map: i64, key: i64) -> i64 {
    if map == 0 {
        return 0;
    }
    let mm = unsafe { &mut *(map as *mut ManagedMap) };
    let mk = raw_to_map_key(key, mm.key_print_kind);
    let val_kind = mm.val_kind;
    if let Some(old) = mm.inner.remove(&mk) {
        // Match the cascade-on-drop semantics — a removed key drops
        // its value just as if the whole map were released. Without
        // this, deleting a heap-valued key leaks the value.
        if val_kind == 1 {
            super::release_object(old);
        }
        1
    } else {
        0
    }
}

pub(super) extern "C" fn host_map_keys(map: i64) -> i64 {
    if map == 0 {
        return build_array(&[], KIND_NONE);
    }
    let mm = unsafe { &*(map as *const ManagedMap) };
    // String-keyed maps return interned key pointers (str_key_origs
    // is the original `cstrFromString` user passed in for hash key
    // i.e. registry-tracked). Tag KIND_NONE for those — the keys
    // are borrowed views, not freshly allocated copies the result
    // array should free. Same for int keys (KIND_NONE).
    let v: Vec<i64> = mm
        .inner
        .keys()
        .map(|k| {
            mm.str_key_origs
                .get(k)
                .copied()
                .unwrap_or_else(|| map_key_to_raw(k))
        })
        .collect();
    build_array(&v, KIND_NONE)
}

pub(super) extern "C" fn host_map_values(map: i64) -> i64 {
    if map == 0 {
        return build_array(&[], KIND_NONE);
    }
    let mm = unsafe { &*(map as *const ManagedMap) };
    let val_kind = mm.val_kind;
    let v: Vec<i64> = mm.inner.values().copied().collect();
    // Result array's kind reflects the map's value side. A
    // Map<K, ClassT>.values() should cascade release_object on
    // drop so each value's deinit fires when the result array
    // is reclaimed. Borrowed-from-map values need a retain so
    // both the source map and the result array account for the
    // reference.
    let elem_kind = if val_kind == 1 { KIND_OBJECT } else { KIND_NONE };
    if elem_kind != KIND_NONE {
        for &cell in &v {
            retain_by_kind(cell, elem_kind);
        }
    }
    build_array(&v, elem_kind)
}
