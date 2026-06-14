//! Structural value equality, shared by `==` / `!=` on heap value types
//! (string, enum, tuple, dynamic array, optional) and by array
//! `indexOf` / `includes` / `remove`.
//!
//! Numbers / bools (KIND_NONE) compare bitwise; reference types (object,
//! map, set, closure, weak, promise) compare by pointer — matching `==`
//! on classes. Container kinds (tuple / array / optional) and enums
//! recurse element-wise so two independently-built values with equal
//! contents compare equal even though their cells differ.

use crate::kind::{KIND_ARRAY, KIND_ENUM, KIND_OPTIONAL, KIND_STR, KIND_TUPLE};

/// Compare two cell values that share the static `kind`.
pub(crate) fn value_structural_eq(a: i64, b: i64, kind: i64) -> bool {
    match kind {
        KIND_STR => crate::strings::__str_eq(a, b) != 0,
        KIND_ENUM => crate::enums::__enum_structural_eq(a, b) != 0,
        KIND_TUPLE => __tuple_structural_eq(a, b) != 0,
        KIND_ARRAY => __array_structural_eq(a, b) != 0,
        KIND_OPTIONAL => __optional_structural_eq(a, b) != 0,
        // KIND_NONE (raw word) + object / map / set / closure / weak /
        // promise: bit / reference equality.
        _ => a == b,
    }
}

/// Tuple cell: `base = ptr - 16`; the packed word at `base + 8`
/// (i.e. `ptr - 8`) holds arity in the low 16 bits and a 4-bit KIND tag
/// per element for the first 12 slots (same encoding the release cascade
/// reads).
#[unsafe(export_name = "$tuple.structuralEq")]
pub extern "C" fn __tuple_structural_eq(a: i64, b: i64) -> i64 {
    if a == b {
        return 1;
    }
    if a == 0 || b == 0 {
        return 0;
    }
    let packed_a = unsafe { *((a - 8) as *const i64) } as u64;
    let packed_b = unsafe { *((b - 8) as *const i64) } as u64;
    let arity = (packed_a & 0xFFFF) as i64;
    if arity != (packed_b & 0xFFFF) as i64 {
        return 0;
    }
    for i in 0..arity {
        let ea = unsafe { *((a + i * 8) as *const i64) };
        let eb = unsafe { *((b + i * 8) as *const i64) };
        // Past the 12 packed slots the kind is unknown (0 = raw word) —
        // the same bound the release cascade uses.
        let kind = if i < 12 {
            ((packed_a >> (16 + (i as u64) * 4)) & 0xF) as i64
        } else {
            0
        };
        if !value_structural_eq(ea, eb, kind) {
            return 0;
        }
    }
    1
}

/// Dynamic-array header: `+0 len | +8 cap | +16 data | +24 rc |
/// +32 elem KIND | +40 stride`.
#[unsafe(export_name = "$array.structuralEq")]
pub extern "C" fn __array_structural_eq(a: i64, b: i64) -> i64 {
    if a == b {
        return 1;
    }
    if a == 0 || b == 0 {
        return 0;
    }
    let len = unsafe { *(a as *const i64) };
    if len != unsafe { *(b as *const i64) } {
        return 0;
    }
    let data_a = unsafe { *((a as *const i64).add(2)) };
    let data_b = unsafe { *((b as *const i64).add(2)) };
    let kind = unsafe { *((a as *const i64).add(4)) };
    let stride = unsafe { *((a as *const i64).add(5)) };
    for i in 0..len {
        let ea = unsafe { crate::arrays::load_packed(data_a, i, stride) };
        let eb = unsafe { crate::arrays::load_packed(data_b, i, stride) };
        if !value_structural_eq(ea, eb, kind) {
            return 0;
        }
    }
    1
}

/// Optional cell: `+0 value | +8 rc | +16 inner KIND`. `none` is the
/// null pointer, so unequal presence is caught by the null guards.
#[unsafe(export_name = "$optional.structuralEq")]
pub extern "C" fn __optional_structural_eq(a: i64, b: i64) -> i64 {
    if a == b {
        return 1; // both none, or the same cell
    }
    if a == 0 || b == 0 {
        return 0; // exactly one none
    }
    let val_a = unsafe { *(a as *const i64) };
    let val_b = unsafe { *(b as *const i64) };
    let kind = unsafe { *((a + 16) as *const i64) };
    value_structural_eq(val_a, val_b, kind) as i64
}
