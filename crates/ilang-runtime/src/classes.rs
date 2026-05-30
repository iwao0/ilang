//! Class objects: vtable / drop dispatch / field-cascade table /
//! retain+release / class-name + print.
//!
//! Object header (16 bytes):
//!   +0  i64 class_id
//!   +8  i64 refcount
//!   +16 fields...

use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

use crate::alloc::__mir_free;
use crate::cascade::release_field_by_kind;
use crate::strings::{cstr_to_str, leak_cstring};

// --------------------------------------------------------------------
// Class size table
// --------------------------------------------------------------------

static CLASS_SIZE_TABLE: OnceLock<RwLock<HashMap<u32, i64>>> = OnceLock::new();

fn class_size_table() -> &'static RwLock<HashMap<u32, i64>> {
    CLASS_SIZE_TABLE.get_or_init(|| RwLock::new(HashMap::new()))
}

#[unsafe(export_name = "$class.registerSize")]
pub extern "C" fn __register_class_size(class_id: i64, size: i64) {
    let mut t = class_size_table().write().expect("class size table poisoned");
    t.insert(class_id as u32, size);
}

/// Look up the registered byte size for a class. Returns `None` for
/// CRepr / packed / union classes (different free path); regular
/// classes — including those referenced via `.weak` — always register
/// here so `__release_object` and `__release_weak` can free the box.
pub fn class_size_for(class_id: i64) -> Option<i64> {
    let t = class_size_table().read().expect("class size table poisoned");
    t.get(&(class_id as u32)).copied()
}

// --------------------------------------------------------------------
// Weak-reference counter side-table
// --------------------------------------------------------------------
//
// `ClassName.weak` references share storage with the strong cell — the
// weak pointer IS the object pointer. To free that storage safely we
// keep a side-table of "live weak refs per object." `__release_object`
// (strong → 0) consults the table and skips `__mir_free` if any weak
// refs are still pending; `__release_weak`'s final decrement then does
// the free instead. The write-lock serialises the free decision so a
// concurrent strong / weak release pair can't race past each other.
//
// Entry exists only for objects with at least one outstanding weak ref.
// `WeakUpgrade` reads the object's strong rc field directly (no table
// lookup) — that's safe as long as the box is alive, which the entry
// keeps it.

static WEAK_COUNT_TABLE: OnceLock<RwLock<HashMap<i64, AtomicI64>>> = OnceLock::new();

fn weak_count_table() -> &'static RwLock<HashMap<i64, AtomicI64>> {
    WEAK_COUNT_TABLE.get_or_init(|| RwLock::new(HashMap::new()))
}

#[unsafe(export_name = "$weak.retain")]
pub extern "C" fn __retain_weak(obj_ptr: i64) {
    if obj_ptr == 0 {
        return;
    }
    let mut t = weak_count_table().write().expect("weak count table poisoned");
    t.entry(obj_ptr)
        .or_insert_with(|| AtomicI64::new(0))
        .fetch_add(1, Ordering::Relaxed);
}

#[unsafe(export_name = "$weak.release")]
pub extern "C" fn __release_weak(obj_ptr: i64) {
    if obj_ptr == 0 {
        return;
    }
    let mut t = weak_count_table().write().expect("weak count table poisoned");
    let prev = match t.get(&obj_ptr) {
        Some(entry) => entry.fetch_sub(1, Ordering::Relaxed),
        None => return,
    };
    if prev > 1 {
        return;
    }
    // Final weak release: remove the table entry, then — while still
    // holding the write lock so `__release_object` can't race — check
    // strong rc. If it's already 0 the object is "logically dead" and
    // we own the free.
    t.remove(&obj_ptr);
    let strong = unsafe { (*((obj_ptr + 8) as *const AtomicI64)).load(Ordering::Acquire) };
    if strong == 0 {
        let class_id = unsafe { *(obj_ptr as *const i64) };
        if let Some(sz) = class_size_for(class_id) {
            __mir_free(obj_ptr, sz);
        }
    }
}

// --------------------------------------------------------------------
// Object field table (heap-typed field cascade)
// --------------------------------------------------------------------

static OBJECT_FIELD_TABLE: OnceLock<RwLock<HashMap<u32, Arc<Vec<(i64, i64)>>>>> =
    OnceLock::new();

