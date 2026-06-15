//! Capability enforcement (the JIT-side runtime gate).
//!
//! Capabilities are granted by an `ilang.toml` manifest (read by the
//! CLI) and checked here at runtime: a `FuncRef::Extern` call lowered
//! out of `std.fs` requires `file`, out of `std.os` requires `os`, and
//! out of user code requires `ffi`. A MIR pass inserts a no-arg
//! `cap_require_*` call before each such extern call; if the capability
//! isn't in the granted set the process aborts with an actionable
//! message. AOT skips the runtime gate — the CLI verifies the same
//! requirement statically at build time.

use std::sync::atomic::{AtomicU32, Ordering};

pub const CAP_FILE: u32 = 1 << 0;
pub const CAP_OS: u32 = 1 << 1;
pub const CAP_FFI: u32 = 1 << 2;
pub const CAP_NET: u32 = 1 << 3;

/// The granted-capability bitset. Set once by the CLI (`__cap_set_granted`)
/// from the parsed `ilang.toml` before the program's entry runs; defaults
/// to none granted (deny-all) so a program with no manifest can't reach a
/// gated sink.
static GRANTED: AtomicU32 = AtomicU32::new(0);

pub fn cap_name(bit: u32) -> &'static str {
    match bit {
        CAP_FILE => "file",
        CAP_OS => "os",
        CAP_FFI => "ffi",
        CAP_NET => "net",
        _ => "unknown",
    }
}

/// Install the granted set. Called by the CLI before running the JIT'd
/// entry. Not exposed to ilang code — invoked Rust-side.
pub fn set_granted(bits: u32) {
    GRANTED.store(bits, Ordering::SeqCst);
}

#[inline]
fn require(bit: u32) {
    if GRANTED.load(Ordering::SeqCst) & bit == 0 {
        crate::print::rt_panic(&format!(
            "capability '{0}' is not granted — add it to ilang.toml \
             (e.g. `capabilities = [\"{0}\"]`)",
            cap_name(bit),
        ));
    }
}

#[unsafe(export_name = "$cap.requireFile")]
pub extern "C" fn __cap_require_file() {
    require(CAP_FILE);
}

#[unsafe(export_name = "$cap.requireOs")]
pub extern "C" fn __cap_require_os() {
    require(CAP_OS);
}

#[unsafe(export_name = "$cap.requireFfi")]
pub extern "C" fn __cap_require_ffi() {
    require(CAP_FFI);
}

#[unsafe(export_name = "$cap.requireNet")]
pub extern "C" fn __cap_require_net() {
    require(CAP_NET);
}
