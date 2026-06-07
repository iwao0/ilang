//! Set runtime — `ManagedSet` wraps Rust's `HashSet` with a refcount
//! and a print-kind tag. The element-kind constraints mirror `Map`'s
//! keys (string / integer / bool); the type checker enforces that, the
//! runtime only cares whether the raw cell needs string handling on
//! insert / iterate.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicI64, Ordering, fence};

use crate::arrays::build_i64_array;
use crate::kind::{KIND_NONE, KIND_STR, PK_STR};
use crate::strings::{__release_string, __retain_string, cstr_bytes, leak_cstring};

/// A set's element kind is fixed for its lifetime, so elements live in
/// one of two stores rather than a unified enum. Splitting them lets
/// string lookups probe by `&str` (`Box<str>: Borrow<str>`) instead of
/// allocating a fresh owned element per `has` / `delete`.
enum SetStore {
    /// Integer / float-bits / bool keys — the raw i64 cell is the element.
    Int(HashSet<i64>),
    /// String elements, stored canonically (UTF-8-lossy).
    Str(HashSet<Box<str>>),
    /// Class instances under the user-supplied value-equality protocol:
    /// every element is identified by `(hash_fn(obj), eq_fn(obj, other))`
    /// pairs supplied by the class. Buckets are keyed by the i64 hash;
    /// equality walks the bucket via the user's `equals`. The set owns
    /// an `__retain_object`-bumped share of every stored pointer and
    /// drops the share via `__release_object` on delete / clear / drop.
    Object(ObjectStore),
}

/// Object-element set storage. The four function pointers come from
/// the class's `equals` / `hashCode` methods and the runtime ARC
/// hooks; `new Set<Class>()` lowering binds them at construction.
/// The bucketed `HashMap<i64, Vec<i64>>` is a chaining hashtable on
/// top of the user's hash function — Rust's HashMap has no
/// "dynamic equality" hook, so we keep the user's eq isolated to
/// our walk rather than impl PartialEq for a wrapper type.
struct ObjectStore {
    eq_fn: extern "C" fn(i64, i64) -> i64,
    hash_fn: extern "C" fn(i64) -> i64,
    /// `hash → vector of object pointers that hashed there`. Bucket
    /// length is the only thing the size counter needs to know about.
    /// Stored pointers carry the set's +1 ARC share — `__retain_object`
    /// / `__release_object` are called directly here rather than going
    /// through extra user-supplied hooks; the value-eq protocol only
    /// asks the class to provide `equals` + `hashCode`.
    buckets: HashMap<i64, Vec<i64>>,
    count: usize,
}

/// Call the user's `equals` and read its truthiness. An ilang `bool`
/// lives in the low byte of the return register; on SysV x86_64 a
/// `setcc` result leaves the upper bits of `rax` undefined, so a
/// full-width `!= 0` would read garbage. Mask to the low byte
/// (mirrors `arrays::call_predicate_1`). Only ever exercised on a hash
/// collision — same-bucket distinct elements — which is why the
/// missing mask slipped through. Free fn (not a `&self` method) so the
/// caller can pass the `eq_fn` field while `buckets` is borrowed.
#[inline]
fn eq_bool(eq_fn: extern "C" fn(i64, i64) -> i64, a: i64, b: i64) -> bool {
    (eq_fn(a, b) as u8) != 0
}

impl ObjectStore {
    fn empty_like(&self) -> Self {
        ObjectStore {
            eq_fn: self.eq_fn,
            hash_fn: self.hash_fn,
            buckets: HashMap::new(),
            count: 0,
        }
    }

    fn contains(&self, obj: i64) -> bool {
        if obj == 0 {
            return false;
        }
        let hash = (self.hash_fn)(obj);
        if let Some(bucket) = self.buckets.get(&hash) {
            for &existing in bucket {
                if eq_bool(self.eq_fn, existing, obj) {
                    return true;
                }
            }
        }
        false
    }

    /// Insert `obj`, retaining a +1 share on success. Returns `true`
    /// when the element was actually added (i.e. no existing equal
    /// element was found).
    fn insert(&mut self, obj: i64) -> bool {
        if obj == 0 {
            return false;
        }
        let hash = (self.hash_fn)(obj);
        let bucket = self.buckets.entry(hash).or_default();
        for &existing in bucket.iter() {
            if eq_bool(self.eq_fn, existing, obj) {
                return false;
            }
        }
        crate::classes::__retain_object(obj);
        bucket.push(obj);
        self.count += 1;
        true
    }

