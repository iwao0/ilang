//! Minimal Objective-C block creation for ilang.
//!
//! Builds a heap block whose `invoke` trampoline calls an ilang
//! closure value. The block is autoreleased (via the ObjC ARC
//! runtime's `objc_autorelease`) so callers can pass it straight to
//! APIs that expect a completion handler without manual
//! `Block_release` bookkeeping.
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
//! The copy / dispose helpers bump / drop the captured ilang
//! closure's refcount so the closure outlives the block when ObjC's
//! `Block_copy` snapshots us for later invocation.
//!
//! Each supported `void(^)(...)` / `id(^)(...)` shape is generated
//! by `define_block_shape!` below — one macro invocation produces
//! the signature bytes, the static `BlockDescriptor`, the C-ABI
//! invoke trampoline, the user-facing `$objc.make_*` symbol, and
//! the test-side `$objc.invoke_*` driver. Adding a new shape means
//! a single new `define_block_shape!` block.

#[cfg(target_os = "macos")]
use std::ffi::c_void;

#[cfg(target_os = "macos")]
use crate::alloc::__mir_alloc;
#[cfg(target_os = "macos")]
use crate::alloc::__mir_free;

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

// ────────────────────────────────────────────────────────────────────
// `define_block_shape!` — one macro invocation produces the five
// items every supported block flavour needs:
//
//   1. The C `_NSMethodSignature`-compatible encoding bytes.
//   2. A `static DescriptorBox` wiring up copy / dispose / signature.
//   3. The C-ABI `invoke_*` trampoline that decodes the captured
//      closure and dispatches with the (user_args…, closure_ptr)
//      trailing-env calling convention.
//   4. The user-facing `$objc.make_*` export that allocates the
//      block + autoreleases it.
//   5. The test-side `$objc.invoke_*` export that reads the block's
//      invoke slot and dispatches the block as the C ABI demands.
//
// `args` lists the user-visible parameters in source order — the
// trampoline appends `closure_ptr` automatically when synthesising
// the call to the lifted ilang fn. `default` is the early-return
// value used when the block or its captured closure is null;
// callers that don't return a value pass `()`.
// ────────────────────────────────────────────────────────────────────

macro_rules! define_block_shape {
    (
        signature: $sig_bytes:expr,
        sig_static: $sig_static:ident,
        descriptor: $descriptor:ident,
        invoke_trampoline: $invoke:ident($($arg:ident: $arg_ty:ty),* $(,)?)
            $(-> $ret_ty:ty)? = $default:expr,
        make_fn: $make_fn:ident @ $make_export:literal,
        invoke_runtime_fn: $invoke_rt:ident @ $invoke_rt_export:literal,
    ) => {
        #[cfg(target_os = "macos")]
        static $sig_static: &[u8] = $sig_bytes;

        #[cfg(target_os = "macos")]
        static $descriptor: DescriptorBox = DescriptorBox(BlockDescriptor {
            reserved: 0,
            size: std::mem::size_of::<BlockLayout>(),
            copy_helper,
            dispose_helper,
            signature: $sig_static.as_ptr(),
        });

        #[cfg(target_os = "macos")]
        extern "C" fn $invoke(b: *mut BlockLayout, $($arg: $arg_ty),*) $(-> $ret_ty)? {
            if b.is_null() {
                return $default;
            }
            let closure_ptr = unsafe { (*b).closure };
            if closure_ptr == 0 {
                return $default;
            }
            // Closure layout: [fn_ptr | rc | captures…]. The lifted
            // ilang fn takes `(user_args…, env)` — env is the *last*
            // clif param, mirroring lower_function's `env_value`.
            unsafe {
                let fn_ptr = *(closure_ptr as *const usize);
                let f: extern "C" fn($($arg_ty,)* i64) $(-> $ret_ty)? =
                    std::mem::transmute(fn_ptr);
                f($($arg,)* closure_ptr)
            }
        }

        #[unsafe(export_name = $make_export)]
        pub extern "C" fn $make_fn(closure_ptr: i64) -> i64 {
            if closure_ptr == 0 {
                return 0;
            }
            #[cfg(target_os = "macos")]
            {
                return make_block(
                    closure_ptr,
                    $invoke as *const c_void,
                    &$descriptor.0,
                );
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = closure_ptr;
                0
            }
        }

        #[unsafe(export_name = $invoke_rt_export)]
        pub extern "C" fn $invoke_rt(block_ptr: i64, $($arg: $arg_ty),*) $(-> $ret_ty)? {
            if block_ptr == 0 {
                return $default;
            }
            unsafe {
                let invoke_slot = (block_ptr + 16) as *const usize;
                let invoke_addr = *invoke_slot;
                if invoke_addr == 0 {
                    return $default;
                }
                let f: extern "C" fn(i64, $($arg_ty),*) $(-> $ret_ty)? =
                    std::mem::transmute(invoke_addr);
                f(block_ptr, $($arg),*)
            }
        }
    };
}

// ─── void(^)(void) ────────────────────────────────────────────────
// Encoding `v8@?0` → return void; arg frame = 8 bytes (just the
// block id `@?` at offset 0). The simplest shape; used by APIs
// that take a no-arg completion handler.
define_block_shape! {
    signature: b"v8@?0\0",
    sig_static: VOID_VOID_SIGNATURE,
    descriptor: VOID_DESCRIPTOR,
    invoke_trampoline: invoke_void_block() = (),
    make_fn: make_void_block @ "$objc.make_void_block",
    invoke_runtime_fn: invoke_void_block_via_runtime @ "$objc.invoke_void_block",
}

