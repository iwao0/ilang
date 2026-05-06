//! Host-side test-assertion FFI exposed to JITed code as `@extern fn`s.
//! Names match the qualified form produced by the loader
//! (`test.expect`, `test.expectStr`, ...). Each helper aborts with
//! exit code 2 on mismatch so the harness sees a non-zero status.

use cranelift_jit::JITBuilder;

use crate::runtime::StringRc;

extern "C" fn test_expect(actual: i64, expected: i64) {
    if actual != expected {
        eprintln!("test assertion failed: expected {expected}, got {actual}");
        std::process::exit(2);
    }
}

extern "C" fn test_expect_str(actual_ptr: i64, expected_ptr: i64) {
    let a = if actual_ptr == 0 {
        String::new()
    } else {
        unsafe { (*(actual_ptr as *const StringRc)).s.clone() }
    };
    let e = if expected_ptr == 0 {
        String::new()
    } else {
        unsafe { (*(expected_ptr as *const StringRc)).s.clone() }
    };
    if a != e {
        eprintln!("test assertion failed: expected {e:?}, got {a:?}");
        std::process::exit(2);
    }
}

extern "C" fn test_expect_bool(actual: i8, expected: i8) {
    if actual != expected {
        let a = actual != 0;
        let e = expected != 0;
        eprintln!("test assertion failed: expected {e}, got {a}");
        std::process::exit(2);
    }
}

extern "C" fn test_expect_f64(actual: f64, expected: f64) {
    if actual != expected {
        eprintln!("test assertion failed: expected {expected}, got {actual}");
        std::process::exit(2);
    }
}

extern "C" fn test_expect_true(condition: i8) {
    if condition == 0 {
        eprintln!("test assertion failed: expected true, got false");
        std::process::exit(2);
    }
}

extern "C" fn test_expect_false(condition: i8) {
    if condition != 0 {
        eprintln!("test assertion failed: expected false, got true");
        std::process::exit(2);
    }
}

/// Counter-wrapped libc::free. Each invocation bumps an atomic
/// counter so a test can observe how many times it was called.
/// Pair with `test.countedFreeCount` to check counts before/after
/// a section of code.
static FREE_COUNT: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

extern "C" fn test_counted_free(ptr: i64) {
    if ptr == 0 {
        return;
    }
    FREE_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    unsafe { libc_free_for_test(ptr) };
}

extern "C" fn test_counted_free_count() -> i32 {
    FREE_COUNT.load(std::sync::atomic::Ordering::SeqCst)
}

unsafe extern "C" {
    #[link_name = "free"]
    fn libc_free_for_test(ptr: i64);
}

/// Invoke a 2-argument i32 callback and return the result. Used by
/// the JIT-side callback round-trip test: lets us observe that an
/// ilang fn was actually called from a non-Cranelift context (the
/// Rust runtime here, simulating arbitrary native code).
extern "C" fn test_apply_i32_cb(
    cb: extern "C" fn(i64, i64) -> i32,
    a: i64,
    b: i64,
) -> i32 {
    cb(a, b)
}

// ─── pass-by-value `@extern(C) struct` struct (by_value flag) ─────────────────
// 8-byte struct: passes in a single GPR on AArch64 / SysV.
#[repr(C)]
#[derive(Copy, Clone)]
struct Point2 { x: i32, y: i32 }

extern "C" fn test_sum_point2(p: Point2) -> i64 {
    (p.x as i64) + (p.y as i64)
}

// 16-byte struct: passes in 2 GPRs on the same ABIs.
#[repr(C)]
#[derive(Copy, Clone)]
struct Range64 { lo: i64, hi: i64 }

extern "C" fn test_range64_width(r: Range64) -> i64 {
    r.hi - r.lo
}

// 12-byte struct (i32 + i64): two chunks — first chunk holds tag (low
// 4 B) plus the low 4 B of payload, second chunk holds the upper 4 B
// of payload. The C side reassembles via natural field reads.
#[repr(C)]
#[derive(Copy, Clone)]
struct Tagged { tag: i32, payload: i64 }

extern "C" fn test_tagged_payload_if(t: Tagged, expected_tag: i32) -> i64 {
    if t.tag == expected_tag { t.payload } else { -1 }
}

