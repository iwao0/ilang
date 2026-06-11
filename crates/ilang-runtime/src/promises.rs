//! `Promise<T>` runtime — a small state machine that shuttles a
//! single resolved value (or a rejection string) to any number of
//! `.then` / `.catch` callbacks. JS-style run-to-completion:
//! executors run synchronously inside `new Promise(...)`;
//! continuations are queued on the single-threaded event loop in
//! `pool.rs` and only execute at a drain point — never concurrently
//! with user code.
//!
//! State machine:
//!
//!   Pending ──resolve──▶ Resolved(value, kind)
//!         └──reject──▶ Rejected(msg)
//!
//! `.then(cb)` and `.catch(cb)` register the callback against this
//! promise and immediately allocate a downstream `Pending` promise.
//! When the upstream settles:
//!   - resolved: queue `cb(value)` on the event loop, settle
//!     downstream with whatever cb returned
//!   - rejected (in `.then`): propagate the rejection to downstream
//!   - rejected (in `.catch`): queue `cb(msg)`, settle downstream
//!     with cb's return value
//!
//! Cascade-on-drop: a Resolved promise releases its inner value via
//! the kind tag on free; a Rejected promise releases its msg string.
//! Continuations hold +1 references on their downstream promise +
//! the callback closure; they release both after firing.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};

/// `ILANG_DEBUG_PROMISE=1` traces promise retain/release/settle to
/// stderr. Env lookup cached — hot-path cost is one atomic load.
fn debug_promise() -> bool {
    static FLAG: OnceLock<bool> = OnceLock::new();
    *FLAG.get_or_init(|| std::env::var_os("ILANG_DEBUG_PROMISE").is_some())
}

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
    /// Float-kind of the callback's parameter / return (0 = int/ptr,
    /// 1 = f32, 2 = f64) so a float value rides a float register
    /// rather than an integer one. `on_reject`'s input is always a
    /// string, so `in_fk` only applies to the `on_resolve` path.
    in_fk: i64,
    out_fk: i64,
}

