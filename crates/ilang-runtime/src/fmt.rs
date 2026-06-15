//! Value-to-string formatters used by backtick template literals.
//! Each `$fmt.*` mirrors the corresponding `$print.*` but returns a
//! newly allocated ilang string (registered with the string registry)
//! rather than writing to stdout. The codegen layer's
//! `emit_format_value` selects which one to call based on the static
//! MIR type of each interpolated expression.

use crate::strings::{cstr_bytes, leak_cstring};

#[unsafe(export_name = "$fmt.int")]
pub extern "C" fn __fmt_int(n: i64) -> i64 {
    leak_cstring(n.to_string())
}

/// `${u64}` interpolation — unsigned decimal (see `__uint_to_string`).
#[unsafe(export_name = "$fmt.uint")]
pub extern "C" fn __fmt_uint(n: i64) -> i64 {
    leak_cstring((n as u64).to_string())
}

#[unsafe(export_name = "$fmt.bool")]
pub extern "C" fn __fmt_bool(b: i64) -> i64 {
    leak_cstring(if b != 0 { "true".to_string() } else { "false".to_string() })
}

#[unsafe(export_name = "$fmt.f64")]
pub extern "C" fn __fmt_f64(x: f64) -> i64 {
    let s = if x.is_nan() {
        "NaN".to_string()
    } else if x.is_infinite() {
        if x > 0.0 { "Infinity".to_string() } else { "-Infinity".to_string() }
    } else if x == x.trunc() && x.abs() < 1e16 {
        format!("{}.0", x as i64)
    } else {
        format!("{x}")
    };
    leak_cstring(s)
}

#[unsafe(export_name = "$fmt.str")]
pub extern "C" fn __fmt_str(p: i64) -> i64 {
    if p == 0 {
        return leak_cstring(String::new());
    }
    let bytes = unsafe { cstr_bytes(p) };
    leak_cstring(String::from_utf8_lossy(bytes).into_owned())
}

#[unsafe(export_name = "$fmt.weak")]
pub extern "C" fn __fmt_weak(weak_ptr: i64) -> i64 {
    if weak_ptr == 0 {
        return leak_cstring("weak(<dead>)".to_string());
    }
    let rc = unsafe { *((weak_ptr + 8) as *const i64) };
    let s = if rc <= 0 { "weak(<dead>)" } else { "weak(<alive>)" };
    leak_cstring(s.to_string())
}

#[unsafe(export_name = "$fmt.fn")]
pub extern "C" fn __fmt_fn(closure_ptr: i64) -> i64 {
    if closure_ptr == 0 {
        return leak_cstring("<fn>".to_string());
    }
    let fn_addr = unsafe { *(closure_ptr as *const i64) };
    let s = crate::print::fn_name_for_addr(fn_addr)
        .map(|n| format!("<fn {n}>"))
        .unwrap_or_else(|| "<fn>".to_string());
    leak_cstring(s)
}

#[unsafe(export_name = "$fmt.object")]
pub extern "C" fn __fmt_object(obj_ptr: i64) -> i64 {
    let mut s = String::new();
    if obj_ptr == 0 {
        s.push_str("<null>");
    } else {
        crate::classes::format_object_into(&mut s, obj_ptr);
    }
    leak_cstring(s)
}

#[unsafe(export_name = "$fmt.struct")]
pub extern "C" fn __fmt_struct(class_id: i64, ptr: i64) -> i64 {
    let mut s = String::new();
    if ptr == 0 {
        s.push_str("<null>");
    } else {
        crate::classes::format_struct_into(&mut s, class_id, ptr);
    }
    leak_cstring(s)
}

#[unsafe(export_name = "$fmt.map")]
pub extern "C" fn __fmt_map(map_ptr: i64) -> i64 {
    let mut s = String::new();
    crate::maps::format_map_into(&mut s, map_ptr);
    leak_cstring(s)
}

#[unsafe(export_name = "$fmt.set")]
pub extern "C" fn __fmt_set(set_ptr: i64) -> i64 {
    let mut s = String::new();
    crate::sets::format_set_into(&mut s, set_ptr);
    leak_cstring(s)
}

#[unsafe(export_name = "$fmt.enum")]
pub extern "C" fn __fmt_enum(enum_id: i64, ptr: i64) -> i64 {
    let mut s = String::new();
    crate::enums::format_enum_into(&mut s, enum_id, ptr);
    leak_cstring(s)
}