// Struct returns by value. 8 B → single GPR (X0 on AArch64,
// RAX on x86_64 SysV). 16 B → register pair (X0:X1 / RAX:RDX).
extern "C" fn test_make_point2(x: i32, y: i32) -> Point2 {
    Point2 { x, y }
}

extern "C" fn test_make_range64(lo: i64, hi: i64) -> Range64 {
    Range64 { lo, hi }
}

// 32-byte struct: indirect pass via Cranelift's StructArgument
// purpose (AArch64 AAPCS64: hidden pointer; x86_64 SysV: stack).
#[repr(C)]
#[derive(Copy, Clone)]
struct Big32 { a: i64, b: i64, c: i64, d: i64 }

extern "C" fn test_big32_sum(big: Big32) -> i64 {
    big.a + big.b + big.c + big.d
}

// HFA: homogeneous floating-point aggregates flow through FP regs.
#[repr(C)]
#[derive(Copy, Clone)]
struct Vec3f { x: f32, y: f32, z: f32 }

extern "C" fn test_vec3f_dot(a: Vec3f, b: Vec3f) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

extern "C" fn test_vec3f_make(x: f32, y: f32, z: f32) -> Vec3f {
    Vec3f { x, y, z }
}

#[repr(C)]
#[derive(Copy, Clone)]
struct Pair64 { a: f64, b: f64 }

extern "C" fn test_pair64_sum(p: Pair64) -> f64 {
    p.a + p.b
}

// sret return: the caller passes a hidden pointer in the indirect-
// result register (X8 on AArch64, RDI on x86_64 SysV); the callee
// writes the struct there.
extern "C" fn test_make_big32(a: i64, b: i64, c: i64, d: i64) -> Big32 {
    Big32 { a, b, c, d }
}

extern "C" fn test_fail(msg_ptr: i64) {
    let msg = if msg_ptr == 0 {
        "<empty>".to_string()
    } else {
        unsafe { (*(msg_ptr as *const StringRc)).s.clone() }
    };
    eprintln!("test assertion failed: {msg}");
    std::process::exit(2);
}

pub(crate) fn register_test_symbols(builder: &mut JITBuilder) {
    builder.symbol("test.expect", test_expect as *const u8);
    builder.symbol("test.expectStr", test_expect_str as *const u8);
    builder.symbol("test.expectBool", test_expect_bool as *const u8);
    builder.symbol("test.expectF64", test_expect_f64 as *const u8);
    builder.symbol("test.expectTrue", test_expect_true as *const u8);
    builder.symbol("test.expectFalse", test_expect_false as *const u8);
    builder.symbol("test.fail", test_fail as *const u8);
    builder.symbol("test.applyI32Cb", test_apply_i32_cb as *const u8);
    builder.symbol("test.countedFree", test_counted_free as *const u8);
    builder.symbol("test.countedFreeCount", test_counted_free_count as *const u8);
    builder.symbol("test.sumPoint2", test_sum_point2 as *const u8);
    builder.symbol("test.range64Width", test_range64_width as *const u8);
    builder.symbol("test.taggedPayloadIf", test_tagged_payload_if as *const u8);
    // Bare names too so test fixtures can declare the extern fn
    // locally (`@extern(by_value) fn sum_point2(...)`) without
    // having to add a class type to the global `test` module.
    builder.symbol("sum_point2", test_sum_point2 as *const u8);
    builder.symbol("range64_width", test_range64_width as *const u8);
    builder.symbol("tagged_payload_if", test_tagged_payload_if as *const u8);
    builder.symbol("make_point2", test_make_point2 as *const u8);
    builder.symbol("make_range64", test_make_range64 as *const u8);
    builder.symbol("big32_sum", test_big32_sum as *const u8);
    builder.symbol("vec3f_dot", test_vec3f_dot as *const u8);
    builder.symbol("vec3f_make", test_vec3f_make as *const u8);
    builder.symbol("pair64_sum", test_pair64_sum as *const u8);
    builder.symbol("make_big32", test_make_big32 as *const u8);
    // Bare-name aliases for layout-test fixtures that declare per-
    // struct extern fns of the form `(p: SomeStruct, off: i64): i32`.
    // The JIT auto-passes the struct's user pointer as an i64 at the
    // C boundary, which matches `test_byte_at`'s signature exactly.
    builder.symbol("pkt_byte_at", test_byte_at as *const u8);
    builder.symbol("alg_byte_at", test_byte_at as *const u8);
    builder.symbol("get_byte_slice", test_get_byte_slice as *const u8);
    builder.symbol("get_i32_slice", test_get_i32_slice as *const u8);
    builder.symbol("maybe_byte_slice", test_maybe_byte_slice as *const u8);
    builder.symbol("maybe_succeed", test_maybe_succeed as *const u8);
    builder.symbol("maybe_succeed_i64", test_maybe_succeed_i64 as *const u8);
    builder.symbol("get_cstr_array", test_get_cstr_array as *const u8);
    builder.symbol("get_empty_cstr_array", test_get_empty_cstr_array as *const u8);
}

