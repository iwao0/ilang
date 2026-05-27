//! Set runtime — `ManagedSet` wraps Rust's `HashSet<SetElem>` with a
//! refcount and a print-kind tag. The element-kind constraints mirror
//! `Map`'s keys (string / integer / bool); the type checker enforces
//! that, the runtime only cares whether the raw cell needs string
//! handling on insert / iterate.

use std::collections::HashSet;
use std::sync::atomic::{AtomicI64, Ordering, fence};

use crate::arrays::build_i64_array;
use crate::kind::{KIND_NONE, KIND_STR, PK_STR};
use crate::strings::{__release_string, __retain_string, cstr_bytes, leak_cstring};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum SetElem {
    Int(i64),
    Str(String),
}

pub(crate) struct ManagedSet {
    rc: AtomicI64,
    /// Print-kind tag for the element side (PK_STR / PK_OTHER). Set
    /// from codegen via `$set.setElemPrintKind` right after `$set.new`,
    /// mirroring how `Map` records its key print kind.
    elem_print_kind: i64,
    inner: HashSet<SetElem>,
    /// For string-element sets: canonical element → original C-string
    /// pointer the user inserted. Lets follow-up methods (`has`,
    /// `delete`) round-trip without re-allocating registry strings.
    str_origs: std::collections::HashMap<SetElem, i64>,
}

fn raw_to_set_elem(raw: i64, elem_print_kind: i64) -> SetElem {
    if elem_print_kind == PK_STR {
        if raw == 0 {
            SetElem::Str(String::new())
        } else {
            let bytes = unsafe { cstr_bytes(raw) };
            SetElem::Str(String::from_utf8_lossy(bytes).into_owned())
        }
    } else {
        SetElem::Int(raw)
    }
}

fn set_elem_to_raw(e: &SetElem) -> i64 {
    match e {
        SetElem::Int(n) => *n,
        SetElem::Str(s) => leak_cstring(s.clone()),
    }
}

impl ManagedSet {
    /// Return the original raw pointer for `e` if we recorded one at
    /// insertion time, otherwise mint a fresh C-string. Used by
    /// iteration methods so emitted strings match what the user
    /// passed into `add()`.
    fn str_orig_or_leak(&self, e: &SetElem) -> i64 {
        self.str_origs
            .get(e)
            .copied()
            .unwrap_or_else(|| set_elem_to_raw(e))
    }
}

#[unsafe(export_name = "$set.new")]
pub extern "C" fn __set_new() -> i64 {
    let s = Box::new(ManagedSet {
        rc: AtomicI64::new(1),
        elem_print_kind: crate::kind::PK_OTHER,
        inner: HashSet::new(),
        str_origs: std::collections::HashMap::new(),
    });
    Box::into_raw(s) as i64
}

#[unsafe(export_name = "$set.setElemPrintKind")]
pub extern "C" fn __set_set_elem_print_kind(set: i64, kind: i64) {
    if set == 0 {
        return;
    }
    let s = unsafe { &mut *(set as *mut ManagedSet) };
    s.elem_print_kind = kind;
}

#[unsafe(export_name = "$set.add")]
pub extern "C" fn __set_add(set: i64, raw: i64) {
    if set == 0 {
        return;
    }
    let s = unsafe { &mut *(set as *mut ManagedSet) };
    let elem = raw_to_set_elem(raw, s.elem_print_kind);
    let is_str = s.elem_print_kind == PK_STR;
    if s.inner.insert(elem.clone()) {
        // New entry — the set takes its own +1 on string elements
        // (mirrors Map's key handling) so subsequent `has` /
        // iteration can hand the same pointer back.
        if is_str && raw != 0 {
            __retain_string(raw);
            s.str_origs.insert(elem, raw);
        }
    }
    // Duplicate — drop; the set already owns its share.
}

#[unsafe(export_name = "$set.has")]
pub extern "C" fn __set_has(set: i64, raw: i64) -> i64 {
    if set == 0 {
        return 0;
    }
    let s = unsafe { &*(set as *const ManagedSet) };
    let elem = raw_to_set_elem(raw, s.elem_print_kind);
    if s.inner.contains(&elem) { 1 } else { 0 }
}