/// Invoke a 1-arg promise callback (`fn(value, env) -> output`)
/// through an ABI that matches its float parameter / return kind
/// (`arg_fk` / `ret_fk`: 0 = int/ptr cell, 1 = f32, 2 = f64). The
/// value and result live in the i64 cell as raw bits; convert with
/// `from_bits` / `to_bits` so a float is passed / read in a float
/// register instead of an integer one (mirrors the array-HOF fix).
unsafe fn call_cb_1(fn_addr: i64, env: i64, arg: i64, arg_fk: i64, ret_fk: i64) -> i64 {
    unsafe {
        match (arg_fk, ret_fk) {
            (1, 0) => {
                let f: extern "C" fn(f32, i64) -> i64 = std::mem::transmute(fn_addr);
                f(f32::from_bits(arg as u32), env)
            }
            (2, 0) => {
                let f: extern "C" fn(f64, i64) -> i64 = std::mem::transmute(fn_addr);
                f(f64::from_bits(arg as u64), env)
            }
            (0, 1) => {
                let f: extern "C" fn(i64, i64) -> f32 = std::mem::transmute(fn_addr);
                f(arg, env).to_bits() as i64
            }
            (1, 1) => {
                let f: extern "C" fn(f32, i64) -> f32 = std::mem::transmute(fn_addr);
                f(f32::from_bits(arg as u32), env).to_bits() as i64
            }
            (2, 1) => {
                let f: extern "C" fn(f64, i64) -> f32 = std::mem::transmute(fn_addr);
                f(f64::from_bits(arg as u64), env).to_bits() as i64
            }
            (0, 2) => {
                let f: extern "C" fn(i64, i64) -> f64 = std::mem::transmute(fn_addr);
                f(arg, env).to_bits() as i64
            }
            (1, 2) => {
                let f: extern "C" fn(f32, i64) -> f64 = std::mem::transmute(fn_addr);
                f(f32::from_bits(arg as u32), env).to_bits() as i64
            }
            (2, 2) => {
                let f: extern "C" fn(f64, i64) -> f64 = std::mem::transmute(fn_addr);
                f(f64::from_bits(arg as u64), env).to_bits() as i64
            }
            _ => {
                let f: extern "C" fn(i64, i64) -> i64 = std::mem::transmute(fn_addr);
                f(arg, env)
            }
        }
    }
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

#[unsafe(export_name = "$promise.pending")]
pub extern "C" fn __promise_pending() -> i64 {
    alloc_pending()
}

#[unsafe(export_name = "$promise.resolve")]
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

#[unsafe(export_name = "$promise.reject")]
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

#[unsafe(export_name = "$promise.retain")]
pub extern "C" fn __retain_promise(p: i64) {
    if p == 0 {
        return;
    }
    let pr = unsafe { promise_ref(p) };
    if debug_promise() {
        eprintln!("[promise] retain  {p:#x} rc {}", pr.rc.load(std::sync::atomic::Ordering::Relaxed));
    }
    crate::refcount::retain_atomic(&pr.rc);
}

#[unsafe(export_name = "$promise.release")]
pub extern "C" fn __release_promise(p: i64) {
    if p == 0 {
        return;
    }
    let pr = unsafe { promise_ref(p) };
    if debug_promise() {
        eprintln!("[promise] release {p:#x} rc {}", pr.rc.load(std::sync::atomic::Ordering::Relaxed));
    }
    if crate::refcount::release_atomic(&pr.rc) != Some(0) {
        return;
    }
    if debug_promise() {
        eprintln!("[promise] FINAL-DROP {p:#x}");
    }
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

/// Internal: register a continuation. Both `on_resolve` and
/// `on_reject` are closure cell pointers (or 0 if absent). Returns
/// the downstream Promise pointer (+1, caller owns).
fn register_continuation(
    upstream: i64,
    on_resolve: i64,
    on_reject: i64,
    out_kind: i64,
    in_fk: i64,
    out_fk: i64,
) -> i64 {
    let downstream = alloc_pending();
    if upstream == 0 {
        // Calling .then on a null promise — propagate as never-settled.
        return downstream;
    }
    // `on_resolve` / `on_reject` arrive with the caller's +1 — we
    // transfer that reference directly into the waiter (or the
    // queued task below) without adding a second retain, since the
    // caller releases its local copy after the call returns and
    // the firing path releases exactly once. A retain here was a
    // refcount leak (the second +1 had no matching release once
    // the waiter fired or was drained on a never-settled promise).
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
                    in_fk,
                    out_fk,
                });
                to_fire = None;
            }
            other => to_fire = Some(other.clone()),
        }
    }

    if let Some(state) = to_fire {
        // Already settled — queue the firing on the event loop (the
        // JS microtask rule: even a settled promise's callback never
        // runs inline). We held refs on the closures + downstream;
        // the queued task consumes them. Need to also retain the
        // value/msg stored in the upstream for our own handler to
        // use, since the upstream may be dropped before the task runs.
        match state {
            State::Resolved { value, kind } => {
                retain_field_by_kind(value, kind);
                pool::submit(move || {
                    fire_resolved(
                        on_resolve, on_reject, downstream, out_kind, value, kind, in_fk, out_fk,
                    );
                });
            }
            State::Rejected { msg } => {
                __retain_string(msg);
                pool::submit(move || {
                    fire_rejected(on_resolve, on_reject, downstream, out_kind, msg, out_fk);
                });
            }
            State::Pending => unreachable!(),
        }
    }

    downstream
}

