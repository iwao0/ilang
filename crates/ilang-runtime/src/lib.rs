//! Runtime support library linked into ilang AOT executables and
//! also called by the JIT (via `JITBuilder::symbol` taking the same
//! function pointers). Every `extern "C"` symbol exported here is
//! the canonical body for one ilang runtime helper; the two compile
//! backends share it bit-for-bit.
//!
//! Modules are organised by feature:
//!
//! - `alloc`           — `__mir_alloc` / `__mir_free` + counters
//! - `kind`            — KIND_* and PK_* constants
//! - `cascade`         — per-kind release / retain dispatch
//! - `print`           — print primitives, panic, weak / fn print
//! - `strings`         — string layout, registry, ops, C-string interop
//! - `arrays`          — Array leaf ops + map/filter/slice/for_each
//! - `maps`            — Map runtime
//! - `tuples`          — tuple retain / release
//! - `optionals`       — optional retain / release
//! - `closures`        — closure retain / release + capture / size tables
//! - `enums`           — enum lifecycle + print + as-string cast
//! - `classes`         — class lifecycle + vtable / drop / fields + print
//! - `print_dispatch`  — `format_kind_id` (used by maps / classes / enums)
//! - `slots`           — top-level `let` slot storage
//! - `test_helpers`    — `test.*` bindings
//! - `math`            — `math.*` bindings
//! - `raw_mem`         — `__read_*` / `__write_*` raw helpers
//!
//! All `extern "C"` items + the public helpers (`leak_cstring`,
//! `cstr_bytes`, `cstr_to_str`, `live_alloc_*`, `class_size_for`,
//! `format_object_into`, `reset_repl_slots`) are re-exported at the
//! crate root so existing `ilang_runtime::__xxx` call sites keep
//! resolving.

pub mod alloc;
pub mod arrays;
pub mod cascade;
pub mod classes;
pub mod closures;
pub mod enums;
pub mod fs;
pub mod kind;
pub mod maps;
pub mod math;
pub mod objc_blocks;
pub mod optionals;
pub mod os;
pub mod pool;
pub mod print;
pub mod promises;
pub mod print_dispatch;
pub mod raw_mem;
pub mod refcount;
pub mod regex;
pub mod slots;
pub mod strings;
pub mod fmt;
pub mod test_externs;
pub mod test_helpers;
pub mod time;
pub mod tuples;

// Re-exports — keep the historical `ilang_runtime::xxx` flat API so
// the codegen crate's symbol-pointer registrations and helper calls
// don't have to change.
pub use alloc::*;
pub use arrays::*;
pub use classes::*;
pub use closures::*;
pub use enums::*;
pub use kind::*;
pub use maps::*;
pub use math::*;
pub use objc_blocks::*;
pub use optionals::*;
pub use os::*;
pub use print::*;
pub use promises::*;
pub use raw_mem::*;
pub use slots::*;
pub use strings::*;
pub use fmt::*;
pub use test_helpers::*;
pub use tuples::*;
