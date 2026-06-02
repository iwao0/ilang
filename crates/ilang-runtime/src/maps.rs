//! Map runtime — `ManagedMap` wraps Rust's `HashMap<MapKey, i64>`
//! with a refcount, the per-value KIND_* tag (for cascade-release on
//! drop), and per-side print-kind tags (so `__print_map` can
//! stringify the cells).

use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::AtomicI64;

use crate::alloc::__mir_alloc;
use crate::arrays::build_i64_array;
use crate::cascade::{release_field_by_kind, retain_field_by_kind};
use crate::kind::{KIND_NONE, KIND_STR, PK_OTHER, PK_STR};
use crate::print_dispatch::format_kind_id;
use crate::strings::{__retain_string, cstr_bytes, leak_cstring};

/// A map's key kind is fixed for its lifetime (the type checker gives
/// every `Map<K, V>` a single `K`), so keys live in one of two stores
/// rather than a unified enum. Splitting them lets string lookups
/// probe by `&str` (`Box<str>: Borrow<str>`) instead of allocating a
/// fresh owned key per `get` / `has` / `delete`.
enum MapStore {
    /// Integer / float-bits / pointer keys — the raw i64 cell is the key.
    Int(HashMap<i64, i64>),
    /// String keys, stored canonically (UTF-8-lossy) so two raw
    /// pointers with the same bytes collapse to one entry.
    Str(HashMap<Box<str>, i64>),
    /// Class-instance keys driven by user-supplied `equals` /
    /// `hashCode` — `Set<MyClass>` uses the parallel `ObjectStore`
    /// in `sets.rs`. The set's +1 ARC share is taken via
    /// `__retain_object` (and released via `__release_object`),
    /// mirroring how the string store retains string-registry keys.
    Object(ObjectMapStore),
}

struct ObjectMapStore {
    eq_fn: extern "C" fn(i64, i64) -> i64,
    hash_fn: extern "C" fn(i64) -> i64,
    /// `hash → Vec of (key_ptr, value_cell)`. Bucket walks compare
    /// with the user's `eq_fn`.
    buckets: HashMap<i64, Vec<(i64, i64)>>,
    count: usize,
}

impl ObjectMapStore {
    /// `(value, replaced_previous)` on insertion. The caller handles
    /// the value's kind-based retain/release around this; key
    /// retention happens here on a genuinely new entry.
    fn insert(&mut self, key: i64, value: i64) -> Option<i64> {
        if key == 0 {
            return None;
        }
        let hash = (self.hash_fn)(key);
        let bucket = self.buckets.entry(hash).or_default();
        for slot in bucket.iter_mut() {
            if (self.eq_fn)(slot.0, key) != 0 {
                let prev = std::mem::replace(&mut slot.1, value);
                return Some(prev);
            }
        }
        crate::classes::__retain_object(key);
        bucket.push((key, value));
        self.count += 1;
        None
    }

    fn get(&self, key: i64) -> Option<i64> {
        if key == 0 {
            return None;
        }
        let hash = (self.hash_fn)(key);
        let bucket = self.buckets.get(&hash)?;
        for (k, v) in bucket {
            if (self.eq_fn)(*k, key) != 0 {
                return Some(*v);
            }
        }
        None
    }

    fn remove(&mut self, key: i64) -> Option<i64> {
        if key == 0 {
            return None;
        }
        let hash = (self.hash_fn)(key);
        let bucket = self.buckets.get_mut(&hash)?;
        for i in 0..bucket.len() {
            if (self.eq_fn)(bucket[i].0, key) != 0 {
                let (k, v) = bucket.swap_remove(i);
                if bucket.is_empty() {
                    self.buckets.remove(&hash);
                }
                self.count -= 1;
                crate::classes::__release_object(k);
                return Some(v);
            }
        }
        None
    }

    /// Drop every entry's key share. Caller releases values
    /// separately because that needs the map's `val_kind`.
    fn clear_keys(&mut self) -> Vec<i64> {
        let mut values: Vec<i64> = Vec::with_capacity(self.count);
        for bucket in self.buckets.values_mut() {
            for &(k, v) in bucket.iter() {
                crate::classes::__release_object(k);
                values.push(v);
            }
        }
        self.buckets.clear();
        self.count = 0;
        values
    }

