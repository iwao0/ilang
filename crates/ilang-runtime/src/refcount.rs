//! Atomic refcount helpers for the per-heap-type `__retain_*` /
//! `__release_*` primitives. The runtime stores the rc as a plain
//! i64 in the heap header (or as a struct field for the registry-
//! backed types) — the layout is identical to `AtomicI64`, so we
//! just reinterpret the slot here.
//!
//! Why atomic: the work-stealing thread pool that drives Promise
//! executors lets heap values cross thread boundaries. If retain /
//! release stayed non-atomic, two workers racing on the same value
//! would tear the count and double-free or leak.
//!
//! Ordering follows the canonical `Arc` recipe:
//! - retain (`fetch_add(Relaxed)`) — increments don't synchronise
//!   anything; the caller already holds a +1 reference, so the
//!   value can't disappear under us.
//! - release (`fetch_sub(Release)`) — pairs with the Acquire fence
//!   on the 1→0 transition, ensuring all prior writes to the
//!   payload happen-before the destructor runs.
//!
//! Both helpers preserve the legacy "rc <= 0 → skip" semantic via
//! a CAS loop. Existing callers use that to no-op on statically
//! allocated objects (literal strings, etc.) where the slot is
//! never refcounted.

use std::sync::atomic::{AtomicI64, Ordering, fence};

/// Increment the rc through `atomic`. No-op when the current value
/// is `<= 0` (the slot isn't refcounted — see module doc).
#[inline]
pub fn retain_atomic(atomic: &AtomicI64) {
    let mut cur = atomic.load(Ordering::Relaxed);
    loop {
        if cur <= 0 {
            return;
        }
        match atomic.compare_exchange_weak(
            cur,
            cur + 1,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return,
            Err(actual) => cur = actual,
        }
    }
}

/// Decrement the rc through `atomic`. Returns `Some(new_value)` if a
/// decrement happened (caller frees when it equals 0), `None` when
/// the slot is non-refcounted (rc was `<= 0`). The 1→0 transition
/// emits the Acquire fence that pairs with the Release on the CAS.
#[inline]
pub fn release_atomic(atomic: &AtomicI64) -> Option<i64> {
    let mut cur = atomic.load(Ordering::Relaxed);
    loop {
        if cur <= 0 {
            return None;
        }
        let new = cur - 1;
        match atomic.compare_exchange_weak(
            cur,
            new,
            Ordering::Release,
            Ordering::Relaxed,
        ) {
            Ok(_) => {
                if new == 0 {
                    fence(Ordering::Acquire);
                }
                return Some(new);
            }
            Err(actual) => cur = actual,
        }
    }
}

/// Increment the rc at `rc_ptr`. No-op when the current value is
/// `<= 0` (the slot isn't refcounted).
#[inline]
pub unsafe fn atomic_retain(rc_ptr: *mut i64) {
    retain_atomic(unsafe { &*(rc_ptr as *const AtomicI64) });
}

/// Decrement the rc at `rc_ptr`. Returns `Some(new_value)` if a
/// decrement happened (caller frees when it equals 0), `None` when
/// the slot is non-refcounted (rc was `<= 0`).
#[inline]
pub unsafe fn atomic_release(rc_ptr: *mut i64) -> Option<i64> {
    release_atomic(unsafe { &*(rc_ptr as *const AtomicI64) })
}
