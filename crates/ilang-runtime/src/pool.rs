//! Single-threaded event loop that backs `Promise<T>` continuations
//! and `std.time` timers — JS-style run-to-completion semantics.
//!
//! All user code (main, executors, `.then` callbacks, async-fn
//! resumptions, timer callbacks) runs on one thread. Continuations
//! are queued FIFO via `submit` and only ever execute at a drain
//! point — never concurrently with the code that scheduled them.
//!
//! - `submit(task)` pushes a closure onto the FIFO queue.
//! - `schedule_timer(id, delay, repeat, f)` registers a timer on the
//!   due-ordered heap; `cancel_timer(id)` marks it cancelled (the
//!   entry is discarded the next time it reaches the front).
//! - `drain()` runs queued tasks, then sleeps until the next timer
//!   is due and fires it, until both the queue and the timer heap
//!   are empty. Called at end-of-program (mirrors Node.js's "wait
//!   for the event loop to empty before exit") and from test
//!   helpers.
//! - `pump()` is the non-blocking variant: it runs queued tasks and
//!   already-due timers, then returns without waiting. Exposed to
//!   user code as `time.tick()` for apps that own their main loop
//!   (GUI frame loops etc.).
//!
//! The state lives in a thread-local: tasks are queued and drained
//! on the same thread that runs the program, so no synchronisation
//! is needed. Re-entrant calls (a task calling `drain()` or `pump()`
//! mid-run — e.g. `test.liveAllocBytes`) are safe because every
//! entry is popped out of the `RefCell` borrow before it executes.

use std::cell::RefCell;
use std::cmp::Ordering as CmpOrdering;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::time::{Duration, Instant};

pub(crate) type Task = Box<dyn FnOnce() + 'static>;

struct TimerEntry {
    due: Instant,
    /// Tie-breaker so same-due timers fire in registration order.
    seq: u64,
    id: i64,
    /// `Some(interval)` re-arms the entry after each firing.
    repeat: Option<Duration>,
    task: Box<dyn FnMut() + 'static>,
}

// Min-heap by (due, seq) on top of std's max-heap: reverse compare.
impl PartialEq for TimerEntry {
    fn eq(&self, other: &Self) -> bool {
        self.due == other.due && self.seq == other.seq
    }
}
impl Eq for TimerEntry {}
impl PartialOrd for TimerEntry {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}
impl Ord for TimerEntry {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        other
            .due
            .cmp(&self.due)
            .then_with(|| other.seq.cmp(&self.seq))
    }
}

struct EventLoop {
    tasks: VecDeque<Task>,
    timers: BinaryHeap<TimerEntry>,
    /// Live timer ids → cancelled flag. An entry exists from
    /// `schedule_timer` until the timer completes (fires for a
    /// one-shot, observed-cancelled for either kind), so cancelling
    /// an unknown / already-finished id is a no-op and the map
    /// can't grow with stale ids.
    live: HashMap<i64, bool>,
    next_seq: u64,
}

thread_local! {
    static LOOP: RefCell<EventLoop> = RefCell::new(EventLoop {
        tasks: VecDeque::new(),
        timers: BinaryHeap::new(),
        live: HashMap::new(),
        next_seq: 0,
    });
}

/// Queue a task. It runs at the next drain point on this same
/// thread — never concurrently with the caller.
pub fn submit<F>(f: F)
where
    F: FnOnce() + 'static,
{
    LOOP.with(|l| l.borrow_mut().tasks.push_back(Box::new(f)));
}

/// Register a timer. `repeat: Some(interval)` re-arms after each
/// firing until `cancel_timer`. The caller owns id allocation;
/// ids must be unique among live timers.
pub fn schedule_timer<F>(id: i64, delay: Duration, repeat: Option<Duration>, f: F)
where
    F: FnMut() + 'static,
{
    LOOP.with(|l| {
        let mut l = l.borrow_mut();
        l.next_seq += 1;
        let seq = l.next_seq;
        l.live.insert(id, false);
        l.timers.push(TimerEntry {
            due: Instant::now() + delay,
            seq,
            id,
            repeat,
            task: Box::new(f),
        });
    });
}

/// Mark a live timer cancelled. Unknown / already-completed ids are
/// a no-op. The heap entry is discarded (and its task dropped) the
/// next time it reaches the front of the heap — a drain doesn't
/// wait out a cancelled timer's remaining delay.
pub fn cancel_timer(id: i64) {
    LOOP.with(|l| {
        if let Some(flag) = l.borrow_mut().live.get_mut(&id) {
            *flag = true;
        }
    });
}

fn pop_task() -> Option<Task> {
    LOOP.with(|l| l.borrow_mut().tasks.pop_front())
}

enum TimerStep {
    /// No timers left.
    Empty,
    /// Front timer not yet due.
    Wait(Duration),
    /// Front timer is due — run it.
    Fire(TimerEntry),
    /// Front timer was cancelled — drop it (outside the borrow:
    /// dropping the task releases its closure, which can cascade).
    Discard(TimerEntry),
}

fn next_timer_step() -> TimerStep {
    LOOP.with(|l| {
        let mut l = l.borrow_mut();
        let Some(top) = l.timers.peek() else {
            return TimerStep::Empty;
        };
        let cancelled = l.live.get(&top.id).copied().unwrap_or(true);
        if cancelled {
            let e = l.timers.pop().expect("peeked entry vanished");
            l.live.remove(&e.id);
            return TimerStep::Discard(e);
        }
        let now = Instant::now();
        if top.due <= now {
            TimerStep::Fire(l.timers.pop().expect("peeked entry vanished"))
        } else {
            TimerStep::Wait(top.due - now)
        }
    })
}

/// Run a due timer's task, then either re-arm it (interval, not
/// cancelled mid-run) or retire it. The task executes outside the
/// `RefCell` borrow so it can freely submit / schedule / cancel.
fn fire_timer(mut entry: TimerEntry) {
    (entry.task)();
    let retired: Option<TimerEntry> = LOOP.with(|l| {
        let mut l = l.borrow_mut();
        let cancelled = l.live.get(&entry.id).copied().unwrap_or(true);
        match entry.repeat {
            Some(interval) if !cancelled => {
                l.next_seq += 1;
                entry.seq = l.next_seq;
                entry.due = Instant::now() + interval;
                l.timers.push(entry);
                None
            }
            _ => {
                l.live.remove(&entry.id);
                Some(entry)
            }
        }
    });
    // Dropping the retired entry releases the callback closure —
    // run that cascade outside the borrow.
    drop(retired);
}

/// Run the event loop to exhaustion: queued tasks first, then sleep
/// until the next timer is due and fire it, until both the queue and
/// the timer heap are empty. An interval that's never cancelled
/// keeps this loop (and the process) alive — matches Node.js.
pub fn drain() {
    loop {
        if let Some(t) = pop_task() {
            t();
            continue;
        }
        match next_timer_step() {
            TimerStep::Fire(e) => fire_timer(e),
            TimerStep::Discard(e) => drop(e),
            TimerStep::Wait(d) => std::thread::sleep(d),
            TimerStep::Empty => break,
        }
    }
}

/// Non-blocking drain: run queued tasks and already-due timers,
/// then return. Backs `time.tick()` for apps that own their main
/// loop and want timers / continuations serviced once per frame.
pub fn pump() {
    loop {
        if let Some(t) = pop_task() {
            t();
            continue;
        }
        match next_timer_step() {
            TimerStep::Fire(e) => fire_timer(e),
            TimerStep::Discard(e) => drop(e),
            TimerStep::Wait(_) | TimerStep::Empty => break,
        }
    }
}