#[allow(clippy::too_many_arguments)]
fn fire_resolved(
    on_resolve: i64,
    on_reject: i64,
    downstream: i64,
    out_kind: i64,
    value: i64,
    in_kind: i64,
    in_fk: i64,
    out_fk: i64,
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
    // as the trailing argument — `extern "C" fn(value, env) -> output`,
    // through a float-kind-matched ABI.
    let fn_addr = unsafe { *(on_resolve as *const i64) };
    let out = unsafe { call_cb_1(fn_addr, on_resolve, value, in_fk, out_fk) };
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
    out_fk: i64,
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
    // The on-reject input is always the error string (fk 0); only the
    // recovery return value can be a float.
    let fn_addr = unsafe { *(on_reject as *const i64) };
    let out = unsafe { call_cb_1(fn_addr, on_reject, msg, 0, out_fk) };
    __release_string(msg);
    release_closure_arg(on_reject);
    // .catch's handler returns a recovery value of type T; the
    // downstream becomes Resolved with it.
    settle_resolve(downstream, out, out_kind);
    __release_promise(downstream);
}

/// Try to flip `p` from Pending to `new_state`, returning the queued
/// waiters on success. Returns `None` when `p` is null or already
/// settled; the caller is then responsible for releasing whatever
/// resource (value rc, msg rc) it was trying to install.
fn take_pending_waiters(p: i64, new_state: State) -> Option<Vec<Continuation>> {
    if p == 0 {
        return None;
    }
    let pr = unsafe { promise_ref(p) };
    let mut g = pr.inner.lock().expect("promise mutex poisoned");
    if matches!(g.state, State::Pending) {
        g.state = new_state;
        Some(std::mem::take(&mut g.waiters))
    } else {
        None
    }
}

/// Transition `p` Pending → Resolved. Takes ownership of `value`'s
/// +1 refcount. Schedules every queued continuation. No-op if `p`
/// is already settled.
fn settle_resolve(p: i64, value: i64, kind: i64) {
    let waiters = match take_pending_waiters(p, State::Resolved { value, kind }) {
        Some(w) => w,
        None => {
            release_field_by_kind(value, kind);
            return;
        }
    };
    // Queue each waiter on the event loop — never inline, so a
    // settle call mid-user-code can't re-enter user callbacks
    // (run-to-completion). Each waiter needs its own retain on the
    // value (since value lives in the promise).
    for w in waiters {
        retain_field_by_kind(value, kind);
        pool::submit(move || {
            fire_resolved(
                w.on_resolve, w.on_reject, w.downstream, w.out_kind, value, kind, w.in_fk, w.out_fk,
            );
        });
    }
}

fn settle_reject(p: i64, msg: i64) {
    let waiters = match take_pending_waiters(p, State::Rejected { msg }) {
        Some(w) => w,
        None => {
            if debug_promise() {
                eprintln!("[promise] settle_reject {p:#x}: not pending, dropping msg");
            }
            __release_string(msg);
            return;
        }
    };
    if debug_promise() {
        eprintln!("[promise] settle_reject {p:#x}: {} waiter(s)", waiters.len());
    }
    for w in waiters {
        __retain_string(msg);
        pool::submit(move || {
            fire_rejected(w.on_resolve, w.on_reject, w.downstream, w.out_kind, msg, w.out_fk);
        });
    }
}

#[unsafe(export_name = "$promise.then")]
pub extern "C" fn __promise_then(
    p: i64,
    on_resolve: i64,
    out_kind: i64,
    in_fk: i64,
    out_fk: i64,
) -> i64 {
    register_continuation(p, on_resolve, 0, out_kind, in_fk, out_fk)
}

#[unsafe(export_name = "$promise.catch")]
pub extern "C" fn __promise_catch(
    p: i64,
    on_reject: i64,
    out_kind: i64,
    out_fk: i64,
) -> i64 {
    // `catch`'s callback takes the error string (fk 0); only its
    // recovery return value can be a float.
    register_continuation(p, 0, on_reject, out_kind, 0, out_fk)
}

// --------------------------------------------------------------------
// Executor: `new Promise<T>(executor: fn(fn(T), fn(string)))`
//
// Runtime allocates the pending Promise + two synthetic callback
// closures (resolve_cb, reject_cb) that point back at it, then
// runs the executor synchronously. Each callback's `fn_addr` is
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

