//! Enum cell layout — every enum allocation (heap-tracked or
//! interned unit) carries a 24-byte header before the body:
//!
//! ```text
//! [ i64 cap | i64 rc | i64 eid | i64 tag | payload_0 | payload_1 | ... ]
//! ```
//!
//! with the body pointer (what user code sees) sitting 24 bytes past
//! the allocation base. `cap` is the total allocation size, used by
//! `__release_enum` to `__mir_free` the right amount. `rc` is the
//! atomic refcount — `rc = -1` marks an interned-unit / box cell so
//! `atomic_retain` / `atomic_release` (which skip on `rc <= 0`) treat
//! them as permanent. `eid` lets the release path look up the payload
//! kind table without an out-of-band side map. This removes the old
//! global `ENUM_REGISTRY` mutex from every retain / release call.

use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, OnceLock, RwLock};

use crate::alloc::{__mir_alloc, __mir_free};
use crate::cascade::release_field_by_kind;
use crate::print_dispatch::format_kind_id;
use crate::strings::{cstr_to_str, leak_cstring};

const ENUM_HEADER: i64 = 24;

#[inline]
unsafe fn write_enum_header(base: i64, cap: i64, rc: i64, eid: u32) {
    unsafe {
        *((base) as *mut i64) = cap;
        *((base + 8) as *mut i64) = rc;
        *((base + 16) as *mut i64) = eid as i64;
    }
}

/// Per-variant payload kinds, keyed by `(global_eid, tag)`. Each
/// `Vec` slot holds the `KIND_*` tag for the matching payload slot.
/// Wrapped in `Arc` so `__release_enum` can grab a pointer copy
/// without cloning the inner vec on every drop.
static ENUM_PAYLOAD_KINDS: OnceLock<RwLock<HashMap<(u32, i64), Arc<Vec<i64>>>>> = OnceLock::new();

fn enum_payload_kinds() -> &'static RwLock<HashMap<(u32, i64), Arc<Vec<i64>>>> {
    ENUM_PAYLOAD_KINDS.get_or_init(|| RwLock::new(HashMap::new()))
}

#[unsafe(export_name = "$enum.registerPayloadKind")]
pub extern "C" fn __register_enum_payload_kind(
    global_eid: i64,
    tag: i64,
    slot_idx: i64,
    kind: i64,
) {
    let mut t = enum_payload_kinds().write().expect("enum payload kinds poisoned");
    let entry = t.entry((global_eid as u32, tag)).or_default();
    let v = Arc::make_mut(entry);
    let idx = slot_idx as usize;
    while v.len() <= idx {
        v.push(0);
    }
    v[idx] = kind;
}

static ENUM_UNIT_CACHE: OnceLock<RwLock<HashMap<(u32, i64), i64>>> = OnceLock::new();

fn enum_unit_cache() -> &'static RwLock<HashMap<(u32, i64), i64>> {
    ENUM_UNIT_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

#[unsafe(export_name = "$enum.unitGet")]
pub extern "C" fn __enum_unit_get(global_eid: i64, disc: i64) -> i64 {
    let key = (global_eid as u32, disc);
    {
        let m = enum_unit_cache().read().expect("enum unit cache poisoned");
        if let Some(&p) = m.get(&key) {
            return p;
        }
    }
    let total = ENUM_HEADER + 8;
    let base = __mir_alloc(total);
    unsafe {
        // rc=-1 marks interned-unit so retain / release are no-ops.
        write_enum_header(base, 0, -1, global_eid as u32);
        *((base + ENUM_HEADER) as *mut i64) = disc;
    }
    let body = base + ENUM_HEADER;
    let mut m = enum_unit_cache().write().expect("enum unit cache poisoned");
    let installed = *m.entry(key).or_insert(body);
    if installed != body {
        // Lost the race against another thread that interned the
        // same variant — drop our allocation.
        drop(m);
        __mir_free(base, total);
    }
    installed
}