    /// Remove an element equal to `obj`, releasing the set's share.
    fn remove(&mut self, obj: i64) -> bool {
        if obj == 0 {
            return false;
        }
        let hash = (self.hash_fn)(obj);
        let Some(bucket) = self.buckets.get_mut(&hash) else {
            return false;
        };
        for i in 0..bucket.len() {
            if eq_bool(self.eq_fn, bucket[i], obj) {
                let removed = bucket.swap_remove(i);
                if bucket.is_empty() {
                    self.buckets.remove(&hash);
                }
                self.count -= 1;
                crate::classes::__release_object(removed);
                return true;
            }
        }
        false
    }

    fn clear(&mut self) {
        for bucket in self.buckets.values_mut() {
            for &obj in bucket.iter() {
                crate::classes::__release_object(obj);
            }
        }
        self.buckets.clear();
        self.count = 0;
    }

    fn iter(&self) -> impl Iterator<Item = i64> + '_ {
        self.buckets.values().flat_map(|b| b.iter().copied())
    }
}

pub(crate) struct ManagedSet {
    rc: AtomicI64,
    /// Print-kind tag for the element side (PK_STR / PK_OTHER). Set
    /// from codegen via `$set.setElemPrintKind` right after `$set.new`,
    /// mirroring how `Map` records its key print kind.
    elem_print_kind: i64,
    store: SetStore,
    /// For string-element sets: canonical element → original C-string
    /// pointer the user inserted. Lets follow-up methods (`has`,
    /// `delete`) round-trip without re-allocating registry strings.
    str_origs: HashMap<Box<str>, i64>,
}

/// Borrow a raw C-string element as `&str` for hash-set probing.
/// Returns `Cow::Borrowed` for valid UTF-8 (no allocation); only
/// malformed bytes force an owned, lossy-replaced copy.
unsafe fn elem_str<'a>(raw: i64) -> Cow<'a, str> {
    if raw == 0 {
        Cow::Borrowed("")
    } else {
        String::from_utf8_lossy(unsafe { cstr_bytes(raw) })
    }
}

impl ManagedSet {
    /// Return the original raw pointer for string element `e` if we
    /// recorded one at insertion time, otherwise mint a fresh C-string.
    /// Used by iteration methods so emitted strings match what the user
    /// passed into `add()`.
    fn str_orig_or_leak(&self, e: &str) -> i64 {
        self.str_origs
            .get(e)
            .copied()
            .unwrap_or_else(|| leak_cstring(e.to_string()))
    }

    fn contains_int(&self, e: i64) -> bool {
        matches!(&self.store, SetStore::Int(t) if t.contains(&e))
    }

    fn contains_str(&self, e: &str) -> bool {
        matches!(&self.store, SetStore::Str(t) if t.contains(e))
    }

    fn contains_obj(&self, obj: i64) -> bool {
        matches!(&self.store, SetStore::Object(t) if t.contains(obj))
    }
}

#[unsafe(export_name = "$set.new")]
pub extern "C" fn __set_new() -> i64 {
    let s = Box::new(ManagedSet {
        rc: AtomicI64::new(1),
        elem_print_kind: crate::kind::PK_OTHER,
        store: SetStore::Int(HashSet::new()),
        str_origs: HashMap::new(),
    });
    Box::into_raw(s) as i64
}

/// Object-element set constructor. Takes the class's `equals` and
/// `hashCode` method addresses as raw i64s — codegen materialises
/// them at the `new Set<Class>()` site via `Inst::FuncAddr`.
/// `elem_print_kind` defaults to `PK_OBJECT` so `console.log` on
/// the set formats elements as object refs. ARC bookkeeping reuses
/// the global `$class.retainObject` / `$class.releaseObject` hooks
/// directly, so the user-side protocol stays the two-method
/// `equals` + `hashCode`.
#[unsafe(export_name = "$set.newObject")]
pub extern "C" fn __set_new_object(eq_fn: i64, hash_fn: i64) -> i64 {
    let s = Box::new(ManagedSet {
        rc: AtomicI64::new(1),
        elem_print_kind: crate::kind::PK_OBJECT,
        store: SetStore::Object(ObjectStore {
            eq_fn: unsafe { std::mem::transmute::<i64, extern "C" fn(i64, i64) -> i64>(eq_fn) },
            hash_fn: unsafe { std::mem::transmute::<i64, extern "C" fn(i64) -> i64>(hash_fn) },
            buckets: HashMap::new(),
            count: 0,
        }),
        str_origs: HashMap::new(),
    });
    Box::into_raw(s) as i64
}