/// Float-ABI variants of the resolve stub. The executor's `resolve`
/// parameter is typed `fn(f32)` / `fn(f64)` in ilang, so the call
/// site passes the value in a FLOAT register and the env pointer in
/// the first integer register — an i64-ABI stub would read the env
/// as the value and garbage as the env (garbage values, SIGSEGV on
/// the capture reads). Each variant rebits the float into the i64
/// cell `State::Resolved` stores; `.then` converts back via its
/// `in_fk` (the established bits-in-cell convention).
extern "C" fn promise_resolve_stub_f32(value: f32, closure_ptr: i64) -> i64 {
    promise_resolve_stub(value.to_bits() as i64, closure_ptr)
}

extern "C" fn promise_resolve_stub_f64(value: f64, closure_ptr: i64) -> i64 {
    promise_resolve_stub(value.to_bits() as i64, closure_ptr)
}

extern "C" fn promise_reject_stub(msg: i64, closure_ptr: i64) -> i64 {
    let p = unsafe { *((closure_ptr + 16) as *const i64) };
    if debug_promise() {
        eprintln!("[promise] reject_stub cell={closure_ptr:#x} -> promise {p:#x}");
    }
    __retain_string(msg);
    settle_reject(p, msg);
    0
}

static STUBS_REGISTERED: OnceLock<()> = OnceLock::new();