fn object_field_table() -> &'static RwLock<HashMap<u32, Arc<Vec<(i64, i64)>>>> {
    OBJECT_FIELD_TABLE.get_or_init(|| RwLock::new(HashMap::new()))
}

#[unsafe(export_name = "$class.registerObjectField")]
pub extern "C" fn __register_object_field(class_id: i64, offset: i64, kind: i64) {
    let mut t = object_field_table().write().expect("field table poisoned");
    let entry = t.entry(class_id as u32).or_default();
    Arc::make_mut(entry).push((offset, kind));
}

#[unsafe(export_name = "$class.releaseObjectFields")]
pub extern "C" fn __release_object_fields(class_id: i64, obj_ptr: i64) {
    if obj_ptr == 0 {
        return;
    }
    // Bump an Arc reference instead of cloning the Vec — the table is
    // append-only during registration and never mutated after startup,
    // so this is a cheap pointer copy. Released outside the lock to
    // avoid re-entering `object_field_table()` from nested releases.
    let entries = {
        let t = object_field_table().read().expect("field table poisoned");
        match t.get(&(class_id as u32)) {
            Some(e) if !e.is_empty() => Arc::clone(e),
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

static VTABLE: OnceLock<RwLock<HashMap<(u32, u32), i64>>> = OnceLock::new();

fn vtable() -> &'static RwLock<HashMap<(u32, u32), i64>> {
    VTABLE.get_or_init(|| RwLock::new(HashMap::new()))
}

#[unsafe(export_name = "$class.registerVtableEntry")]
pub extern "C" fn __register_vtable_entry(class_id: i64, slot: i64, fn_addr: i64) {
    let mut t = vtable().write().expect("vtable poisoned");
    t.insert((class_id as u32, slot as u32), fn_addr);
}

#[unsafe(export_name = "$class.virtDispatch")]
pub extern "C" fn __virt_dispatch(class_id: i64, slot: i64) -> i64 {
    let t = vtable().read().expect("vtable poisoned");
    *t.get(&(class_id as u32, slot as u32)).unwrap_or(&0)
}

static DROP_TABLE: OnceLock<RwLock<HashMap<u32, i64>>> = OnceLock::new();

fn drop_table() -> &'static RwLock<HashMap<u32, i64>> {
    DROP_TABLE.get_or_init(|| RwLock::new(HashMap::new()))
}

#[unsafe(export_name = "$class.registerDrop")]
pub extern "C" fn __register_drop(class_id: i64, fn_addr: i64) {
    let mut t = drop_table().write().expect("drop table poisoned");
    t.insert(class_id as u32, fn_addr);
}

#[unsafe(export_name = "$class.dropDispatch")]
pub extern "C" fn __drop_dispatch(class_id: i64) -> i64 {
    let t = drop_table().read().expect("drop table poisoned");
    *t.get(&(class_id as u32)).unwrap_or(&0)
}

// --------------------------------------------------------------------
// Object retain / release
// --------------------------------------------------------------------

#[unsafe(export_name = "$class.retainObject")]
pub extern "C" fn __retain_object(obj_ptr: i64) {
    if obj_ptr == 0 {
        return;
    }
    let rc_ptr = (obj_ptr + 8) as *mut i64;
    unsafe { crate::refcount::atomic_retain(rc_ptr) };
}

#[unsafe(export_name = "$class.releaseObject")]
pub extern "C" fn __release_object(obj_ptr: i64) {
    if obj_ptr == 0 {
        return;
    }
    let rc_ptr = (obj_ptr + 8) as *mut i64;
    match unsafe { crate::refcount::atomic_release(rc_ptr) } {
        Some(0) => {}
        _ => return,
    }
    let class_id = unsafe { *(obj_ptr as *const i64) };
    let user_drop = __drop_dispatch(class_id);
    if user_drop != 0 {
        let f: extern "C" fn(i64, i64) = unsafe { std::mem::transmute(user_drop) };
        f(obj_ptr, 0);
    }
    __release_object_fields(class_id, obj_ptr);
    // If any weak refs are still outstanding, keep the box alive so
    // `WeakUpgrade` can safely peek the (now-zero) rc field. The final
    // `__release_weak` will free it. Hold the write lock across the
    // check + free so a concurrent weak-release-to-zero can't see no
    // entry, decide we own the free, and free under us.
    let t = weak_count_table().write().expect("weak count table poisoned");
    let has_weaks = t
        .get(&obj_ptr)
        .map(|c| c.load(Ordering::Relaxed) > 0)
        .unwrap_or(false);
    if !has_weaks {
        if let Some(sz) = class_size_for(class_id) {
            __mir_free(obj_ptr, sz);
        }
    }
    drop(t);
}

// --------------------------------------------------------------------
// Class print info + __print_object + __class_name
// --------------------------------------------------------------------

pub(crate) struct ClassPrintInfo {
    pub(crate) name: String,
    pub(crate) fields: Vec<(String, i64)>,
}

static CLASS_PRINT_INFO: OnceLock<RwLock<HashMap<u32, ClassPrintInfo>>> = OnceLock::new();

pub(crate) fn class_print_info() -> &'static RwLock<HashMap<u32, ClassPrintInfo>> {
    CLASS_PRINT_INFO.get_or_init(|| RwLock::new(HashMap::new()))
}