// ─── void(^)(id) ──────────────────────────────────────────────────
// Encoding `v16@?0@8` → return void; block at offset 0, one `id`
// argument at offset 8. The ilang closure is `fn(handle: i64): ()`;
// users wrap with `NSObject.wrap` for a typed view.
define_block_shape! {
    signature: b"v16@?0@8\0",
    sig_static: VOID_OBJ_SIGNATURE,
    descriptor: OBJ_DESCRIPTOR,
    invoke_trampoline: invoke_obj_block(arg: i64) = (),
    make_fn: make_obj_block @ "$objc.make_obj_block",
    invoke_runtime_fn: invoke_obj_block_via_runtime @ "$objc.invoke_obj_block",
}

// ─── id(^)(id) ────────────────────────────────────────────────────
// Encoding `@16@?0@8` → return id; block + id args. Used by
// monitor / filter APIs (`addLocalMonitorForEventsMatchingMask:
// handler:`) where the handler returns nil to swallow the event.
define_block_shape! {
    signature: b"@16@?0@8\0",
    sig_static: OBJ_TO_OBJ_SIGNATURE,
    descriptor: OBJ_TO_OBJ_DESCRIPTOR,
    invoke_trampoline: invoke_obj_to_obj_block(arg: i64) -> i64 = 0,
    make_fn: make_obj_to_obj_block @ "$objc.make_obj_to_obj_block",
    invoke_runtime_fn: invoke_obj_to_obj_block_via_runtime @ "$objc.invoke_obj_to_obj_block",
}

// ─── void(^)(void *, size_t) ──────────────────────────────────────
// Encoding `v24@?0^v8Q16` → return void; block@0 (8B), `void *`@8
// (8B), `unsigned long long`@16 (8B) = 24B total. Used by
// `SKMutableTexture.modifyPixelDataWithBlock:` and the like, where
// the callback mutates a raw byte buffer in place via readU8 /
// writeU8.
define_block_shape! {
    signature: b"v24@?0^v8Q16\0",
    sig_static: VOID_BYTES_SIGNATURE,
    descriptor: VOID_BYTES_DESCRIPTOR,
    invoke_trampoline: invoke_void_bytes_block(ptr: i64, len: i64) = (),
    make_fn: make_void_bytes_block @ "$objc.make_void_bytes_block",
    invoke_runtime_fn: invoke_void_bytes_block_via_runtime @ "$objc.invoke_void_bytes_block",
}

// ─── void(^)(id, id, id) ──────────────────────────────────────────
// Encoding `v32@?0@8@16@24` → return void; block + three id args.
// Used by `NSURLSession`'s `dataTaskWithRequest:completionHandler:`
// family — the trio is `(NSData *, NSURLResponse *, NSError *)`.
define_block_shape! {
    signature: b"v32@?0@8@16@24\0",
    sig_static: VOID_THREE_OBJ_SIGNATURE,
    descriptor: VOID_THREE_OBJ_DESCRIPTOR,
    invoke_trampoline: invoke_void_three_obj_block(a: i64, b: i64, c: i64) = (),
    make_fn: make_void_three_obj_block @ "$objc.make_void_three_obj_block",
    invoke_runtime_fn: invoke_void_three_obj_block_via_runtime @ "$objc.invoke_void_three_obj_block",
}

// ─── void(^)(BOOL) ────────────────────────────────────────────────
// Encoding `v16@?0c8` → return void; block + BOOL (`c` is signed
// char, the encoding for BOOL on both macOS x86_64 and arm64).
// Used by single-flag completion handlers (e.g.
// `NSExtensionContext.openURL:completionHandler:`).
define_block_shape! {
    signature: b"v16@?0c8\0",
    sig_static: VOID_BOOL_SIGNATURE,
    descriptor: VOID_BOOL_DESCRIPTOR,
    invoke_trampoline: invoke_void_bool_block(val: bool) = (),
    make_fn: make_void_bool_block @ "$objc.make_void_bool_block",
    invoke_runtime_fn: invoke_void_bool_block_via_runtime @ "$objc.invoke_void_bool_block",
}

// ─── void(^)(id, id) ──────────────────────────────────────────────
// Encoding `v24@?0@8@16` → same arg-frame size as `void_bytes` but
// the NSMethodSignature correctly says "id, id" so receivers that
// introspect (NSInvocation etc.) see the right argument kinds.
define_block_shape! {
    signature: b"v24@?0@8@16\0",
    sig_static: VOID_TWO_OBJ_SIGNATURE,
    descriptor: VOID_TWO_OBJ_DESCRIPTOR,
    invoke_trampoline: invoke_void_two_obj_block(a: i64, b: i64) = (),
    make_fn: make_void_two_obj_block @ "$objc.make_void_two_obj_block",
    invoke_runtime_fn: invoke_void_two_obj_block_via_runtime @ "$objc.invoke_void_two_obj_block",
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

#[unsafe(export_name = "$objc.make_block")]
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

#[unsafe(export_name = "$objc.err_slot_ptr")]
pub extern "C" fn objc_err_slot_ptr() -> i64 {
    OBJC_ERR_SLOT.with(|c| c.as_ptr() as i64)
}

#[unsafe(export_name = "$objc.take_err")]
pub extern "C" fn objc_take_err() -> i64 {
    OBJC_ERR_SLOT.with(|c| c.replace(0))
}
