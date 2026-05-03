//! Runtime FFI helpers linked into the JIT module.
//!
//! Every `extern "C"` here is registered with `JITBuilder::symbol` and
//! `Module::declare_function` so JITed code can call it. Layouts for
//! heap values (`StringRc`, `ArrayHeader`) live here too — the host
//! side (`read_array`, `run_main`) needs to walk them.

// ─── ARC for objects (Phase A/D/E) ────────────────────────────────────
// Each `new` allocation lays out memory as:
//   [ strong_rc | weak_rc | drop_fn_ptr | field0 | field1 | ... ]
// (each header slot is i64). The pointer surfaced to JITed code points
// at field0; the three header slots sit at offsets -24, -16, -8.
// Field offsets stay the same as before, so user-pointer arithmetic in
// generated code is unchanged.
//
// Two-rc lifecycle (Phase E):
//  - strong_rc reaches 0 → run `drop_fn` (user deinit + heap field
//    release). Storage is freed only once weak_rc is also 0; until then
//    weak refs can detect "dead" by reading strong_rc==0 and `get()`
//    returns none.
//  - weak_rc reaches 0 with strong_rc==0 → free the storage.
//
// `drop_fn` is a JIT-generated wrapper (see drops.rs). Trivial classes
// (no deinit, no heap fields) use 0 to skip the call.

const STRONG_OFFSET: i64 = -24;
const WEAK_OFFSET: i64 = -16;
const DROP_OFFSET: i64 = -8;
const HEADER_SIZE: usize = 24;

pub(crate) extern "C" fn ilang_jit_alloc_object(user_size: i64, drop_fn_ptr: i64) -> i64 {
    let total = HEADER_SIZE + (user_size as usize);
    let layout = std::alloc::Layout::from_size_align(total.max(1), 8).unwrap();
    unsafe {
        let raw = std::alloc::alloc_zeroed(layout);
        *(raw as *mut i64) = 1; // strong
        *(raw.add(8) as *mut i64) = 0; // weak
        *(raw.add(16) as *mut i64) = drop_fn_ptr;
        raw.add(HEADER_SIZE) as i64
    }
}

pub(crate) extern "C" fn ilang_jit_retain_object(ptr: i64) {
    if ptr == 0 {
        return;
    }
    unsafe {
        let rc = (ptr + STRONG_OFFSET) as *mut i64;
        *rc += 1;
    }
}

/// Decrement the strong refcount. On zero, run the drop wrapper
/// (deinit + heap field release). Free the storage only if weak_rc is
/// also 0; otherwise leave the allocation around so weak refs see
/// strong_rc==0 and report "dead" through `weak_get`.
///
/// `deinit`'s own exit-release of `this` is suppressed by the JIT
/// (see `define_function_body`), so the body observes rc=0 without
/// re-entering this routine.
pub(crate) extern "C" fn ilang_jit_release_object(ptr: i64, user_size: i64) {
    if ptr == 0 {
        return;
    }
    unsafe {
        let strong_ptr = (ptr + STRONG_OFFSET) as *mut i64;
        *strong_ptr -= 1;
        if *strong_ptr != 0 {
            return;
        }
        // Sentinel: bump weak_rc by 1 across the drop_fn call so a
        // weak field that points back at *us* (e.g. a Parent owning a
        // Child whose `p: Parent.weak` is the back-edge) can run its
        // release_weak without prematurely deallocating our own
        // storage from inside the drop wrapper.
        let weak_ptr = (ptr + WEAK_OFFSET) as *mut i64;
        *weak_ptr += 1;
        let drop_ptr = *((ptr + DROP_OFFSET) as *const i64);
        if drop_ptr != 0 {
            let f: extern "C" fn(i64) = std::mem::transmute(drop_ptr);
            f(ptr);
        }
        *weak_ptr -= 1;
        if *weak_ptr == 0 {
            let total = HEADER_SIZE + (user_size as usize);
            let layout = std::alloc::Layout::from_size_align(total.max(1), 8).unwrap();
            std::alloc::dealloc((ptr - HEADER_SIZE as i64) as *mut u8, layout);
        }
        // else: keep storage alive for surviving weak references;
        // freed when ilang_jit_release_weak drops the last weak.
    }
}

/// Increment a weak ref's count. Used when a weak binding is created
/// (downgrade from strong, or re-bind from another weak).
pub(crate) extern "C" fn ilang_jit_retain_weak(ptr: i64) {
    if ptr == 0 {
        return;
    }
    unsafe {
        let weak_ptr = (ptr + WEAK_OFFSET) as *mut i64;
        *weak_ptr += 1;
    }
}