#[unsafe(export_name = "$set.setElemPrintKind")]
pub extern "C" fn __set_set_elem_print_kind(set: i64, kind: i64) {
    if set == 0 {
        return;
    }
    let s = unsafe { &mut *(set as *mut ManagedSet) };
    // Object sets carry their own elem_print_kind = PK_OBJECT and
    // never need this swap — codegen still emits the call for symmetry
    // with primitive sets, but we leave their Object store untouched.
    if matches!(s.store, SetStore::Object(_)) {
        return;
    }
    s.elem_print_kind = kind;
    // Codegen always calls this immediately after `$set.new`, before any
    // `add`, so the store is still empty here — pick the variant that
    // matches the (now-known) element kind.
    match (&s.store, kind == PK_STR) {
        (SetStore::Int(_), true) => s.store = SetStore::Str(HashSet::new()),
        (SetStore::Str(_), false) => s.store = SetStore::Int(HashSet::new()),
        _ => {}
    }
}

#[unsafe(export_name = "$set.add")]
pub extern "C" fn __set_add(set: i64, raw: i64) {
    if set == 0 {
        return;
    }
    let s = unsafe { &mut *(set as *mut ManagedSet) };
    match &mut s.store {
        SetStore::Object(t) => {
            t.insert(raw);
            return;
        }
        SetStore::Int(t) => {
            t.insert(raw);
        }
        SetStore::Str(t) => {
            // Probe by borrowed `&str` first — `elem_str` hands back a
            // `Cow::Borrowed` for valid UTF-8, so a duplicate add allocates
            // nothing. Only a genuinely new element mints owned key(s).
            let key = unsafe { elem_str(raw) };
            if !t.contains(&*key) {
                let e: Box<str> = key.into_owned().into_boxed_str();
                // New entry — the set takes its own +1 on string elements
                // (mirrors Map's key handling) so subsequent `has` /
                // iteration can hand the same pointer back.
                if raw != 0 {
                    __retain_string(raw);
                    s.str_origs.insert(e.clone(), raw);
                }
                t.insert(e);
            }
        }
    }
}

#[unsafe(export_name = "$set.has")]
pub extern "C" fn __set_has(set: i64, raw: i64) -> i64 {
    if set == 0 {
        return 0;
    }
    let s = unsafe { &*(set as *const ManagedSet) };
    let found = match &s.store {
        SetStore::Int(t) => t.contains(&raw),
        SetStore::Str(t) => t.contains(&*unsafe { elem_str(raw) }),
        SetStore::Object(t) => t.contains(raw),
    };
    if found { 1 } else { 0 }
}

#[unsafe(export_name = "$set.delete")]
pub extern "C" fn __set_delete(set: i64, raw: i64) -> i64 {
    if set == 0 {
        return 0;
    }
    let s = unsafe { &mut *(set as *mut ManagedSet) };
    // Object delete fast-pathed: bucket walk + release happens inside
    // ObjectStore::remove, so no follow-up str_origs maintenance.
    if let SetStore::Object(t) = &mut s.store {
        return if t.remove(raw) { 1 } else { 0 };
    }
    let removed = match &mut s.store {
        SetStore::Int(t) => t.remove(&raw),
        SetStore::Str(t) => t.remove(&*unsafe { elem_str(raw) }),
        SetStore::Object(_) => unreachable!("handled above"),
    };
    if removed {
        if matches!(&s.store, SetStore::Str(_)) {
            if let Some(orig) = s.str_origs.remove(&*unsafe { elem_str(raw) }) {
                __release_string(orig);
            }
        }
        1
    } else {
        0
    }
}

