//! Per-kind retain / release dispatcher. Sits between the container
//! release functions and the leaf release_xxx in sibling modules.
//! Every container that holds heap-shaped cells (array elem, map
//! value, tuple slot, object field, closure capture, optional inner,
//! enum payload) routes through here when it's the cell's owner
//! turn to release.

use crate::arrays::{__release_array, __retain_array};
use crate::classes::{__release_object, __release_weak, __retain_object, __retain_weak};
use crate::closures::{__release_closure, __retain_closure};
use crate::enums::{__release_enum, __retain_enum};
use crate::kind::{
    KIND_ARRAY, KIND_CLOSURE, KIND_ENUM, KIND_MAP, KIND_OBJECT,
    KIND_OPTIONAL, KIND_PROMISE, KIND_SET, KIND_STR, KIND_TUPLE, KIND_WEAK,
};
use crate::maps::{__release_map, __retain_map};
use crate::sets::{__release_set, __retain_set};
use crate::optionals::{__release_optional, __retain_optional};
use crate::promises::{__release_promise, __retain_promise};
use crate::strings::{__release_string, __retain_string};
use crate::tuples::{__release_tuple, __retain_tuple};

use std::cell::{Cell, RefCell};

thread_local! {
    /// `true` while the outermost `release_field_by_kind` frame is
    /// draining. Nested calls (a container's release cascading into
    /// its children) enqueue instead of recursing.
    static CASCADE_ACTIVE: Cell<bool> = const { Cell::new(false) };
    /// Deferred `(ptr, kind)` releases collected by nested calls;
    /// drained iteratively by the outermost frame.
    static CASCADE_QUEUE: RefCell<Vec<(i64, i64)>> = const { RefCell::new(Vec::new()) };
}

/// Release one heap cell by kind. Nested releases (every container's
/// child cascade routes back through here) are deferred to a
/// thread-local worklist that the outermost frame drains in a loop —
/// the call depth stays O(1) no matter how deep the object graph is.
/// A recursive cascade blew the native stack at ~100k links
/// (`class Node { next: Node? }` chains). Release order within a
/// graph becomes breadth-ish instead of depth-first; ARC makes no
/// ordering promise between siblings/descendants, and anything a
/// deinit can still reach is by definition not yet released.
pub(crate) fn release_field_by_kind(ptr: i64, kind: i64) {
    if ptr == 0 {
        return;
    }
    if CASCADE_ACTIVE.with(|a| a.get()) {
        CASCADE_QUEUE.with(|q| q.borrow_mut().push((ptr, kind)));
        return;
    }
    CASCADE_ACTIVE.with(|a| a.set(true));
    dispatch_release(ptr, kind);
    loop {
        let next = CASCADE_QUEUE.with(|q| q.borrow_mut().pop());
        match next {
            Some((p, k)) => dispatch_release(p, k),
            None => break,
        }
    }
    CASCADE_ACTIVE.with(|a| a.set(false));
}

fn dispatch_release(ptr: i64, kind: i64) {
    match kind {
        KIND_OBJECT => __release_object(ptr),
        KIND_ARRAY => __release_array(ptr),
        KIND_OPTIONAL => __release_optional(ptr),
        KIND_TUPLE => __release_tuple(ptr),
        KIND_MAP => __release_map(ptr),
        KIND_SET => __release_set(ptr),
        KIND_CLOSURE => __release_closure(ptr),
        KIND_STR => __release_string(ptr),
        KIND_ENUM => __release_enum(ptr),
        KIND_PROMISE => __release_promise(ptr),
        KIND_WEAK => __release_weak(ptr),
        _ => {}
    }
}

pub(crate) fn retain_field_by_kind(ptr: i64, kind: i64) {
    if ptr == 0 {
        return;
    }
    match kind {
        KIND_OBJECT => __retain_object(ptr),
        KIND_ARRAY => __retain_array(ptr),
        KIND_OPTIONAL => __retain_optional(ptr),
        KIND_TUPLE => __retain_tuple(ptr),
        KIND_MAP => __retain_map(ptr),
        KIND_SET => __retain_set(ptr),
        KIND_CLOSURE => __retain_closure(ptr),
        KIND_STR => __retain_string(ptr),
        KIND_ENUM => __retain_enum(ptr),
        KIND_PROMISE => __retain_promise(ptr),
        KIND_WEAK => __retain_weak(ptr),
        _ => {}
    }
}
