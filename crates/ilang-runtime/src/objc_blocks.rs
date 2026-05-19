//! Minimal Objective-C block creation for ilang.
//!
//! Builds a heap block whose `invoke` trampoline calls an ilang
//! `fn(): unit` closure. The block is autoreleased (via the ObjC
//! ARC runtime's `objc_autorelease`) so callers can pass it
//! straight to APIs that expect `^void(^)(void)` without manual
//! `Block_release` bookkeeping — typical completion-handler use.
//!
//! Layout follows clang's compiler-rt:
//!
//! ```c
//! struct Block_layout {
//!     void *isa;
//!     int32_t flags;
//!     int32_t reserved;
//!     void *invoke;            // void (*)(struct Block_layout *)
//!     struct Block_descriptor *descriptor;
//!     // captured vars follow — we store the ilang closure pointer here
//! };
//! ```
//!
//! Flag bits we set:
//!   * `BLOCK_NEEDS_FREE` (1 << 24)        — runtime should `free()` on rc → 0
//!   * `BLOCK_HAS_COPY_DISPOSE` (1 << 25)  — descriptor carries copy / dispose helpers
//!   * `BLOCK_HAS_SIGNATURE` (1 << 30)     — descriptor carries an ObjC method signature
//!   * refcount bits (BLOCK_REFCOUNT_MASK = 0xFFFE) start at `1 << 1` (rc = 1)
//!
//! The copy / dispose helpers bump / drop the captured ilang closure's
//! refcount so the closure outlives the block when ObjC's
//! `Block_copy` snapshots us for later invocation.

#[cfg(target_os = "macos")]
use std::ffi::c_void;

#[cfg(target_os = "macos")]
use crate::alloc::{__mir_alloc, __mir_free};

