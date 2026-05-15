//! `Promise<T>` runtime — a small thread-safe state machine that
//! shuttles a single resolved value (or a rejection string) to any
//! number of `.then` / `.catch` callbacks. Continuations and
//! executor bodies all run on the work-stealing pool in `pool.rs`.
//!
//! State machine:
//!
//!   Pending ──resolve──▶ Resolved(value, kind)
//!         └──reject──▶ Rejected(msg)
//!
//! `.then(cb)` and `.catch(cb)` register the callback against this
//! promise and immediately allocate a downstream `Pending` promise.
//! When the upstream settles:
//!   - resolved: schedule `cb(value)` on the pool, settle downstream
//!     with whatever cb returned
//!   - rejected (in `.then`): propagate the rejection to downstream
//!   - rejected (in `.catch`): schedule `cb(msg)`, settle downstream
//!     with cb's return value
//!
//! Cascade-on-drop: a Resolved promise releases its inner value via
//! the kind tag on free; a Rejected promise releases its msg string.
//! Continuations hold +1 references on their downstream promise +
//! the callback closure; they release both after firing.

use std::sync::atomic::{AtomicI64, Ordering, fence};
use std::sync::{Mutex, OnceLock};

use crate::cascade::{release_field_by_kind, retain_field_by_kind};
use crate::kind::KIND_PROMISE;
use crate::pool;
use crate::strings::{__release_string, __retain_string};

#[derive(Clone)]
enum State {
    Pending,
    Resolved { value: i64, kind: i64 },
    Rejected { msg: i64 },
}

struct Continuation {
    /// `extern "C" fn(closure_ptr, input_value_or_msg) -> output_value`
    /// Either the on-resolve or on-reject path; the other slot is 0.
    on_resolve: i64,
    on_reject: i64,
    /// Downstream Promise this continuation will settle. Owned (+1).
    downstream: i64,
    /// Cascade kind for the value `cb` returns (output kind of the
    /// downstream's resolved value).
    out_kind: i64,
}

struct Inner {
    state: State,
    waiters: Vec<Continuation>,
}

pub(crate) struct ManagedPromise {
    rc: AtomicI64,
    inner: Mutex<Inner>,
}

// We never expose the raw struct; users hold an `i64` pointer that
// the runtime treats as `*mut ManagedPromise`. The state and waiter
// list are protected by the inner mutex.
unsafe impl Send for ManagedPromise {}
unsafe impl Sync for ManagedPromise {}

fn alloc_pending() -> i64 {
    let p = Box::new(ManagedPromise {
        rc: AtomicI64::new(1),
        inner: Mutex::new(Inner { state: State::Pending, waiters: Vec::new() }),
    });
    Box::into_raw(p) as i64
}

unsafe fn promise_ref<'a>(ptr: i64) -> &'a ManagedPromise {
    unsafe { &*(ptr as *const ManagedPromise) }
}

#[unsafe(no_mangle)]
pub extern "C" fn __promise_pending() -> i64 {
    alloc_pending()
}

