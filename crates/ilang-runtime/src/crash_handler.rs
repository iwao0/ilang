//! Best-effort crash reporter for Windows. Installs a vectored
//! exception handler that prints a stack trace + offending RIP when
//! the JIT-compiled program faults (typically `EXCEPTION_ACCESS_VIOLATION`).
//!
//! The handler runs as `EXCEPTION_CONTINUE_SEARCH` so the OS still
//! terminates the process — this is purely a diagnostic preview
//! ahead of the normal teardown. Symbol resolution uses
//! `backtrace::resolve` which routes through `dbghelp.dll` on
//! Windows; runtime-side frames get nice names, JIT'd frames show
//! up as raw addresses (no debug info attached to the JIT module).
//!
//! Opt in via `ILANG_TRACE_CRASH=1`. The installer is idempotent and
//! a no-op outside Windows.

use std::sync::Once;

static INIT: Once = Once::new();

/// Install the crash reporter on first call. Subsequent calls are
/// no-ops. Reads `ILANG_TRACE_CRASH` once — set the env var to opt in.
pub fn install_if_enabled() {
    if std::env::var("ILANG_TRACE_CRASH").is_err() {
        return;
    }
    INIT.call_once(|| {
        #[cfg(windows)]
        unsafe {
            install_windows();
        }
    });
}

#[cfg(windows)]
unsafe fn install_windows() {
    use windows_sys::Win32::System::Diagnostics::Debug::AddVectoredExceptionHandler;
    unsafe { AddVectoredExceptionHandler(1, Some(crash_handler)); }
}

#[cfg(windows)]
unsafe extern "system" fn crash_handler(
    info: *mut windows_sys::Win32::System::Diagnostics::Debug::EXCEPTION_POINTERS,
) -> i32 {
    use windows_sys::Win32::Foundation::{
        EXCEPTION_ACCESS_VIOLATION, EXCEPTION_STACK_OVERFLOW,
    };
    const EXCEPTION_CONTINUE_SEARCH: i32 = 0;

    let info = unsafe { &*info };
    let rec = unsafe { &*info.ExceptionRecord };
    let code = rec.ExceptionCode as u32;

    // Only fire on access violations / stack overflows / illegal
    // instructions — let the rest (FP, breakpoints from a real
    // debugger, etc.) flow through to the system handler.
    let relevant = matches!(
        code,
        x if x == EXCEPTION_ACCESS_VIOLATION as u32
          || x == EXCEPTION_STACK_OVERFLOW as u32
          || x == 0xC000_001D /* ILLEGAL_INSTRUCTION */
          || x == 0xC000_0094 /* INT_DIVIDE_BY_ZERO */
    );
    if !relevant {
        return EXCEPTION_CONTINUE_SEARCH;
    }

    // Print code + address + RIP. ExceptionAddress is where the
    // faulting instruction lives; for an access violation,
    // ExceptionInformation[0] is the access kind (0=read, 1=write,
    // 8=DEP) and [1] is the faulting data address.
    let mut buf = String::new();
    use std::fmt::Write as _;
    let _ = writeln!(buf, "");
    let _ = writeln!(buf, "=== ilang crash handler ===");
    let _ = writeln!(
        buf,
        "exception code = 0x{:08X} ({})",
        code,
        exception_name(code)
    );
    let _ = writeln!(
        buf,
        "instruction RIP = 0x{:016X}",
        rec.ExceptionAddress as usize
    );
    if code == EXCEPTION_ACCESS_VIOLATION as u32 && rec.NumberParameters >= 2 {
        let kind = rec.ExceptionInformation[0];
        let addr = rec.ExceptionInformation[1];
        let kind_str = match kind {
            0 => "read",
            1 => "write",
            8 => "execute (DEP)",
            _ => "?",
        };
        let _ = writeln!(buf, "fault address  = 0x{:016X} ({})", addr, kind_str);
    }
    let _ = writeln!(buf, "");
    let _ = writeln!(buf, "stack trace (runtime frames symbolicate,");
    let _ = writeln!(buf, "JIT frames show as raw RIP):");

    // Walk the stack via `backtrace`. This is reentrant-unsafe in the
    // strict sense but the process is about to die anyway — best
    // effort is the goal.
    let mut frame_no = 0usize;
    backtrace::trace(|frame| {
        let ip = frame.ip() as usize;
        let mut name = format!("0x{ip:016X}");
        backtrace::resolve(frame.ip(), |sym| {
            if let Some(n) = sym.name() {
                name = format!("{n}");
            }
        });
        let _ = writeln!(buf, "  #{:02} {ip:#018x}  {name}", frame_no);
        frame_no += 1;
        frame_no < 40
    });
    let _ = writeln!(buf, "=== end crash report ===");

    // Single write to stderr keeps the lines together when other
    // threads might also be printing.
    let _ = std::io::Write::write_all(&mut std::io::stderr(), buf.as_bytes());
    let _ = std::io::Write::flush(&mut std::io::stderr());

    EXCEPTION_CONTINUE_SEARCH
}

#[cfg(windows)]
fn exception_name(code: u32) -> &'static str {
    match code {
        0xC000_0005 => "ACCESS_VIOLATION",
        0xC000_001D => "ILLEGAL_INSTRUCTION",
        0xC000_0025 => "NONCONTINUABLE_EXCEPTION",
        0xC000_008C => "ARRAY_BOUNDS_EXCEEDED",
        0xC000_0094 => "INT_DIVIDE_BY_ZERO",
        0xC000_00FD => "STACK_OVERFLOW",
        0xC000_0135 => "DLL_NOT_FOUND",
        _ => "?",
    }
}
