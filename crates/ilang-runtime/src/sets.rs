//! Set runtime — `ManagedSet` wraps Rust's `HashSet<SetElem>` with a
//! refcount and a print-kind tag. The element-kind constraints mirror
//! `Map`'s keys (string / integer / bool); the type checker enforces
//! that, the runtime only cares whether the raw cell needs string
//! handling on insert / iterate.

use std::collections::HashSet;
use std::sync::atomic::{AtomicI64, Ordering, fence};

use crate::kind::PK_STR;
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

#[unsafe(export_name = "$set.retain")]
pub extern "C" fn __retain_set(set: i64) {
    if set == 0 {
        return;
    }
    let s = unsafe { &*(set as *const ManagedSet) };
    let mut cur = s.rc.load(Ordering::Relaxed);
    loop {
        if cur <= 0 {
            return;
        }
        match s.rc.compare_exchange_weak(cur, cur + 1, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(actual) => cur = actual,
        }
    }
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