#[unsafe(export_name = "$class.registerPrintName")]
pub extern "C" fn __register_class_print_name(class_id: i64, name_str_ptr: i64) {
    let name = cstr_to_str(name_str_ptr).to_string();
    let mut t = class_print_info().write().expect("class print info poisoned");
    let entry = t
        .entry(class_id as u32)
        .or_insert_with(|| ClassPrintInfo { name: String::new(), fields: Vec::new() });
    entry.name = name;
}

#[unsafe(export_name = "$class.registerPrintField")]
pub extern "C" fn __register_class_print_field(
    class_id: i64,
    idx: i64,
    name_str_ptr: i64,
    pk: i64,
) {
    let name = cstr_to_str(name_str_ptr).to_string();
    let mut t = class_print_info().write().expect("class print info poisoned");
    let entry = t
        .entry(class_id as u32)
        .or_insert_with(|| ClassPrintInfo { name: String::new(), fields: Vec::new() });
    let i = idx as usize;
    while entry.fields.len() <= i {
        entry.fields.push((String::new(), 0));
    }
    entry.fields[i] = (name, pk);
}

/// `class_id` → leaked C-string for the base class name (i.e. the
/// name with any `<TypeArgs>` suffix stripped). The cache holds a +1
/// registry rc per class so callers can release their own +1 without
/// dropping the body.
static CLASS_BASENAME_CACHE: OnceLock<RwLock<HashMap<u32, i64>>> = OnceLock::new();

fn class_basename_cache() -> &'static RwLock<HashMap<u32, i64>> {
    CLASS_BASENAME_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

#[unsafe(export_name = "$class.name")]
pub extern "C" fn __class_name(class_id: i64) -> i64 {
    let key = class_id as u32;
    {
        let cache = class_basename_cache().read().expect("class name cache poisoned");
        if let Some(&ptr) = cache.get(&key) {
            crate::strings::__retain_string(ptr);
            return ptr;
        }
    }
    // Cold path: build the basename once.
    let name = {
        let t = class_print_info().read().expect("class print info poisoned");
        t.get(&key).map(|i| i.name.clone())
    };
    let name = name.unwrap_or_else(|| format!("<obj#{class_id}>"));
    let base = name.split('<').next().unwrap_or(name.as_str()).to_string();
    let new_ptr = leak_cstring(base);
    let installed = {
        let mut cache = class_basename_cache().write().expect("class name cache poisoned");
        *cache.entry(key).or_insert(new_ptr)
    };
    if installed != new_ptr {
        // Lost the race against another thread — drop our pointer.
        crate::strings::__release_string(new_ptr);
    }
    crate::strings::__retain_string(installed);
    installed
}

#[unsafe(export_name = "$print.object")]
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
        let t = class_print_info().read().expect("class print info poisoned");
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

// --------------------------------------------------------------------
// `@extern(C)` struct print info
//
// CRepr / CPacked / CUnion structs have no class_id header and use C
// natural alignment, so the class-print path above can't read them.
// Each field is registered with its byte offset and a PK_* print kind;
// `__print_struct(class_id, ptr)` walks the registry and reads each
// field at its declared offset with the size implied by its PK_*.
// --------------------------------------------------------------------

