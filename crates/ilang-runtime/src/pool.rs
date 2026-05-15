//! Work-stealing thread pool that backs `Promise<T>` continuations
//! and executors. Lazily started on first submit; one worker per
//! logical CPU (capped at 1 minimum).
//!
//! - `submit(task)` pushes a closure onto the global injector.
//! - Each worker maintains its own LIFO deque (`Worker<Task>`) and
//!   tries: own LIFO → injector batch → other workers' stealers.
//! - `drain()` blocks the caller until every submitted task has run.
//!   Used to keep `main` alive while pending Promises still have
//!   continuations to fire (mirrors Node.js's "wait for the event
//!   loop to empty before exit").
//! - `shutdown()` joins all workers (best-effort; intended for tests).

use std::iter;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_deque::{Injector, Stealer, Worker};

pub(crate) type Task = Box<dyn FnOnce() + Send + 'static>;

struct PoolInner {
    injector: Arc<Injector<Task>>,
    pending: Arc<AtomicI64>,
    drain_cond: Arc<(Mutex<()>, Condvar)>,
    shutdown: Arc<AtomicBool>,
    workers: Mutex<Vec<JoinHandle<()>>>,
}

static POOL: OnceLock<PoolInner> = OnceLock::new();

fn pool() -> &'static PoolInner {
    POOL.get_or_init(|| {
        let n = thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2)
            .max(1);
        PoolInner::start(n)
    })
}

impl PoolInner {
    fn start(n_workers: usize) -> Self {
        let injector: Arc<Injector<Task>> = Arc::new(Injector::new());
        let mut workers_local: Vec<Worker<Task>> = (0..n_workers)
            .map(|_| Worker::new_lifo())
            .collect();
        let stealers: Vec<Stealer<Task>> =
            workers_local.iter().map(|w| w.stealer()).collect();
        let pending = Arc::new(AtomicI64::new(0));
        let drain_cond = Arc::new((Mutex::new(()), Condvar::new()));
        let shutdown = Arc::new(AtomicBool::new(false));

        let mut handles = Vec::with_capacity(n_workers);
        for _ in 0..n_workers {
            let local = workers_local.remove(0);
            let injector = Arc::clone(&injector);
            let stealers = stealers.clone();
            let pending = Arc::clone(&pending);
            let drain_cond = Arc::clone(&drain_cond);
            let shutdown = Arc::clone(&shutdown);
            let h = thread::Builder::new()
                .name("ilang-pool".into())
                .spawn(move || {
                    worker_loop(local, injector, stealers, pending, drain_cond, shutdown)
                })
                .expect("failed to spawn pool worker");
            handles.push(h);
        }
        // `stealers` is consumed by the worker threads; nothing
        // outside the spawn loop needs it after this point.
        let _ = stealers;
        PoolInner {
            injector,
            pending,
            drain_cond,
            shutdown,
            workers: Mutex::new(handles),
        }
    }
}

fn find_task(
    local: &Worker<Task>,
    injector: &Injector<Task>,
    stealers: &[Stealer<Task>],
) -> Option<Task> {
    if let Some(t) = local.pop() {
        return Some(t);
    }
    iter::repeat_with(|| {
        injector
            .steal_batch_and_pop(local)
            .or_else(|| stealers.iter().map(|s| s.steal()).collect())
    })
    .find(|s| !s.is_retry())
    .and_then(|s| s.success())
}

fn worker_loop(
    local: Worker<Task>,
    injector: Arc<Injector<Task>>,
    stealers: Vec<Stealer<Task>>,
    pending: Arc<AtomicI64>,
    drain_cond: Arc<(Mutex<()>, Condvar)>,
    shutdown: Arc<AtomicBool>,
) {
    let mut idle_spins: u32 = 0;
    loop {
        if let Some(task) = find_task(&local, &injector, &stealers) {
            idle_spins = 0;
            // Run the task. Catch panics so one bad continuation
            // doesn't kill the worker.
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(task));
            // After completion, drop pending count and signal
            // any drainer that might be waiting on zero.
            let prev = pending.fetch_sub(1, Ordering::AcqRel);
            if prev == 1 {
                let (lock, cv) = &*drain_cond;
                let _g = lock.lock().expect("drain mutex poisoned");
                cv.notify_all();
            }
        } else if shutdown.load(Ordering::Acquire) && pending.load(Ordering::Acquire) == 0
        {
            break;
        } else {
            // Back off: spin a few times, then sleep briefly.
            idle_spins += 1;
            if idle_spins < 32 {
                std::hint::spin_loop();
            } else {
                thread::sleep(Duration::from_micros(50));
            }
        }
    }
    // Drop residual local tasks (shouldn't happen at clean shutdown).
    while let Some(_) = local.pop() {}
}

/// Push a task onto the pool. Caller does not need to hold any lock.
pub fn submit<F>(f: F)
where
    F: FnOnce() + Send + 'static,
{
    let p = pool();
    p.pending.fetch_add(1, Ordering::AcqRel);
    p.injector.push(Box::new(f));
}

/// Block until every previously submitted task has finished. Safe to
/// call concurrently from multiple threads (each gets its own wait).
/// Re-checks `pending` after every notify, so additional tasks
/// submitted while draining are also waited on.
pub fn drain() {
    // Fast path: never used the pool.
    if POOL.get().is_none() {
        return;
    }
    let p = pool();
    let (lock, cv) = &*p.drain_cond;
    loop {
        if p.pending.load(Ordering::Acquire) == 0 {
            return;
        }
        let g = lock.lock().expect("drain mutex poisoned");
        // Re-check inside the mutex to close the window between the
        // load above and the wait — the worker takes the same mutex
        // before notify_all on the 1→0 transition.
        if p.pending.load(Ordering::Acquire) == 0 {
            return;
        }
        let _g = cv
            .wait_timeout(g, Duration::from_millis(10))
            .expect("drain wait poisoned")
            .0;
    }
}

/// Best-effort shutdown — used by the test suite. Signals workers
/// and joins them. Production binaries don't need to call this; the
/// process exits and the OS reaps the threads.
#[allow(dead_code)]
pub fn shutdown() {
    let Some(p) = POOL.get() else { return };
    drain();
    p.shutdown.store(true, Ordering::Release);
    let mut hs = p.workers.lock().expect("workers mutex poisoned");
    for h in hs.drain(..) {
        let _ = h.join();
    }
}

/// Number of in-flight tasks. Test/diagnostic helper.
#[allow(dead_code)]
pub fn pending_count() -> i64 {
    POOL.get().map(|p| p.pending.load(Ordering::Acquire)).unwrap_or(0)
}