#[cfg(target_os = "macos")]
#[repr(C)]
struct BlockLayout {
    isa: *mut c_void,
    flags: i32,
    reserved: i32,
    invoke: *const c_void,
    descriptor: *const BlockDescriptor,
    // Captured ilang closure pointer (one slot).
    closure: i64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct BlockDescriptor {
    reserved: usize,
    size: usize,
    copy_helper: extern "C" fn(*mut c_void, *const c_void),
    dispose_helper: extern "C" fn(*const c_void),
    signature: *const u8,
}

// ObjC runtime symbols we lean on.
//
// `_NSConcreteMallocBlock` is a *data symbol* (clang declares it as
// `void *_NSConcreteMallocBlock[32]`). The block ABI requires the
// block's `isa` slot to hold the *address* of this symbol — not the
// first 8 bytes of the array. Declaring it here as an opaque
// `c_void` static and taking `&` gives us that address; declaring
// it as `static FOO: *mut c_void` would instead read the first
// pointer-sized slot of the array and feed garbage to
// `objc_autorelease`'s class-object dereference, which is exactly
// the SIGSEGV the previous form produced.
#[cfg(target_os = "macos")]
#[link(name = "objc")]
unsafe extern "C" {
    static _NSConcreteMallocBlock: c_void;
    fn objc_autorelease(obj: *mut c_void) -> *mut c_void;
}

#[cfg(target_os = "macos")]
const BLOCK_NEEDS_FREE: i32 = 1 << 24;
#[cfg(target_os = "macos")]
const BLOCK_HAS_COPY_DISPOSE: i32 = 1 << 25;
#[cfg(target_os = "macos")]
const BLOCK_HAS_SIGNATURE: i32 = 1 << 30;
// `rc = 1` lives in bits 1..16 (bit 0 is the deallocating flag).
#[cfg(target_os = "macos")]
const BLOCK_RC_ONE: i32 = 1 << 1;

// `void(^)(void)` — encoding: "v8@?0" → return void, total args 8 bytes,
// arg 0 is the block itself (type `@?` for "block id"). Apple's
// runtime checks the size prefix loosely; the encoding matters for
// NSMethodSignature-based introspection.
#[cfg(target_os = "macos")]
static VOID_VOID_SIGNATURE: &[u8] = b"v8@?0\0";
// `void(^)(id)` — block + one id arg = 16 bytes total. `@8` says
// the second arg is an id starting at offset 8 in the arg frame.
#[cfg(target_os = "macos")]
static VOID_OBJ_SIGNATURE: &[u8] = b"v16@?0@8\0";
// `id(^)(id)` — return id, block + id args.
#[cfg(target_os = "macos")]
static OBJ_TO_OBJ_SIGNATURE: &[u8] = b"@16@?0@8\0";
// `void(^)(void *, size_t)` — block + raw bytes pointer + length.
// Used by `SKMutableTexture.modifyPixelDataWithBlock:` and the
// like. Arg-frame layout: block@0 (8B), pointer@8 (8B), length@16
// (8B) = 24B total. `^v` is `void *`; `Q` is `unsigned long long`
// (size_t on 64-bit).
#[cfg(target_os = "macos")]
static VOID_BYTES_SIGNATURE: &[u8] = b"v24@?0^v8Q16\0";
// `void(^)(id, id, id)` — block + three `id` arguments. Total arg
// frame: block@0 (8B), a@8 (8B), b@16 (8B), c@24 (8B) = 32B. Used
// by `NSURLSession`'s `dataTaskWithRequest:completionHandler:`
// family where the callback receives (NSData *, NSURLResponse *,
// NSError *).
#[cfg(target_os = "macos")]
static VOID_THREE_OBJ_SIGNATURE: &[u8] = b"v32@?0@8@16@24\0";
// `void(^)(BOOL)` — `c` is `signed char` (Objective-C's BOOL is a
// signed char on macOS x86_64 and a `_Bool` (i8) on arm64; the
// encoding is `c` either way for the size 1, signed slot). Block
// + BOOL = 9 bytes nominally, padded to 16 in the arg frame.
#[cfg(target_os = "macos")]
static VOID_BOOL_SIGNATURE: &[u8] = b"v16@?0c8\0";
// `void(^)(id, id)` — block + two id args = 24 bytes total.
// Identical calling convention to `void(^)(void *, size_t)` but
// the NSMethodSignature encoding correctly says "id, id" so any
// receiver that introspects (NSInvocation etc.) sees the right
// argument kinds.
#[cfg(target_os = "macos")]
static VOID_TWO_OBJ_SIGNATURE: &[u8] = b"v24@?0@8@16\0";

#[cfg(target_os = "macos")]
extern "C" fn invoke_void_block(b: *mut BlockLayout) {
    if b.is_null() {
        return;
    }
    let closure_ptr = unsafe { (*b).closure };
    if closure_ptr == 0 {
        return;
    }
    // Closure layout: [fn_ptr @ 0 | rc @ 8 | captures…]. Call
    // `fn_ptr(closure_ptr)` — the lifted fn's first param is its
    // own env pointer.
    unsafe {
        let fn_ptr = *(closure_ptr as *const usize);
        let f: extern "C" fn(i64) = std::mem::transmute(fn_ptr);
        f(closure_ptr);
    }
}

/// `id(^)(id)` trampoline. The ilang closure signature is
/// `fn(arg: i64): i64` — receives a raw `id` and returns a raw
/// `id` (or 0 for nil). Same env-is-last calling convention as
/// `invoke_obj_block`.
#[cfg(target_os = "macos")]
extern "C" fn invoke_obj_to_obj_block(b: *mut BlockLayout, arg: i64) -> i64 {
    if b.is_null() {
        return 0;
    }
    let closure_ptr = unsafe { (*b).closure };
    if closure_ptr == 0 {
        return 0;
    }
    unsafe {
        let fn_ptr = *(closure_ptr as *const usize);
        let f: extern "C" fn(i64, i64) -> i64 = std::mem::transmute(fn_ptr);
        f(arg, closure_ptr)
    }
}

/// `void(^)(id)` trampoline. The ilang closure signature is
/// `fn(arg: i64): unit`; the user is expected to wrap `arg` via
/// `NSObject.wrap(arg)` if they want a typed NSObject view.
///
/// Note the calling convention: ilang's lifted closure fn takes
/// `(user_args..., env)` — env is the *last* clif param, not the
/// first (see ilang-mir-codegen's lower_function `env_value`).
#[cfg(target_os = "macos")]
extern "C" fn invoke_obj_block(b: *mut BlockLayout, arg: i64) {
    if b.is_null() {
        return;
    }
    let closure_ptr = unsafe { (*b).closure };
    if closure_ptr == 0 {
        return;
    }
    unsafe {
        let fn_ptr = *(closure_ptr as *const usize);
        let f: extern "C" fn(i64, i64) = std::mem::transmute(fn_ptr);
        f(arg, closure_ptr);
    }
}

/// `void(^)(void *, size_t)` trampoline. The ilang closure
/// signature is `fn(ptr: i64, len: i64): unit` — raw bytes
/// pointer and length in bytes. Used by
/// `SKMutableTexture.modifyPixelDataWithBlock:` where the
/// callback writes pixel data into the texture's backing store
/// in-place; the user reaches for `readU8` / `writeU8` etc. to
/// poke individual bytes.
#[cfg(target_os = "macos")]
extern "C" fn invoke_void_bytes_block(b: *mut BlockLayout, ptr: i64, len: i64) {
    if b.is_null() {
        return;
    }
    let closure_ptr = unsafe { (*b).closure };
    if closure_ptr == 0 {
        return;
    }
    unsafe {
        let fn_ptr = *(closure_ptr as *const usize);
        // Lifted closure shape: (user_args..., env).
        let f: extern "C" fn(i64, i64, i64) = std::mem::transmute(fn_ptr);
        f(ptr, len, closure_ptr);
    }
}

/// `void(^)(id, id, id)` trampoline. ilang closure signature is
/// `fn(a: i64, b: i64, c: i64): unit`. The three `id`s come
/// straight off the wire as raw handles; the closure body wraps
/// each in `NSObject.wrap` (or a subclass equivalent) if it wants
/// a typed view. Used by every Foundation completion handler that
/// hands back `(NSData *, NSURLResponse *, NSError *)`.
/// `void(^)(id, id)` trampoline. Shares the calling convention
/// of `invoke_void_bytes_block` but is paired with the
/// `(id, id)` ObjC signature so introspection sees the right
/// arg kinds.
#[cfg(target_os = "macos")]
extern "C" fn invoke_void_two_obj_block(b: *mut BlockLayout, a: i64, c: i64) {
    if b.is_null() {
        return;
    }
    let closure_ptr = unsafe { (*b).closure };
    if closure_ptr == 0 {
        return;
    }
    unsafe {
        let fn_ptr = *(closure_ptr as *const usize);
        let f: extern "C" fn(i64, i64, i64) = std::mem::transmute(fn_ptr);
        f(a, c, closure_ptr);
    }
}

/// `void(^)(BOOL)` trampoline. ilang closure shape is
/// `fn(b: bool): unit`. ObjC's BOOL is a signed char on
/// macOS, which Rust models as `bool` (1-byte i8). The lifted
/// closure trailing-env signature is `(bool, i64)`.
#[cfg(target_os = "macos")]
extern "C" fn invoke_void_bool_block(b: *mut BlockLayout, val: bool) {
    if b.is_null() {
        return;
    }
    let closure_ptr = unsafe { (*b).closure };
    if closure_ptr == 0 {
        return;
    }
    unsafe {
        let fn_ptr = *(closure_ptr as *const usize);
        let f: extern "C" fn(bool, i64) = std::mem::transmute(fn_ptr);
        f(val, closure_ptr);
    }
}

#[cfg(target_os = "macos")]
extern "C" fn invoke_void_three_obj_block(
    b: *mut BlockLayout, a: i64, c: i64, d: i64,
) {
    if b.is_null() {
        return;
    }
    let closure_ptr = unsafe { (*b).closure };
    if closure_ptr == 0 {
        return;
    }
    unsafe {
        let fn_ptr = *(closure_ptr as *const usize);
        let f: extern "C" fn(i64, i64, i64, i64) = std::mem::transmute(fn_ptr);
        f(a, c, d, closure_ptr);
    }
}

#[cfg(target_os = "macos")]
extern "C" fn copy_helper(dst: *mut c_void, src: *const c_void) {
    let src_b = src as *const BlockLayout;
    let dst_b = dst as *mut BlockLayout;
    let closure_ptr = unsafe { (*src_b).closure };
    unsafe {
        (*dst_b).closure = closure_ptr;
    }
    if closure_ptr != 0 {
        unsafe {
            let rc_ptr = (closure_ptr + 8) as *mut i64;
            crate::refcount::atomic_retain(rc_ptr);
        }
    }
}

#[cfg(target_os = "macos")]
extern "C" fn dispose_helper(src: *const c_void) {
    let src_b = src as *const BlockLayout;
    let closure_ptr = unsafe { (*src_b).closure };
    if closure_ptr == 0 {
        return;
    }
    unsafe {
        let rc_ptr = (closure_ptr + 8) as *mut i64;
        if let Some(0) = crate::refcount::atomic_release(rc_ptr) {
            // Closure refcount hit zero. The closure layout is
            // [fn_ptr | rc | captures…]; ilang's closure
            // allocator sizes it as `(2 + n_caps) * 8`. We don't
            // know n_caps here without recording it on the block.
            // For now release with size 16 (works for closures
            // with zero captures). Closures with captures may
            // leak a per-block capture region until we record the
            // closure's allocation size on the block.
            __mir_free(closure_ptr, 16);
            let _ = &__mir_alloc;
        }
    }
}

// Static descriptors — `BlockDescriptor`'s raw signature pointer
// doesn't implement `Sync`, so wrap them. The pointers are
// read-only across threads, so the wrapper is safe in practice.
#[cfg(target_os = "macos")]
struct DescriptorBox(BlockDescriptor);
#[cfg(target_os = "macos")]
unsafe impl Sync for DescriptorBox {}

#[cfg(target_os = "macos")]
static VOID_DESCRIPTOR: DescriptorBox = DescriptorBox(BlockDescriptor {
    reserved: 0,
    size: std::mem::size_of::<BlockLayout>(),
    copy_helper,
    dispose_helper,
    signature: VOID_VOID_SIGNATURE.as_ptr(),
});

#[cfg(target_os = "macos")]
static OBJ_DESCRIPTOR: DescriptorBox = DescriptorBox(BlockDescriptor {
    reserved: 0,
    size: std::mem::size_of::<BlockLayout>(),
    copy_helper,
    dispose_helper,
    signature: VOID_OBJ_SIGNATURE.as_ptr(),
});

#[cfg(target_os = "macos")]
static OBJ_TO_OBJ_DESCRIPTOR: DescriptorBox = DescriptorBox(BlockDescriptor {
    reserved: 0,
    size: std::mem::size_of::<BlockLayout>(),
    copy_helper,
    dispose_helper,
    signature: OBJ_TO_OBJ_SIGNATURE.as_ptr(),
});

#[cfg(target_os = "macos")]
static VOID_BYTES_DESCRIPTOR: DescriptorBox = DescriptorBox(BlockDescriptor {
    reserved: 0,
    size: std::mem::size_of::<BlockLayout>(),
    copy_helper,
    dispose_helper,
    signature: VOID_BYTES_SIGNATURE.as_ptr(),
});

#[cfg(target_os = "macos")]
static VOID_THREE_OBJ_DESCRIPTOR: DescriptorBox = DescriptorBox(BlockDescriptor {
    reserved: 0,
    size: std::mem::size_of::<BlockLayout>(),
    copy_helper,
    dispose_helper,
    signature: VOID_THREE_OBJ_SIGNATURE.as_ptr(),
});

#[cfg(target_os = "macos")]
static VOID_BOOL_DESCRIPTOR: DescriptorBox = DescriptorBox(BlockDescriptor {
    reserved: 0,
    size: std::mem::size_of::<BlockLayout>(),
    copy_helper,
    dispose_helper,
    signature: VOID_BOOL_SIGNATURE.as_ptr(),
});

#[cfg(target_os = "macos")]
static VOID_TWO_OBJ_DESCRIPTOR: DescriptorBox = DescriptorBox(BlockDescriptor {
    reserved: 0,
    size: std::mem::size_of::<BlockLayout>(),
    copy_helper,
    dispose_helper,
    signature: VOID_TWO_OBJ_SIGNATURE.as_ptr(),
});

/// Read the block's `invoke` slot (offset 16 in Block_layout) and
/// call it with the block as the sole argument. The standard
/// `void(^)(void)` calling convention. Exposed for unit-test
/// drivers; production code hands the block to ObjC methods that
/// own the invocation.
#[unsafe(export_name = "__ilang_invoke_void_block")]
pub extern "C" fn invoke_void_block_via_runtime(block_ptr: i64) {
    if block_ptr == 0 {
        return;
    }
    unsafe {
        let invoke_slot = (block_ptr + 16) as *const usize;
        let invoke_addr = *invoke_slot;
        if invoke_addr == 0 {
            return;
        }
        let f: extern "C" fn(i64) = std::mem::transmute(invoke_addr);
        f(block_ptr);
    }
}

/// Same idea for `void(^)(id)` — invoke the block with the given
/// raw id argument. Test-only driver for `make_obj_block`.
#[unsafe(export_name = "__ilang_invoke_obj_block")]
pub extern "C" fn invoke_obj_block_via_runtime(block_ptr: i64, arg: i64) {
    if block_ptr == 0 {
        return;
    }
    unsafe {
        let invoke_slot = (block_ptr + 16) as *const usize;
        let invoke_addr = *invoke_slot;
        if invoke_addr == 0 {
            return;
        }
        let f: extern "C" fn(i64, i64) = std::mem::transmute(invoke_addr);
        f(block_ptr, arg);
    }
}

/// Same idea for `id(^)(id)` — invoke and return the result.
/// Test-only driver for `make_obj_to_obj_block`.
#[unsafe(export_name = "__ilang_invoke_obj_to_obj_block")]
pub extern "C" fn invoke_obj_to_obj_block_via_runtime(block_ptr: i64, arg: i64) -> i64 {
    if block_ptr == 0 {
        return 0;
    }
    unsafe {
        let invoke_slot = (block_ptr + 16) as *const usize;
        let invoke_addr = *invoke_slot;
        if invoke_addr == 0 {
            return 0;
        }
        let f: extern "C" fn(i64, i64) -> i64 = std::mem::transmute(invoke_addr);
        f(block_ptr, arg)
    }
}

/// `void(^)(void *, size_t)` invoker. Both extra args are passed
/// straight through as i64 — the block's invoke trampoline reads
/// them with the C ABI's natural register layout.
#[unsafe(export_name = "__ilang_invoke_void_bytes_block")]
pub extern "C" fn invoke_void_bytes_block_via_runtime(
    block_ptr: i64, ptr: i64, len: i64,
) {
    if block_ptr == 0 {
        return;
    }
    unsafe {
        let invoke_slot = (block_ptr + 16) as *const usize;
        let invoke_addr = *invoke_slot;
        if invoke_addr == 0 {
            return;
        }
        let f: extern "C" fn(i64, i64, i64) = std::mem::transmute(invoke_addr);
        f(block_ptr, ptr, len);
    }
}

/// `void(^)(id, id, id)` invoker — three raw handles forwarded
/// to the block's trampoline. Used by callers that want to
/// trigger an incoming completion handler delivered through a
/// delegate slot.
#[unsafe(export_name = "__ilang_invoke_void_three_obj_block")]
pub extern "C" fn invoke_void_three_obj_block_via_runtime(
    block_ptr: i64, a: i64, b: i64, c: i64,
) {
    if block_ptr == 0 {
        return;
    }
    unsafe {
        let invoke_slot = (block_ptr + 16) as *const usize;
        let invoke_addr = *invoke_slot;
        if invoke_addr == 0 {
            return;
        }
        let f: extern "C" fn(i64, i64, i64, i64) = std::mem::transmute(invoke_addr);
        f(block_ptr, a, b, c);
    }
}

/// `void(^)(BOOL)` invoker. The single `val` is a Rust `bool`
/// (1-byte) to match the block ABI on macOS.
#[unsafe(export_name = "__ilang_invoke_void_bool_block")]
pub extern "C" fn invoke_void_bool_block_via_runtime(block_ptr: i64, val: bool) {
    if block_ptr == 0 {
        return;
    }
    unsafe {
        let invoke_slot = (block_ptr + 16) as *const usize;
        let invoke_addr = *invoke_slot;
        if invoke_addr == 0 {
            return;
        }
        let f: extern "C" fn(i64, bool) = std::mem::transmute(invoke_addr);
        f(block_ptr, val);
    }
}

/// Shared allocation path for every `make_*_block` flavour. Builds
/// a heap block with the given `invoke` trampoline + descriptor,
/// returns it autoreleased.
#[cfg(target_os = "macos")]
fn make_block(
    closure_ptr: i64,
    invoke: *const c_void,
    descriptor: &'static BlockDescriptor,
) -> i64 {
    let size = std::mem::size_of::<BlockLayout>() as i64;
    let raw = __mir_alloc(size);
    if raw == 0 {
        return 0;
    }
    let b = raw as *mut BlockLayout;
    unsafe {
        (*b).isa = &_NSConcreteMallocBlock as *const _ as *mut c_void;
        (*b).flags = BLOCK_NEEDS_FREE
            | BLOCK_HAS_COPY_DISPOSE
            | BLOCK_HAS_SIGNATURE
            | BLOCK_RC_ONE;
        (*b).reserved = 0;
        (*b).invoke = invoke;
        (*b).descriptor = descriptor;
        (*b).closure = closure_ptr;
        // Bump the closure refcount so it outlives the ilang local
        // that handed it to us — the block now owns a +1 the
        // dispose helper drops.
        let rc_ptr = (closure_ptr + 8) as *mut i64;
        crate::refcount::atomic_retain(rc_ptr);
        objc_autorelease(b as *mut c_void) as i64
    }
}

/// Build a heap ObjC `void(^)(void)` block whose `invoke`
/// trampoline calls `closure_ptr` (an ilang `fn(): unit` value).
/// Comes back autoreleased — pass straight to ObjC APIs that
/// take a completion handler.
#[unsafe(export_name = "__ilang_make_void_block")]
pub extern "C" fn make_void_block(closure_ptr: i64) -> i64 {
    if closure_ptr == 0 {
        return 0;
    }
    #[cfg(target_os = "macos")]
    {
        return make_block(
            closure_ptr,
            invoke_void_block as *const c_void,
            &VOID_DESCRIPTOR.0,
        );
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = closure_ptr;
        0
    }
}

/// Build a heap ObjC `void(^)(id)` block whose trampoline calls
/// `closure_ptr` with the raw `id` argument as `i64`. The ilang
/// closure should be `fn(handle: i64): unit`; use `NSObject.wrap`
/// (or a subclass equivalent) inside if you want a typed view.
#[unsafe(export_name = "__ilang_make_obj_block")]
pub extern "C" fn make_obj_block(closure_ptr: i64) -> i64 {
    if closure_ptr == 0 {
        return 0;
    }
    #[cfg(target_os = "macos")]
    {
        return make_block(
            closure_ptr,
            invoke_obj_block as *const c_void,
            &OBJ_DESCRIPTOR.0,
        );
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = closure_ptr;
        0
    }
}

/// Build a heap ObjC `id(^)(id)` block. The ilang closure is
/// `fn(handle: i64): i64` — returns a raw `id` (or 0 for nil).
/// Used for monitoring APIs (`addLocalMonitorForEventsMatchingMask
/// :handler:`) where the handler decides whether to forward,
/// replace, or swallow the event.
#[unsafe(export_name = "__ilang_make_obj_to_obj_block")]
pub extern "C" fn make_obj_to_obj_block(closure_ptr: i64) -> i64 {
    if closure_ptr == 0 {
        return 0;
    }
    #[cfg(target_os = "macos")]
    {
        return make_block(
            closure_ptr,
            invoke_obj_to_obj_block as *const c_void,
            &OBJ_TO_OBJ_DESCRIPTOR.0,
        );
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = closure_ptr;
        0
    }
}

/// Build a heap ObjC `void(^)(void *, size_t)` block. The ilang
/// closure is `fn(ptr: i64, len: i64): unit` — receives a raw
/// bytes pointer and length so the body can mutate the buffer
/// in-place via `writeU8` etc.
#[unsafe(export_name = "__ilang_make_void_bytes_block")]
pub extern "C" fn make_void_bytes_block(closure_ptr: i64) -> i64 {
    if closure_ptr == 0 {
        return 0;
    }
    #[cfg(target_os = "macos")]
    {
        return make_block(
            closure_ptr,
            invoke_void_bytes_block as *const c_void,
            &VOID_BYTES_DESCRIPTOR.0,
        );
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = closure_ptr;
        0
    }
}

/// Build a heap ObjC `void(^)(id, id)` block. The ilang closure
/// is `fn(a: i64, b: i64): unit` (or with NSObject-shaped
/// params) — the two args land in the closure verbatim as raw
/// `id` handles. Differs from `make_void_bytes_block` only in
/// the NSMethodSignature encoding string.
#[unsafe(export_name = "__ilang_make_void_two_obj_block")]
pub extern "C" fn make_void_two_obj_block(closure_ptr: i64) -> i64 {
    if closure_ptr == 0 {
        return 0;
    }
    #[cfg(target_os = "macos")]
    {
        return make_block(
            closure_ptr,
            invoke_void_two_obj_block as *const c_void,
            &VOID_TWO_OBJ_DESCRIPTOR.0,
        );
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = closure_ptr;
        0
    }
}

/// Build a heap ObjC `void(^)(BOOL)` block. The ilang closure is
/// `fn(b: bool): unit`. Used by completion handlers that report
/// a single success / failure flag (e.g.
/// `NSExtensionContext.openURL:completionHandler:`).
#[unsafe(export_name = "__ilang_make_void_bool_block")]
pub extern "C" fn make_void_bool_block(closure_ptr: i64) -> i64 {
    if closure_ptr == 0 {
        return 0;
    }
    #[cfg(target_os = "macos")]
    {
        return make_block(
            closure_ptr,
            invoke_void_bool_block as *const c_void,
            &VOID_BOOL_DESCRIPTOR.0,
        );
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = closure_ptr;
        0
    }
}

/// Build a heap ObjC `void(^)(id, id, id)` block. The ilang
/// closure is `fn(a: i64, b: i64, c: i64): unit` — receives three
/// raw `id` handles. Used by `NSURLSession`'s
/// `dataTaskWithRequest:completionHandler:` family, where the
/// trio is `(NSData *, NSURLResponse *, NSError *)`.
#[unsafe(export_name = "__ilang_make_void_three_obj_block")]
pub extern "C" fn make_void_three_obj_block(closure_ptr: i64) -> i64 {
    if closure_ptr == 0 {
        return 0;
    }
    #[cfg(target_os = "macos")]
    {
        return make_block(
            closure_ptr,
            invoke_void_three_obj_block as *const c_void,
            &VOID_THREE_OBJ_DESCRIPTOR.0,
        );
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = closure_ptr;
        0
    }
}

// ─── Unified dispatcher for `new ObjCBlock(closure)` ────────────────
//
// MIR lowering of `new ObjCBlock(closure)` emits a single call to
// this entry point with a `kind` selector chosen from the closure's
// inspected ilang fn signature. The selector picks one of the
// per-shape `make_*_block` paths above; if the inspected shape
// doesn't match any pre-baked invoke trampoline, the lowerer
// errors out at compile time and this dispatcher never sees an
// unknown kind. We still guard the default arm so an obviously
// bogus value can't silently autorelease a zero handle.
//
// Kind values are stable — they're baked into MIR by the lowerer,
// so adding new shapes must append rather than re-number.

/// Stable kind codes mapped to the existing per-shape helpers
/// above. Append new shapes at the end so MIR programs compiled
/// against older builds keep matching the same trampoline.
#[repr(i64)]
pub enum BlockKind {
    /// `fn(): ()` — completion handler with no arguments.
    Void = 0,
    /// `fn(i64): ()` — single `id` argument, no return.
    Obj = 1,
    /// `fn(i64): i64` — single `id` argument, returns `id` (monitor
    /// callbacks where the handler decides to forward / replace /
    /// swallow).
    ObjToObj = 2,
    /// `fn(i64, i64): ()` — raw bytes pointer + length (e.g.
    /// `SKMutableTexture.modifyPixelDataWithBlock:`).
    VoidBytes = 3,
    /// `fn(i64, i64, i64): ()` — three `id` arguments. Used by
    /// every `NSURLSession` completion handler family.
    VoidThreeObj = 4,
    /// `fn(bool): ()` — single BOOL argument. Used by completion
    /// handlers that report success / failure (e.g.
    /// `NSExtensionContext.openURL:completionHandler:`).
    VoidBool = 5,
    /// `fn(i64, i64): ()` with NSObject-typed params — two id
    /// arguments. Shares ABI with VoidBytes but pairs with the
    /// proper `(id, id)` ObjC signature for receivers that
    /// introspect the block.
    VoidTwoObj = 6,
}

#[unsafe(export_name = "__ilang_make_objc_block")]
pub extern "C" fn make_objc_block(closure_ptr: i64, kind: i64) -> i64 {
    match kind {
        0 => make_void_block(closure_ptr),
        1 => make_obj_block(closure_ptr),
        2 => make_obj_to_obj_block(closure_ptr),
        3 => make_void_bytes_block(closure_ptr),
        4 => make_void_three_obj_block(closure_ptr),
        5 => make_void_bool_block(closure_ptr),
        6 => make_void_two_obj_block(closure_ptr),
        _ => 0,
    }
}

// ─── NSError ** out-parameter slot ─────────────────────────────────
//
// Most error-bearing @objc methods (`newLibraryWithSource:options:error:`
// etc.) take an `NSError **` out-parameter. ilang's `&local`
// syntax is only legal inside `@extern(C) {}` bodies, which the
// public binding wrappers we expose to user code aren't — so we
// host the slot on the runtime side and hand its address out via
// `__ilang_objc_err_slot_ptr`. After the call returns,
// `__ilang_objc_take_err` reads + clears the slot so a subsequent
// call doesn't see a stale value. Thread-local so concurrent
// callers get their own slot.
use std::cell::Cell;

thread_local! {
    static OBJC_ERR_SLOT: Cell<i64> = const { Cell::new(0) };
}

#[unsafe(export_name = "__ilang_objc_err_slot_ptr")]
pub extern "C" fn objc_err_slot_ptr() -> i64 {
    OBJC_ERR_SLOT.with(|c| c.as_ptr() as i64)
}

#[unsafe(export_name = "__ilang_objc_take_err")]
pub extern "C" fn objc_take_err() -> i64 {
    OBJC_ERR_SLOT.with(|c| c.replace(0))
}
