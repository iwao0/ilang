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

// Static descriptor — `BlockDescriptor`'s raw signature pointer
// doesn't implement `Sync`, so wrap it. The pointer is read-only
// across threads, so the wrapper is safe in practice.
struct DescriptorBox(BlockDescriptor);
unsafe impl Sync for DescriptorBox {}

static DESCRIPTOR: DescriptorBox = DescriptorBox(BlockDescriptor {
    reserved: 0,
    size: std::mem::size_of::<BlockLayout>(),
    copy_helper,
    dispose_helper,
    signature: VOID_VOID_SIGNATURE.as_ptr(),
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

/// Build a heap ObjC block that calls `closure_ptr` (an ilang
/// `fn(): unit` value) when invoked. The block comes back
/// autoreleased — pass it straight to an ObjC method that wants
/// a `void (^)(void)`; the surrounding autorelease pool drains it
/// once nobody's holding a `Block_copy`.
///
/// Returns `0` on macOS-only-symbol-missing builds (non-Apple
/// targets) or when `closure_ptr` is null.
#[unsafe(export_name = "__ilang_make_void_block")]
pub extern "C" fn make_void_block(closure_ptr: i64) -> i64 {
    if closure_ptr == 0 {
        return 0;
    }
    #[cfg(target_os = "macos")]
    {
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
            (*b).invoke = invoke_void_block as *const c_void;
            (*b).descriptor = &DESCRIPTOR.0;
            (*b).closure = closure_ptr;
            // Bump the closure refcount so it outlives the
            // ilang local that handed it to us — the block now
            // owns a +1 reference that the dispose helper drops.
            let rc_ptr = (closure_ptr + 8) as *mut i64;
            crate::refcount::atomic_retain(rc_ptr);
            // Hand off to the ObjC autorelease pool. Callers
            // don't need to think about `Block_release`.
            objc_autorelease(b as *mut c_void) as i64
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = closure_ptr;
        0
    }
}