// --------------------------------------------------------------------
// Float-specialised add / has / delete
//
// f32 / f64 values can't ride the generic `(set, i64)` ABI of
// `$set.add` etc. without first being bit-cast; cranelift's calling
// convention treats them as float-register args. Routing through
// `$set.addF*` lets the JIT pass the raw float value and the
// runtime perform the bit-cast in Rust where it's cheap and
// well-defined. The stored cell is the raw bit pattern (zero-
// extended for f32), so `format_kind_id` (PK_F32 / PK_F64) recovers
// the original value via the matching `from_bits` call.
//
// NaN follows IEEE semantics — `NaN != NaN`, so inserting two
// distinct NaN bit patterns keeps both entries; inserting the same
// NaN bit pattern twice keeps one. The runtime tracks identity by
// HashSet on the bit pattern, mirroring how Rust's own collections
// would behave if they accepted floats. Float sets are never PK_STR,
// so they always live in the `Int` store.

#[unsafe(export_name = "$set.addF32")]
pub extern "C" fn __set_add_f32(set: i64, v: f32) {
    if set == 0 {
        return;
    }
    let s = unsafe { &mut *(set as *mut ManagedSet) };
    if let SetStore::Int(t) = &mut s.store {
        t.insert(v.to_bits() as i64);
    }
}

#[unsafe(export_name = "$set.addF64")]
pub extern "C" fn __set_add_f64(set: i64, v: f64) {
    if set == 0 {
        return;
    }
    let s = unsafe { &mut *(set as *mut ManagedSet) };
    if let SetStore::Int(t) = &mut s.store {
        t.insert(v.to_bits() as i64);
    }
}

#[unsafe(export_name = "$set.hasF32")]
pub extern "C" fn __set_has_f32(set: i64, v: f32) -> i64 {
    if set == 0 {
        return 0;
    }
    let s = unsafe { &*(set as *const ManagedSet) };
    if s.contains_int(v.to_bits() as i64) { 1 } else { 0 }
}

#[unsafe(export_name = "$set.hasF64")]
pub extern "C" fn __set_has_f64(set: i64, v: f64) -> i64 {
    if set == 0 {
        return 0;
    }
    let s = unsafe { &*(set as *const ManagedSet) };
    if s.contains_int(v.to_bits() as i64) { 1 } else { 0 }
}

#[unsafe(export_name = "$set.deleteF32")]
pub extern "C" fn __set_delete_f32(set: i64, v: f32) -> i64 {
    if set == 0 {
        return 0;
    }
    let s = unsafe { &mut *(set as *mut ManagedSet) };
    let removed = if let SetStore::Int(t) = &mut s.store {
        t.remove(&(v.to_bits() as i64))
    } else {
        false
    };
    if removed { 1 } else { 0 }
}

#[unsafe(export_name = "$set.deleteF64")]
pub extern "C" fn __set_delete_f64(set: i64, v: f64) -> i64 {
    if set == 0 {
        return 0;
    }
    let s = unsafe { &mut *(set as *mut ManagedSet) };
    let removed = if let SetStore::Int(t) = &mut s.store {
        t.remove(&(v.to_bits() as i64))
    } else {
        false
    };
    if removed { 1 } else { 0 }
}

#[unsafe(export_name = "$set.size")]
pub extern "C" fn __set_size(set: i64) -> i64 {
    if set == 0 {
        return 0;
    }
    let s = unsafe { &*(set as *const ManagedSet) };
    let len = match &s.store {
        SetStore::Int(t) => t.len(),
        SetStore::Str(t) => t.len(),
        SetStore::Object(t) => t.count,
    };
    len as i64
}

#[unsafe(export_name = "$set.clear")]
pub extern "C" fn __set_clear(set: i64) {
    if set == 0 {
        return;
    }
    let s = unsafe { &mut *(set as *mut ManagedSet) };
    if s.elem_print_kind == PK_STR {
        for v in s.str_origs.values() {
            __release_string(*v);
        }
        s.str_origs.clear();
    }
    match &mut s.store {
        SetStore::Int(t) => t.clear(),
        SetStore::Str(t) => t.clear(),
        SetStore::Object(t) => t.clear(),
    }
}

