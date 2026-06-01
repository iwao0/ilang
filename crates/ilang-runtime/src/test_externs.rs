//! Fixture-only `@extern(C)` helpers used by the language's
//! struct-by-value / slice / cstrArray marshalling tests. Exported
//! under bare C names matching the fixture-side `@lib("c")`
//! declarations. The link-time `-Wl,-dead_strip` pass removes these
//! from any production user binary that doesn't reference them, so
//! shipping them in `libilang_runtime.a` only costs a few KB of
//! intermediate object size.

// ─── 1-arg / 2-arg by-value structs ──────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Point2 { pub x: i32, pub y: i32 }

#[unsafe(export_name = "sum_point2")]
pub extern "C" fn sum_point2(p: Point2) -> i64 {
    (p.x as i64) + (p.y as i64)
}

#[unsafe(export_name = "make_point2")]
pub extern "C" fn make_point2(x: i32, y: i32) -> Point2 {
    Point2 { x, y }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Range64 { pub lo: i64, pub hi: i64 }

#[unsafe(export_name = "range64_width")]
pub extern "C" fn range64_width(r: Range64) -> i64 {
    r.hi - r.lo
}

#[unsafe(export_name = "make_range64")]
pub extern "C" fn make_range64(lo: i64, hi: i64) -> Range64 {
    Range64 { lo, hi }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Tagged { pub tag: i32, pub payload: i64 }

#[unsafe(export_name = "tagged_payload_if")]
pub extern "C" fn tagged_payload_if(t: Tagged, expected_tag: i32) -> i64 {
    if t.tag == expected_tag { t.payload } else { -1 }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Big32 { pub a: i64, pub b: i64, pub c: i64, pub d: i64 }

#[unsafe(export_name = "big32_sum")]
pub extern "C" fn big32_sum(big: Big32) -> i64 {
    big.a + big.b + big.c + big.d
}

#[unsafe(export_name = "make_big32")]
pub extern "C" fn make_big32(a: i64, b: i64, c: i64, d: i64) -> Big32 {
    Big32 { a, b, c, d }
}

// HFA: homogeneous floating-point aggregates flow through FP regs.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct Vec3f { pub x: f32, pub y: f32, pub z: f32 }

#[unsafe(export_name = "vec3f_dot")]
pub extern "C" fn vec3f_dot(a: Vec3f, b: Vec3f) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

#[unsafe(export_name = "vec3f_make")]
pub extern "C" fn vec3f_make(x: f32, y: f32, z: f32) -> Vec3f {
    Vec3f { x, y, z }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Pair64 { pub a: f64, pub b: f64 }

#[unsafe(export_name = "pair64_sum")]
pub extern "C" fn pair64_sum(p: Pair64) -> f64 {
    p.a + p.b
}

// ─── slice / cstrArray return ────────────────────────────────────────

#[repr(C)]
pub struct U8Slice {
    pub ptr: *const u8,
    pub len: usize,
}

#[repr(C)]
pub struct I32Slice {
    pub ptr: *const i32,
    pub len: usize,
}

static TEST_SLICE_BYTES: [u8; 5] = [10, 20, 30, 40, 50];
static TEST_SLICE_INTS: [i32; 4] = [-1, 100, 200, 300];

#[unsafe(export_name = "get_byte_slice")]
pub extern "C" fn get_byte_slice() -> U8Slice {
    U8Slice {
        ptr: TEST_SLICE_BYTES.as_ptr(),
        len: TEST_SLICE_BYTES.len(),
    }
}

#[unsafe(export_name = "get_i32_slice")]
pub extern "C" fn get_i32_slice() -> I32Slice {
    I32Slice {
        ptr: TEST_SLICE_INTS.as_ptr(),
        len: TEST_SLICE_INTS.len(),
    }
}

#[unsafe(export_name = "maybe_byte_slice")]
pub extern "C" fn maybe_byte_slice(ok: i32) -> U8Slice {
    if ok != 0 {
        U8Slice {
            ptr: TEST_SLICE_BYTES.as_ptr(),
            len: TEST_SLICE_BYTES.len(),
        }
    } else {
        U8Slice {
            ptr: std::ptr::null(),
            len: 0,
        }
    }
}

#[unsafe(export_name = "maybe_succeed")]
pub extern "C" fn maybe_succeed(ok: i32) -> i32 {
    if ok != 0 { 42 } else { -1 }
}

#[unsafe(export_name = "maybe_succeed_i64")]
pub extern "C" fn maybe_succeed_i64(ok: i32) -> i64 {
    if ok != 0 { 1_234_567_890_123 } else { -1 }
}

#[unsafe(export_name = "set_via_ptr")]
pub extern "C" fn set_via_ptr(out: *mut i32, value: i32) {
    unsafe { *out = value }
}

#[unsafe(export_name = "set_via_ptr_f64")]
pub extern "C" fn set_via_ptr_f64(out: *mut f64, value: f64) {
    unsafe { *out = value }
}

#[unsafe(export_name = "test_alias_inc")]
pub extern "C" fn test_alias_inc(x: i64) -> i64 {
    x + 1
}

#[unsafe(export_name = "test_alias_pair")]
pub extern "C" fn test_alias_pair(a: i64, b: i64) -> i64 {
    a * 100 + b
}

#[unsafe(export_name = "get_cstr_array")]
pub extern "C" fn get_cstr_array() -> *const *const u8 {
    static mut PTRS: [*const u8; 4] = [std::ptr::null(); 4];
    static ONCE: std::sync::Once = std::sync::Once::new();
    unsafe {
        ONCE.call_once(|| {
            PTRS[0] = b"first\0".as_ptr();
            PTRS[1] = b"second\0".as_ptr();
            PTRS[2] = b"third\0".as_ptr();
            PTRS[3] = std::ptr::null();
        });
        std::ptr::addr_of!(PTRS).cast::<*const u8>()
    }
}

#[unsafe(export_name = "get_empty_cstr_array")]
pub extern "C" fn get_empty_cstr_array() -> *const *const u8 {
    static mut PTRS: [*const u8; 1] = [std::ptr::null()];
    std::ptr::addr_of!(PTRS).cast::<*const u8>()
}

// Read a raw byte from an address — used by layout-test fixtures.
#[unsafe(export_name = "alg_byte_at")]
pub extern "C" fn alg_byte_at(ptr: i64, offset: i64) -> i32 {
    if ptr == 0 {
        return 0;
    }
    unsafe { *((ptr + offset) as *const u8) as i32 }
}

#[unsafe(export_name = "pkt_byte_at")]
pub extern "C" fn pkt_byte_at(ptr: i64, offset: i64) -> i32 {
    if ptr == 0 {
        return 0;
    }
    unsafe { *((ptr + offset) as *const u8) as i32 }
}

// Inspect a `T[]` that has crossed into C as `*T` for a CRepr
// struct element. Reads the first two `{ a: i32, b: i32 }` pairs
// from the pointer and packs them into a single i64 so the
// fixture can assert the C side really saw the element payload
// and not the ilang array header. Used by
// `04_modules/struct_array_to_c_ptr.il`.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct PairI32 { pub a: i32, pub b: i32 }

#[unsafe(export_name = "pair_first_a")]
pub extern "C" fn pair_first_a(p: *const PairI32) -> i32 {
    unsafe { (*p).a }
}
#[unsafe(export_name = "pair_first_b")]
pub extern "C" fn pair_first_b(p: *const PairI32) -> i32 {
    unsafe { (*p).b }
}
#[unsafe(export_name = "pair_nth_a")]
pub extern "C" fn pair_nth_a(p: *const PairI32, n: i64) -> i32 {
    unsafe { (*p.offset(n as isize)).a }
}
#[unsafe(export_name = "pair_nth_b")]
pub extern "C" fn pair_nth_b(p: *const PairI32, n: i64) -> i32 {
    unsafe { (*p.offset(n as isize)).b }
}

// Mirrors the wgpu descriptor / attachment shape: a descriptor
// struct holding a `*T` field where T is a multi-field CRepr
// struct. Used by `04_modules/struct_array_to_c_ptr.il` to
// confirm the field assignment extracts the array's element
// data pointer (not the array header).
#[repr(C)]
pub struct BigPair {
    pub next: *const std::ffi::c_void,
    pub view: i64,
    pub depth_slice: u32,
    pub resolve_target: i64,
    pub load_op: u32,
    pub store_op: u32,
    pub clear_value: f64,
}

#[repr(C)]
pub struct PairDesc {
    pub next: *const std::ffi::c_void,
    pub count: u64,
    pub items: *const BigPair,
}

#[unsafe(export_name = "desc_first_depth")]
pub extern "C" fn desc_first_depth(d: *const PairDesc) -> u32 {
    unsafe { (*(*d).items).depth_slice }
}
#[unsafe(export_name = "desc_first_view")]
pub extern "C" fn desc_first_view(d: *const PairDesc) -> i64 {
    unsafe { (*(*d).items).view }
}
#[unsafe(export_name = "desc_first_load_op")]
pub extern "C" fn desc_first_load_op(d: *const PairDesc) -> u32 {
    unsafe { (*(*d).items).load_op }
}
#[unsafe(export_name = "bp_depth_slice")]
pub extern "C" fn bp_depth_slice(p: *const BigPair) -> u32 {
    unsafe { (*p).depth_slice }
}