    fn iter_keys(&self) -> impl Iterator<Item = i64> + '_ {
        self.buckets.values().flat_map(|b| b.iter().map(|(k, _)| *k))
    }

    fn iter_values(&self) -> impl Iterator<Item = i64> + '_ {
        self.buckets.values().flat_map(|b| b.iter().map(|(_, v)| *v))
    }

    fn iter_entries(&self) -> impl Iterator<Item = (i64, i64)> + '_ {
        self.buckets.values().flat_map(|b| b.iter().copied())
    }
}

pub(crate) struct ManagedMap {
    rc: AtomicI64,
    val_kind: i64,
    pub(crate) key_print_kind: i64,
    pub(crate) val_print_kind: i64,
    store: MapStore,
    /// For string-keyed maps: canonical key → original C-string ptr
    /// the user inserted. Lets `keys()` return the original ptrs.
    str_key_origs: HashMap<Box<str>, i64>,
}

/// Borrow a raw C-string key as `&str` for hash-map probing. Returns
/// `Cow::Borrowed` for valid UTF-8 (the common case → no allocation);
/// only malformed bytes force an owned, lossy-replaced copy.
unsafe fn key_str<'a>(raw: i64) -> Cow<'a, str> {
    if raw == 0 {
        Cow::Borrowed("")
    } else {
        String::from_utf8_lossy(unsafe { cstr_bytes(raw) })
    }
}

impl ManagedMap {
    /// Return the original raw pointer for string key `k` if we recorded
    /// one at insertion time, otherwise mint a fresh C-string. Used by
    /// `keys()` / `entries()` so the strings handed back match what the
    /// user passed into `set()`.
    fn str_orig_or_leak(&self, k: &str) -> i64 {
        self.str_key_origs
            .get(k)
            .copied()
            .unwrap_or_else(|| leak_cstring(k.to_string()))
    }
}

#[unsafe(export_name = "$map.new")]
pub extern "C" fn __map_new() -> i64 {
    let m = Box::new(ManagedMap {
        rc: AtomicI64::new(1),
        val_kind: 0,
        key_print_kind: PK_OTHER,
        val_print_kind: PK_OTHER,
        store: MapStore::Int(HashMap::new()),
        str_key_origs: HashMap::new(),
    });
    Box::into_raw(m) as i64
}