fn ensure_stubs_registered() {
    STUBS_REGISTERED.get_or_init(|| {
        let reject_addr = promise_reject_stub as *const () as i64;
        // Capture layout: [fn_addr | rc | promise_ptr (KIND_PROMISE) | kind (KIND_NONE)]
        // All three resolve-ABI variants share it. capture at +24 is
        // a plain i64 (kind tag), not heap-cascaded.
        for addr in [
            promise_resolve_stub as *const () as i64,
            promise_resolve_stub_f32 as *const () as i64,
            promise_resolve_stub_f64 as *const () as i64,
        ] {
            crate::closures::__register_closure_size(addr, 32);
            crate::closures::__register_closure_capture(addr, 16, KIND_PROMISE);
        }
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

#[unsafe(export_name = "$promise.withExecutor")]
pub extern "C" fn __promise_with_executor(
    executor_closure: i64,
    value_kind: i64,
    value_fk: i64,
) -> i64 {
    ensure_stubs_registered();
    let promise = alloc_pending();
    if executor_closure == 0 {
        return promise;
    }
    // Build resolve / reject callback closures pointing at this
    // promise. The resolve stub must match the ABI the executor's
    // `resolve` parameter type implies: a float `T` puts the value
    // in a float register, so it needs the float-ABI variant
    // (`value_fk`: 0 = int/ptr, 1 = f32, 2 = f64).
    let resolve_stub = match value_fk {
        1 => promise_resolve_stub_f32 as *const () as i64,
        2 => promise_resolve_stub_f64 as *const () as i64,
        _ => promise_resolve_stub as *const () as i64,
    };
    let resolve_cb = alloc_callback_closure(resolve_stub, promise, Some(value_kind));
    let reject_cb =
        alloc_callback_closure(promise_reject_stub as *const () as i64, promise, None);

    // Run the executor synchronously, JS-style: `new Promise(...)`
    // executes its executor before the expression returns. A
    // resolve/reject call from inside only settles state and queues
    // continuations — it never runs user callbacks inline, so
    // run-to-completion holds.
    let fn_addr = unsafe { *(executor_closure as *const i64) };
    // executor signature: ilang env-trailing — `fn(resolve, reject, env)`.
    let f: extern "C" fn(i64, i64, i64) = unsafe { std::mem::transmute(fn_addr) };
    f(resolve_cb, reject_cb, executor_closure);
    // ARC: all three cells leaked before (the leak was exactly
    // 16 + 32 + 24 bytes per `new Promise` with an empty executor).
    // - `executor_closure` arrives with a transferred +1: the
    //   `new Promise` lowering retains a non-fresh executor and
    //   hands a fresh literal's own +1 straight in (lower/expr.rs's
    //   Promise arm), so this builtin owns one reference and must
    //   consume it.
    // - `resolve_cb` / `reject_cb` are minted above with rc=1 owned
    //   by us; the executor body's params are borrows and release
    //   nothing. An executor that stores a callback (deferred /
    //   Promise.withResolvers pattern) adds its own retain via the
    //   container store, so releasing ours here keeps escapes alive.
    release_closure_arg(executor_closure);
    release_closure_arg(resolve_cb);
    release_closure_arg(reject_cb);

    promise
}

/// Run the event loop to exhaustion (queued continuations + timer
/// heap). Called from the driver at program-end so pending Promise
/// continuations and timers actually finish before exit.
#[unsafe(export_name = "$promise.drain")]
pub extern "C" fn __promise_drain() {
    pool::drain();
}

// --------------------------------------------------------------------
// Settle hooks for the upcoming async/await state-machine lowering.
//
// The poll fn that the multi-state lowering generates needs to call
// `settle_resolve` / `settle_reject` on its result Promise from
// inside generated ilang code. Both internals already exist (the
// `.then` / `.catch` paths route through them); here we expose them
// as `extern "C"` wrappers so codegen can declare them as imports
// alongside the rest of the `__promise_*` family.
//
// Both take ownership of the value / msg's +1 reference (consistent
// with the rest of the Promise API).
// --------------------------------------------------------------------

#[unsafe(export_name = "$promise.settleResolve")]
pub extern "C" fn __promise_settle_resolve(p: i64, value: i64, kind: i64) {
    settle_resolve(p, value, kind);
}

#[unsafe(export_name = "$promise.settleReject")]
pub extern "C" fn __promise_settle_reject(p: i64, msg: i64) {
    settle_reject(p, msg);
}

// --------------------------------------------------------------------
// Promise.all / Promise.race — aggregate combinators.
//
// Both take an array of `Promise<T>` (an i64 ptr to an array header
// whose data section holds promise pointers) plus the runtime KIND_*
// tag for `T` so the result array / single value can release its
// inner properly. Both return a freshly-allocated aggregate Promise.
//
// Implementation notes:
//   - We allocate per-call shared state (counter + result slots for
//     all, single-flag for race). Callbacks run as queued event-loop
//     tasks, so the `settle_resolve` / `settle_reject` no-op-on-
//     already-settled semantics give us the "first one wins" race
//     guarantee.
//   - Each upstream promise's callback is a synthetic closure
//     allocated like the executor's resolve/reject stubs — the
//     fn_addr points at a runtime stub that decodes its captures.
// --------------------------------------------------------------------

extern "C" fn promise_all_resolve_stub(value: i64, closure_ptr: i64) -> i64 {
    // Captures: [+16 aggregate promise, +24 idx, +32 kind, +40 state ptr]
    let agg = unsafe { *((closure_ptr + 16) as *const i64) };
    let idx = unsafe { *((closure_ptr + 24) as *const i64) };
    let kind = unsafe { *((closure_ptr + 32) as *const i64) };
    let state = unsafe { *((closure_ptr + 40) as *const i64) } as *mut PromiseAllState;
    retain_field_by_kind(value, kind);
    let st = unsafe { &*state };
    // Write our slot atomically — we own this slot uniquely.
    unsafe {
        let slot = st.values.add(idx as usize);
        *slot = value;
    }
    let prev = st.remaining.fetch_sub(1, Ordering::AcqRel);
    if prev == 1 {
        // Last to arrive: build the result array and settle.
        let arr = unsafe {
            crate::arrays::build_i64_array(
                std::slice::from_raw_parts(st.values, st.len),
                kind,
            )
        };
        // build_i64_array copies the slots; release the per-slot
        // retains we accumulated above (they now live inside the
        // array and the array will cascade-release them).
        for i in 0..st.len {
            let v = unsafe { *st.values.add(i) };
            release_field_by_kind(v, kind);
        }
        unsafe { drop_promise_all_state(state) };
        settle_resolve(agg, arr, crate::kind::KIND_ARRAY);
    }
    0
}

extern "C" fn promise_all_reject_stub(msg: i64, closure_ptr: i64) -> i64 {
    let agg = unsafe { *((closure_ptr + 16) as *const i64) };
    let kind = unsafe { *((closure_ptr + 32) as *const i64) };
    let state = unsafe { *((closure_ptr + 40) as *const i64) } as *mut PromiseAllState;
    __retain_string(msg);
    settle_reject(agg, msg);
    // Drop any partially-collected value retains. We can't tell
    // here which slots were filled, so we walk all of them and
    // release the non-zero ones (slots start at 0).
    let st = unsafe { &*state };
    for i in 0..st.len {
        let v = unsafe { *st.values.add(i) };
        if v != 0 {
            release_field_by_kind(v, kind);
            unsafe {
                *st.values.add(i) = 0;
            }
        }
    }
    // Decrement remaining; only the LAST caller (whether resolve
    // or reject) frees the state. Without this, an earlier reject
    // and a later resolve would race on the state.
    let prev = st.remaining.fetch_sub(1, Ordering::AcqRel);
    if prev == 1 {
        unsafe { drop_promise_all_state(state) };
    }
    0
}

extern "C" fn promise_race_resolve_stub(value: i64, closure_ptr: i64) -> i64 {
    let agg = unsafe { *((closure_ptr + 16) as *const i64) };
    let kind = unsafe { *((closure_ptr + 24) as *const i64) };
    retain_field_by_kind(value, kind);
    settle_resolve(agg, value, kind);
    0
}

extern "C" fn promise_race_reject_stub(msg: i64, closure_ptr: i64) -> i64 {
    let agg = unsafe { *((closure_ptr + 16) as *const i64) };
    __retain_string(msg);
    settle_reject(agg, msg);
    0
}

struct PromiseAllState {
    values: *mut i64,
    len: usize,
    /// Counts down both resolves and rejects. The last caller
    /// frees the buffer.
    remaining: AtomicI64,
}

unsafe fn drop_promise_all_state(state: *mut PromiseAllState) {
    let layout =
        std::alloc::Layout::array::<i64>(unsafe { (*state).len }.max(1)).unwrap();
    unsafe {
        std::alloc::dealloc((*state).values as *mut u8, layout);
        let _ = Box::from_raw(state);
    }
}

static AGG_STUBS_REGISTERED: OnceLock<()> = OnceLock::new();

fn ensure_agg_stubs_registered() {
    AGG_STUBS_REGISTERED.get_or_init(|| {
        let r1 = promise_all_resolve_stub as *const () as i64;
        let r2 = promise_all_reject_stub as *const () as i64;
        let r3 = promise_race_resolve_stub as *const () as i64;
        let r4 = promise_race_reject_stub as *const () as i64;
        // Promise.all callbacks: 4 captures = 48 bytes total.
        // Cascade: aggregate promise (KIND_PROMISE) at +16; the rest
        // are plain i64 / pointers we manage manually.
        crate::closures::__register_closure_size(r1, 48);
        crate::closures::__register_closure_capture(r1, 16, KIND_PROMISE);
        crate::closures::__register_closure_size(r2, 48);
        crate::closures::__register_closure_capture(r2, 16, KIND_PROMISE);
        // Promise.race callbacks: 2 captures = 32 bytes.
        crate::closures::__register_closure_size(r3, 32);
        crate::closures::__register_closure_capture(r3, 16, KIND_PROMISE);
        crate::closures::__register_closure_size(r4, 24);
        crate::closures::__register_closure_capture(r4, 16, KIND_PROMISE);
    });
}

/// Read promise pointers out of an ilang array header (`arr_ptr`).
/// Layout: `[len | cap | data_ptr | rc | kind | stride]`. Each cell
/// is an i64 pointer to a `ManagedPromise`.
fn read_promise_array(arr_ptr: i64) -> Vec<i64> {
    if arr_ptr == 0 {
        return Vec::new();
    }
    let len = unsafe { *(arr_ptr as *const i64) };
    let data = unsafe { *((arr_ptr + 16) as *const i64) };
    let mut out = Vec::with_capacity(len as usize);
    for i in 0..len {
        let p = unsafe { *((data + i * 8) as *const i64) };
        out.push(p);
    }
    out
}

#[unsafe(export_name = "$promise.all")]
pub extern "C" fn __promise_all(arr_ptr: i64, value_kind: i64) -> i64 {
    ensure_agg_stubs_registered();
    let promises = read_promise_array(arr_ptr);
    let n = promises.len();
    let agg = alloc_pending();
    if n == 0 {
        // Empty input: resolve immediately with an empty array.
        let empty = crate::arrays::build_i64_array(&[], value_kind);
        settle_resolve(agg, empty, crate::kind::KIND_ARRAY);
        return agg;
    }
    // Allocate the per-call state: a buffer of `n` i64 slots and
    // an atomic counter. The counter starts at `n`: each upstream
    // fires exactly one of its resolve/reject stubs, so one
    // decrement per upstream converges to 0 and the last caller
    // frees the state.
    let layout = std::alloc::Layout::array::<i64>(n).unwrap();
    let values = unsafe { std::alloc::alloc_zeroed(layout) as *mut i64 };
    let state = Box::into_raw(Box::new(PromiseAllState {
        values,
        len: n,
        remaining: AtomicI64::new(n as i64),
    }));

    // Pre-scan for a synchronously-rejected upstream and lock the
    // aggregate to Rejected before any queued stub can run.
    // Otherwise, with every upstream already settled at call time,
    // the FIFO order would let resolve stubs registered earlier in
    // the array settle slots first and an "all resolved" aggregate
    // could form before the reject stub's turn. By settling agg
    // here, every later stub's settle attempt becomes a no-op via
    // `take_pending_waiters`, so the outcome is "rejects on first
    // rejection" regardless of queue order.
    for &up in promises.iter() {
        if up == 0 {
            continue;
        }
        let pr = unsafe { promise_ref(up) };
        let snapshot = {
            let g = pr.inner.lock().expect("promise mutex poisoned");
            g.state.clone()
        };
        if let State::Rejected { msg } = snapshot {
            __retain_string(msg);
            settle_reject(agg, msg);
            break;
        }
    }

    for (i, &up) in promises.iter().enumerate() {
        // Allocate a 48-byte synthetic closure per upstream:
        // [fn_addr | rc=1 | agg_promise | idx | kind | state_ptr]
        let cell = crate::alloc::__mir_alloc(48) as *mut i64;
        unsafe {
            *cell = promise_all_resolve_stub as *const () as i64;
            *cell.add(1) = 1;
            *cell.add(2) = agg;
            *cell.add(3) = i as i64;
            *cell.add(4) = value_kind;
            *cell.add(5) = state as i64;
        }
        __retain_promise(agg);
        let resolve_cb = cell as i64;

        let rcell = crate::alloc::__mir_alloc(48) as *mut i64;
        unsafe {
            *rcell = promise_all_reject_stub as *const () as i64;
            *rcell.add(1) = 1;
            *rcell.add(2) = agg;
            *rcell.add(3) = i as i64;
            *rcell.add(4) = value_kind;
            *rcell.add(5) = state as i64;
        }
        __retain_promise(agg);
        let reject_cb = rcell as i64;

        // Wire both callbacks to the same upstream. Each takes
        // ownership of its closure via register_continuation; we
        // discard the downstream promises (they never settle visibly).
        let d1 = register_continuation(up, resolve_cb, 0, value_kind, 0, 0);
        __release_promise(d1);
        let d2 = register_continuation(up, 0, reject_cb, value_kind, 0, 0);
        __release_promise(d2);
    }
    agg
}

#[unsafe(export_name = "$promise.race")]
pub extern "C" fn __promise_race(arr_ptr: i64, value_kind: i64) -> i64 {
    ensure_agg_stubs_registered();
    let promises = read_promise_array(arr_ptr);
    let agg = alloc_pending();
    if promises.is_empty() {
        // Empty race: stays Pending forever in JS. We mirror that —
        // the caller's `.then` / `.catch` simply never fires.
        return agg;
    }
    // Pre-scan for the first already-settled upstream and lock agg
    // to that outcome before any queued stub runs. The spec is
    // "first to settle wins" and synchronously-settled inputs all
    // "settle" at the same instant — pick array order as the
    // tie-breaker (which the FIFO queue would also yield; the scan
    // keeps the rule explicit and skips the queue round-trip).
    for &up in promises.iter() {
        if up == 0 {
            continue;
        }
        let pr = unsafe { promise_ref(up) };
        let snapshot = {
            let g = pr.inner.lock().expect("promise mutex poisoned");
            g.state.clone()
        };
        match snapshot {
            State::Resolved { value, kind } => {
                retain_field_by_kind(value, kind);
                settle_resolve(agg, value, kind);
                break;
            }
            State::Rejected { msg } => {
                __retain_string(msg);
                settle_reject(agg, msg);
                break;
            }
            State::Pending => continue,
        }
    }
    for &up in &promises {
        let cell = crate::alloc::__mir_alloc(32) as *mut i64;
        unsafe {
            *cell = promise_race_resolve_stub as *const () as i64;
            *cell.add(1) = 1;
            *cell.add(2) = agg;
            *cell.add(3) = value_kind;
        }
        __retain_promise(agg);
        let resolve_cb = cell as i64;

        let rcell = crate::alloc::__mir_alloc(24) as *mut i64;
        unsafe {
            *rcell = promise_race_reject_stub as *const () as i64;
            *rcell.add(1) = 1;
            *rcell.add(2) = agg;
        }
        __retain_promise(agg);
        let reject_cb = rcell as i64;

        let d1 = register_continuation(up, resolve_cb, 0, value_kind, 0, 0);
        __release_promise(d1);
        let d2 = register_continuation(up, 0, reject_cb, value_kind, 0, 0);
        __release_promise(d2);
    }
    agg
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

        let downstream = __promise_then(p, closure as i64, KIND_STR, 0, 0);
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
        let mid = __promise_then(p, 0, KIND_STR, 0, 0);
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
        let final_ = __promise_catch(mid, closure as i64, KIND_STR, 0);
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

/// Human-readable promise state for `console.log(p)` / trailing
/// prints and `${p}` template interpolation. Without these, the
/// print/format dispatch fell back to the raw-int arm and a Promise
/// rendered as its pointer value.
fn promise_state_label(p: i64) -> &'static str {
    if p == 0 {
        return "<promise <null>>";
    }
    let pr = unsafe { &*(p as *const ManagedPromise) };
    let g = pr.inner.lock().expect("promise mutex poisoned");
    match g.state {
        State::Pending => "<promise pending>",
        State::Resolved { .. } => "<promise resolved>",
        State::Rejected { .. } => "<promise rejected>",
    }
}

#[unsafe(export_name = "$print.promise")]
pub extern "C" fn __print_promise(p: i64) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(promise_state_label(p).as_bytes());
}

#[unsafe(export_name = "$fmt.promise")]
pub extern "C" fn __fmt_promise(p: i64) -> i64 {
    crate::strings::leak_cstring(promise_state_label(p).to_string())
}
