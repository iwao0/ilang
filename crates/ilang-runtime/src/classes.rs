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
    // Pin the box with a guard weak ref across the deinit + field
    // cascade. The strong rc is already 0 here, so if the cascade
    // frees a child that holds a `.weak` back-reference to *this*
    // object (parent ↔ child shapes), the child's nested
    // `__release_weak` would see strong == 0, decide it owns the
    // free, and pull the box out from under the in-progress field
    // walk — then the tail of this function would free it again.
    // With the guard the weak count stays ≥ 1 throughout; dropping
    // it below performs the final free (or defers to a genuinely
    // outstanding weak ref, keeping the zombie box for
    // `WeakUpgrade`'s rc peek).
    __retain_weak(obj_ptr);
    let user_drop = __drop_dispatch(class_id);
    if user_drop != 0 {
        let f: extern "C" fn(i64, i64) = unsafe { std::mem::transmute(user_drop) };
        f(obj_ptr, 0);
    }
    __release_object_fields(class_id, obj_ptr);
    __release_weak(obj_ptr);
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

// --------------------------------------------------------------------
// Reflection meta tables
// --------------------------------------------------------------------
//
// `typeof(x).<member>` for `.methods` / `.parent` / `.typeArgs` and
// the per-member lookup methods reads from these side-tables. Each is
// populated once per class at JIT setup / AOT init time, alongside
// the existing print-info registrations.

static CLASS_METHODS_TABLE: OnceLock<RwLock<HashMap<u32, Vec<String>>>> = OnceLock::new();
static CLASS_PARENT_TABLE: OnceLock<RwLock<HashMap<u32, i64>>> = OnceLock::new();
static CLASS_TYPEARGS_TABLE: OnceLock<RwLock<HashMap<u32, Vec<i64>>>> = OnceLock::new();
static CLASS_FIELD_TYPE_TABLE: OnceLock<RwLock<HashMap<u32, HashMap<String, i64>>>> = OnceLock::new();
static CLASS_METHOD_RETURN_TABLE: OnceLock<RwLock<HashMap<u32, HashMap<String, i64>>>> = OnceLock::new();
static CLASS_METHOD_PARAMS_TABLE: OnceLock<RwLock<HashMap<u32, HashMap<String, Vec<i64>>>>> = OnceLock::new();

fn class_methods_table() -> &'static RwLock<HashMap<u32, Vec<String>>> {
    CLASS_METHODS_TABLE.get_or_init(|| RwLock::new(HashMap::new()))
}
fn class_parent_table() -> &'static RwLock<HashMap<u32, i64>> {
    CLASS_PARENT_TABLE.get_or_init(|| RwLock::new(HashMap::new()))
}
fn class_typeargs_table() -> &'static RwLock<HashMap<u32, Vec<i64>>> {
    CLASS_TYPEARGS_TABLE.get_or_init(|| RwLock::new(HashMap::new()))
}
fn class_field_type_table() -> &'static RwLock<HashMap<u32, HashMap<String, i64>>> {
    CLASS_FIELD_TYPE_TABLE.get_or_init(|| RwLock::new(HashMap::new()))
}
fn class_method_return_table() -> &'static RwLock<HashMap<u32, HashMap<String, i64>>> {
    CLASS_METHOD_RETURN_TABLE.get_or_init(|| RwLock::new(HashMap::new()))
}
fn class_method_params_table() -> &'static RwLock<HashMap<u32, HashMap<String, Vec<i64>>>> {
    CLASS_METHOD_PARAMS_TABLE.get_or_init(|| RwLock::new(HashMap::new()))
}

static CLASS_DECLARED_FIELD_COUNT: OnceLock<RwLock<HashMap<u32, i64>>> = OnceLock::new();

