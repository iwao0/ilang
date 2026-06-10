//! Best-effort crash reporter. On Windows installs a vectored
//! exception handler that prints a stack trace + offending RIP; on
//! Unix installs a `sigaction` for SIGSEGV / SIGBUS / SIGABRT that
//! writes the signal name + PID to stderr before re-raising the
//! signal under the default disposition.
//!
//! Both paths are best-effort — the process is about to die anyway —
//! so the signal-side handler only uses async-signal-safe primitives
//! (`write(2)` to fd 2, raw integer formatting). The Windows handler
//! runs as `EXCEPTION_CONTINUE_SEARCH` so the OS still terminates
//! the process; the Unix handler restores the default disposition
//! and re-raises so libsystem-malloc's own diagnostic prints stay
//! intact and the parent harness still sees `WIFSIGNALED`.
//!
//! Opt in via `ILANG_TRACE_CRASH=1`. The installer is idempotent.

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
        #[cfg(unix)]
        unsafe {
            install_unix();
        }
    });
}

// ─── Unix / macOS / Linux ────────────────────────────────────────

#[cfg(unix)]
const SIGSEGV: i32 = 11;
#[cfg(unix)]
const SIGBUS: i32 = 10;
#[cfg(unix)]
const SIGABRT: i32 = 6;
#[cfg(unix)]
const SIGILL: i32 = 4;
#[cfg(unix)]
const SIGFPE: i32 = 8;

// `signal()` is declared with `usize` for the handler argument so we
// can pass `0` (= `SIG_DFL`) without transmuting to a fn pointer
// (which is UB for the null pointer). All callers cast their fn
// pointer to `usize` at the call site.
#[cfg(unix)]
unsafe extern "C" {
    fn signal(signum: i32, handler: usize) -> usize;
    fn write(fd: i32, buf: *const u8, count: usize) -> isize;
    fn raise(signum: i32) -> i32;
    fn getpid() -> i32;
}

#[cfg(unix)]
const SIG_DFL: usize = 0;

#[cfg(unix)]
unsafe fn install_unix() {
    let h: extern "C" fn(i32) = unix_handler;
    let h_addr = h as usize;
    for sig in [SIGSEGV, SIGBUS, SIGABRT, SIGILL, SIGFPE] {
        unsafe {
            signal(sig, h_addr);
        }
    }
}

#[cfg(unix)]
fn signal_name(sig: i32) -> &'static [u8] {
    match sig {
        SIGSEGV => b"SIGSEGV",
        SIGBUS => b"SIGBUS",
        SIGABRT => b"SIGABRT",
        SIGILL => b"SIGILL",
        SIGFPE => b"SIGFPE",
        _ => b"?",
    }
}

/// async-signal-safe: only raw fd writes + integer formatting via a
/// tiny stack buffer. Restores the default disposition for this
/// signal and re-raises so the OS terminates with the expected
/// `WIFSIGNALED`, leaving any libsystem-printed diagnostic line
/// already on stderr intact in the parent's pipe.
#[cfg(unix)]
extern "C" fn unix_handler(sig: i32) {
    // Format: "ilang: caught <NAME> (signal <sig>) pid=<pid>\n"
    let mut buf = [0u8; 96];
    let mut n = 0usize;

    let prefix = b"ilang: caught ";
    for &b in prefix {
        if n < buf.len() {
            buf[n] = b;
            n += 1;
        }
    }
    for &b in signal_name(sig) {
        if n < buf.len() {
            buf[n] = b;
            n += 1;
        }
    }
    let mid = b" (signal ";
    for &b in mid {
        if n < buf.len() {
            buf[n] = b;
            n += 1;
        }
    }
    n += write_dec(sig as i64, &mut buf[n..]);
    let pid_pre = b") pid=";
    for &b in pid_pre {
        if n < buf.len() {
            buf[n] = b;
            n += 1;
        }
    }
    let pid = unsafe { getpid() };
    n += write_dec(pid as i64, &mut buf[n..]);
    if n < buf.len() {
        buf[n] = b'\n';
        n += 1;
    }
    unsafe {
        write(2, buf.as_ptr(), n);
    }

    // Restore default disposition and re-raise so the OS terminates
    // with the expected `WIFSIGNALED` signal code. We deliberately
    // skip the backtrace step: `backtrace::trace` allocates and
    // resolves symbols, neither of which is async-signal-safe, and
    // under parallel-spawn load it produced SIGKILL'd children
    // (libsystem's malloc reentered itself). The plain signal-name
    // line is already enough for the parent harness to tell
    // SIGABRT / SIGSEGV / SIGBUS apart from a plain non-zero exit.
    unsafe {
        signal(sig, SIG_DFL);
        raise(sig);
    }
}

#[cfg(unix)]
fn write_dec(mut v: i64, out: &mut [u8]) -> usize {
    if out.is_empty() {
        return 0;
    }
    if v == 0 {
        out[0] = b'0';
        return 1;
    }
    let neg = v < 0;
    if neg {
        v = -v;
    }
    let mut tmp = [0u8; 20];
    let mut i = 0;
    while v > 0 && i < tmp.len() {
        tmp[i] = b'0' + (v % 10) as u8;
        v /= 10;
        i += 1;
    }
    let mut n = 0;
    if neg && n < out.len() {
        out[n] = b'-';
        n += 1;
    }
    while i > 0 && n < out.len() {
        i -= 1;
        out[n] = tmp[i];
        n += 1;
    }
    n
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