/// Decrement a weak ref's count. Frees the storage only if both
/// strong and weak hit zero (i.e. the object's contents have already
/// been dropped and no other weak refs survive).
pub(crate) extern "C" fn ilang_jit_release_weak(ptr: i64, user_size: i64) {
    if ptr == 0 {
        return;
    }
    unsafe {
        let weak_ptr = (ptr + WEAK_OFFSET) as *mut i64;
        *weak_ptr -= 1;
        if *weak_ptr != 0 {
            return;
        }
        let strong_count = *((ptr + STRONG_OFFSET) as *const i64);
        if strong_count != 0 {
            return;
        }
        let total = HEADER_SIZE + (user_size as usize);
        let layout = std::alloc::Layout::from_size_align(total.max(1), 8).unwrap();
        std::alloc::dealloc((ptr - HEADER_SIZE as i64) as *mut u8, layout);
    }
}

/// Try to upgrade a weak reference to a strong one. If the target is
/// still alive (strong_rc > 0), bumps strong_rc and returns the same
/// pointer; the caller now owns +1 strong reference, equivalent to a
/// fresh allocation. If dead, returns 0 (which the JIT treats as the
/// `none` value of an Optional).
pub(crate) extern "C" fn ilang_jit_weak_get(ptr: i64) -> i64 {
    if ptr == 0 {
        return 0;
    }
    unsafe {
        let strong_ptr = (ptr + STRONG_OFFSET) as *mut i64;
        if *strong_ptr == 0 {
            return 0;
        }
        *strong_ptr += 1;
        ptr
    }
}

// ─── console.log per-type print helpers ────────────────────────────────
// `console.log(a, b, c)` lowers to:
//   ilang_jit_print_<type>(a)
//   ilang_jit_print_space()
//   ilang_jit_print_<type>(b)
//   ilang_jit_print_space()
//   ilang_jit_print_<type>(c)
//   ilang_jit_print_newline()

pub(crate) extern "C" fn ilang_jit_print_i64(n: i64) {
    print!("{n}");
}
pub(crate) extern "C" fn ilang_jit_print_u64(n: u64) {
    print!("{n}");
}
pub(crate) extern "C" fn ilang_jit_print_f64(x: f64) {
    if x.is_finite() && x.fract() == 0.0 {
        print!("{x:.1}");
    } else {
        print!("{x}");
    }
}
pub(crate) extern "C" fn ilang_jit_print_f32(x: f32) {
    ilang_jit_print_f64(x as f64);
}
pub(crate) extern "C" fn ilang_jit_print_bool(b: i8) {
    print!("{}", b != 0);
}
pub(crate) extern "C" fn ilang_jit_print_space() {
    print!(" ");
}
pub(crate) extern "C" fn ilang_jit_print_newline() {
    println!();
}


// ─── String runtime (ARC Phase B) ──────────────────────────────────────
// Strings are heap-allocated `Box<StringRc>`; the JIT carries the raw
// pointer (i64). String literals are interned with a *saturated* rc so
// `release` decrements never reach 0 — the literal storage is owned by
// the compiler's interning bucket and freed when the compiler drops.

#[repr(C)]
pub(crate) struct StringRc {
    pub rc: i64,
    pub s: String,
}

/// Used for interned literals. release_string skips when rc >= this so
/// literal storage is never freed by the runtime.
pub(crate) const STRING_RC_SATURATED: i64 = i64::MAX / 2;

pub(crate) extern "C" fn ilang_jit_retain_string(ptr: i64) {
    if ptr == 0 {
        return;
    }
    unsafe {
        let rc = ptr as *mut i64;
        if *rc >= STRING_RC_SATURATED {
            return;
        }
        *rc += 1;
    }
}

pub(crate) extern "C" fn ilang_jit_release_string(ptr: i64) {
    if ptr == 0 {
        return;
    }
    unsafe {
        let rc = ptr as *mut i64;
        if *rc >= STRING_RC_SATURATED {
            return;
        }
        *rc -= 1;
        if *rc != 0 {
            return;
        }
        drop(Box::from_raw(ptr as *mut StringRc));
    }
}

pub(crate) extern "C" fn ilang_jit_print_str(ptr: i64) {
    let sr = unsafe { &*(ptr as *const StringRc) };
    print!("{}", sr.s);
}

pub(crate) extern "C" fn ilang_jit_str_concat(a: i64, b: i64) -> i64 {
    let a = unsafe { &*(a as *const StringRc) };
    let b = unsafe { &*(b as *const StringRc) };
    let boxed = Box::new(StringRc {
        rc: 1,
        s: format!("{}{}", a.s, b.s),
    });
    Box::into_raw(boxed) as i64
}