fn class_declared_field_count() -> &'static RwLock<HashMap<u32, i64>> {
    CLASS_DECLARED_FIELD_COUNT.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Number of fields declared on this class itself (i.e. excluding
/// fields inherited from the parent chain). Set once per class at
/// registration time so `__type_fields` reports only the syntactic
/// declared set.
#[unsafe(export_name = "$type.registerDeclaredFieldCount")]
pub extern "C" fn __register_type_declared_field_count(class_id: i64, count: i64) {
    let mut t = class_declared_field_count().write().expect("declared count table poisoned");
    t.insert(class_id as u32, count);
}

#[unsafe(export_name = "$type.registerMethod")]
pub extern "C" fn __register_type_method(class_id: i64, _idx: i64, name_ptr: i64) {
    let name = cstr_to_str(name_ptr).to_string();
    {
        let mut t = class_methods_table().write().expect("methods table poisoned");
        t.entry(class_id as u32).or_default().push(name.clone());
    }
    // Seed an empty params entry so `methodParams("zero_arg")` can
    // tell "method exists, zero args" from "method not declared". The
    // entry stays empty until `__register_type_method_param` pushes.
    let mut t = class_method_params_table().write().expect("method-params table poisoned");
    t.entry(class_id as u32)
        .or_default()
        .entry(name)
        .or_default();
}

#[unsafe(export_name = "$type.registerParent")]
pub extern "C" fn __register_type_parent(class_id: i64, parent_id: i64) {
    let mut t = class_parent_table().write().expect("parent table poisoned");
    t.insert(class_id as u32, parent_id);
}

#[unsafe(export_name = "$type.registerTypeArg")]
pub extern "C" fn __register_type_arg(class_id: i64, _idx: i64, arg_id: i64) {
    let mut t = class_typeargs_table().write().expect("typeargs table poisoned");
    t.entry(class_id as u32).or_default().push(arg_id);
}

#[unsafe(export_name = "$type.registerFieldType")]
pub extern "C" fn __register_type_field_type(class_id: i64, name_ptr: i64, type_id: i64) {
    let name = cstr_to_str(name_ptr).to_string();
    let mut t = class_field_type_table().write().expect("field-type table poisoned");
    t.entry(class_id as u32).or_default().insert(name, type_id);
}

#[unsafe(export_name = "$type.registerMethodReturn")]
pub extern "C" fn __register_type_method_return(class_id: i64, name_ptr: i64, ret_id: i64) {
    let name = cstr_to_str(name_ptr).to_string();
    let mut t = class_method_return_table().write().expect("method-return table poisoned");
    t.entry(class_id as u32).or_default().insert(name, ret_id);
}

#[unsafe(export_name = "$type.registerMethodParam")]
pub extern "C" fn __register_type_method_param(
    class_id: i64,
    name_ptr: i64,
    _idx: i64,
    param_id: i64,
) {
    let name = cstr_to_str(name_ptr).to_string();
    let mut t = class_method_params_table().write().expect("method-params table poisoned");
    t.entry(class_id as u32)
        .or_default()
        .entry(name)
        .or_default()
        .push(param_id);
}

#[unsafe(export_name = "$type.methods")]
pub extern "C" fn __type_methods(class_id: i64) -> i64 {
    let names: Vec<String> = {
        let t = class_methods_table().read().expect("methods table poisoned");
        t.get(&(class_id as u32)).cloned().unwrap_or_default()
    };
    new_string_array(&names)
}

/// `typeof(x).parent` — `Type?` (none for root classes / non-class
/// types). Layout matches a primitive Optional cell:
///   [ value | rc | kind_tag=Object ].
#[unsafe(export_name = "$type.parent")]
pub extern "C" fn __type_parent(class_id: i64) -> i64 {
    use crate::alloc::__mir_alloc;
    let parent: Option<i64> = {
        let t = class_parent_table().read().expect("parent table poisoned");
        t.get(&(class_id as u32)).copied().filter(|p| *p != 0)
    };
    match parent {
        None => 0,
        Some(p) => {
            // Allocate a 3-cell Optional. kind_tag follows the same
            // PrintKind::Object cascade tag (1) used by WeakUpgrade.
            let cell = __mir_alloc(24);
            unsafe {
                let h = cell as *mut i64;
                *h = p;       // value
                *h.add(1) = 1; // rc
                *h.add(2) = 1; // kind_tag (Object cascade)
            }
            cell
        }
    }
}

/// `typeof(x).typeArgs` — `Type[]` of generic-instance arguments.
/// Empty for non-generic types.
#[unsafe(export_name = "$type.typeArgs")]
pub extern "C" fn __type_type_args(class_id: i64) -> i64 {
    use crate::alloc::__mir_alloc;
    let ids: Vec<i64> = {
        let t = class_typeargs_table().read().expect("typeargs table poisoned");
        t.get(&(class_id as u32)).cloned().unwrap_or_default()
    };
    // `Type` values are i64 in the ABI; build an i64[] (kind_tag = 0).
    let n = ids.len() as i64;
    let header = __mir_alloc(48);
    let data = __mir_alloc((n * 8).max(8));
    unsafe {
        for (i, id) in ids.iter().enumerate() {
            *((data + (i as i64) * 8) as *mut i64) = *id;
        }
        let h = header as *mut i64;
        *h = n;
        *h.add(1) = n;
        *h.add(2) = data;
        *h.add(3) = 1;
        *h.add(4) = 0; // PK_I64_SIG — Type handles are scalar i64
        *h.add(5) = 8;
    }
    header
}

/// `typeof(x).fieldType(name)` — `Type?`. Caller passes the field
/// name pointer; returns an Optional<Type> heap cell or 0 (none).
#[unsafe(export_name = "$type.fieldType")]
pub extern "C" fn __type_field_type(class_id: i64, name_ptr: i64) -> i64 {
    use crate::alloc::__mir_alloc;
    let name = cstr_to_str(name_ptr);
    let ty: Option<i64> = {
        let t = class_field_type_table().read().expect("field-type table poisoned");
        t.get(&(class_id as u32)).and_then(|m| m.get(name).copied())
    };
    match ty {
        None => 0,
        Some(p) => {
            let cell = __mir_alloc(24);
            unsafe {
                let h = cell as *mut i64;
                *h = p;
                *h.add(1) = 1;
                *h.add(2) = 0; // i64 scalar, no cascade
            }
            cell
        }
    }
}

/// `typeof(x).methodReturn(name)` — `Type?`.
#[unsafe(export_name = "$type.methodReturn")]
pub extern "C" fn __type_method_return(class_id: i64, name_ptr: i64) -> i64 {
    use crate::alloc::__mir_alloc;
    let name = cstr_to_str(name_ptr);
    let ty: Option<i64> = {
        let t = class_method_return_table().read().expect("method-return table poisoned");
        t.get(&(class_id as u32)).and_then(|m| m.get(name).copied())
    };
    match ty {
        None => 0,
        Some(p) => {
            let cell = __mir_alloc(24);
            unsafe {
                let h = cell as *mut i64;
                *h = p;
                *h.add(1) = 1;
                *h.add(2) = 0;
            }
            cell
        }
    }
}

/// `typeof(x).methodParams(name)` — `Type[]?`. Returns an
/// Optional<Type[]> heap cell or 0.
#[unsafe(export_name = "$type.methodParams")]
pub extern "C" fn __type_method_params(class_id: i64, name_ptr: i64) -> i64 {
    use crate::alloc::__mir_alloc;
    let name = cstr_to_str(name_ptr);
    let params: Option<Vec<i64>> = {
        let t = class_method_params_table().read().expect("method-params table poisoned");
        t.get(&(class_id as u32)).and_then(|m| m.get(name).cloned())
    };
    let Some(ids) = params else {
        return 0;
    };
    // Build the inner Type[] (i64[]).
    let n = ids.len() as i64;
    let arr_header = __mir_alloc(48);
    let arr_data = __mir_alloc((n * 8).max(8));
    unsafe {
        for (i, id) in ids.iter().enumerate() {
            *((arr_data + (i as i64) * 8) as *mut i64) = *id;
        }
        let h = arr_header as *mut i64;
        *h = n;
        *h.add(1) = n;
        *h.add(2) = arr_data;
        *h.add(3) = 1;
        *h.add(4) = 0;
        *h.add(5) = 8;
    }
    // Wrap in an Optional cell. kind_tag uses the array cascade tag
    // (PrintKind::Array → 2 in the existing scheme) so a release of
    // the Optional triggers `__release_array` on the payload.
    let cell = __mir_alloc(24);
    unsafe {
        let h = cell as *mut i64;
        *h = arr_header;
        *h.add(1) = 1;
        *h.add(2) = 2; // Array cascade
    }
    cell
}

/// Global enum id for the built-in `TypeKind` enum, set once per
/// process by the codegen during init. `__type_kind` reads it to
/// route through the shared `__enum_unit_get` cache so the result
/// is a real MIR enum cell (and `match` works the same way as on
/// any user-declared enum).
static TYPEKIND_ENUM_GLOBAL: OnceLock<AtomicI64> = OnceLock::new();

fn typekind_enum_global() -> &'static AtomicI64 {
    TYPEKIND_ENUM_GLOBAL.get_or_init(|| AtomicI64::new(-1))
}

