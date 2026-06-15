//! Capability gate — capability enforcement at the MIR level.
//!
//! The real sinks are calls to `@extern(C)` / `@intrinsic` declarations
//! (the C functions and runtime intrinsics behind `std.fs` / `std.os`
//! and a user `@extern(C)` block). The capability a call requires is
//! decided by the callee's C symbol, so it is robust to inlining (the
//! requirement rides the sink, not its original module):
//!
//! - `$fs.*` intrinsic → `file`
//! - `$os.*` intrinsic → `os`
//! - any other `$…` intrinsic (math / time / test / regex / events) →
//!   exempt: trusted runtime infrastructure, no capability needed.
//! - a real C symbol (no `$`) → `ffi` — the universal escape hatch.
//!
//! For the JIT, `insert_gates` emits a no-arg `cap_require_*` builtin
//! call right before every sink call (the runtime aborts if the
//! capability isn't granted). For AOT, `required_caps` returns the set so
//! the CLI can fail the build statically instead.

use std::collections::HashMap;

use ilang_ast::Symbol;

use crate::inst::{FuncRef, Inst};
use crate::program::{FunctionKind, Program};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CapKind {
    File,
    Os,
    Ffi,
    Net,
}

impl CapKind {
    /// The runtime builtin that gates this capability (no-arg).
    pub fn builtin_name(self) -> &'static str {
        match self {
            CapKind::File => "cap_require_file",
            CapKind::Os => "cap_require_os",
            CapKind::Ffi => "cap_require_ffi",
            CapKind::Net => "cap_require_net",
        }
    }

    /// Manifest name (`capabilities = ["file", ...]`).
    pub fn manifest_name(self) -> &'static str {
        match self {
            CapKind::File => "file",
            CapKind::Os => "os",
            CapKind::Ffi => "ffi",
            CapKind::Net => "net",
        }
    }
}

/// The capability a sink with C symbol `sym` requires, or `None` if it's
/// trusted runtime infrastructure (a non-fs/os `$…` intrinsic).
pub fn cap_for_symbol(sym: &str) -> Option<CapKind> {
    if sym.starts_with("$fs.") {
        Some(CapKind::File)
    } else if sym.starts_with("$os.") {
        Some(CapKind::Os)
    } else if sym.starts_with('$') {
        None // math / time / test / regex / events / ffi-helpers
    } else {
        Some(CapKind::Ffi) // real C symbol — user FFI
    }
}

/// Map each `@extern(C)` / `@intrinsic` declaration id to the capability
/// a call to it requires. An extern with no `c_symbol` is a real C fn
/// named by its ilang name (no `$`) → `ffi`.
fn extern_caps(prog: &Program) -> HashMap<u32, Option<CapKind>> {
    prog.functions
        .iter()
        .enumerate()
        .filter(|(_, f)| matches!(f.kind, FunctionKind::Extern { .. }))
        .map(|(i, f)| {
            let cap = match f.c_symbol {
                Some(s) => cap_for_symbol(s.as_str()),
                None => Some(CapKind::Ffi),
            };
            (i as u32, cap)
        })
        .collect()
}

/// The capability `inst` requires, or `None` (not a sink, or a trusted
/// intrinsic).
///
/// Two ways a sink is reached:
/// - a **direct call** to an `@extern(C)` / `@intrinsic` declaration; and
/// - **materializing the address** of an extern sink as a value
///   (`let f = abs`, passing a bare C fn as a callback). Once code holds a
///   pointer to a C function it can call it indirectly, past any
///   call-site gate, so the capability must be charged here. The address
///   of a non-extern wrapper (e.g. a `std.fs` helper) is exempt — its own
///   body still carries the gate at the inner intrinsic call.
fn call_cap(inst: &Inst, caps: &HashMap<u32, Option<CapKind>>) -> Option<CapKind> {
    match inst {
        Inst::Call { callee: FuncRef::Extern { sym, .. }, .. } => cap_for_symbol(sym.as_str()),
        Inst::Call { callee: FuncRef::Local(id), .. } => caps.get(&id.0).copied().flatten(),
        Inst::MakeClosure { func, .. } | Inst::FuncAddr { func, .. } => {
            caps.get(&func.0).copied().flatten()
        }
        _ => None,
    }
}

/// JIT: insert a no-arg `cap_require_*` call before every sink call.
pub fn insert_gates(prog: &mut Program) {
    let caps = extern_caps(prog);
    for f in &mut prog.functions {
        for block in &mut f.blocks {
            let old = std::mem::take(&mut block.insts);
            let mut next = Vec::with_capacity(old.len());
            for inst in old {
                if let Some(cap) = call_cap(&inst, &caps) {
                    next.push(Inst::Call {
                        dst: None,
                        callee: FuncRef::Builtin(Symbol::intern(cap.builtin_name())),
                        args: Box::new([]),
                    });
                }
                next.push(inst);
            }
            block.insts = next;
        }
    }
}

/// AOT: the distinct capabilities the program's sink calls require.
pub fn required_caps(prog: &Program) -> Vec<CapKind> {
    let caps = extern_caps(prog);
    let mut out: Vec<CapKind> = Vec::new();
    for f in &prog.functions {
        for block in &f.blocks {
            for inst in &block.insts {
                if let Some(c) = call_cap(inst, &caps) {
                    if !out.contains(&c) {
                        out.push(c);
                    }
                }
            }
        }
    }
    out
}
