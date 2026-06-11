//! `setTimeout` / `clearTimeout` / `setInterval` / `clearInterval`
//! backing `libs/std/time.il`. JS-style semantics:
//!
//! - `setTimeout(cb, ms)` schedules `cb` to run once after `ms`
//!   milliseconds. Returns a non-zero `i64` id.
//! - `setInterval(cb, ms)` schedules `cb` to run every `ms`
//!   milliseconds until cancelled. Returns the id.
//! - `clearTimeout(id)` / `clearInterval(id)` cancels a pending
//!   timer — once cancelled it never fires again.
//!
//! Callbacks fire on the main thread at a drain point (`pool.rs`'s
//! timer heap): the end-of-program drain — already wired into
//! `run_main` — sleeps until each due time and fires, so a
//! `setTimeout` outstanding at the moment `main` returns gets to
//! run before the process exits. Apps that own their main loop
//! service due timers per-frame via `time.tick()` (`pool::pump`).
//! A `setInterval` that's never `clearInterval`d will keep the
//! process alive indefinitely; this matches Node.js.

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use crate::closures::{__release_closure, __retain_closure};
use crate::pool;

/// `0` is reserved as "no timer" — both setTimeout and setInterval
/// return `0` on a null callback so callers can use the returned id
/// as a truthy check.
static NEXT_ID: AtomicI64 = AtomicI64::new(1);

/// Owns the +1 retain taken on the callback cell at schedule time.
/// The timer task moves the guard into its closure, so the release
/// happens exactly when the pool drops the entry — after a one-shot
/// fires, or when a cancelled entry is discarded.
struct ClosureGuard(i64);

impl ClosureGuard {
    /// Accessor instead of direct `.0` field reads inside the timer
    /// task: a `move` closure touching only `guard.0` would capture
    /// just the (Copy) i64 under Rust 2021 disjoint capture and drop
    /// the guard — releasing the callback — before the timer fires.
    /// A method call captures the whole guard.
    fn ptr(&self) -> i64 {
        self.0
    }
}

impl Drop for ClosureGuard {
    fn drop(&mut self) {
        __release_closure(self.0);
    }
}

/// Invoke a closure cell's `fn_addr` slot as `extern "C" fn(i64)`.
/// The closure pointer is passed as the implicit first arg so the
/// callee can rehydrate its captures from offsets ≥ 16 (see
/// `closures.rs` for the cell layout).
unsafe fn invoke_closure(closure_ptr: i64) {
    if closure_ptr == 0 {
        return;
    }
    // Cell layout: `[ fn_addr @ 0 | rc @ 8 | captures @ 16+ ]`.
    let fn_addr = unsafe { *(closure_ptr as *const i64) };
    if fn_addr == 0 {
        return;
    }
    let f: extern "C" fn(i64) = unsafe { std::mem::transmute(fn_addr) };
    f(closure_ptr);
}

/// Schedule a one-shot timer. `ms <= 0` is due immediately — it
/// fires at the next drain point (after already-queued tasks), not
/// inline.
#[unsafe(export_name = "$time.set_timeout")]
pub extern "C" fn time_set_timeout(ms: i64, callback: i64) -> i64 {
    if callback == 0 {
        return 0;
    }
    let id = NEXT_ID.fetch_add(1, Ordering::AcqRel);
    // ilang passes `callback` as a borrowed ref (params follow the
    // borrow convention — caller still owns the +1 across the
    // call). The timer entry needs its own +1 to survive past the
    // caller's post-call release; the guard pairs it with exactly
    // one release when the entry is dropped.
    __retain_closure(callback);
    let guard = ClosureGuard(callback);
    let delay = Duration::from_millis(ms.max(0) as u64);
    pool::schedule_timer(id, delay, None, move || {
        unsafe { invoke_closure(guard.ptr()) };
    });
    id
}

#[unsafe(export_name = "$time.clear_timeout")]
pub extern "C" fn time_clear_timeout(id: i64) {
    pool::cancel_timer(id);
}

/// Schedule a repeating timer. The first firing is `ms` after the
/// call; each firing re-arms the entry until `clearInterval(id)`.
#[unsafe(export_name = "$time.set_interval")]
pub extern "C" fn time_set_interval(ms: i64, callback: i64) -> i64 {
    if callback == 0 {
        return 0;
    }
    let id = NEXT_ID.fetch_add(1, Ordering::AcqRel);
    // See `set_timeout` above for the borrow / retain rationale.
    __retain_closure(callback);
    let guard = ClosureGuard(callback);
    let interval = Duration::from_millis(ms.max(1) as u64);
    pool::schedule_timer(id, interval, Some(interval), move || {
        unsafe { invoke_closure(guard.ptr()) };
    });
    id
}

#[unsafe(export_name = "$time.clear_interval")]
pub extern "C" fn time_clear_interval(id: i64) {
    pool::cancel_timer(id);
}

/// Non-blocking pump — `time.tick()`. Runs queued continuations and
/// already-due timers, then returns. For apps that own their main
/// loop (GUI / game frame loops).
#[unsafe(export_name = "$time.tick")]
pub extern "C" fn time_tick() {
    pool::pump();
}