#[unsafe(export_name = "$type.registerTypeKindEnumId")]
pub extern "C" fn __register_typekind_enum_id(global_eid: i64) {
    typekind_enum_global().store(global_eid, Ordering::SeqCst);
}

/// Discriminant of the `TypeKind` enum corresponding to `class_id`,
/// boxed into an enum-cell pointer the same way `Inst::NewEnum`
/// would for a user enum.
///
/// Must agree with the variant order in
/// `crates/ilang-mir/src/lower/lower_state.rs::inject_typekind_enum`
/// and the type-checker's enum sig in `builtins.rs`:
///   0 = primitive, 1 = class, 2 = enum, 3 = optional, 4 = array,
///   5 = fn, 6 = tuple, 7 = string, 8 = unit.
#[unsafe(export_name = "$type.kind")]
pub extern "C" fn __type_kind(class_id: i64) -> i64 {
    let disc: i64 = match class_id {
        TYPE_ID_STRING => 7,
        TYPE_ID_BOOL
        | TYPE_ID_I64
        | TYPE_ID_U64
        | TYPE_ID_I32
        | TYPE_ID_U32
        | TYPE_ID_I16
        | TYPE_ID_U16
        | TYPE_ID_I8
        | TYPE_ID_U8
        | TYPE_ID_F64
        | TYPE_ID_F32 => 0,
        TYPE_ID_UNIT => 8,
        TYPE_ID_ARRAY => 4,
        TYPE_ID_TUPLE => 6,
        TYPE_ID_FN => 5,
        TYPE_ID_OPTIONAL => 3,
        TYPE_ID_ENUM => 2,
        TYPE_ID_WEAK | TYPE_ID_MAP | TYPE_ID_SET | TYPE_ID_PROMISE => 1,
        _ => 1, // every real class id reports as `class`
    };
    let global = typekind_enum_global().load(Ordering::SeqCst);
    if global < 0 {
        // Init hasn't run — return the bare discriminant so the
        // caller at least gets a non-null value. Won't match through
        // EnumTag (which deref-loads), but keeps tests deterministic.
        return disc;
    }
    crate::enums::__enum_unit_get(global, disc)
}