pub(crate) extern "C" fn ilang_jit_str_eq(a: i64, b: i64) -> i8 {
    let a = unsafe { &*(a as *const StringRc) };
    let b = unsafe { &*(b as *const StringRc) };
    if a.s == b.s {
        1
    } else {
        0
    }
}

fn alloc_str(s: String) -> i64 {
    Box::into_raw(Box::new(StringRc { rc: 1, s })) as i64
}

// ─── Native extern string marshalling ──────────────────────────────────
// `@extern("libfoo") fn f(s: string)` needs to hand C the
// `*const c_char` it expects, not our StringRc pointer. The JIT
// inserts calls to these helpers around each native-extern call:
//   ilang_jit_str_to_c_str  →  malloc'd null-terminated copy
//   ilang_jit_free_c_str    →  free that copy after the call returns
//   ilang_jit_c_str_to_string → copy a C-returned pointer into a fresh
//                               StringRc (assumes the C side owns the
//                               memory and won't free it under us)

/// Allocate a null-terminated copy of `s` and return its pointer as i64.
/// Returns 0 if `ptr` is 0 (caller checks).
pub(crate) extern "C" fn ilang_jit_str_to_c_str(ptr: i64) -> i64 {
    if ptr == 0 {
        return 0;
    }
    let sr = unsafe { &*(ptr as *const StringRc) };
    // CString rejects interior NULs — fall back to truncating at the
    // first NUL so we never panic on user strings. C land treats them
    // as terminators anyway.
    let bytes = sr.s.as_bytes();
    let len_to_nul = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let mut buf = Vec::with_capacity(len_to_nul + 1);
    buf.extend_from_slice(&bytes[..len_to_nul]);
    buf.push(0);
    let boxed = buf.into_boxed_slice();
    Box::into_raw(boxed) as *mut u8 as i64
}

/// Free a buffer previously returned by `ilang_jit_str_to_c_str`.
/// The buffer was allocated as `Box<[u8]>` with the trailing NUL
/// included in its length — `len_to_nul` here recomputes that length
/// by walking to the NUL.
pub(crate) extern "C" fn ilang_jit_free_c_str(ptr: i64) {
    if ptr == 0 {
        return;
    }
    unsafe {
        let p = ptr as *mut u8;
        let mut n = 0usize;
        while *p.add(n) != 0 {
            n += 1;
        }
        // +1 for the NUL byte (matches what str_to_c_str pushed).
        let slice = std::slice::from_raw_parts_mut(p, n + 1);
        drop(Box::from_raw(slice as *mut [u8]));
    }
}

/// `libc::free` wrapper for `@extern(..., owned_return)`. Called
/// after `c_str_to_string` has copied the bytes out of the C-owned
/// buffer. NULL is a no-op (matches libc's free semantics).
pub(crate) extern "C" fn ilang_jit_libc_free(ptr: i64) {
    if ptr == 0 {
        return;
    }
    extern "C" {
        fn free(ptr: *mut std::ffi::c_void);
    }
    unsafe {
        free(ptr as *mut std::ffi::c_void);
    }
}

/// Copy a C-owned `*const c_char` (null-terminated UTF-8) into a fresh
/// StringRc. The C-side memory is **not** freed by this helper alone —
/// the JIT separately calls `ilang_jit_libc_free` when the fn was
/// declared `@extern(..., owned_return)`.
pub(crate) extern "C" fn ilang_jit_c_str_to_string(ptr: i64) -> i64 {
    if ptr == 0 {
        // Empty string fallback — null pointer in StringRc world is
        // undefined behaviour for downstream string ops.
        return alloc_str(String::new());
    }
    unsafe {
        let cstr = std::ffi::CStr::from_ptr(ptr as *const i8);
        // Lossy so any invalid UTF-8 becomes U+FFFD instead of panicking.
        let s = cstr.to_string_lossy().into_owned();
        alloc_str(s)
    }
}

/// Unicode code-point count, matching JS-style `.length` semantics for
/// non-BMP characters (each surrogate pair counts as one in `chars()`).
pub(crate) extern "C" fn ilang_jit_str_length(ptr: i64) -> i64 {
    let s = unsafe { &*(ptr as *const StringRc) };
    s.s.chars().count() as i64
}

/// JS-style: out-of-range index returns an empty string.
pub(crate) extern "C" fn ilang_jit_str_char_at(ptr: i64, idx: i64) -> i64 {
    let s = unsafe { &*(ptr as *const StringRc) };
    if idx < 0 {
        return alloc_str(String::new());
    }
    let out: String = s
        .s
        .chars()
        .nth(idx as usize)
        .map(|c| c.to_string())
        .unwrap_or_default();
    alloc_str(out)
}

