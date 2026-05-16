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

use std::ffi::c_void;

use crate::alloc::{__mir_alloc, __mir_free};

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

#[repr(C)]
struct BlockDescriptor {
    reserved: usize,
    size: usize,
    copy_helper: extern "C" fn(*mut c_void, *const c_void),
    dispose_helper: extern "C" fn(*const c_void),
    signature: *const u8,
}

// ObjC runtime symbols we lean on.
#[cfg(target_os = "macos")]
#[link(name = "objc")]
unsafe extern "C" {
    static _NSConcreteMallocBlock: *mut c_void;
    fn objc_autorelease(obj: *mut c_void) -> *mut c_void;
}

const BLOCK_NEEDS_FREE: i32 = 1 << 24;
const BLOCK_HAS_COPY_DISPOSE: i32 = 1 << 25;
const BLOCK_HAS_SIGNATURE: i32 = 1 << 30;
// `rc = 1` lives in bits 1..16 (bit 0 is the deallocating flag).
const BLOCK_RC_ONE: i32 = 1 << 1;

// `void(^)(void)` — encoding: "v8@?0" → return void, total args 8 bytes,
// arg 0 is the block itself (type `@?` for "block id"). Apple's
// runtime checks the size prefix loosely; the encoding matters for
// NSMethodSignature-based introspection.
static VOID_VOID_SIGNATURE: &[u8] = b"v8@?0\0";
// `void(^)(id)` — block + one id arg = 16 bytes total. `@8` says
// the second arg is an id starting at offset 8 in the arg frame.
static VOID_OBJ_SIGNATURE: &[u8] = b"v16@?0@8\0";
// `id(^)(id)` — return id, block + id args.
static OBJ_TO_OBJ_SIGNATURE: &[u8] = b"@16@?0@8\0";

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
struct DescriptorBox(BlockDescriptor);
unsafe impl Sync for DescriptorBox {}

static VOID_DESCRIPTOR: DescriptorBox = DescriptorBox(BlockDescriptor {
    reserved: 0,
    size: std::mem::size_of::<BlockLayout>(),
    copy_helper,
    dispose_helper,
    signature: VOID_VOID_SIGNATURE.as_ptr(),
});

static OBJ_DESCRIPTOR: DescriptorBox = DescriptorBox(BlockDescriptor {
    reserved: 0,
    size: std::mem::size_of::<BlockLayout>(),
    copy_helper,
    dispose_helper,
    signature: VOID_OBJ_SIGNATURE.as_ptr(),
});

static OBJ_TO_OBJ_DESCRIPTOR: DescriptorBox = DescriptorBox(BlockDescriptor {
    reserved: 0,
    size: std::mem::size_of::<BlockLayout>(),
    copy_helper,
    dispose_helper,
    signature: OBJ_TO_OBJ_SIGNATURE.as_ptr(),
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
        (*b).isa = _NSConcreteMallocBlock;
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
