//! Runtime backing for the built-in `Regex` class declared in
//! `libs/std/regex.il`. Compiled patterns live on the Rust heap and
//! are reached through an opaque `i64` handle = `Box::into_raw` of a
//! `Regex`. `__regex_destroy` drops the box; ilang's `deinit`
//! invokes that when the wrapping `Regex` object's refcount drops
//! to zero.
//!
//! The wrapped engine is the upstream `regex` crate, so backrefs
//! and lookaround are not available (real regular languages only).

use regex::{Regex, RegexBuilder};

use crate::arrays::__c_array_to_array;
use crate::kind::{KIND_NONE, KIND_STR};
use crate::strings::{cstr_to_str, leak_cstring};

unsafe fn from_handle<'a>(handle: i64) -> Option<&'a Regex> {
    if handle == 0 {
        None
    } else {
        Some(unsafe { &*(handle as *const Regex) })
    }
}

/// Compile `pattern` with the flags from `flags` (each char is
/// case-insensitive `i`, multi-line `m`, dot-matches-newline `s`,
/// or extended/ignore-whitespace `x`). Returns the boxed handle or
/// aborts the process when the pattern is malformed — the same
/// failure shape as other "construction can't fail at runtime"
/// builtins.
#[unsafe(export_name = "$regex.compile")]
pub extern "C" fn __regex_compile(pattern: i64, flags: i64) -> i64 {
    let p = cstr_to_str(pattern);
    let f = if flags == 0 { "" } else { cstr_to_str(flags) };
    let mut builder = RegexBuilder::new(p);
    for c in f.chars() {
        match c {
            'i' => { builder.case_insensitive(true); }
            'm' => { builder.multi_line(true); }
            's' => { builder.dot_matches_new_line(true); }
            'x' => { builder.ignore_whitespace(true); }
            _ => {
                eprintln!(
                    "regex: unsupported flag {:?} (allowed: i / m / s / x)",
                    c
                );
                std::process::abort();
            }
        }
    }
    match builder.build() {
        Ok(r) => Box::into_raw(Box::new(r)) as i64,
        Err(e) => {
            eprintln!("regex: failed to compile pattern {p:?}: {e}");
            std::process::abort();
        }
    }
}

#[unsafe(export_name = "$regex.destroy")]
pub extern "C" fn __regex_destroy(handle: i64) {
    if handle == 0 {
        return;
    }
    unsafe {
        drop(Box::from_raw(handle as *mut Regex));
    }
}

#[unsafe(export_name = "$regex.test")]
pub extern "C" fn __regex_test(handle: i64, s: i64) -> i32 {
    let Some(r) = (unsafe { from_handle(handle) }) else { return 0 };
    let txt = cstr_to_str(s);
    if r.is_match(txt) { 1 } else { 0 }
}

#[unsafe(export_name = "$regex.has_match")]
pub extern "C" fn __regex_has_match(handle: i64, s: i64) -> i32 {
    __regex_test(handle, s)
}

/// Return the first match's substring as a freshly-allocated ilang
/// string. Caller has already checked `__regex_has_match`; when no
/// match exists this returns an empty string (callers shouldn't
/// reach here in that case).
#[unsafe(export_name = "$regex.first_match")]
pub extern "C" fn __regex_first_match(handle: i64, s: i64) -> i64 {
    let Some(r) = (unsafe { from_handle(handle) }) else {
        return leak_cstring(String::new());
    };
    let txt = cstr_to_str(s);
    let m = r.find(txt).map(|m| m.as_str().to_string()).unwrap_or_default();
    leak_cstring(m)
}

#[unsafe(export_name = "$regex.replace_all")]
pub extern "C" fn __regex_replace_all(handle: i64, s: i64, with: i64) -> i64 {
    let Some(r) = (unsafe { from_handle(handle) }) else {
        return leak_cstring(cstr_to_str(s).to_string());
    };
    let txt = cstr_to_str(s);
    let rep = cstr_to_str(with);
    leak_cstring(r.replace_all(txt, rep).into_owned())
}

#[unsafe(export_name = "$regex.find_all")]
pub extern "C" fn __regex_find_all(handle: i64, s: i64) -> i64 {
    let Some(r) = (unsafe { from_handle(handle) }) else {
        return empty_string_array();
    };
    let txt = cstr_to_str(s);
    let cells: Vec<i64> = r
        .find_iter(txt)
        .map(|m| leak_cstring(m.as_str().to_string()))
        .collect();
    build_string_array(&cells)
}

#[unsafe(export_name = "$regex.split")]
pub extern "C" fn __regex_split(handle: i64, s: i64) -> i64 {
    let Some(r) = (unsafe { from_handle(handle) }) else {
        return empty_string_array();
    };
    let txt = cstr_to_str(s);
    let cells: Vec<i64> = r
        .split(txt)
        .map(|piece| leak_cstring(piece.to_string()))
        .collect();
    build_string_array(&cells)
}

fn empty_string_array() -> i64 {
    let empty: Vec<i64> = Vec::new();
    build_string_array(&empty)
}

fn build_string_array(cells: &[i64]) -> i64 {
    // Copy the i64 cells into a fresh dynamic array. stride = 8
    // (i64 cells), kind = KIND_STR so the array's cascade releases
    // the inner strings when the array's rc hits zero.
    if cells.is_empty() {
        return __c_array_to_array(0, 0, 8, KIND_STR);
    }
    __c_array_to_array(
        cells.as_ptr() as i64,
        cells.len() as i64,
        8,
        KIND_STR,
    )
}

// Silence the unused-import warning when `regex`'s feature flags
// are pared down — `KIND_NONE` is part of the wider runtime API
// surface we re-export, not used here.
#[allow(dead_code)]
fn _kind_none_ref() -> i64 {
    KIND_NONE
}