pub(crate) extern "C" fn ilang_jit_str_includes(haystack: i64, needle: i64) -> i8 {
    let h = unsafe { &*(haystack as *const StringRc) };
    let n = unsafe { &*(needle as *const StringRc) };
    if h.s.contains(&n.s) {
        1
    } else {
        0
    }
}

pub(crate) extern "C" fn ilang_jit_str_starts_with(s: i64, prefix: i64) -> i8 {
    let s = unsafe { &*(s as *const StringRc) };
    let p = unsafe { &*(prefix as *const StringRc) };
    if s.s.starts_with(&p.s) {
        1
    } else {
        0
    }
}

pub(crate) extern "C" fn ilang_jit_str_ends_with(s: i64, suffix: i64) -> i8 {
    let s = unsafe { &*(s as *const StringRc) };
    let f = unsafe { &*(suffix as *const StringRc) };
    if s.s.ends_with(&f.s) {
        1
    } else {
        0
    }
}

pub(crate) extern "C" fn ilang_jit_str_to_upper(ptr: i64) -> i64 {
    let s = unsafe { &*(ptr as *const StringRc) };
    alloc_str(s.s.to_uppercase())
}

pub(crate) extern "C" fn ilang_jit_str_to_lower(ptr: i64) -> i64 {
    let s = unsafe { &*(ptr as *const StringRc) };
    alloc_str(s.s.to_lowercase())
}

pub(crate) extern "C" fn ilang_jit_str_trim(ptr: i64) -> i64 {
    let s = unsafe { &*(ptr as *const StringRc) };
    alloc_str(s.s.trim().to_string())
}

// ─── Array runtime (ARC Phase B + D) ──────────────────────────────────
// Layout:
//   header (40 bytes): [rc, drop_fn, len, cap, data_ptr] — all i64
//   data buffer: separately heap-allocated `cap * elem_size` bytes
// The two-level layout means `push` can reallocate the data buffer
// without invalidating any aliased reference to the header.
//
// `drop_fn` is a JIT-generated per-array-kind wrapper (see drops.rs)
// that loops over elements and recursively releases each. For arrays
// of non-heap elements it's 0, and the runtime just frees the
// allocations.

pub(crate) const ARRAY_LEN_OFFSET: i32 = 16;
pub(crate) const ARRAY_DATA_OFFSET: i32 = 32;

#[repr(C)]
pub(crate) struct ArrayHeader {
    pub rc: i64,
    pub drop_fn: i64,
    pub len: i64,
    pub cap: i64,
    pub data_ptr: i64,
}

pub(crate) extern "C" fn ilang_jit_array_new(elem_size: i64, len: i64, drop_fn_ptr: i64) -> i64 {
    let cap = len.max(4);
    let data = if cap == 0 || elem_size == 0 {
        0
    } else {
        let layout = std::alloc::Layout::from_size_align(
            (cap as usize) * (elem_size as usize),
            8,
        )
        .unwrap();
        unsafe { std::alloc::alloc_zeroed(layout) as i64 }
    };
    let header_layout = std::alloc::Layout::new::<ArrayHeader>();
    let header = unsafe { std::alloc::alloc_zeroed(header_layout) as *mut ArrayHeader };
    unsafe {
        (*header).rc = 1;
        (*header).drop_fn = drop_fn_ptr;
        (*header).len = len;
        (*header).cap = cap;
        (*header).data_ptr = data;
    }
    header as i64
}

pub(crate) extern "C" fn ilang_jit_retain_array(ptr: i64) {
    if ptr == 0 {
        return;
    }
    unsafe {
        let rc = ptr as *mut i64;
        *rc += 1;
    }
}

/// elem_size lets us compute the data-buffer Layout for `dealloc`. The
/// per-kind `drop_fn` (if any) handles element-level recursive release
/// before we free the storage.
pub(crate) extern "C" fn ilang_jit_release_array(ptr: i64, elem_size: i64) {
    if ptr == 0 {
        return;
    }
    unsafe {
        let header = ptr as *mut ArrayHeader;
        (*header).rc -= 1;
        if (*header).rc != 0 {
            return;
        }
        let drop_ptr = (*header).drop_fn;
        if drop_ptr != 0 {
            let f: extern "C" fn(i64) = std::mem::transmute(drop_ptr);
            f(ptr);
        }
        let cap = (*header).cap;
        let data = (*header).data_ptr;
        if data != 0 && cap != 0 && elem_size != 0 {
            let layout = std::alloc::Layout::from_size_align(
                (cap as usize) * (elem_size as usize),
                8,
            )
            .unwrap();
            std::alloc::dealloc(data as *mut u8, layout);
        }
        let header_layout = std::alloc::Layout::new::<ArrayHeader>();
        std::alloc::dealloc(ptr as *mut u8, header_layout);
    }
}

