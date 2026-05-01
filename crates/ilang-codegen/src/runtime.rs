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