/// `set.values() -> T[]` — snapshot of every element in arbitrary
/// order. String elements take a fresh `__retain_string` so the
/// returned array owns its own +1 share alongside the set's; other
/// element kinds (int / float / bool stored as bit patterns) pass
/// through untouched.
#[unsafe(export_name = "$set.values")]
pub extern "C" fn __set_values(set: i64) -> i64 {
    if set == 0 {
        return build_i64_array(&[], KIND_NONE);
    }
    let s = unsafe { &*(set as *const ManagedSet) };
    let elem_kind = if s.elem_print_kind == PK_STR {
        KIND_STR
    } else if s.elem_print_kind == crate::kind::PK_OBJECT {
        crate::kind::KIND_OBJECT
    } else {
        KIND_NONE
    };
    let values: Vec<i64> = match &s.store {
        SetStore::Int(t) => t.iter().copied().collect(),
        SetStore::Str(t) => t
            .iter()
            .map(|e| {
                let orig = s.str_orig_or_leak(e);
                __retain_string(orig);
                orig
            })
            .collect(),
        SetStore::Object(t) => t
            .iter()
            .map(|obj| {
                // Array element takes its own +1 share; the set
                // retains a separate share that stays put.
                crate::classes::__retain_object(obj);
                obj
            })
            .collect(),
    };
    build_i64_array(&values, elem_kind)
}

// Closure-call helpers — closures are `[fn_ptr | rc | captures...]`
// blocks, called as `f(arg, env_ptr)`. Float receivers go through
// dedicated ABI-matched variants so cranelift's float-arg passing
// matches the closure's declared parameter type.

unsafe fn call_closure_1_i64(closure: i64, arg: i64) {
    unsafe {
        let fn_ptr = *(closure as *const i64);
        let f: extern "C" fn(i64, i64) -> i64 = std::mem::transmute(fn_ptr);
        let _ = f(arg, closure);
    }
}

unsafe fn call_closure_1_f32(closure: i64, arg: f32) {
    unsafe {
        let fn_ptr = *(closure as *const i64);
        let f: extern "C" fn(f32, i64) -> i64 = std::mem::transmute(fn_ptr);
        let _ = f(arg, closure);
    }
}

unsafe fn call_closure_1_f64(closure: i64, arg: f64) {
    unsafe {
        let fn_ptr = *(closure as *const i64);
        let f: extern "C" fn(f64, i64) -> i64 = std::mem::transmute(fn_ptr);
        let _ = f(arg, closure);
    }
}

/// `set.forEach(cb)` — invoke `cb(elem)` once per element. String
/// elements get a fresh registry rc for the call and release after.
#[unsafe(export_name = "$set.forEach")]
pub extern "C" fn __set_for_each(set: i64, closure: i64) {
    if set == 0 || closure == 0 {
        if closure != 0 {
            crate::closures::__release_closure(closure);
        }
        return;
    }
    let s = unsafe { &*(set as *const ManagedSet) };
    let is_str = s.elem_print_kind == PK_STR;
    let is_object = matches!(&s.store, SetStore::Object(_));
    // Snapshot as raw i64 — for string and object elements we retain
    // the corresponding share up-front so each entry survives any
    // mutation the callback may perform on the set (e.g. delete
    // during iteration).
    let args: Vec<i64> = match &s.store {
        SetStore::Int(t) => t.iter().copied().collect(),
        SetStore::Str(t) => t
            .iter()
            .map(|e| {
                let raw = s.str_orig_or_leak(e);
                __retain_string(raw);
                raw
            })
            .collect(),
        SetStore::Object(t) => t
            .iter()
            .map(|obj| {
                crate::classes::__retain_object(obj);
                obj
            })
            .collect(),
    };
    for arg in args {
        unsafe { call_closure_1_i64(closure, arg) };
        if is_str {
            __release_string(arg);
        } else if is_object {
            crate::classes::__release_object(arg);
        }
    }
    crate::closures::__release_closure(closure);
}

#[unsafe(export_name = "$set.forEachF32")]
pub extern "C" fn __set_for_each_f32(set: i64, closure: i64) {
    if set == 0 || closure == 0 {
        if closure != 0 {
            crate::closures::__release_closure(closure);
        }
        return;
    }
    let s = unsafe { &*(set as *const ManagedSet) };
    let bits: Vec<i64> = match &s.store {
        SetStore::Int(t) => t.iter().copied().collect(),
        SetStore::Str(_) | SetStore::Object(_) => Vec::new(),
    };
    for b in bits {
        let v = f32::from_bits(b as u32);
        unsafe { call_closure_1_f32(closure, v) };
    }
    crate::closures::__release_closure(closure);
}