pub(crate) struct StructPrintField {
    pub(crate) name: String,
    pub(crate) pk: i64,
    pub(crate) offset: i64,
    /// Non-zero when the field is itself a CRepr struct inlined into
    /// the parent's bytes. The formatter recurses with this class id
    /// and the address `parent_ptr + offset` (no pointer load). Zero
    /// for primitive / `string` / heap-pointer fields.
    pub(crate) nested_cid: u32,
}

static STRUCT_PRINT_INFO: OnceLock<RwLock<HashMap<u32, Vec<StructPrintField>>>> = OnceLock::new();

fn struct_print_info() -> &'static RwLock<HashMap<u32, Vec<StructPrintField>>> {
    STRUCT_PRINT_INFO.get_or_init(|| RwLock::new(HashMap::new()))
}

#[unsafe(export_name = "$class.registerStructPrintField")]
pub extern "C" fn __register_struct_print_field(
    class_id: i64,
    idx: i64,
    name_str_ptr: i64,
    pk: i64,
    offset: i64,
    nested_cid: i64,
) {
    let name = cstr_to_str(name_str_ptr).to_string();
    let mut t = struct_print_info().write().expect("struct print info poisoned");
    let entry = t.entry(class_id as u32).or_default();
    let i = idx as usize;
    while entry.len() <= i {
        entry.push(StructPrintField {
            name: String::new(),
            pk: 0,
            offset: 0,
            nested_cid: 0,
        });
    }
    entry[i] = StructPrintField {
        name,
        pk,
        offset,
        nested_cid: nested_cid as u32,
    };
}

#[unsafe(export_name = "$print.struct")]
pub extern "C" fn __print_struct(class_id: i64, ptr: i64) {
    let mut out = std::io::stdout().lock();
    if ptr == 0 {
        let _ = out.write_all(b"<null>");
        return;
    }
    let mut s = String::new();
    format_struct_into(&mut s, class_id, ptr);
    let _ = out.write_all(s.as_bytes());
}

pub fn format_struct_into(out: &mut String, class_id: i64, ptr: i64) {
    use std::fmt::Write;
    if ptr == 0 {
        out.push_str("<null>");
        return;
    }
    let cid = class_id as u32;
    let name = {
        let t = class_print_info().read().expect("class print info poisoned");
        t.get(&cid).map(|i| i.name.clone())
    };
    let fields = {
        let t = struct_print_info().read().expect("struct print info poisoned");
        t.get(&cid).map(|fs| {
            fs.iter()
                .map(|f| (f.name.clone(), f.pk, f.offset, f.nested_cid))
                .collect::<Vec<_>>()
        })
    };
    let name = name.unwrap_or_else(|| format!("struct#{cid}"));
    let base = name.split('<').next().unwrap_or(name.as_str());
    out.push_str(base);
    out.push_str(" {");
    let fields = fields.unwrap_or_default();
    if !fields.is_empty() {
        out.push(' ');
        for (i, (fname, pk, offset, nested_cid)) in fields.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(fname);
            out.push_str(": ");
            if *nested_cid != 0 {
                // Inline struct field — recurse with the address of
                // the inlined bytes rather than treating it as a heap
                // pointer.
                format_struct_into(out, *nested_cid as i64, ptr + *offset);
            } else {
                let raw = read_field_raw(ptr, *offset, *pk);
                crate::print_dispatch::format_kind_id(out, *pk, raw);
            }
        }
        out.push(' ');
    }
    out.push('}');
    let _ = write!(out, "");
}

/// Read a field of a CRepr struct at byte `offset`, sized according
/// to its PK_*. The result is widened to i64 so it can flow into the
/// shared `format_kind_id` dispatcher.
fn read_field_raw(base: i64, offset: i64, pk: i64) -> i64 {
    use crate::kind::*;
    let addr = (base + offset) as *const u8;
    unsafe {
        match pk {
            x if x == PK_I8_SIG => *(addr as *const i8) as i64,
            x if x == PK_I8_UNS || x == PK_BOOL => *(addr as *const u8) as i64,
            x if x == PK_I16_SIG => *(addr as *const i16) as i64,
            x if x == PK_I16_UNS => *(addr as *const u16) as i64,
            x if x == PK_I32_SIG => *(addr as *const i32) as i64,
            x if x == PK_I32_UNS => *(addr as *const u32) as i64,
            x if x == PK_F32 => *(addr as *const i32) as i64,
            // 8-byte slots: i64/u64, f64, pointers (string/object).
            _ => *(addr as *const i64),
        }
    }
}