/// Internal helper: ensure the data buffer has room for one more
/// element, reallocating if needed. The previous buffer (if any) is
/// freed since nothing else references it (single header owns it).
unsafe fn array_grow_if_full(header: *mut ArrayHeader, elem_size: i64) {
    let len = (*header).len;
    let cap = (*header).cap;
    if len < cap {
        return;
    }
    let new_cap = (cap * 2).max(4);
    let old_size = (cap as usize) * (elem_size as usize);
    let new_size = (new_cap as usize) * (elem_size as usize);
    let layout = std::alloc::Layout::from_size_align(new_size.max(1), 8).unwrap();
    let new_data = std::alloc::alloc_zeroed(layout);
    let old_data = (*header).data_ptr;
    if old_data != 0 && old_size != 0 {
        std::ptr::copy_nonoverlapping(old_data as *const u8, new_data, old_size);
        let old_layout =
            std::alloc::Layout::from_size_align(old_size, 8).unwrap();
        std::alloc::dealloc(old_data as *mut u8, old_layout);
    }
    (*header).cap = new_cap;
    (*header).data_ptr = new_data as i64;
}

macro_rules! push_fn {
    ($name:ident, $ty:ty, $size:expr) => {
        pub(crate) extern "C" fn $name(header: i64, val: $ty) {
            unsafe {
                let header = header as *mut ArrayHeader;
                array_grow_if_full(header, $size);
                let dst =
                    ((*header).data_ptr + (*header).len * $size) as *mut $ty;
                *dst = val;
                (*header).len += 1;
            }
        }
    };
}
push_fn!(ilang_jit_array_push_i8, i8, 1);
push_fn!(ilang_jit_array_push_i16, i16, 2);
push_fn!(ilang_jit_array_push_i32, i32, 4);
push_fn!(ilang_jit_array_push_i64, i64, 8);
push_fn!(ilang_jit_array_push_f32, f32, 4);
push_fn!(ilang_jit_array_push_f64, f64, 8);

// ─── Map runtime ──────────────────────────────────────────────────────
//
// Map<K, V> is implemented as a Rust `HashMap<MapKey, i64>` boxed and
// pointed to by a JIT-visible header. Keys are typed at the Rust side
// (Str / Int / UInt / Bool — same shape as the interpreter's MapKey).
// Values are stored as raw 8-byte slots; per-Map `drop_fn` (if any)
// releases each value as a heap pointer when the map dies or an entry
// is overwritten / deleted. `key_kind` tags the K representation so
// the runtime can convert raw key bits ↔ MapKey.
//
// Layout (32 bytes):
//   0  rc:        i64
//   8  drop_fn:   i64  // extern "C" fn(val: i64) — releases one value
//  16  key_kind:  i64  // 0=Str, 1=Int (i64), 2=UInt (u64), 3=Bool
//  24  inner:     i64  // *mut HashMap<MapKey, i64>

pub(crate) const MAP_KEY_KIND_STR: i64 = 0;
pub(crate) const MAP_KEY_KIND_INT: i64 = 1;
pub(crate) const MAP_KEY_KIND_UINT: i64 = 2;
pub(crate) const MAP_KEY_KIND_BOOL: i64 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum MapKey {
    Str(String),
    Int(i64),
    UInt(u64),
    Bool(bool),
}

#[repr(C)]
pub(crate) struct MapHeader {
    pub rc: i64,
    pub drop_fn: i64,
    pub key_kind: i64,
    pub inner: i64,
}

unsafe fn key_from_bits(kind: i64, bits: i64) -> MapKey {
    match kind {
        MAP_KEY_KIND_STR => {
            // String keys arrive as a `*mut StringRc` pointer; copy the
            // backing String so the map can hash on its content (and
            // owns its own copy independent of the input's ARC lifetime).
            if bits == 0 {
                MapKey::Str(String::new())
            } else {
                let s = &(*(bits as *const StringRc)).s;
                MapKey::Str(s.clone())
            }
        }
        MAP_KEY_KIND_INT => MapKey::Int(bits),
        MAP_KEY_KIND_UINT => MapKey::UInt(bits as u64),
        MAP_KEY_KIND_BOOL => MapKey::Bool(bits != 0),
        _ => panic!("ilang_jit_map: unknown key_kind {kind}"),
    }
}

unsafe fn inner_mut<'a>(ptr: i64) -> &'a mut std::collections::HashMap<MapKey, i64> {
    &mut *((*(ptr as *mut MapHeader)).inner as *mut std::collections::HashMap<MapKey, i64>)
}

