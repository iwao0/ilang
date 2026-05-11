//! Class objects: vtable / drop dispatch / field-cascade table /
//! retain+release / class-name + print.
//!
//! Object header (16 bytes):
//!   +0  i64 class_id
//!   +8  i64 refcount
//!   +16 fields...

use std::collections::HashMap;
use std::io::Write;
use std::sync::{Mutex, OnceLock};

use crate::alloc::__mir_free;
use crate::cascade::release_field_by_kind;
use crate::strings::{cstr_to_str, leak_cstring};

// --------------------------------------------------------------------
// Class size table
// --------------------------------------------------------------------

static CLASS_SIZE_TABLE: OnceLock<Mutex<HashMap<u32, i64>>> = OnceLock::new();

fn class_size_table() -> &'static Mutex<HashMap<u32, i64>> {
    CLASS_SIZE_TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __register_class_size(class_id: i64, size: i64) {
    let mut t = class_size_table().lock().expect("class size table poisoned");
    t.insert(class_id as u32, size);
}

/// Look up the registered byte size for a class. Returns `None` when
/// the class was deliberately left out of the table (CRepr / packed /
/// union, weak-referenced, etc.).
pub fn class_size_for(class_id: i64) -> Option<i64> {
    let t = class_size_table().lock().expect("class size table poisoned");
    t.get(&(class_id as u32)).copied()
}

// --------------------------------------------------------------------
// Object field table (heap-typed field cascade)
// --------------------------------------------------------------------

static OBJECT_FIELD_TABLE: OnceLock<Mutex<HashMap<u32, Vec<(i64, i64)>>>> =
    OnceLock::new();

fn object_field_table() -> &'static Mutex<HashMap<u32, Vec<(i64, i64)>>> {
    OBJECT_FIELD_TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __register_object_field(class_id: i64, offset: i64, kind: i64) {
    let mut t = object_field_table().lock().expect("field table poisoned");
    t.entry(class_id as u32).or_default().push((offset, kind));
}

#[unsafe(no_mangle)]
pub extern "C" fn __release_object_fields(class_id: i64, obj_ptr: i64) {
    if obj_ptr == 0 {
        return;
    }
    let entries = {
        let t = object_field_table().lock().expect("field table poisoned");
        match t.get(&(class_id as u32)) {
            Some(e) if !e.is_empty() => e.clone(),
            _ => return,
        }
    };
    for (off, kind) in entries.iter() {
        let raw = unsafe { *((obj_ptr + *off) as *const i64) };
        release_field_by_kind(raw, *kind);
    }
}

// --------------------------------------------------------------------
// Vtable + drop dispatch
// --------------------------------------------------------------------

static VTABLE: OnceLock<Mutex<HashMap<(u32, u32), i64>>> = OnceLock::new();

fn vtable() -> &'static Mutex<HashMap<(u32, u32), i64>> {
    VTABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __register_vtable_entry(class_id: i64, slot: i64, fn_addr: i64) {
    let mut t = vtable().lock().expect("vtable poisoned");
    t.insert((class_id as u32, slot as u32), fn_addr);
}

#[unsafe(no_mangle)]
pub extern "C" fn __virt_dispatch(class_id: i64, slot: i64) -> i64 {
    let t = vtable().lock().expect("vtable poisoned");
    *t.get(&(class_id as u32, slot as u32)).unwrap_or(&0)
}

static DROP_TABLE: OnceLock<Mutex<HashMap<u32, i64>>> = OnceLock::new();

fn drop_table() -> &'static Mutex<HashMap<u32, i64>> {
    DROP_TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __register_drop(class_id: i64, fn_addr: i64) {
    let mut t = drop_table().lock().expect("drop table poisoned");
    t.insert(class_id as u32, fn_addr);
}

#[unsafe(no_mangle)]
pub extern "C" fn __drop_dispatch(class_id: i64) -> i64 {
    let t = drop_table().lock().expect("drop table poisoned");
    *t.get(&(class_id as u32)).unwrap_or(&0)
}