#[unsafe(export_name = "$set.forEachF64")]
pub extern "C" fn __set_for_each_f64(set: i64, closure: i64) {
    if set == 0 || closure == 0 {
        if closure != 0 {
            crate::closures::__release_closure(closure);
        }
        return;
    }
    let s = unsafe { &*(set as *const ManagedSet) };
    let bits: Vec<i64> = match &s.store {
        SetStore::Int(t) => t.iter().copied().collect(),
        SetStore::Str(_) | SetStore::Object(_) => Vec::new(),
    };
    for b in bits {
        let v = f64::from_bits(b as u64);
        unsafe { call_closure_1_f64(closure, v) };
    }
    crate::closures::__release_closure(closure);
}

/// Allocate an empty `ManagedSet` whose Object store inherits the
/// fn-pointer hooks from `src`. Used by union / intersection /
/// difference so the output set drives elements through the same
/// equals / hash / retain / release callbacks as the inputs.
fn make_object_set_like(src: &ManagedSet) -> i64 {
    let SetStore::Object(ot) = &src.store else {
        return __set_new();
    };
    let s = Box::new(ManagedSet {
        rc: AtomicI64::new(1),
        elem_print_kind: crate::kind::PK_OBJECT,
        store: SetStore::Object(ot.empty_like()),
        str_origs: HashMap::new(),
    });
    Box::into_raw(s) as i64
}

fn is_object_set(s: &ManagedSet) -> bool {
    matches!(s.store, SetStore::Object(_))
}

/// Insert an integer element into `out`'s store (no string management).
fn set_insert_int(out: &mut ManagedSet, e: i64) {
    if let SetStore::Int(t) = &mut out.store {
        t.insert(e);
    }
}

/// Insert a string element into `out`'s store, taking ownership of the
/// matching string-registry rc. Caller arranges the retain on the
/// original side and we transfer that share into `str_origs` here; a
/// duplicate drops the share we'd otherwise leak.
fn set_insert_str_transferred(out: &mut ManagedSet, key: Box<str>, orig: i64) {
    if let SetStore::Str(t) = &mut out.store {
        if t.contains(&key) {
            __release_string(orig);
            return;
        }
        out.str_origs.insert(key.clone(), orig);
        t.insert(key);
    } else {
        __release_string(orig);
    }
}

/// Copy every element of `src` into `out`, retaining string / object
/// shares. Mixing element kinds (e.g. Object src into an Int out)
/// is a codegen bug — type-check guarantees both sides have the same
/// element shape, so the mismatched arms become no-ops here.
fn set_copy_into(out: &mut ManagedSet, src: &ManagedSet) {
    match (&src.store, &mut out.store) {
        (SetStore::Int(t), _) => {
            for &e in t.iter() {
                set_insert_int(out, e);
            }
        }
        (SetStore::Str(t), _) => {
            for k in t.iter() {
                let orig = src.str_orig_or_leak(k);
                __retain_string(orig);
                set_insert_str_transferred(out, k.clone(), orig);
            }
        }
        (SetStore::Object(s_obj), SetStore::Object(o_obj)) => {
            for obj in s_obj.iter() {
                o_obj.insert(obj); // handles retain
            }
        }
        (SetStore::Object(_), _) => {}
    }
}

#[unsafe(export_name = "$set.union")]
pub extern "C" fn __set_union(a: i64, b: i64) -> i64 {
    // Output's storage shape mirrors whichever input is Object (if any);
    // otherwise fall through to the generic Int / Str ctor + the
    // elem-print-kind nudge below.
    let template = if a != 0 && is_object_set(unsafe { &*(a as *const ManagedSet) }) {
        Some(a)
    } else if b != 0 && is_object_set(unsafe { &*(b as *const ManagedSet) }) {
        Some(b)
    } else {
        None
    };
    let out = match template {
        Some(src) => make_object_set_like(unsafe { &*(src as *const ManagedSet) }),
        None => __set_new(),
    };
    let pk = if a != 0 {
        unsafe { &*(a as *const ManagedSet) }.elem_print_kind
    } else if b != 0 {
        unsafe { &*(b as *const ManagedSet) }.elem_print_kind
    } else {
        crate::kind::PK_OTHER
    };
    __set_set_elem_print_kind(out, pk);
    let out_s = unsafe { &mut *(out as *mut ManagedSet) };
    if a != 0 {
        set_copy_into(out_s, unsafe { &*(a as *const ManagedSet) });
    }
    if b != 0 {
        set_copy_into(out_s, unsafe { &*(b as *const ManagedSet) });
    }
    out
}