pub(crate) extern "C" fn ilang_jit_map_new(key_kind: i64, drop_fn: i64) -> i64 {
    let inner = Box::new(std::collections::HashMap::<MapKey, i64>::new());
    let inner_ptr = Box::into_raw(inner) as i64;
    let header_layout = std::alloc::Layout::new::<MapHeader>();
    let header = unsafe { std::alloc::alloc_zeroed(header_layout) as *mut MapHeader };
    unsafe {
        (*header).rc = 1;
        (*header).drop_fn = drop_fn;
        (*header).key_kind = key_kind;
        (*header).inner = inner_ptr;
    }
    header as i64
}

pub(crate) extern "C" fn ilang_jit_retain_map(ptr: i64) {
    if ptr == 0 {
        return;
    }
    unsafe {
        let rc = ptr as *mut i64;
        *rc += 1;
    }
}

pub(crate) extern "C" fn ilang_jit_release_map(ptr: i64) {
    if ptr == 0 {
        return;
    }
    unsafe {
        let header = ptr as *mut MapHeader;
        (*header).rc -= 1;
        if (*header).rc != 0 {
            return;
        }
        // Walk values and let the per-(K,V) drop_fn release any heap
        // payloads. The HashMap itself drops keys (Rust-side memory).
        let drop_fn = (*header).drop_fn;
        let inner_ptr = (*header).inner as *mut std::collections::HashMap<MapKey, i64>;
        if drop_fn != 0 {
            let f: extern "C" fn(i64) = std::mem::transmute(drop_fn);
            for (_, v) in (*inner_ptr).iter() {
                f(*v);
            }
        }
        // Free inner HashMap.
        drop(Box::from_raw(inner_ptr));
        // Free header.
        let header_layout = std::alloc::Layout::new::<MapHeader>();
        std::alloc::dealloc(ptr as *mut u8, header_layout);
    }
}

/// Insert (key_bits, val). If a previous value existed at the same
/// key, it is released via the per-Map drop_fn (heap V) before being
/// dropped from the map.
pub(crate) extern "C" fn ilang_jit_map_set(ptr: i64, key_bits: i64, val: i64) {
    if ptr == 0 {
        return;
    }
    unsafe {
        let header = ptr as *mut MapHeader;
        let kind = (*header).key_kind;
        let drop_fn = (*header).drop_fn;
        let key = key_from_bits(kind, key_bits);
        if let Some(old) = inner_mut(ptr).insert(key, val) {
            if drop_fn != 0 {
                let f: extern "C" fn(i64) = std::mem::transmute(drop_fn);
                f(old);
            }
        }
    }
}

pub(crate) extern "C" fn ilang_jit_map_has(ptr: i64, key_bits: i64) -> i8 {
    if ptr == 0 {
        return 0;
    }
    unsafe {
        let header = ptr as *mut MapHeader;
        let key = key_from_bits((*header).key_kind, key_bits);
        if inner_mut(ptr).contains_key(&key) { 1 } else { 0 }
    }
}

pub(crate) extern "C" fn ilang_jit_map_size(ptr: i64) -> i64 {
    if ptr == 0 {
        return 0;
    }
    unsafe { inner_mut(ptr).len() as i64 }
}

/// Returns 1 if the entry existed (and was removed), 0 otherwise.
/// Releases the value via drop_fn before removing.
pub(crate) extern "C" fn ilang_jit_map_delete(ptr: i64, key_bits: i64) -> i8 {
    if ptr == 0 {
        return 0;
    }
    unsafe {
        let header = ptr as *mut MapHeader;
        let kind = (*header).key_kind;
        let drop_fn = (*header).drop_fn;
        let key = key_from_bits(kind, key_bits);
        if let Some(old) = inner_mut(ptr).remove(&key) {
            if drop_fn != 0 {
                let f: extern "C" fn(i64) = std::mem::transmute(drop_fn);
                f(old);
            }
            1
        } else {
            0
        }
    }
}

/// Index get: `m[k]` returns the value bits or aborts with a runtime
/// panic when the key is missing (mirrors the interpreter's
/// "map key not found"). Heap values are NOT retained — the caller is
/// responsible for retain if it wants its own reference (the map keeps
/// its own +1 internally; aliased reads behave like array indexing).
pub(crate) extern "C" fn ilang_jit_map_index_get(ptr: i64, key_bits: i64) -> i64 {
    unsafe {
        if ptr == 0 {
            eprintln!("ilang runtime: index on null Map");
            std::process::abort();
        }
        let header = ptr as *mut MapHeader;
        let key = key_from_bits((*header).key_kind, key_bits);
        match inner_mut(ptr).get(&key) {
            Some(v) => *v,
            None => {
                eprintln!("ilang runtime: map key not found");
                std::process::abort();
            }
        }
    }
}