// --------------------------------------------------------------------
// Object retain / release
// --------------------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn __retain_object(obj_ptr: i64) {
    if obj_ptr == 0 {
        return;
    }
    let rc_ptr = (obj_ptr + 8) as *mut i64;
    let rc = unsafe { *rc_ptr };
    if rc <= 0 {
        return;
    }
    unsafe {
        *rc_ptr = rc + 1;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __release_object(obj_ptr: i64) {
    if obj_ptr == 0 {
        return;
    }
    let rc_ptr = (obj_ptr + 8) as *mut i64;
    let rc = unsafe { *rc_ptr };
    if rc <= 0 {
        return;
    }
    let new_rc = rc - 1;
    unsafe {
        *rc_ptr = new_rc;
    }
    if new_rc != 0 {
        return;
    }
    let class_id = unsafe { *(obj_ptr as *const i64) };
    let user_drop = __drop_dispatch(class_id);
    if user_drop != 0 {
        let f: extern "C" fn(i64, i64) = unsafe { std::mem::transmute(user_drop) };
        f(obj_ptr, 0);
    }
    __release_object_fields(class_id, obj_ptr);
    if let Some(sz) = class_size_for(class_id) {
        __mir_free(obj_ptr, sz);
    }
}

// --------------------------------------------------------------------
// Class print info + __print_object + __class_name
// --------------------------------------------------------------------

pub(crate) struct ClassPrintInfo {
    pub(crate) name: String,
    pub(crate) fields: Vec<(String, i64)>,
}

static CLASS_PRINT_INFO: OnceLock<Mutex<HashMap<u32, ClassPrintInfo>>> = OnceLock::new();

pub(crate) fn class_print_info() -> &'static Mutex<HashMap<u32, ClassPrintInfo>> {
    CLASS_PRINT_INFO.get_or_init(|| Mutex::new(HashMap::new()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __register_class_print_name(class_id: i64, name_str_ptr: i64) {
    let name = cstr_to_str(name_str_ptr).to_string();
    let mut t = class_print_info().lock().expect("class print info poisoned");
    let entry = t
        .entry(class_id as u32)
        .or_insert_with(|| ClassPrintInfo { name: String::new(), fields: Vec::new() });
    entry.name = name;
}

#[unsafe(no_mangle)]
pub extern "C" fn __register_class_print_field(
    class_id: i64,
    idx: i64,
    name_str_ptr: i64,
    pk: i64,
) {
    let name = cstr_to_str(name_str_ptr).to_string();
    let mut t = class_print_info().lock().expect("class print info poisoned");
    let entry = t
        .entry(class_id as u32)
        .or_insert_with(|| ClassPrintInfo { name: String::new(), fields: Vec::new() });
    let i = idx as usize;
    while entry.fields.len() <= i {
        entry.fields.push((String::new(), 0));
    }
    entry.fields[i] = (name, pk);
}

#[unsafe(no_mangle)]
pub extern "C" fn __class_name(class_id: i64) -> i64 {
    let name = {
        let t = class_print_info().lock().expect("class print info poisoned");
        t.get(&(class_id as u32)).map(|i| i.name.clone())
    };
    let name = name.unwrap_or_else(|| format!("<obj#{class_id}>"));
    let base = name.split('<').next().unwrap_or(name.as_str()).to_string();
    leak_cstring(base)
}

#[unsafe(no_mangle)]
pub extern "C" fn __print_object(obj_ptr: i64) {
    let mut out = std::io::stdout().lock();
    if obj_ptr == 0 {
        let _ = out.write_all(b"<null>");
        return;
    }
    let mut s = String::new();
    format_object_into(&mut s, obj_ptr);
    let _ = out.write_all(s.as_bytes());
}

pub fn format_object_into(out: &mut String, obj_ptr: i64) {
    use std::fmt::Write;
    if obj_ptr == 0 {
        out.push_str("<null>");
        return;
    }
    let class_id = unsafe { *(obj_ptr as *const i64) } as u32;
    let info = {
        let t = class_print_info().lock().expect("class print info poisoned");
        t.get(&class_id).map(|i| (i.name.clone(), i.fields.clone()))
    };
    let (name, fields) = match info {
        Some(x) => x,
        None => {
            let _ = write!(out, "<obj#{class_id}>");
            return;
        }
    };
    let base = name.split('<').next().unwrap_or(name.as_str());
    out.push_str(base);
    out.push_str(" {");
    if !fields.is_empty() {
        out.push(' ');
        for (i, (fname, pk)) in fields.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(fname);
            out.push_str(": ");
            let raw = unsafe { *((obj_ptr + 16 + (i as i64) * 8) as *const i64) };
            crate::print_dispatch::format_kind_id(out, *pk, raw);
        }
        out.push(' ');
    }
    out.push('}');
}