#[unsafe(no_mangle)]
pub extern "C" fn __promise_resolve(value: i64, kind: i64) -> i64 {
    // Take ownership of `value` (caller's +1 transfers in).
    let p = Box::new(ManagedPromise {
        rc: AtomicI64::new(1),
        inner: Mutex::new(Inner {
            state: State::Resolved { value, kind },
            waiters: Vec::new(),
        }),
    });
    Box::into_raw(p) as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __promise_reject(msg: i64) -> i64 {
    // `msg` is a string pointer; we take ownership.
    let p = Box::new(ManagedPromise {
        rc: AtomicI64::new(1),
        inner: Mutex::new(Inner {
            state: State::Rejected { msg },
            waiters: Vec::new(),
        }),
    });
    Box::into_raw(p) as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __retain_promise(p: i64) {
    if p == 0 {
        return;
    }
    let pr = unsafe { promise_ref(p) };
    let mut cur = pr.rc.load(Ordering::Relaxed);
    loop {
        if cur <= 0 {
            return;
        }
        match pr.rc.compare_exchange_weak(
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

#[unsafe(no_mangle)]
pub extern "C" fn __release_promise(p: i64) {
    if p == 0 {
        return;
    }
    let pr = unsafe { promise_ref(p) };
    let mut cur = pr.rc.load(Ordering::Relaxed);
    loop {
        if cur <= 0 {
            return;
        }
        match pr.rc.compare_exchange_weak(
            cur,
            cur - 1,
            Ordering::Release,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(actual) => cur = actual,
        }
    }
    if cur != 1 {
        return;
    }
    fence(Ordering::Acquire);
    // Final drop — cascade-release whatever the state owns.
    let mut owned = unsafe { Box::from_raw(p as *mut ManagedPromise) };
    let inner = owned.inner.get_mut().expect("promise mutex poisoned");
    match inner.state {
        State::Pending => {}
        State::Resolved { value, kind } => {
            release_field_by_kind(value, kind);
        }
        State::Rejected { msg } => {
            __release_string(msg);
        }
    }
    // Any remaining waiters means the promise was dropped without
    // settling. Each waiter holds +1 on its downstream + the
    // callback closure; release them so we don't leak.
    for w in inner.waiters.drain(..) {
        if w.downstream != 0 {
            __release_promise(w.downstream);
        }
        if w.on_resolve != 0 {
            release_closure_arg(w.on_resolve);
        }
        if w.on_reject != 0 {
            release_closure_arg(w.on_reject);
        }
    }
}

fn release_closure_arg(fn_or_closure: i64) {
    // Continuations store the closure pointer (cell). Use the
    // closure cascade.
    crate::closures::__release_closure(fn_or_closure);
}

fn retain_closure_arg(fn_or_closure: i64) {
    crate::closures::__retain_closure(fn_or_closure);
}

/// Internal: register a continuation. Both `on_resolve` and
/// `on_reject` are closure cell pointers (or 0 if absent). Returns
/// the downstream Promise pointer (+1, caller owns).
fn register_continuation(
    upstream: i64,
    on_resolve: i64,
    on_reject: i64,
    out_kind: i64,
) -> i64 {
    let downstream = alloc_pending();
    if upstream == 0 {
        // Calling .then on a null promise — propagate as never-settled.
        return downstream;
    }
    // Take +1 on every reference we're about to stash in the
    // continuation list.
    if on_resolve != 0 {
        retain_closure_arg(on_resolve);
    }
    if on_reject != 0 {
        retain_closure_arg(on_reject);
    }
    __retain_promise(downstream);

    let pr = unsafe { promise_ref(upstream) };
    let to_fire: Option<State>;
    {
        let mut g = pr.inner.lock().expect("promise mutex poisoned");
        match &g.state {
            State::Pending => {
                g.waiters.push(Continuation {
                    on_resolve,
                    on_reject,
                    downstream,
                    out_kind,
                });
                to_fire = None;
            }
            other => to_fire = Some(other.clone()),
        }
    }

    if let Some(state) = to_fire {
        // Already settled — fire immediately on the pool. We held
        // refs on the closures + downstream; the worker consumes them.
        // Need to also retain the value/msg stored in the upstream
        // for our own handler to use, since the upstream may be
        // dropped before the worker runs.
        match state {
            State::Resolved { value, kind } => {
                retain_field_by_kind(value, kind);
                pool::submit(move || {
                    fire_resolved(on_resolve, on_reject, downstream, out_kind, value, kind);
                });
            }
            State::Rejected { msg } => {
                __retain_string(msg);
                pool::submit(move || {
                    fire_rejected(on_resolve, on_reject, downstream, out_kind, msg);
                });
            }
            State::Pending => unreachable!(),
        }
    }

    downstream
}

fn fire_resolved(
    on_resolve: i64,
    on_reject: i64,
    downstream: i64,
    out_kind: i64,
    value: i64,
    in_kind: i64,
) {
    // We took +1 on on_reject for symmetry; on a resolved path it's
    // unused, drop it.
    if on_reject != 0 {
        release_closure_arg(on_reject);
    }
    if on_resolve == 0 {
        // No on-resolve handler (a bare .catch): pass-through value.
        retain_field_by_kind(value, in_kind);
        settle_resolve(downstream, value, in_kind);
        release_field_by_kind(value, in_kind);
        __release_promise(downstream);
        return;
    }
    // Invoke the closure. ilang closures carry the env (closure_ptr)
    // as the trailing argument — `extern "C" fn(value, env) -> output`.
    let fn_addr = unsafe { *(on_resolve as *const i64) };
    let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(fn_addr) };
    let out = f(value, on_resolve);
    // Closure call doesn't consume `value`; release our extra retain.
    release_field_by_kind(value, in_kind);
    release_closure_arg(on_resolve);
    settle_resolve(downstream, out, out_kind);
    __release_promise(downstream);
}

fn fire_rejected(
    on_resolve: i64,
    on_reject: i64,
    downstream: i64,
    out_kind: i64,
    msg: i64,
) {
    if on_resolve != 0 {
        release_closure_arg(on_resolve);
    }
    if on_reject == 0 {
        // No on-reject (a bare .then): propagate the rejection.
        __retain_string(msg);
        settle_reject(downstream, msg);
        __release_string(msg);
        __release_promise(downstream);
        return;
    }
    let fn_addr = unsafe { *(on_reject as *const i64) };
    let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(fn_addr) };
    let out = f(msg, on_reject);
    __release_string(msg);
    release_closure_arg(on_reject);
    // .catch's handler returns a recovery value of type T; the
    // downstream becomes Resolved with it.
    settle_resolve(downstream, out, out_kind);
    __release_promise(downstream);
}

/// Transition `p` Pending → Resolved. Takes ownership of `value`'s
/// +1 refcount. Schedules every queued continuation. No-op if `p`
/// is already settled.
fn settle_resolve(p: i64, value: i64, kind: i64) {
    if p == 0 {
        release_field_by_kind(value, kind);
        return;
    }
    let pr = unsafe { promise_ref(p) };
    let mut waiters: Vec<Continuation> = Vec::new();
    let mut accepted = false;
    {
        let mut g = pr.inner.lock().expect("promise mutex poisoned");
        if matches!(g.state, State::Pending) {
            g.state = State::Resolved { value, kind };
            std::mem::swap(&mut waiters, &mut g.waiters);
            accepted = true;
        }
    }
    if !accepted {
        // Already settled — caller's value is dropped.
        release_field_by_kind(value, kind);
        return;
    }
    // Fire each waiter on the pool. Each waiter needs its own
    // retain on the value (since value lives in the promise).
    for w in waiters {
        retain_field_by_kind(value, kind);
        let v = value;
        let k = kind;
        pool::submit(move || {
            fire_resolved(w.on_resolve, w.on_reject, w.downstream, w.out_kind, v, k);
        });
    }
}

fn settle_reject(p: i64, msg: i64) {
    if p == 0 {
        __release_string(msg);
        return;
    }
    let pr = unsafe { promise_ref(p) };
    let mut waiters: Vec<Continuation> = Vec::new();
    let mut accepted = false;
    {
        let mut g = pr.inner.lock().expect("promise mutex poisoned");
        if matches!(g.state, State::Pending) {
            g.state = State::Rejected { msg };
            std::mem::swap(&mut waiters, &mut g.waiters);
            accepted = true;
        }
    }
    if !accepted {
        __release_string(msg);
        return;
    }
    for w in waiters {
        __retain_string(msg);
        let m = msg;
        pool::submit(move || {
            fire_rejected(w.on_resolve, w.on_reject, w.downstream, w.out_kind, m);
        });
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __promise_then(p: i64, on_resolve: i64, out_kind: i64) -> i64 {
    register_continuation(p, on_resolve, 0, out_kind)
}

#[unsafe(no_mangle)]
pub extern "C" fn __promise_catch(p: i64, on_reject: i64, out_kind: i64) -> i64 {
    register_continuation(p, 0, on_reject, out_kind)
}

// --------------------------------------------------------------------
// Executor: `new Promise<T>(executor: fn(fn(T), fn(string)))`
//
// Runtime allocates the pending Promise + two synthetic callback
// closures (resolve_cb, reject_cb) that point back at it, then
// schedules the executor on the pool. Each callback's `fn_addr` is
// a runtime stub that decodes its single capture (the promise ptr)
// and calls `settle_resolve` / `settle_reject`.
// --------------------------------------------------------------------

extern "C" fn promise_resolve_stub(value: i64, closure_ptr: i64) -> i64 {
    // Capture[0] at offset 16 = promise ptr. Take ownership of value
    // (caller in ilang doesn't release after the call; that's the
    // convention for resolve/reject).
    let p = unsafe { *((closure_ptr + 16) as *const i64) };
    // Determine the inner kind from the capture[1] at offset 24.
    let kind = unsafe { *((closure_ptr + 24) as *const i64) };
    // Caller still owns `value`; settle takes ownership, so retain.
    retain_field_by_kind(value, kind);
    settle_resolve(p, value, kind);
    0
}

extern "C" fn promise_reject_stub(msg: i64, closure_ptr: i64) -> i64 {
    let p = unsafe { *((closure_ptr + 16) as *const i64) };
    __retain_string(msg);
    settle_reject(p, msg);
    0
}

static STUBS_REGISTERED: OnceLock<()> = OnceLock::new();

fn ensure_stubs_registered() {
    STUBS_REGISTERED.get_or_init(|| {
        let resolve_addr = promise_resolve_stub as *const () as i64;
        let reject_addr = promise_reject_stub as *const () as i64;
        // Capture layout: [fn_addr | rc | promise_ptr (KIND_PROMISE) | kind (KIND_NONE)]
        crate::closures::__register_closure_size(resolve_addr, 32);
        crate::closures::__register_closure_capture(resolve_addr, 16, KIND_PROMISE);
        // capture at +24 is a plain i64 (kind tag), not heap-cascaded.
        crate::closures::__register_closure_size(reject_addr, 24);
        crate::closures::__register_closure_capture(reject_addr, 16, KIND_PROMISE);
    });
}

fn alloc_callback_closure(stub_addr: i64, promise_ptr: i64, extra_kind: Option<i64>) -> i64 {
    let size = if extra_kind.is_some() { 32 } else { 24 };
    let cell = crate::alloc::__mir_alloc(size) as *mut i64;
    unsafe {
        *cell = stub_addr;
        *cell.add(1) = 1; // rc
        *cell.add(2) = promise_ptr;
        if let Some(k) = extra_kind {
            *cell.add(3) = k;
        }
    }
    // The capture is the promise — retain it.
    __retain_promise(promise_ptr);
    cell as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __promise_with_executor(executor_closure: i64, value_kind: i64) -> i64 {
    ensure_stubs_registered();
    let promise = alloc_pending();
    if executor_closure == 0 {
        return promise;
    }
    // Build resolve / reject callback closures pointing at this promise.
    let resolve_cb = alloc_callback_closure(
        promise_resolve_stub as *const () as i64,
        promise,
        Some(value_kind),
    );
    let reject_cb =
        alloc_callback_closure(promise_reject_stub as *const () as i64, promise, None);

    // Hand out +1 of the executor + each callback to the worker; we
    // also retain `promise` once for the worker so the closures'
    // captures stay valid.
    retain_closure_arg(executor_closure);
    let exec = executor_closure;
    let r_cb = resolve_cb;
    let j_cb = reject_cb;
    pool::submit(move || {
        let fn_addr = unsafe { *(exec as *const i64) };
        // executor signature: ilang env-trailing — `fn(resolve, reject, env)`.
        let f: extern "C" fn(i64, i64, i64) = unsafe { std::mem::transmute(fn_addr) };
        f(r_cb, j_cb, exec);
        // Drop the +1 references the worker held.
        release_closure_arg(exec);
        release_closure_arg(r_cb);
        release_closure_arg(j_cb);
    });

    promise
}

/// Block until every pending pool task has run. Called from the
/// driver at program-end so pending Promise continuations actually
/// finish before exit.
#[unsafe(no_mangle)]
pub extern "C" fn __promise_drain() {
    pool::drain();
}

// --------------------------------------------------------------------
// Convenience for Rust unit tests in this crate.
// --------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::KIND_STR;
    use crate::strings::{cstr_to_str, leak_cstring};

    fn make_string(s: &str) -> i64 {
        leak_cstring(s.to_string())
    }

    #[test]
    fn resolved_then_callback_fires() {
        // p = Promise.resolve("hi")  (KIND_STR)
        let v = make_string("hi");
        let p = __promise_resolve(v, KIND_STR);

        // Synthetic callback (ilang env-trailing ABI: value first,
        // closure ptr last) that strcats " world" and returns a new string.
        extern "C" fn cb(value: i64, _closure: i64) -> i64 {
            let s = cstr_to_str(value);
            leak_cstring(format!("{} world", s))
        }
        // Build a zero-capture closure cell: [fn_addr | rc=1].
        let closure = crate::alloc::__mir_alloc(16) as *mut i64;
        unsafe {
            *closure = cb as *const () as i64;
            *closure.add(1) = 1;
        }
        crate::closures::__register_closure_size(cb as *const () as i64, 16);

        let downstream = __promise_then(p, closure as i64, KIND_STR);
        pool::drain();
        // Inspect downstream state.
        let pr = unsafe { promise_ref(downstream) };
        let g = pr.inner.lock().unwrap();
        match &g.state {
            State::Resolved { value, .. } => {
                assert_eq!(cstr_to_str(*value), "hi world");
            }
            other => panic!(
                "expected resolved, got {:?}",
                std::mem::discriminant(other)
            ),
        }
        drop(g);
        __release_promise(downstream);
        __release_promise(p);
    }

    #[test]
    fn rejection_propagates_through_then_to_catch() {
        let msg = make_string("boom");
        let p = __promise_reject(msg);
        // .then with no handler — should propagate.
        let mid = __promise_then(p, 0, KIND_STR);
        // .catch recovers to a string.
        extern "C" fn recover(_msg: i64, _closure: i64) -> i64 {
            leak_cstring("recovered".to_string())
        }
        let closure = crate::alloc::__mir_alloc(16) as *mut i64;
        unsafe {
            *closure = recover as *const () as i64;
            *closure.add(1) = 1;
        }
        crate::closures::__register_closure_size(recover as *const () as i64, 16);
        let final_ = __promise_catch(mid, closure as i64, KIND_STR);
        pool::drain();
        let pr = unsafe { promise_ref(final_) };
        let g = pr.inner.lock().unwrap();
        match &g.state {
            State::Resolved { value, .. } => {
                assert_eq!(cstr_to_str(*value), "recovered");
            }
            _ => panic!("expected resolved-from-catch"),
        }
        drop(g);
        __release_promise(final_);
        __release_promise(mid);
        __release_promise(p);
    }
}