/// Returns 0 if the key is missing, else the value bits (no retain).
/// Used by `m.get(k): V?` for V=heap (the JIT-side lowering then bumps
/// the pointer's rc so the caller has its own reference).
pub(crate) extern "C" fn ilang_jit_map_get_or_null(ptr: i64, key_bits: i64) -> i64 {
    if ptr == 0 {
        return 0;
    }
    unsafe {
        let header = ptr as *mut MapHeader;
        let key = key_from_bits((*header).key_kind, key_bits);
        match inner_mut(ptr).get(&key) {
            Some(v) => *v,
            None => 0,
        }
    }
}

/// Build a JIT array (`ArrayHeader` + data buffer) of all keys, in
/// arbitrary HashMap iteration order. `elem_size` matches the JitTy
/// width of K. String keys are materialized as fresh `Box<StringRc>`
/// instances with rc=1 so the returned array owns them; non-string
/// keys are stored as their raw bits.
pub(crate) extern "C" fn ilang_jit_map_keys_to_array(
    ptr: i64,
    elem_size: i64,
    drop_fn: i64,
) -> i64 {
    if ptr == 0 {
        return ilang_jit_array_new(elem_size, 0, drop_fn);
    }
    unsafe {
        let header = ptr as *mut MapHeader;
        let key_kind = (*header).key_kind;
        let len = ilang_jit_map_size(ptr);
        let arr = ilang_jit_array_new(elem_size, len, drop_fn);
        let arr_header = arr as *mut ArrayHeader;
        let data = (*arr_header).data_ptr;
        for (i, k) in inner_mut(ptr).keys().enumerate() {
            let bits: i64 = match k {
                MapKey::Str(s) => {
                    let boxed = Box::new(StringRc { rc: 1, s: s.clone() });
                    Box::into_raw(boxed) as i64
                }
                MapKey::Int(n) => *n,
                MapKey::UInt(u) => *u as i64,
                MapKey::Bool(b) => if *b { 1 } else { 0 },
            };
            let _ = key_kind; // currently unused beyond the MapKey discriminant
            let dst = data + (i as i64) * elem_size;
            write_array_slot(dst, elem_size, bits);
        }
        arr
    }
}

/// Build a JIT array of all values. `retain_fn` (per-V heap retain
/// helper, JIT-generated) is invoked on each value being copied so the
/// new array owns its own +1; pass 0 for non-heap V.
pub(crate) extern "C" fn ilang_jit_map_values_to_array(
    ptr: i64,
    elem_size: i64,
    drop_fn: i64,
    retain_fn: i64,
) -> i64 {
    if ptr == 0 {
        return ilang_jit_array_new(elem_size, 0, drop_fn);
    }
    unsafe {
        let len = ilang_jit_map_size(ptr);
        let arr = ilang_jit_array_new(elem_size, len, drop_fn);
        let arr_header = arr as *mut ArrayHeader;
        let data = (*arr_header).data_ptr;
        for (i, v) in inner_mut(ptr).values().enumerate() {
            if retain_fn != 0 {
                let f: extern "C" fn(i64) = std::mem::transmute(retain_fn);
                f(*v);
            }
            let dst = data + (i as i64) * elem_size;
            write_array_slot(dst, elem_size, *v);
        }
        arr
    }
}

unsafe fn write_array_slot(addr: i64, elem_size: i64, bits: i64) {
    match elem_size {
        1 => *(addr as *mut i8) = bits as i8,
        2 => *(addr as *mut i16) = bits as i16,
        4 => *(addr as *mut i32) = bits as i32,
        8 => *(addr as *mut i64) = bits,
        n => panic!("ilang runtime: unexpected array elem_size {n}"),
    }
}

// ─── Primitive Optional runtime ──────────────────────────────────────
//
// `i64?` / `bool?` / `f64?` and similar primitive-payload Optionals
// can't reuse the heap-pointer-as-tag scheme (0 is a valid payload),
// so each `Some(v)` boxes the value on the heap with a leading rc:
//
//   [ rc: i64 | payload: T ]
//
// `None` stays as the bare 0 pointer. The JIT writes / reads the
// payload itself via raw load/store at offset 8; this runtime owns
// only allocation, retain, and release.

pub(crate) const OPT_PRIM_PAYLOAD_OFFSET: i32 = 8;

pub(crate) extern "C" fn ilang_jit_optional_box_new(payload_size: i64) -> i64 {
    let total = 8 + payload_size as usize;
    let layout = std::alloc::Layout::from_size_align(total.max(16), 8).unwrap();
    let ptr = unsafe { std::alloc::alloc_zeroed(layout) as *mut i64 };
    unsafe { *ptr = 1; } // rc = 1; payload zeroed and overwritten by caller
    ptr as i64
}