// ─── @extern static globals (test-only) ─────────────────────────────
// Real C globals (`errno`, `stdin`, ...) are resolved by dlsym on the
// platform libc. For self-contained tests we expose plain Rust statics
// at fixed addresses; test fixtures declare `@extern static <name>: T`
// with the same name and the JIT lowers reads/writes to load/store
// against the registered address.
static mut TEST_STATIC_I32: i32 = 0;
static mut TEST_STATIC_F64: f64 = 0.0;

// `slice_return`: C-side helpers return a `{ ptr, len }` 16 B struct
// that the JIT marshals into a fresh ilang `T[]`. The data they
// reference lives in static storage so the JIT-side memcpy reads
// stable bytes after the call returns.
#[repr(C)]
struct U8Slice {
    ptr: *const u8,
    len: usize,
}

#[repr(C)]
struct I32Slice {
    ptr: *const i32,
    len: usize,
}

static TEST_SLICE_BYTES: [u8; 5] = [10, 20, 30, 40, 50];
static TEST_SLICE_INTS: [i32; 4] = [-1, 100, 200, 300];

extern "C" fn test_get_byte_slice() -> U8Slice {
    U8Slice {
        ptr: TEST_SLICE_BYTES.as_ptr(),
        len: TEST_SLICE_BYTES.len(),
    }
}

extern "C" fn test_get_i32_slice() -> I32Slice {
    I32Slice {
        ptr: TEST_SLICE_INTS.as_ptr(),
        len: TEST_SLICE_INTS.len(),
    }
}

// Conditional slice return — drives the NULL-ptr branch the JIT
// inserts when the declared return is `T[]?`.
// `cstrArray`: C side returns a NUL-terminated `char**` (e.g.
// `environ`, glib `g_strsplit`). The JIT marshals each entry into
// a fresh ilang string and assembles a `string[]`. The pointers
// live in `static mut` storage that's initialised on first call;
// this avoids raw-pointer-in-`static` `Sync` issues while keeping
// the bytes valid for the duration of every test.
extern "C" fn test_get_cstr_array() -> *const *const u8 {
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

extern "C" fn test_get_empty_cstr_array() -> *const *const u8 {
    static mut PTRS: [*const u8; 1] = [std::ptr::null()];
    std::ptr::addr_of!(PTRS).cast::<*const u8>()
}

// Mimics a POSIX call that returns -1 on failure (and would set
// errno). Used to drive the `errnoCheck` flag's branch.
extern "C" fn test_maybe_succeed(ok: i32) -> i32 {
    if ok != 0 { 42 } else { -1 }
}

extern "C" fn test_maybe_succeed_i64(ok: i32) -> i64 {
    if ok != 0 { 1_234_567_890_123 } else { -1 }
}

extern "C" fn test_maybe_byte_slice(ok: i32) -> U8Slice {
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

// Read a raw byte from an address — used by layout tests to verify
// `@packed` actually packs (no padding bytes inserted).
extern "C" fn test_byte_at(ptr: i64, offset: i64) -> i32 {
    if ptr == 0 {
        return 0;
    }
    unsafe { *((ptr + offset) as *const u8) as i32 }
}

pub(crate) fn register_test_static_addrs(out: &mut std::collections::HashMap<String, i64>) {
    out.insert(
        "test_static_i32".into(),
        std::ptr::addr_of_mut!(TEST_STATIC_I32) as i64,
    );
    out.insert(
        "test_static_f64".into(),
        std::ptr::addr_of_mut!(TEST_STATIC_F64) as i64,
    );
}