#[unsafe(export_name = "$set.intersection")]
pub extern "C" fn __set_intersection(a: i64, b: i64) -> i64 {
    let out = if a != 0 && is_object_set(unsafe { &*(a as *const ManagedSet) }) {
        make_object_set_like(unsafe { &*(a as *const ManagedSet) })
    } else {
        __set_new()
    };
    if a == 0 || b == 0 {
        return out;
    }
    let sa = unsafe { &*(a as *const ManagedSet) };
    let sb = unsafe { &*(b as *const ManagedSet) };
    __set_set_elem_print_kind(out, sa.elem_print_kind);
    let out_s = unsafe { &mut *(out as *mut ManagedSet) };
    match &sa.store {
        SetStore::Int(t) => {
            for &e in t.iter() {
                if sb.contains_int(e) {
                    set_insert_int(out_s, e);
                }
            }
        }
        SetStore::Str(t) => {
            for k in t.iter() {
                if sb.contains_str(k) {
                    let orig = sa.str_orig_or_leak(k);
                    __retain_string(orig);
                    set_insert_str_transferred(out_s, k.clone(), orig);
                }
            }
        }
        SetStore::Object(t) => {
            if let SetStore::Object(o_obj) = &mut out_s.store {
                for obj in t.iter() {
                    if sb.contains_obj(obj) {
                        o_obj.insert(obj);
                    }
                }
            }
        }
    }
    out
}

#[unsafe(export_name = "$set.difference")]
pub extern "C" fn __set_difference(a: i64, b: i64) -> i64 {
    let out = if a != 0 && is_object_set(unsafe { &*(a as *const ManagedSet) }) {
        make_object_set_like(unsafe { &*(a as *const ManagedSet) })
    } else {
        __set_new()
    };
    if a == 0 {
        return out;
    }
    let sa = unsafe { &*(a as *const ManagedSet) };
    let pk = sa.elem_print_kind;
    __set_set_elem_print_kind(out, pk);
    let out_s = unsafe { &mut *(out as *mut ManagedSet) };
    // For an absent `b` we treat the diff target as empty — no
    // membership checks fire below.
    let sb_opt: Option<&ManagedSet> = if b == 0 {
        None
    } else {
        Some(unsafe { &*(b as *const ManagedSet) })
    };
    match &sa.store {
        SetStore::Int(t) => {
            for &e in t.iter() {
                if !sb_opt.map(|s| s.contains_int(e)).unwrap_or(false) {
                    set_insert_int(out_s, e);
                }
            }
        }
        SetStore::Str(t) => {
            for k in t.iter() {
                if !sb_opt.map(|s| s.contains_str(k)).unwrap_or(false) {
                    let orig = sa.str_orig_or_leak(k);
                    __retain_string(orig);
                    set_insert_str_transferred(out_s, k.clone(), orig);
                }
            }
        }
        SetStore::Object(t) => {
            if let SetStore::Object(o_obj) = &mut out_s.store {
                for obj in t.iter() {
                    let skip = sb_opt.map(|s| s.contains_obj(obj)).unwrap_or(false);
                    if !skip {
                        o_obj.insert(obj);
                    }
                }
            }
        }
    }
    out
}

#[unsafe(export_name = "$set.isSubsetOf")]
pub extern "C" fn __set_is_subset_of(a: i64, b: i64) -> i64 {
    if a == 0 {
        return 1;
    }
    let sa = unsafe { &*(a as *const ManagedSet) };
    let empty = match &sa.store {
        SetStore::Int(t) => t.is_empty(),
        SetStore::Str(t) => t.is_empty(),
        SetStore::Object(t) => t.count == 0,
    };
    if empty {
        return 1;
    }
    if b == 0 {
        return 0;
    }
    let sb = unsafe { &*(b as *const ManagedSet) };
    let subset = match &sa.store {
        SetStore::Int(t) => t.iter().all(|&e| sb.contains_int(e)),
        SetStore::Str(t) => t.iter().all(|k| sb.contains_str(k)),
        SetStore::Object(t) => t.iter().all(|obj| sb.contains_obj(obj)),
    };
    if subset { 1 } else { 0 }
}

