//! Per-`PK_*` formatter used by `__print_map`, `__print_object`'s
//! field walk, and `__print_enum`'s payload walk. Recurses through
//! `format_object_into` (in `classes`) so nested heap objects print
//! their structured form.

use crate::classes::format_object_into;
use crate::kind::{
    PK_ARRAY_I64_SIG, PK_BOOL, PK_ENUM, PK_F32, PK_F64, PK_I16_SIG, PK_I16_UNS,
    PK_I32_SIG, PK_I32_UNS, PK_I64_SIG, PK_I64_UNS, PK_I8_SIG,
    PK_I8_UNS, PK_OBJECT, PK_STR,
};
use crate::strings::cstr_bytes;

fn format_f64_like_jit(f: f64) -> String {
    if f.is_nan() {
        "NaN".to_string()
    } else if f.is_infinite() {
        if f > 0.0 { "Infinity".to_string() } else { "-Infinity".to_string() }
    } else if f == f.trunc() && f.abs() < 1e16 {
        format!("{}.0", f as i64)
    } else {
        format!("{f}")
    }
}

pub(crate) fn format_kind_id(out: &mut String, kind: i64, raw: i64) {
    use std::fmt::Write;
    match kind {
        PK_I64_SIG => { let _ = write!(out, "{}", raw); }
        PK_I64_UNS => { let _ = write!(out, "{}", raw as u64); }
        PK_I32_SIG => { let _ = write!(out, "{}", raw as i32); }
        PK_I32_UNS => { let _ = write!(out, "{}", raw as u32); }
        PK_I16_SIG => { let _ = write!(out, "{}", raw as i16); }
        PK_I16_UNS => { let _ = write!(out, "{}", raw as u16); }
        PK_I8_SIG => { let _ = write!(out, "{}", raw as i8); }
        PK_I8_UNS => { let _ = write!(out, "{}", raw as u8); }
        PK_BOOL => { let _ = write!(out, "{}", raw != 0); }
        PK_F64 => {
            let f = f64::from_bits(raw as u64);
            let _ = write!(out, "{}", format_f64_like_jit(f));
        }
        PK_F32 => {
            let f = f32::from_bits((raw as i32) as u32);
            let _ = write!(out, "{}", format_f64_like_jit(f as f64));
        }
        PK_STR => {
            if raw != 0 {
                let bytes = unsafe { cstr_bytes(raw) };
                let _ = write!(out, "{}", String::from_utf8_lossy(bytes));
            }
        }
        PK_OBJECT => {
            if raw == 0 {
                out.push_str("<null>");
            } else {
                format_object_into(out, raw);
            }
        }
        PK_ENUM => {
            if raw == 0 {
                out.push_str("<null>");
            } else {
                // The enum id is stored just below the value pointer.
                let eid = unsafe { *((raw - 8) as *const i64) };
                crate::enums::format_enum_into(out, eid, raw);
            }
        }
        PK_ARRAY_I64_SIG => {
            out.push('[');
            if raw != 0 {
                let len = unsafe { *(raw as *const i64) };
                let data_ptr = unsafe { *((raw + 16) as *const i64) };
                for i in 0..len {
                    if i > 0 { out.push_str(", "); }
                    let elem = unsafe { *((data_ptr + i * 8) as *const i64) };
                    let _ = write!(out, "{}", elem);
                }
            }
            out.push(']');
        }
        _ => { let _ = write!(out, "{}", raw); }
    }
}
