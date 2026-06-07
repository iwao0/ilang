//! `setTimeout` / `clearTimeout` / `setInterval` / `clearInterval`
//! backing `libs/std/time.il`. JS-style semantics:
//!
//! - `setTimeout(cb, ms)` schedules `cb` to run once after `ms`
//!   milliseconds. Returns a non-zero `i64` id.
//! - `setInterval(cb, ms)` schedules `cb` to run every `ms`
//!   milliseconds until cancelled. Returns the id.
//! - `clearTimeout(id)` / `clearInterval(id)` cancels a pending
//!   timer — once the cancellation flag is observed, the next
//!   firing (or any already-running one) is the last.
//!
//! All callbacks fire on the shared `pool` worker threads
//! (`pool::submit`). The existing `__promise_drain` call at end-
//! of-program — which is already wired into `run_main` — also
//! drains pending timer tasks, so a `setTimeout` outstanding at
//! the moment `main` returns gets to fire before the process
//! exits. A `setInterval` that's never `clearInterval`d will keep
//! the process alive indefinitely; this matches Node.js.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use crate::closures::{__release_closure, __retain_closure};
use crate::pool;

/// Per-timer cancellation flag — shared between the public
/// `clearTimeout` / `clearInterval` setter and the timer body
/// running on the pool worker.
struct TimerState {
    next_id: AtomicI64,
    timers: Mutex<HashMap<i64, Arc<AtomicBool>>>,
}

static STATE: OnceLock<TimerState> = OnceLock::new();

fn state() -> &'static TimerState {
    STATE.get_or_init(|| TimerState {
        // `0` is reserved as "no timer" — both setTimeout and
        // setInterval return `0` on a null callback so callers can
        // use the returned id as a truthy check.
        next_id: AtomicI64::new(1),
        timers: Mutex::new(HashMap::new()),
    })
}

/// Look up the cancellation flag for `id`, returning a fresh
/// `Arc` clone so the caller can release the table lock before
/// touching the flag.
fn cancel_flag(id: i64) -> Option<Arc<AtomicBool>> {
    state().timers.lock().ok()?.get(&id).map(Arc::clone)
}

/// Invoke a closure cell's `fn_addr` slot as `extern "C" fn(i64)`.
/// The closure pointer is passed as the implicit first arg so the
/// callee can rehydrate its captures from offsets ≥ 16 (see
/// `closures.rs` for the cell layout). Caller is responsible for
/// retaining the closure before scheduling and releasing it after
/// the firing path is done.
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

/// Schedule a one-shot timer. `ms <= 0` fires "as soon as
/// possible" — submitted to the pool with no sleep, runs on the
/// next worker tick. The returned id stays valid until either the
/// timer has fired (or been cancelled) and its entry has been
/// removed from the table.
#[unsafe(export_name = "$time.set_timeout")]
pub extern "C" fn time_set_timeout(ms: i64, callback: i64) -> i64 {
    if callback == 0 {
        return 0;
    }
    let st = state();
    let id = st.next_id.fetch_add(1, Ordering::AcqRel);
    let cancelled = Arc::new(AtomicBool::new(false));
    st.timers.lock().expect("timer table poisoned")
        .insert(id, Arc::clone(&cancelled));
    // ilang passes `callback` as a borrowed ref (params follow the
    // borrow convention — caller still owns the +1 across the
    // call). The pool task needs its own +1 to survive past the
    // caller's post-call release; without the retain the cell would
    // be freed before the worker fires it. The caller-side fresh
    // release happens at the `lower_call` site.
    __retain_closure(callback);
    let sleep_ms = if ms > 0 { ms as u64 } else { 0 };
    pool::submit(move || {
        if sleep_ms > 0 {
            thread::sleep(Duration::from_millis(sleep_ms));
        }
        if !cancelled.load(Ordering::Acquire) {
            unsafe { invoke_closure(callback); }
        }
        __release_closure(callback);
        if let Ok(mut t) = state().timers.lock() {
            t.remove(&id);
        }
    });
    id
}

#[unsafe(export_name = "$time.clear_timeout")]
pub extern "C" fn time_clear_timeout(id: i64) {
    if let Some(flag) = cancel_flag(id) {
        flag.store(true, Ordering::Release);
    }
}

/// Schedule a repeating timer. The body loops `sleep → check
/// cancel → invoke` until the cancellation flag is observed.
/// `clearInterval(id)` flips that flag; the next sleep wake or
/// in-progress invocation is the last.
#[unsafe(export_name = "$time.set_interval")]
pub extern "C" fn time_set_interval(ms: i64, callback: i64) -> i64 {
    if callback == 0 {
        return 0;
    }
    let st = state();
    let id = st.next_id.fetch_add(1, Ordering::AcqRel);
    let cancelled = Arc::new(AtomicBool::new(false));
    st.timers.lock().expect("timer table poisoned")
        .insert(id, Arc::clone(&cancelled));
    // See `set_timeout` above for the borrow / retain rationale.
    __retain_closure(callback);
    let interval = Duration::from_millis(if ms > 0 { ms as u64 } else { 1 });
    pool::submit(move || {
        loop {
            thread::sleep(interval);
            if cancelled.load(Ordering::Acquire) {
                break;
            }
            unsafe { invoke_closure(callback); }
        }
        __release_closure(callback);
        if let Ok(mut t) = state().timers.lock() {
            t.remove(&id);
        }
    });
    id
}

#[unsafe(export_name = "$time.clear_interval")]
pub extern "C" fn time_clear_interval(id: i64) {
    time_clear_timeout(id);
}