#[unsafe(export_name = "$set.delete")]
pub extern "C" fn __set_delete(set: i64, raw: i64) -> i64 {
    if set == 0 {
        return 0;
    }
    let s = unsafe { &mut *(set as *mut ManagedSet) };
    let elem = raw_to_set_elem(raw, s.elem_print_kind);
    let is_str = s.elem_print_kind == PK_STR;
    if s.inner.remove(&elem) {
        if is_str {
            if let Some(orig) = s.str_origs.remove(&elem) {
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
// would behave if they accepted floats.

#[unsafe(export_name = "$set.addF32")]
pub extern "C" fn __set_add_f32(set: i64, v: f32) {
    if set == 0 {
        return;
    }
    let s = unsafe { &mut *(set as *mut ManagedSet) };
    s.inner.insert(SetElem::Int(v.to_bits() as i64));
}

#[unsafe(export_name = "$set.addF64")]
pub extern "C" fn __set_add_f64(set: i64, v: f64) {
    if set == 0 {
        return;
    }
    let s = unsafe { &mut *(set as *mut ManagedSet) };
    s.inner.insert(SetElem::Int(v.to_bits() as i64));
}

#[unsafe(export_name = "$set.hasF32")]
pub extern "C" fn __set_has_f32(set: i64, v: f32) -> i64 {
    if set == 0 {
        return 0;
    }
    let s = unsafe { &*(set as *const ManagedSet) };
    if s.inner.contains(&SetElem::Int(v.to_bits() as i64)) { 1 } else { 0 }
}

#[unsafe(export_name = "$set.hasF64")]
pub extern "C" fn __set_has_f64(set: i64, v: f64) -> i64 {
    if set == 0 {
        return 0;
    }
    let s = unsafe { &*(set as *const ManagedSet) };
    if s.inner.contains(&SetElem::Int(v.to_bits() as i64)) { 1 } else { 0 }
}

#[unsafe(export_name = "$set.deleteF32")]
pub extern "C" fn __set_delete_f32(set: i64, v: f32) -> i64 {
    if set == 0 {
        return 0;
    }
    let s = unsafe { &mut *(set as *mut ManagedSet) };
    if s.inner.remove(&SetElem::Int(v.to_bits() as i64)) { 1 } else { 0 }
}

#[unsafe(export_name = "$set.deleteF64")]
pub extern "C" fn __set_delete_f64(set: i64, v: f64) -> i64 {
    if set == 0 {
        return 0;
    }
    let s = unsafe { &mut *(set as *mut ManagedSet) };
    if s.inner.remove(&SetElem::Int(v.to_bits() as i64)) { 1 } else { 0 }
}

#[unsafe(export_name = "$set.size")]
pub extern "C" fn __set_size(set: i64) -> i64 {
    if set == 0 {
        return 0;
    }
    let s = unsafe { &*(set as *const ManagedSet) };
    s.inner.len() as i64
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
    s.inner.clear();
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
    let elem_kind = if s.elem_print_kind == PK_STR { KIND_STR } else { KIND_NONE };
    let mut values: Vec<i64> = Vec::with_capacity(s.inner.len());
    if s.elem_print_kind == PK_STR {
        for e in s.inner.iter() {
            let orig = s.str_orig_or_leak(e);
            __retain_string(orig);
            values.push(orig);
        }
    } else {
        for e in s.inner.iter() {
            values.push(set_elem_to_raw(e));
        }
    }
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
        return;
    }
    let s = unsafe { &*(set as *const ManagedSet) };
    let is_str = s.elem_print_kind == PK_STR;
    let elems: Vec<SetElem> = s.inner.iter().cloned().collect();
    for e in elems {
        let arg = if is_str {
            let orig = s.str_orig_or_leak(&e);
            __retain_string(orig);
            orig
        } else {
            set_elem_to_raw(&e)
        };
        unsafe { call_closure_1_i64(closure, arg) };
        if is_str {
            __release_string(arg);
        }
    }
}

#[unsafe(export_name = "$set.forEachF32")]
pub extern "C" fn __set_for_each_f32(set: i64, closure: i64) {
    if set == 0 || closure == 0 {
        return;
    }
    let s = unsafe { &*(set as *const ManagedSet) };
    let elems: Vec<SetElem> = s.inner.iter().cloned().collect();
    for e in elems {
        if let SetElem::Int(bits) = e {
            let v = f32::from_bits(bits as u32);
            unsafe { call_closure_1_f32(closure, v) };
        }
    }
}

#[unsafe(export_name = "$set.forEachF64")]
pub extern "C" fn __set_for_each_f64(set: i64, closure: i64) {
    if set == 0 || closure == 0 {
        return;
    }
    let s = unsafe { &*(set as *const ManagedSet) };
    let elems: Vec<SetElem> = s.inner.iter().cloned().collect();
    for e in elems {
        if let SetElem::Int(bits) = e {
            let v = f64::from_bits(bits as u64);
            unsafe { call_closure_1_f64(closure, v) };
        }
    }
}

/// Helper: insert `e` into `target`'s inner set, taking ownership of
/// the matching string registry rc when the element is a PK_STR
/// pointer. Caller arranges the retain on the original side and we
/// transfer that share into `str_origs` here.
fn set_insert_transferred(target: &mut ManagedSet, e: SetElem, orig_str: Option<i64>) {
    if target.inner.contains(&e) {
        // Caller already retained — drop the share we'd otherwise
        // duplicate.
        if let Some(orig) = orig_str {
            __release_string(orig);
        }
        return;
    }
    if let Some(orig) = orig_str {
        target.str_origs.insert(e.clone(), orig);
    }
    target.inner.insert(e);
}

#[unsafe(export_name = "$set.union")]
pub extern "C" fn __set_union(a: i64, b: i64) -> i64 {
    let out = __set_new();
    let pk = if a != 0 {
        unsafe { &*(a as *const ManagedSet) }.elem_print_kind
    } else if b != 0 {
        unsafe { &*(b as *const ManagedSet) }.elem_print_kind
    } else {
        crate::kind::PK_OTHER
    };
    __set_set_elem_print_kind(out, pk);
    let is_str = pk == PK_STR;
    let out_s = unsafe { &mut *(out as *mut ManagedSet) };
    if a != 0 {
        let sa = unsafe { &*(a as *const ManagedSet) };
        for e in sa.inner.iter() {
            let orig = if is_str {
                let p = sa.str_origs.get(e).copied().unwrap_or_else(|| set_elem_to_raw(e));
                __retain_string(p);
                Some(p)
            } else {
                None
            };
            set_insert_transferred(out_s, e.clone(), orig);
        }
    }
    if b != 0 {
        let sb = unsafe { &*(b as *const ManagedSet) };
        for e in sb.inner.iter() {
            let orig = if is_str {
                let p = sb.str_origs.get(e).copied().unwrap_or_else(|| set_elem_to_raw(e));
                __retain_string(p);
                Some(p)
            } else {
                None
            };
            set_insert_transferred(out_s, e.clone(), orig);
        }
    }
    out
}

#[unsafe(export_name = "$set.intersection")]
pub extern "C" fn __set_intersection(a: i64, b: i64) -> i64 {
    let out = __set_new();
    if a == 0 || b == 0 {
        return out;
    }
    let sa = unsafe { &*(a as *const ManagedSet) };
    let sb = unsafe { &*(b as *const ManagedSet) };
    let pk = sa.elem_print_kind;
    __set_set_elem_print_kind(out, pk);
    let is_str = pk == PK_STR;
    let out_s = unsafe { &mut *(out as *mut ManagedSet) };
    for e in sa.inner.iter() {
        if sb.inner.contains(e) {
            let orig = if is_str {
                let p = sa.str_origs.get(e).copied().unwrap_or_else(|| set_elem_to_raw(e));
                __retain_string(p);
                Some(p)
            } else {
                None
            };
            set_insert_transferred(out_s, e.clone(), orig);
        }
    }
    out
}

#[unsafe(export_name = "$set.difference")]
pub extern "C" fn __set_difference(a: i64, b: i64) -> i64 {
    let out = __set_new();
    if a == 0 {
        return out;
    }
    let sa = unsafe { &*(a as *const ManagedSet) };
    let pk = sa.elem_print_kind;
    __set_set_elem_print_kind(out, pk);
    let is_str = pk == PK_STR;
    let out_s = unsafe { &mut *(out as *mut ManagedSet) };
    let empty_set;
    let sb_ref: &ManagedSet = if b == 0 {
        empty_set = ManagedSet {
            rc: AtomicI64::new(1),
            elem_print_kind: pk,
            inner: HashSet::new(),
            str_origs: std::collections::HashMap::new(),
        };
        &empty_set
    } else {
        unsafe { &*(b as *const ManagedSet) }
    };
    for e in sa.inner.iter() {
        if !sb_ref.inner.contains(e) {
            let orig = if is_str {
                let p = sa.str_origs.get(e).copied().unwrap_or_else(|| set_elem_to_raw(e));
                __retain_string(p);
                Some(p)
            } else {
                None
            };
            set_insert_transferred(out_s, e.clone(), orig);
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
    if sa.inner.is_empty() {
        return 1;
    }
    if b == 0 {
        return 0;
    }
    let sb = unsafe { &*(b as *const ManagedSet) };
    if sa.inner.iter().all(|e| sb.inner.contains(e)) { 1 } else { 0 }
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
    if sa.inner.iter().all(|e| !sb.inner.contains(e)) { 1 } else { 0 }
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
    let mut raws: Vec<i64> = s.inner.iter().map(set_elem_to_raw).collect();
    // Stable display: sort by the formatted text, the same trick
    // `__print_map` uses for HashMap iteration order.
    raws.sort_by(|a, b| {
        let mut sa = String::new();
        let mut sb = String::new();
        crate::print_dispatch::format_kind_id(&mut sa, pk, *a);
        crate::print_dispatch::format_kind_id(&mut sb, pk, *b);
        sa.cmp(&sb)
    });
    out.push_str("Set {");
    for (i, r) in raws.iter().enumerate() {
        if i == 0 {
            out.push(' ');
        } else {
            out.push_str(", ");
        }
        crate::print_dispatch::format_kind_id(out, pk, *r);
    }
    if !raws.is_empty() {
        out.push(' ');
    }
    out.push('}');
}