/// `typeof(x).fields` — `string[]` of declared field names on `class_id`.
/// Inherited fields are NOT included (callers chase `.parent` for those).
/// The names are pulled out of `class_print_info` (populated by
/// `$class.registerPrintField`); we leak a fresh heap string per slot
/// because the consumer owns a +1 rc on each element.
///
/// Layout mirrors `__c_array_to_array`: 48-byte header + n × 8-byte
/// data buffer, `kind_tag = 11` (PK_STR mirror from mir-codegen) so
/// the release cascade drops each string at array-rc 0.
#[unsafe(export_name = "$type.fields")]
pub extern "C" fn __type_fields(class_id: i64) -> i64 {
    let declared: usize = {
        let t = class_declared_field_count().read().expect("declared count table poisoned");
        t.get(&(class_id as u32))
            .copied()
            .map(|n| n.max(0) as usize)
            .unwrap_or(usize::MAX)
    };
    let names: Vec<String> = {
        let t = class_print_info().read().expect("class print info poisoned");
        match t.get(&(class_id as u32)) {
            None => Vec::new(),
            Some(info) => {
                // Last `declared` entries are the names declared on
                // this class (the rest came from the inheritance
                // prefix). When the registry hasn't recorded a count
                // — e.g. struct CRepr classes — fall through to "all".
                let total = info.fields.len();
                let take = declared.min(total);
                let skip = total - take;
                info.fields
                    .iter()
                    .skip(skip)
                    .map(|(n, _)| n.clone())
                    .collect()
            }
        }
    };
    new_string_array(&names)
}