#[unsafe(export_name = "$set.isSupersetOf")]
pub extern "C" fn __set_is_superset_of(a: i64, b: i64) -> i64 {
    __set_is_subset_of(b, a)
}

#[unsafe(export_name = "$set.isDisjointFrom")]
pub extern "C" fn __set_is_disjoint_from(a: i64, b: i64) -> i64 {
    if a == 0 || b == 0 {
        return 1;
    }
    let sa = unsafe { &*(a as *const ManagedSet) };
    let sb = unsafe { &*(b as *const ManagedSet) };
    let disjoint = match &sa.store {
        SetStore::Int(t) => t.iter().all(|&e| !sb.contains_int(e)),
        SetStore::Str(t) => t.iter().all(|k| !sb.contains_str(k)),
        SetStore::Object(t) => t.iter().all(|obj| !sb.contains_obj(obj)),
    };
    if disjoint { 1 } else { 0 }
}

#[unsafe(export_name = "$set.retain")]
pub extern "C" fn __retain_set(set: i64) {
    if set == 0 {
        return;
    }
    let s = unsafe { &*(set as *const ManagedSet) };
    crate::refcount::retain_atomic(&s.rc);
}

#[unsafe(export_name = "$set.release")]
pub extern "C" fn __release_set(set: i64) {
    if set == 0 {
        return;
    }
    let prev = {
        let s = unsafe { &*(set as *const ManagedSet) };
        s.rc.fetch_sub(1, Ordering::Release)
    };
    if prev != 1 {
        return;
    }
    fence(Ordering::Acquire);
    // Last reference — release every string element's registry rc and
    // drop the backing Box.
    unsafe {
        let s = &mut *(set as *mut ManagedSet);
        if s.elem_print_kind == PK_STR {
            for v in s.str_origs.values() {
                __release_string(*v);
            }
        }
        // Object elements own a +1 share each; clearing the store
        // drops them through the registered release_fn before we
        // free the Box backing the set.
        if let SetStore::Object(t) = &mut s.store {
            t.clear();
        }
        let _ = Box::from_raw(set as *mut ManagedSet);
    }
}

#[unsafe(export_name = "$print.set")]
pub extern "C" fn __print_set(set: i64) {
    use std::io::Write;
    let mut out = String::new();
    format_set_into(&mut out, set);
    let mut o = std::io::stdout().lock();
    let _ = o.write_all(out.as_bytes());
}

pub fn format_set_into(out: &mut String, set: i64) {
    if set == 0 {
        out.push_str("Set {}");
        return;
    }
    let s = unsafe { &*(set as *const ManagedSet) };
    let pk = s.elem_print_kind;
    // Stable display: pre-render each element once, sort by the
    // formatted text. Same trick `format_map_into` uses to avoid
    // O(n log n) format calls inside the comparator.
    let mut rendered: Vec<String> = match &s.store {
        SetStore::Int(t) => t
            .iter()
            .map(|&e| {
                let mut buf = String::new();
                crate::print_dispatch::format_kind_id(&mut buf, pk, e);
                buf
            })
            .collect(),
        SetStore::Str(t) => t
            .iter()
            .map(|e| {
                let raw = s.str_orig_or_leak(e);
                let mut buf = String::new();
                crate::print_dispatch::format_kind_id(&mut buf, pk, raw);
                buf
            })
            .collect(),
        SetStore::Object(t) => t
            .iter()
            .map(|obj| {
                let mut buf = String::new();
                crate::print_dispatch::format_kind_id(&mut buf, pk, obj);
                buf
            })
            .collect(),
    };
    rendered.sort();
    out.push_str("Set {");
    for (i, r) in rendered.iter().enumerate() {
        if i == 0 {
            out.push(' ');
        } else {
            out.push_str(", ");
        }
        out.push_str(r);
    }
    if !rendered.is_empty() {
        out.push(' ');
    }
    out.push('}');
}