/// Object-key map constructor. Counterpart to `$set.newObject` —
/// takes the class's `equals` / `hashCode` method addresses as raw
/// i64s and routes future inserts through the bucketed
/// `ObjectMapStore`. `key_print_kind` starts at `PK_OBJECT` so
/// `console.log` formats keys as object refs; `$map.setPrintKinds`
/// is a no-op on this variant.
#[unsafe(export_name = "$map.newObject")]
pub extern "C" fn __map_new_object(eq_fn: i64, hash_fn: i64) -> i64 {
    let m = Box::new(ManagedMap {
        rc: AtomicI64::new(1),
        val_kind: 0,
        key_print_kind: crate::kind::PK_OBJECT,
        val_print_kind: PK_OTHER,
        store: MapStore::Object(ObjectMapStore {
            eq_fn: unsafe { std::mem::transmute::<i64, extern "C" fn(i64, i64) -> i64>(eq_fn) },
            hash_fn: unsafe { std::mem::transmute::<i64, extern "C" fn(i64) -> i64>(hash_fn) },
            buckets: HashMap::new(),
            count: 0,
        }),
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
    // Object-key maps carry PK_OBJECT internally and never need the
    // Int↔Str swap; codegen still emits the call for symmetry but the
    // Object arm leaves the bucket store untouched.
    if matches!(m.store, MapStore::Object(_)) {
        m.val_print_kind = val_kind;
        return;
    }
    m.key_print_kind = key_kind;
    m.val_print_kind = val_kind;
    // Codegen always calls this immediately after `$map.new`, before any
    // `set`, so the store is still empty here — pick the variant that
    // matches the (now-known) key kind.
    match (&m.store, key_kind == PK_STR) {
        (MapStore::Int(_), true) => m.store = MapStore::Str(HashMap::new()),
        (MapStore::Str(_), false) => m.store = MapStore::Int(HashMap::new()),
        _ => {}
    }
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
    let v = match &m.store {
        MapStore::Int(t) => *t.get(&key).unwrap_or(&0),
        MapStore::Str(t) => *t.get(&*unsafe { key_str(key) }).unwrap_or(&0),
        MapStore::Object(t) => t.get(key).unwrap_or(0),
    };
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
    let found = match &m.store {
        MapStore::Int(t) => t.get(&key).copied(),
        MapStore::Str(t) => t.get(&*unsafe { key_str(key) }).copied(),
        MapStore::Object(t) => t.get(key),
    };
    match found {
        Some(v) => {
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
    let val_kind = m.val_kind;
    if val_kind != KIND_NONE {
        retain_field_by_kind(value, val_kind);
    }
    // The string key is decoded once (in the `Str` arm below). Both the
    // store and the `str_key_origs` side table want an owned copy, but
    // walking the key buffer twice to build them was wasteful — clone the
    // single decode for the side table instead.
    let print_str = m.key_print_kind == PK_STR && key != 0;
    let mut shared_key: Option<Box<str>> = None;
    let prev = match &mut m.store {
        MapStore::Object(t) => t.insert(key, value),
        MapStore::Int(t) => t.insert(key, value),
        MapStore::Str(t) => {
            let k: Box<str> = unsafe { key_str(key) }.into_owned().into_boxed_str();
            if print_str {
                shared_key = Some(k.clone());
            }
            t.insert(k, value)
        }
    };
    // Record the original key pointer (first insert wins) so `keys()` /
    // `entries()` can hand back exactly what the user passed in. The map
    // adopts its own +1 on that string (mirroring the value retain
    // above); it is released on delete / clear / map drop. The caller's
    // transient share is dropped by codegen for fresh keys, or by the
    // owning binding's scope exit for aliased ones.
    if print_str {
        use std::collections::hash_map::Entry;
        let k = shared_key
            .unwrap_or_else(|| unsafe { key_str(key) }.into_owned().into_boxed_str());
        if let Entry::Vacant(slot) = m.str_key_origs.entry(k) {
            __retain_string(key);
            slot.insert(key);
        }
    }
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
    let found = match &m.store {
        MapStore::Int(t) => t.contains_key(&key),
        MapStore::Str(t) => t.contains_key(&*unsafe { key_str(key) }),
        MapStore::Object(t) => t.get(key).is_some(),
    };
    if found { 1 } else { 0 }
}

#[unsafe(export_name = "$map.size")]
pub extern "C" fn __map_size(map: i64) -> i64 {
    if map == 0 {
        return 0;
    }
    let m = unsafe { &*(map as *const ManagedMap) };
    let len = match &m.store {
        MapStore::Int(t) => t.len(),
        MapStore::Str(t) => t.len(),
        MapStore::Object(t) => t.count,
    };
    len as i64
}

#[unsafe(export_name = "$map.delete")]
pub extern "C" fn __map_delete(map: i64, key: i64) -> i64 {
    if map == 0 {
        return 0;
    }
    let m = unsafe { &mut *(map as *mut ManagedMap) };
    let val_kind = m.val_kind;
    let removed = match &mut m.store {
        MapStore::Int(t) => t.remove(&key),
        MapStore::Str(t) => t.remove(&*unsafe { key_str(key) }),
        MapStore::Object(t) => t.remove(key),
    };
    match removed {
        Some(old) => {
            if val_kind != KIND_NONE {
                release_field_by_kind(old, val_kind);
            }
            // Drop the map's adopted +1 on the key string.
            if m.key_print_kind == PK_STR {
                if let Some(orig) = m.str_key_origs.remove(&*unsafe { key_str(key) }) {
                    crate::strings::__release_string(orig);
                }
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
    let elem_kind = if m.key_print_kind == PK_STR {
        KIND_STR
    } else if m.key_print_kind == crate::kind::PK_OBJECT {
        crate::kind::KIND_OBJECT
    } else {
        KIND_NONE
    };
    let keys: Vec<i64> = match &m.store {
        MapStore::Int(t) => t.keys().copied().collect(),
        MapStore::Str(t) => t.keys().map(|k| m.str_orig_or_leak(k)).collect(),
        MapStore::Object(t) => t.iter_keys().collect(),
    };
    if elem_kind == KIND_STR {
        for k in &keys {
            __retain_string(*k);
        }
    } else if elem_kind == crate::kind::KIND_OBJECT {
        for k in &keys {
            crate::classes::__retain_object(*k);
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
    let values: Vec<i64> = match &m.store {
        MapStore::Int(t) => t.values().copied().collect(),
        MapStore::Str(t) => t.values().copied().collect(),
        MapStore::Object(t) => t.iter_values().collect(),
    };
    if val_kind != KIND_NONE {
        for v in &values {
            retain_field_by_kind(*v, val_kind);
        }
    }
    build_i64_array(&values, val_kind)
}

/// `map.clear()` — drop every entry. Value-side `release_field_by_kind`
/// fires the usual cascade for string / object / nested-collection
/// values; primitive (`KIND_NONE`) values just disappear. String keys
/// are stored as Rust `String` inside the `MapKey`, so dropping the
/// `inner` HashMap is enough — they don't carry registry rc to bump.
#[unsafe(export_name = "$map.clear")]
pub extern "C" fn __map_clear(map: i64) {
    if map == 0 {
        return;
    }
    let m = unsafe { &mut *(map as *mut ManagedMap) };
    let val_kind = m.val_kind;
    if val_kind != KIND_NONE {
        match &m.store {
            MapStore::Int(t) => {
                for &v in t.values() {
                    release_field_by_kind(v, val_kind);
                }
            }
            MapStore::Str(t) => {
                for &v in t.values() {
                    release_field_by_kind(v, val_kind);
                }
            }
            MapStore::Object(t) => {
                for v in t.iter_values() {
                    release_field_by_kind(v, val_kind);
                }
            }
        }
    }
    if let MapStore::Object(t) = &mut m.store {
        // `clear_keys` releases every object key share and resets
        // the bucket store. Returned values are already released
        // above (when `val_kind != KIND_NONE`).
        let _ = t.clear_keys();
        return;
    }
    if m.key_print_kind == PK_STR {
        // Release the canonical key pointers we handed out via
        // `keys()` / `entries()` originals (these live in the
        // string registry).
        for v in m.str_key_origs.values() {
            crate::strings::__release_string(*v);
        }
        m.str_key_origs.clear();
    }
    match &mut m.store {
        MapStore::Int(t) => t.clear(),
        MapStore::Str(t) => t.clear(),
        // Object handled above with `clear_keys`.
        MapStore::Object(_) => {}
    }
}

/// `map.entries()` — list of `(K, V)` tuples in arbitrary
/// (insertion-independent) order. Each tuple is freshly allocated
/// with rc=1 and owns its own +1 share of the key and value: string
/// keys go through `leak_cstring` (or the `str_key_origs` map's
/// retained pointer) and heap-kind values are `retain`ed before
/// being written into the tuple slot. The returned array is
/// `KIND_TUPLE`, so releasing it cascades into every tuple.
#[unsafe(export_name = "$map.entries")]
pub extern "C" fn __map_entries(map: i64) -> i64 {
    if map == 0 {
        return build_i64_array(&[], crate::kind::KIND_TUPLE);
    }
    let m = unsafe { &*(map as *const ManagedMap) };
    let key_kind = if m.key_print_kind == PK_STR {
        KIND_STR
    } else if m.key_print_kind == crate::kind::PK_OBJECT {
        crate::kind::KIND_OBJECT
    } else {
        KIND_NONE
    };
    let val_kind = m.val_kind;
    // Snapshot (key_raw, value) pairs; string keys take a +1 registry rc
    // up-front so the tuple array owns its own share. Object keys
    // mirror the same pattern via `__retain_object`.
    let pairs: Vec<(i64, i64)> = match &m.store {
        MapStore::Int(t) => t.iter().map(|(&k, &v)| (k, v)).collect(),
        MapStore::Str(t) => t
            .iter()
            .map(|(k, &v)| {
                let orig = m.str_orig_or_leak(k);
                __retain_string(orig);
                (orig, v)
            })
            .collect(),
        MapStore::Object(t) => t
            .iter_entries()
            .map(|(k, v)| {
                crate::classes::__retain_object(k);
                (k, v)
            })
            .collect(),
    };
    let mut tuples: Vec<i64> = Vec::with_capacity(pairs.len());
    for (key_raw, v) in pairs {
        if val_kind != KIND_NONE {
            retain_field_by_kind(v, val_kind);
        }
        // [ rc | packed | k | v ] — 32 bytes total. `tup_ptr` points
        // at the first element (offset 16 from base) per the layout
        // documented in `tuples.rs`.
        let base = __mir_alloc(32);
        let packed: u64 = 2u64
            | ((key_kind as u64) & 0xF) << 16
            | ((val_kind as u64) & 0xF) << 20;
        unsafe {
            *(base as *mut i64) = 1; // rc
            *((base + 8) as *mut i64) = packed as i64;
            *((base + 16) as *mut i64) = key_raw;
            *((base + 24) as *mut i64) = v;
        }
        tuples.push(base + 16);
    }
    build_i64_array(&tuples, crate::kind::KIND_TUPLE)
}

/// Reinterpret an i64 cell as the closure-arg value for float-kind
/// `fk` (0 = integer/pointer, 1 = f32, 2 = f64). Used to feed a float
/// key / value into the float register the closure's parameter expects.
macro_rules! kv_call {
    ($fn_ptr:expr, $closure:expr, $k:expr, $kfk:expr, $v:expr, $vfk:expr) => {{
        let fp = $fn_ptr;
        match ($kfk, $vfk) {
            (0, 0) => {
                let f: extern "C" fn(i64, i64, i64) -> i64 = std::mem::transmute(fp);
                let _ = f($k, $v, $closure);
            }
            (0, 1) => {
                let f: extern "C" fn(i64, f32, i64) -> i64 = std::mem::transmute(fp);
                let _ = f($k, f32::from_bits($v as u32), $closure);
            }
            (0, 2) => {
                let f: extern "C" fn(i64, f64, i64) -> i64 = std::mem::transmute(fp);
                let _ = f($k, f64::from_bits($v as u64), $closure);
            }
            (1, 0) => {
                let f: extern "C" fn(f32, i64, i64) -> i64 = std::mem::transmute(fp);
                let _ = f(f32::from_bits($k as u32), $v, $closure);
            }
            (2, 0) => {
                let f: extern "C" fn(f64, i64, i64) -> i64 = std::mem::transmute(fp);
                let _ = f(f64::from_bits($k as u64), $v, $closure);
            }
            (1, 1) => {
                let f: extern "C" fn(f32, f32, i64) -> i64 = std::mem::transmute(fp);
                let _ = f(f32::from_bits($k as u32), f32::from_bits($v as u32), $closure);
            }
            (1, 2) => {
                let f: extern "C" fn(f32, f64, i64) -> i64 = std::mem::transmute(fp);
                let _ = f(f32::from_bits($k as u32), f64::from_bits($v as u64), $closure);
            }
            (2, 1) => {
                let f: extern "C" fn(f64, f32, i64) -> i64 = std::mem::transmute(fp);
                let _ = f(f64::from_bits($k as u64), f32::from_bits($v as u32), $closure);
            }
            (2, 2) => {
                let f: extern "C" fn(f64, f64, i64) -> i64 = std::mem::transmute(fp);
                let _ = f(f64::from_bits($k as u64), f64::from_bits($v as u64), $closure);
            }
            _ => {
                let f: extern "C" fn(i64, i64, i64) -> i64 = std::mem::transmute(fp);
                let _ = f($k, $v, $closure);
            }
        }
    }};
}

/// Invoke an ilang closure with two args (key, value) and the
/// trailing env pointer. `key_fk` / `val_fk` are float-kind tags so a
/// float key / value reaches the closure in a float register rather
/// than an integer one. The closure body's return value is discarded —
/// `forEach` callbacks return Unit by signature.
unsafe fn call_closure_kv(closure: i64, k: i64, v: i64, key_fk: i64, val_fk: i64) {
    unsafe {
        let fn_ptr = *(closure as *const i64);
        kv_call!(fn_ptr, closure, k, key_fk, v, val_fk);
    }
}

/// `map.forEach(cb)` — call `cb(key, value)` once per entry. String
/// keys are handed to the callback as a fresh registry pointer (with
/// a +1 rc owned by this call); the rc is dropped after the callback
/// returns. If the callback wants to keep the key alive past its own
/// return, it must `retain` like any other ilang heap arg.
#[unsafe(export_name = "$map.forEach")]
pub extern "C" fn __map_for_each(map: i64, closure: i64, key_fk: i64, val_fk: i64) {
    if map == 0 || closure == 0 {
        return;
    }
    let m = unsafe { &*(map as *const ManagedMap) };
    let key_is_str = m.key_print_kind == PK_STR;
    let key_is_object = matches!(&m.store, MapStore::Object(_));
    // Snapshot as raw (key, value) pairs so the callback can mutate
    // the map without aliasing our iterator. String / object keys
    // keep a +1 rc through the call so mid-iteration `delete` can't
    // free them out from under the callback.
    let pairs: Vec<(i64, i64)> = match &m.store {
        MapStore::Int(t) => t.iter().map(|(&k, &v)| (k, v)).collect(),
        MapStore::Str(t) => t
            .iter()
            .map(|(k, &v)| {
                let orig = m.str_orig_or_leak(k);
                __retain_string(orig);
                (orig, v)
            })
            .collect(),
        MapStore::Object(t) => t
            .iter_entries()
            .map(|(k, v)| {
                crate::classes::__retain_object(k);
                (k, v)
            })
            .collect(),
    };
    for (key_raw, v) in pairs {
        unsafe { call_closure_kv(closure, key_raw, v, key_fk, val_fk) };
        if key_is_str {
            crate::strings::__release_string(key_raw);
        } else if key_is_object {
            crate::classes::__release_object(key_raw);
        }
    }
}

#[unsafe(export_name = "$print.map")]
pub extern "C" fn __print_map(map_ptr: i64) {
    let mut out = String::new();
    format_map_into(&mut out, map_ptr);
    let mut o = std::io::stdout().lock();
    let _ = o.write_all(out.as_bytes());
}

pub fn format_map_into(out: &mut String, map_ptr: i64) {
    if map_ptr == 0 {
        out.push_str("{}");
        return;
    }
    let m = unsafe { &*(map_ptr as *const ManagedMap) };
    let kk = m.key_print_kind;
    let vk = m.val_print_kind;
    // Pre-render each key once for stable display ordering. Without
    // caching, `sort_by` would call `format_kind_id` twice per
    // comparison — O(n log n) format calls instead of O(n).
    let mut entries: Vec<(String, i64, i64)> = match &m.store {
        MapStore::Int(t) => t
            .iter()
            .map(|(&k, &v)| {
                let mut s = String::new();
                format_kind_id(&mut s, kk, k);
                (s, k, v)
            })
            .collect(),
        MapStore::Str(t) => t
            .iter()
            .map(|(k, &v)| {
                let raw = m.str_orig_or_leak(k);
                let mut s = String::new();
                format_kind_id(&mut s, kk, raw);
                (s, raw, v)
            })
            .collect(),
        MapStore::Object(t) => t
            .iter_entries()
            .map(|(k, v)| {
                let mut s = String::new();
                format_kind_id(&mut s, kk, k);
                (s, k, v)
            })
            .collect(),
    };
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    out.push('{');
    for (i, (key_str, _raw, v)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(key_str);
        out.push_str(": ");
        format_kind_id(out, vk, *v);
    }
    out.push('}');
}

#[unsafe(export_name = "$map.retain")]
pub extern "C" fn __retain_map(map: i64) {
    if map == 0 {
        return;
    }
    let m = unsafe { &*(map as *const ManagedMap) };
    crate::refcount::retain_atomic(&m.rc);
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
    if crate::refcount::release_atomic(&m_ref.rc) != Some(0) {
        return;
    }
    let m_mut = unsafe { &mut *(map as *mut ManagedMap) };
    let val_kind = m_mut.val_kind;
    if val_kind != KIND_NONE {
        let values: Vec<i64> = match &m_mut.store {
            MapStore::Int(t) => t.values().copied().collect(),
            MapStore::Str(t) => t.values().copied().collect(),
            MapStore::Object(t) => t.iter_values().collect(),
        };
        for v in values {
            release_field_by_kind(v, val_kind);
        }
    }
    // Object keys: drop the map's adopted +1 on every key.
    if let MapStore::Object(t) = &mut m_mut.store {
        let _ = t.clear_keys();
    }
    // Drop the map's adopted +1 on every string key (mirrors `clear`).
    if m_mut.key_print_kind == PK_STR {
        for v in m_mut.str_key_origs.values() {
            crate::strings::__release_string(*v);
        }
    }
    unsafe {
        let _ = Box::from_raw(map as *mut ManagedMap);
    }
}