pub(crate) extern "C" fn ilang_jit_optional_box_retain(ptr: i64) {
    if ptr == 0 {
        return;
    }
    unsafe {
        let rc = ptr as *mut i64;
        *rc += 1;
    }
}

pub(crate) extern "C" fn ilang_jit_optional_box_release(ptr: i64, payload_size: i64) {
    if ptr == 0 {
        return;
    }
    unsafe {
        let rc = ptr as *mut i64;
        *rc -= 1;
        if *rc != 0 {
            return;
        }
        let total = 8 + payload_size as usize;
        let layout = std::alloc::Layout::from_size_align(total.max(16), 8).unwrap();
        std::alloc::dealloc(ptr as *mut u8, layout);
    }
}

// ─── Extra string methods (replace / split / slice) ──────────────────

/// Replace ALL occurrences of `needle` with `repl` (Rust-style, not
/// JS's first-only). Returns a fresh `Box<StringRc>` with rc=1; the
/// caller owns the new pointer.
pub(crate) extern "C" fn ilang_jit_str_replace(s: i64, needle: i64, repl: i64) -> i64 {
    if s == 0 {
        return 0;
    }
    unsafe {
        let s_str = &(*(s as *const StringRc)).s;
        let n_str = if needle == 0 { "" } else { &(*(needle as *const StringRc)).s };
        let r_str = if repl == 0 { "" } else { &(*(repl as *const StringRc)).s };
        let out = s_str.replace(n_str, r_str);
        Box::into_raw(Box::new(StringRc { rc: 1, s: out })) as i64
    }
}

/// Substring on Unicode code points (mirrors `.length` / `charAt`).
/// Indices are clamped to [0, len_chars]; if start > end after
/// clamping, returns the empty string.
pub(crate) extern "C" fn ilang_jit_str_slice(s: i64, start: i64, end: i64) -> i64 {
    let result = if s == 0 {
        String::new()
    } else {
        unsafe {
            let s_str = &(*(s as *const StringRc)).s;
            let chars: Vec<char> = s_str.chars().collect();
            let len = chars.len() as i64;
            let s_idx = start.max(0).min(len) as usize;
            let e_idx = end.max(0).min(len) as usize;
            let s_idx = s_idx.min(e_idx);
            chars[s_idx..e_idx].iter().collect()
        }
    };
    Box::into_raw(Box::new(StringRc { rc: 1, s: result })) as i64
}

/// Split on `sep` (Rust-style: empty separator → per-char). Returns
/// an `ArrayHeader` of `*mut StringRc` (i64 slots), each element being
/// a freshly allocated string with rc=1. `drop_fn` is the JIT-
/// generated per-array-kind drop wrapper for the resulting `string[]`.
pub(crate) extern "C" fn ilang_jit_str_split(s: i64, sep: i64, drop_fn: i64) -> i64 {
    let parts: Vec<String> = if s == 0 {
        Vec::new()
    } else {
        unsafe {
            let s_str = &(*(s as *const StringRc)).s;
            let sep_str = if sep == 0 { "" } else { &(*(sep as *const StringRc)).s };
            if sep_str.is_empty() {
                s_str.chars().map(|c| c.to_string()).collect()
            } else {
                s_str.split(sep_str).map(|p| p.to_string()).collect()
            }
        }
    };
    let len = parts.len() as i64;
    let arr = ilang_jit_array_new(8, len, drop_fn);
    if arr == 0 {
        return 0;
    }
    unsafe {
        let header = arr as *mut ArrayHeader;
        let data = (*header).data_ptr;
        for (i, p) in parts.into_iter().enumerate() {
            let sr = Box::into_raw(Box::new(StringRc { rc: 1, s: p })) as i64;
            let dst = (data + (i as i64) * 8) as *mut i64;
            *dst = sr;
        }
    }
    arr
}

// ─── runtime panic helpers ───────────────────────────────────────────

/// Abort with a fixed message + exit non-zero. Used by JIT-emitted
/// bounds checks, division-by-zero checks, and unwrap-on-none checks.
/// Tagged with `#[unsafe(no_mangle)]` would be needed if Cranelift
/// linked symbolically, but we register via JITBuilder::symbol so the
/// raw fn pointer is enough.
pub(crate) extern "C" fn ilang_jit_panic_index_oob(idx: i64, len: i64) -> ! {
    eprintln!("runtime panic: array index {idx} out of bounds (length {len})");
    std::process::exit(1);
}

pub(crate) extern "C" fn ilang_jit_panic_div_zero() -> ! {
    eprintln!("runtime panic: integer division by zero");
    std::process::exit(1);
}

pub(crate) extern "C" fn ilang_jit_panic_unwrap_none() -> ! {
    eprintln!("runtime panic: unwrap on `none`");
    std::process::exit(1);
}
