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

// Captured ASLR slide of the main executable, recorded at install
// time so the (async-signal-safe) handler can print it alongside
// raw return addresses. `atos -o <bin> <addr - slide>` then resolves.
#[cfg(target_os = "macos")]
static MAIN_SLIDE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn _dyld_get_image_vmaddr_slide(image_index: u32) -> isize;
}

#[cfg(unix)]
unsafe fn install_unix() {
    let h: extern "C" fn(i32) = unix_handler;
    let h_addr = h as usize;
    for sig in [SIGSEGV, SIGBUS, SIGABRT, SIGILL, SIGFPE] {
        unsafe {
            signal(sig, h_addr);
        }
    }
    #[cfg(target_os = "macos")]
    {
        let slide = unsafe { _dyld_get_image_vmaddr_slide(0) } as usize;
        MAIN_SLIDE.store(slide, std::sync::atomic::Ordering::Relaxed);
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

    // Async-signal-safe stack trace by walking the AAPCS64 frame
    // pointer chain. No heap alloc / no mutex / no symbol resolution
    // — just raw return addresses written via `write(2)`. Symbolicate
    // later with `atos -o target/release/ilang <addr>`. Skipped on
    // non-aarch64; the backtrace crate is unsafe here (see history
    // for the SIGKILL-masking regression we hit when we tried it).
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let mut stack_buf = [0u8; 1024];
        let stack_n = walk_frames_aarch64(&mut stack_buf);
        write(2, stack_buf.as_ptr(), stack_n);
    }

    // Restore default disposition and re-raise so the OS terminates
    // with the expected `WIFSIGNALED` signal code.
    unsafe {
        signal(sig, SIG_DFL);
        raise(sig);
    }
}

/// AAPCS64 frame walker. ARM64 stack frame layout for any function
/// that established a frame pointer is:
///
/// ```text
/// [fp + 0]  = saved fp (chain to caller)
/// [fp + 8]  = saved lr (return address into caller)
/// ```
///
/// We follow the chain up to 32 frames, with three defenses against
/// runaway walks if the chain is corrupted by the crashing code:
///   - fp must be non-NULL and 8-byte aligned
///   - fp must point into the user-space range
///   - max 32 iterations
///
/// Output format (each line written to `out`):
///
/// ```text
/// ilang: stack (run `atos -o <bin> <addrs>` to symbolicate):
///   #00 0x000000010014abcd
///   #01 0x000000010014ef01
///   ...
/// ```
///
/// Returns the number of bytes written into `out`.
#[cfg(all(unix, target_arch = "aarch64"))]
unsafe fn walk_frames_aarch64(out: &mut [u8]) -> usize {
    let mut fp: usize;
    unsafe {
        std::arch::asm!("mov {}, x29", out(reg) fp, options(nomem, nostack, preserves_flags));
    }

    let mut n = 0usize;
    n += write_bytes(b"ilang: stack (atos -o <bin> -l 0x", &mut out[n..]);
    #[cfg(target_os = "macos")]
    {
        let load = 0x0000_0001_0000_0000usize
            .wrapping_add(MAIN_SLIDE.load(std::sync::atomic::Ordering::Relaxed));
        n += write_hex16(load as u64, &mut out[n..]);
    }
    #[cfg(not(target_os = "macos"))]
    {
        n += write_bytes(b"0000000100000000", &mut out[n..]);
    }
    n += write_bytes(b" <addrs>):\n", &mut out[n..]);

    let mut frame_no = 0usize;
    while frame_no < 32 {
        // Sanity check: fp must be aligned and inside the user-space
        // range. macOS arm64 user space sits roughly in
        // 0x0000_0001_0000_0000..0x0000_0007_ffff_ffff.
        if fp == 0 || fp & 0x7 != 0 || fp < 0x0000_0001_0000_0000 || fp > 0x0000_0007_ffff_ffff {
            break;
        }
        let next_fp = unsafe { *(fp as *const usize) };
        let ret_addr = unsafe { *((fp + 8) as *const usize) };

        // Write line: "  #NN 0x0123456789abcdef\n"
        if n + 26 > out.len() {
            break;
        }
        n += write_bytes(b"  #", &mut out[n..]);
        if frame_no < 10 {
            out[n] = b'0';
            n += 1;
        }
        n += write_dec(frame_no as i64, &mut out[n..]);
        n += write_bytes(b" 0x", &mut out[n..]);
        n += write_hex16(ret_addr as u64, &mut out[n..]);
        if n < out.len() {
            out[n] = b'\n';
            n += 1;
        }

        if next_fp <= fp {
            // Frame pointer didn't grow — corrupt chain or end of stack.
            break;
        }
        fp = next_fp;
        frame_no += 1;
    }
    n
}

#[cfg(unix)]
fn write_bytes(src: &[u8], out: &mut [u8]) -> usize {
    let count = src.len().min(out.len());
    out[..count].copy_from_slice(&src[..count]);
    count
}

#[cfg(unix)]
fn write_hex16(v: u64, out: &mut [u8]) -> usize {
    if out.len() < 16 {
        return 0;
    }
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for i in 0..16 {
        let shift = (15 - i) * 4;
        out[i] = HEX[((v >> shift) & 0xf) as usize];
    }
    16
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
