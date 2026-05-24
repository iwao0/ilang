//! Map runtime — `ManagedMap` wraps Rust's `HashMap<MapKey, i64>`
//! with a refcount, the per-value KIND_* tag (for cascade-release on
//! drop), and per-side print-kind tags (so `__print_map` can
//! stringify the cells).

use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicI64, Ordering, fence};

use crate::alloc::__mir_alloc;
use crate::arrays::build_i64_array;
use crate::cascade::{release_field_by_kind, retain_field_by_kind};
use crate::kind::{KIND_NONE, KIND_STR, PK_OTHER, PK_STR};
use crate::print_dispatch::format_kind_id;
use crate::strings::{__retain_string, cstr_bytes, leak_cstring};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum MapKey {
    Int(i64),
    Str(String),
}

pub(crate) struct ManagedMap {
    rc: AtomicI64,
    val_kind: i64,
    pub(crate) key_print_kind: i64,
    pub(crate) val_print_kind: i64,
    inner: HashMap<MapKey, i64>,
    /// For string-keyed maps: canonical key → original C-string ptr
    /// the user inserted. Lets `keys()` return the original ptrs.
    str_key_origs: HashMap<MapKey, i64>,
}

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

#[unsafe(export_name = "$map.new")]
pub extern "C" fn __map_new() -> i64 {
    let m = Box::new(ManagedMap {
        rc: AtomicI64::new(1),
        val_kind: 0,
        key_print_kind: PK_OTHER,
        val_print_kind: PK_OTHER,
        inner: HashMap::new(),
        str_key_origs: HashMap::new(),
    });
    Box::into_raw(m) as i64
}

#[unsafe(export_name = "$map.setPrintKinds")]
pub extern "C" fn __map_set_print_kinds(map: i64, key_kind: i64, val_kind: i64) {
    if map == 0 {
        return;
    }
    let m = unsafe { &mut *(map as *mut ManagedMap) };
    m.key_print_kind = key_kind;
    m.val_print_kind = val_kind;
}

#[unsafe(export_name = "$map.setValueKind")]
pub extern "C" fn __map_set_value_kind(map: i64, kind: i64) {
    if map == 0 {
        return;
    }
    let m = unsafe { &mut *(map as *mut ManagedMap) };
    m.val_kind = kind;
}

#[unsafe(export_name = "$map.get")]
pub extern "C" fn __map_get(map: i64, key: i64) -> i64 {
    if map == 0 {
        return 0;
    }
    let m = unsafe { &*(map as *const ManagedMap) };
    let mk = raw_to_map_key(key, m.key_print_kind);
    let v = *m.inner.get(&mk).unwrap_or(&0);
    // Heap value-typed maps need a retain on read — the caller
    // gets a `+1` reference whose lifetime is independent of the
    // map's. Without this, `let arr = m["k"]; arr.length` would
    // alias the map's slot and a later scope-exit release of
    // `arr` would free the storage the map still owns.
    if v != 0 && m.val_kind != KIND_NONE {
        retain_field_by_kind(v, m.val_kind);
    }
    v
}

#[unsafe(export_name = "$map.getOptional")]
pub extern "C" fn __map_get_optional(map: i64, key: i64) -> i64 {
    if map == 0 {
        return 0;
    }
    let m = unsafe { &*(map as *const ManagedMap) };
    let mk = raw_to_map_key(key, m.key_print_kind);
    match m.inner.get(&mk) {
        Some(&v) => {
            // See `__map_get`: heap values need a +1 to outlive
            // the map's borrow. The Optional cell that the
            // caller unwraps then owns the strong reference.
            if v != 0 && m.val_kind != KIND_NONE {
                retain_field_by_kind(v, m.val_kind);
            }
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

#[unsafe(export_name = "$map.set")]
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
        retain_field_by_kind(value, val_kind);
    }
    let prev = m.inner.insert(mk, value);
    if let Some(old) = prev {
        if val_kind != KIND_NONE {
            release_field_by_kind(old, val_kind);
        }
    }
}

#[unsafe(export_name = "$map.has")]
pub extern "C" fn __map_has(map: i64, key: i64) -> i64 {
    if map == 0 {
        return 0;
    }
    let m = unsafe { &*(map as *const ManagedMap) };
    let mk = raw_to_map_key(key, m.key_print_kind);
    if m.inner.contains_key(&mk) { 1 } else { 0 }
}

#[unsafe(export_name = "$map.size")]
pub extern "C" fn __map_size(map: i64) -> i64 {
    if map == 0 {
        return 0;
    }
    let m = unsafe { &*(map as *const ManagedMap) };
    m.inner.len() as i64
}

#[unsafe(export_name = "$map.delete")]
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
                release_field_by_kind(old, val_kind);
            }
            1
        }
        None => 0,
    }
}

#[unsafe(export_name = "$map.keys")]
pub extern "C" fn __map_keys(map: i64) -> i64 {
    if map == 0 {
        return build_i64_array(&[], KIND_NONE);
    }
    let m = unsafe { &*(map as *const ManagedMap) };
    let elem_kind = if m.key_print_kind == PK_STR { KIND_STR } else { KIND_NONE };
    let keys: Vec<i64> = if m.key_print_kind == PK_STR {
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

#[unsafe(export_name = "$map.values")]
pub extern "C" fn __map_values(map: i64) -> i64 {
    if map == 0 {
        return build_i64_array(&[], KIND_NONE);
    }
    let m = unsafe { &*(map as *const ManagedMap) };
    let val_kind = m.val_kind;
    let values: Vec<i64> = m.inner.values().copied().collect();
    if val_kind != KIND_NONE {
        for v in &values {
            retain_field_by_kind(*v, val_kind);
        }
    }
    build_i64_array(&values, val_kind)
}

#[unsafe(export_name = "$print.map")]
pub extern "C" fn __print_map(map_ptr: i64) {
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

#[unsafe(export_name = "$map.retain")]
pub extern "C" fn __retain_map(map: i64) {
    if map == 0 {
        return;
    }
    let m = unsafe { &*(map as *const ManagedMap) };
    let mut cur = m.rc.load(Ordering::Relaxed);
    loop {
        if cur <= 0 {
            return;
        }
        match m.rc.compare_exchange_weak(cur, cur + 1, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(actual) => cur = actual,
        }
    }
}

#[unsafe(export_name = "$map.release")]
pub extern "C" fn __release_map(map: i64) {
    if map == 0 {
        return;
    }
    // Decrement atomically; only the thread that takes the count
    // to 0 may run the destructor (and is then the sole owner of
    // the Box).
    let m_ref = unsafe { &*(map as *const ManagedMap) };
    let mut cur = m_ref.rc.load(Ordering::Relaxed);
    loop {
        if cur <= 0 {
            return;
        }
        match m_ref.rc.compare_exchange_weak(
            cur,
            cur - 1,
            Ordering::Release,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(actual) => cur = actual,
        }
    }
    if cur != 1 {
        return;
    }
    fence(Ordering::Acquire);
    let m_mut = unsafe { &mut *(map as *mut ManagedMap) };
    let val_kind = m_mut.val_kind;
    if val_kind != KIND_NONE {
        let values: Vec<i64> = m_mut.inner.values().copied().collect();
        for v in values {
            release_field_by_kind(v, val_kind);
        }
    }
    unsafe {
        let _ = Box::from_raw(map as *mut ManagedMap);
    }
}