#[unsafe(export_name = "$enum.unitGetChecked")]
pub extern "C" fn __enum_unit_get_checked(global_eid: i64, disc: i64) -> i64 {
    let (valid, name) = {
        let t = enum_print_info().read().expect("enum print info poisoned");
        match t.get(&(global_eid as u32)) {
            Some(info) => (info.variants.contains_key(&disc), info.name.clone()),
            None => (false, format!("<enum#{global_eid}>")),
        }
    };
    if !valid {
        eprintln!(
            "ilang: read CRepr struct field of enum `{name}` with \
             unknown discriminant {disc} (0x{disc:X}) — declared variants \
             do not include this value",
        );
        // process::abort kills the process before the stderr pipe
        // flushes — under parent harnesses that read via Command::output
        // the message would otherwise be lost and the symptom looks
        // like a bare SIGABRT with empty stderr.
        use std::io::Write;
        let _ = std::io::stderr().flush();
        std::process::abort();
    }
    __enum_unit_get(global_eid, disc)
}

#[unsafe(export_name = "$enum.alloc")]
pub extern "C" fn __enum_alloc(global_eid: i64, n_payload: i64, disc: i64) -> i64 {
    let body_bytes = (1 + n_payload) * 8;
    let total = ENUM_HEADER + body_bytes;
    let base = __mir_alloc(total);
    unsafe {
        write_enum_header(base, total, 1, global_eid as u32);
        *((base + ENUM_HEADER) as *mut i64) = disc;
    }
    base + ENUM_HEADER
}

#[unsafe(export_name = "$enum.retain")]
pub extern "C" fn __retain_enum(p: i64) {
    if p == 0 {
        return;
    }
    let rc_ptr = (p - 16) as *mut i64;
    unsafe { crate::refcount::atomic_retain(rc_ptr) };
}

#[unsafe(export_name = "$enum.release")]
pub extern "C" fn __release_enum(p: i64) {
    if p == 0 {
        return;
    }
    let rc_ptr = (p - 16) as *mut i64;
    match unsafe { crate::refcount::atomic_release(rc_ptr) } {
        Some(0) => {}
        _ => return,
    }
    // Last reference — cascade-release payloads, then free the
    // [cap | rc | eid | body] block via the inline `cap`.
    let global_eid = unsafe { *((p - 8) as *const i64) } as u32;
    let tag = unsafe { *(p as *const i64) };
    let kinds = {
        let t = enum_payload_kinds().read().expect("enum payload kinds poisoned");
        t.get(&(global_eid, tag)).cloned()
    };
    if let Some(kinds) = kinds {
        for (i, kind) in kinds.iter().enumerate() {
            if *kind == 0 {
                continue;
            }
            let raw = unsafe { *((p + 8 + (i as i64) * 8) as *const i64) };
            release_field_by_kind(raw, *kind);
        }
    }
    let cap = unsafe { *((p - 24) as *const i64) };
    __mir_free(p - ENUM_HEADER, cap);
}

// --------------------------------------------------------------------
// Print info + `enum as string` cast
// --------------------------------------------------------------------

pub(crate) struct EnumPrintInfo {
    pub(crate) name: String,
    pub(crate) variants: HashMap<i64, (String, Vec<i64>)>,
}

static ENUM_PRINT_INFO: OnceLock<RwLock<HashMap<u32, EnumPrintInfo>>> = OnceLock::new();

pub(crate) fn enum_print_info() -> &'static RwLock<HashMap<u32, EnumPrintInfo>> {
    ENUM_PRINT_INFO.get_or_init(|| RwLock::new(HashMap::new()))
}

#[unsafe(export_name = "$enum.registerPrintName")]
pub extern "C" fn __register_enum_print_name(eid: i64, name_str_ptr: i64) {
    let name = cstr_to_str(name_str_ptr).to_string();
    let mut t = enum_print_info().write().expect("enum print info poisoned");
    let entry = t.entry(eid as u32).or_insert_with(|| EnumPrintInfo {
        name: String::new(),
        variants: HashMap::new(),
    });
    entry.name = name;
}

