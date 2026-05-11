//! Per-kind retain / release dispatcher. Sits between the container
//! release functions and the leaf release_xxx in sibling modules.
//! Every container that holds heap-shaped cells (array elem, map
//! value, tuple slot, object field, closure capture, optional inner,
//! enum payload) routes through here when it's the cell's owner
//! turn to release.

use crate::arrays::{__release_array, __retain_array};
use crate::classes::{__release_object, __retain_object};
use crate::closures::{__release_closure, __retain_closure};
use crate::enums::{__release_enum, __retain_enum};
use crate::kind::{
    KIND_ARRAY, KIND_CLOSURE, KIND_ENUM, KIND_MAP, KIND_OBJECT,
    KIND_OPTIONAL, KIND_STR, KIND_TUPLE,
};
use crate::maps::{__release_map, __retain_map};
use crate::optionals::{__release_optional, __retain_optional};
use crate::strings::{__release_string, __retain_string};
use crate::tuples::{__release_tuple, __retain_tuple};

pub(crate) fn release_field_by_kind(ptr: i64, kind: i64) {
    if ptr == 0 {
        return;
    }
    match kind {
        KIND_OBJECT => __release_object(ptr),
        KIND_ARRAY => __release_array(ptr),
        KIND_OPTIONAL => __release_optional(ptr),
        KIND_TUPLE => __release_tuple(ptr),
        KIND_MAP => __release_map(ptr),
        KIND_CLOSURE => __release_closure(ptr),
        KIND_STR => __release_string(ptr),
        KIND_ENUM => __release_enum(ptr),
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
        KIND_CLOSURE => __retain_closure(ptr),
        KIND_STR => __retain_string(ptr),
        KIND_ENUM => __retain_enum(ptr),
        _ => {}
    }
}