/// Build an ilang `string[]` from a slice of owned strings. Used by
/// every reflection getter that returns a name list (`.fields`,
/// `.methods`, ...).
pub(crate) fn new_string_array(names: &[String]) -> i64 {
    use crate::alloc::__mir_alloc;
    const PK_STR: i64 = 11;
    let n = names.len() as i64;
    let header = __mir_alloc(48);
    let data = __mir_alloc((n * 8).max(8));
    unsafe {
        for (i, name) in names.iter().enumerate() {
            let ptr = leak_cstring(name.clone());
            *((data + (i as i64) * 8) as *mut i64) = ptr;
        }
        let h = header as *mut i64;
        *h = n;            // len
        *h.add(1) = n;     // cap
        *h.add(2) = data;  // data ptr
        *h.add(3) = 1;     // rc
        *h.add(4) = PK_STR; // kind_tag
        *h.add(5) = 8;     // stride
    }
    header
}

/// Virtual class ids for primitive / structural type names surfaced
/// through `typeof(x).name`. These ids never collide with real
/// `ClassId`s (which are non-negative); reflection lowering picks the
/// matching id for each non-class `MirTy` so a single `__class_name`
/// path can serve both classes and primitives.
pub const TYPE_ID_STRING: i64 = -1;
pub const TYPE_ID_BOOL: i64 = -2;
pub const TYPE_ID_I64: i64 = -3;
pub const TYPE_ID_U64: i64 = -4;
pub const TYPE_ID_I32: i64 = -5;
pub const TYPE_ID_U32: i64 = -6;
pub const TYPE_ID_I16: i64 = -7;
pub const TYPE_ID_U16: i64 = -8;
pub const TYPE_ID_I8: i64 = -9;
pub const TYPE_ID_U8: i64 = -10;
pub const TYPE_ID_F64: i64 = -11;
pub const TYPE_ID_F32: i64 = -12;
pub const TYPE_ID_UNIT: i64 = -13;
pub const TYPE_ID_ARRAY: i64 = -14;
pub const TYPE_ID_TUPLE: i64 = -15;
pub const TYPE_ID_FN: i64 = -16;
pub const TYPE_ID_OPTIONAL: i64 = -17;
pub const TYPE_ID_MAP: i64 = -18;
pub const TYPE_ID_SET: i64 = -19;
pub const TYPE_ID_PROMISE: i64 = -20;
pub const TYPE_ID_ENUM: i64 = -21;
pub const TYPE_ID_WEAK: i64 = -22;

fn primitive_type_name(class_id: i64) -> Option<&'static str> {
    Some(match class_id {
        TYPE_ID_STRING => "string",
        TYPE_ID_BOOL => "bool",
        TYPE_ID_I64 => "i64",
        TYPE_ID_U64 => "u64",
        TYPE_ID_I32 => "i32",
        TYPE_ID_U32 => "u32",
        TYPE_ID_I16 => "i16",
        TYPE_ID_U16 => "u16",
        TYPE_ID_I8 => "i8",
        TYPE_ID_U8 => "u8",
        TYPE_ID_F64 => "f64",
        TYPE_ID_F32 => "f32",
        TYPE_ID_UNIT => "()",
        TYPE_ID_ARRAY => "array",
        TYPE_ID_TUPLE => "tuple",
        TYPE_ID_FN => "fn",
        TYPE_ID_OPTIONAL => "optional",
        TYPE_ID_MAP => "Map",
        TYPE_ID_SET => "Set",
        TYPE_ID_PROMISE => "Promise",
        TYPE_ID_ENUM => "enum",
        TYPE_ID_WEAK => "weak",
        _ => return None,
    })
}

#[unsafe(export_name = "$class.name")]
pub extern "C" fn __class_name(class_id: i64) -> i64 {
    // Primitive virtual ids — return a static leaked string.
    if let Some(prim) = primitive_type_name(class_id) {
        let ptr = leak_cstring(prim.to_string());
        return ptr;
    }
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