#[unsafe(export_name = "$enum.registerPrintVariantName")]
pub extern "C" fn __register_enum_print_variant_name(
    eid: i64,
    disc: i64,
    name_str_ptr: i64,
) {
    let name = cstr_to_str(name_str_ptr).to_string();
    let mut t = enum_print_info().write().expect("enum print info poisoned");
    let entry = t.entry(eid as u32).or_insert_with(|| EnumPrintInfo {
        name: String::new(),
        variants: HashMap::new(),
    });
    entry.variants.entry(disc).or_insert_with(|| (String::new(), Vec::new())).0 = name;
}

#[unsafe(export_name = "$enum.registerPrintVariantPayloadPk")]
pub extern "C" fn __register_enum_print_variant_payload_pk(
    eid: i64,
    disc: i64,
    slot_idx: i64,
    pk: i64,
) {
    let mut t = enum_print_info().write().expect("enum print info poisoned");
    let entry = t.entry(eid as u32).or_insert_with(|| EnumPrintInfo {
        name: String::new(),
        variants: HashMap::new(),
    });
    let v = entry.variants.entry(disc).or_insert_with(|| (String::new(), Vec::new()));
    let i = slot_idx as usize;
    while v.1.len() <= i {
        v.1.push(0);
    }
    v.1[i] = pk;
}

#[unsafe(export_name = "$print.enum")]
pub extern "C" fn __print_enum(enum_id: i64, ptr: i64) {
    let mut out = String::new();
    format_enum_into(&mut out, enum_id, ptr);
    let mut o = std::io::stdout().lock();
    let _ = o.write_all(out.as_bytes());
}

pub fn format_enum_into(out: &mut String, enum_id: i64, ptr: i64) {
    use std::fmt::Write;
    let info = {
        let t = enum_print_info().read().expect("enum print info poisoned");
        t.get(&(enum_id as u32))
            .map(|i| (i.name.clone(), i.variants.clone()))
    };
    let (name, variants) = match info {
        Some(x) => x,
        None => {
            let _ = write!(out, "<enum#{enum_id}>");
            return;
        }
    };
    if ptr == 0 {
        let _ = write!(out, "{name}::<null>");
        return;
    }
    let tag = unsafe { *(ptr as *const i64) };
    let (vname, pkinds) = match variants.get(&tag) {
        Some(v) => v.clone(),
        None => {
            let _ = write!(out, "{name}::<tag#{tag}>");
            return;
        }
    };
    let base = name.split('<').next().unwrap_or(name.as_str());
    out.push_str(base);
    out.push_str("::");
    out.push_str(&vname);
    if !pkinds.is_empty() {
        out.push('(');
        for (i, pk) in pkinds.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            let raw = unsafe { *((ptr + 8 + (i as i64) * 8) as *const i64) };
            format_kind_id(out, *pk, raw);
        }
        out.push(')');
    }
}

// `(global_eid, disc) → discriminant string` for `: string`-repr
// enums. Populated at compile time; read by `__enum_disc_str`.
static ENUM_DISC_STR: OnceLock<RwLock<HashMap<(u32, i64), String>>> = OnceLock::new();

fn enum_disc_str_table() -> &'static RwLock<HashMap<(u32, i64), String>> {
    ENUM_DISC_STR.get_or_init(|| RwLock::new(HashMap::new()))
}

#[unsafe(export_name = "$enum.registerDiscStr")]
pub extern "C" fn __register_enum_disc_str(
    global_eid: i64,
    disc: i64,
    str_ptr: i64,
) {
    let s = cstr_to_str(str_ptr).to_string();
    enum_disc_str_table()
        .write()
        .expect("enum disc str poisoned")
        .insert((global_eid as u32, disc), s);
}

#[unsafe(export_name = "$enum.discStr")]
pub extern "C" fn __enum_disc_str(global_eid: i64, disc: i64) -> i64 {
    let t = enum_disc_str_table()
        .read()
        .expect("enum disc str poisoned");
    match t.get(&(global_eid as u32, disc)) {
        Some(s) => leak_cstring(s.clone()),
        None => {
            eprintln!(
                "ilang: enum-as-string cast on unregistered (global_eid={global_eid}, disc={disc})"
            );
            use std::io::Write;
            let _ = std::io::stderr().flush();
            std::process::abort();
        }
    }
}
