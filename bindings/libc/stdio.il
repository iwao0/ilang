/// C standard `<stdio.h>` — text + binary stream I/O.
///
/// `FILE` is an opaque handle; the three pre-opened streams
/// (`stdin` / `stdout` / `stderr`) are accessed as
/// `*const *FILE` globals on POSIX, and through
/// `__acrt_iob_func(0/1/2)` on Windows. Ilang's `console.log`
/// covers the common case; reach for this module when you need
/// formatted output, binary I/O, or precise control over a stream.

@extern(C, "c") {
    /// Opaque FILE stream handle.
    pub struct FILE {}

    // ── Stream lifecycle ─────────────────────────────────────────

    /// Open `path` with one of the standard mode strings:
    ///   "r"  / "rb"   read
    ///   "w"  / "wb"   truncate + write
    ///   "a"  / "ab"   append
    ///   "r+" / "w+"   read-write variants
    /// Returns NULL on failure (consult `errno`).
    @lib pub fn fopen(path: *const char, mode: *const char): *FILE

    /// Re-bind `stream` to `path` (e.g. redirect stdout).
    @lib pub fn freopen(path: *const char, mode: *const char, stream: *FILE): *FILE

    /// Close a stream. Returns 0 on success, EOF (-1) on error.
    /// The stream is invalidated even on error.
    @lib pub fn fclose(stream: *FILE): i32

    /// Flush stream's output buffers. `NULL` flushes all streams.
    @lib pub fn fflush(stream: *FILE): i32

    // ── Positioning ──────────────────────────────────────────────

    /// SEEK_SET / SEEK_CUR / SEEK_END pass-through constants.
    @lib pub fn fseek(stream: *FILE, offset: i64, whence: i32): i32

    /// Current byte offset. Returns -1 on error.
    @lib pub fn ftell(stream: *FILE): i64

    /// Rewind to the start (== `fseek(stream, 0, SEEK_SET)` + clear
    /// the EOF / error indicators).
    @lib pub fn rewind(stream: *FILE)

    // ── Read / write ────────────────────────────────────────────

    /// Read up to `nmemb` items of `size` bytes from `stream` into
    /// `ptr`. Returns the number of items actually read; check
    /// `feof` / `ferror` to disambiguate short reads.
    @lib pub fn fread(ptr: *void, size: size_t, nmemb: size_t, stream: *FILE): size_t

    @lib pub fn fwrite(ptr: *const void, size: size_t, nmemb: size_t, stream: *FILE): size_t

    /// Read one byte (as `unsigned char` cast to int). Returns EOF
    /// on end-of-stream or error.
    @lib pub fn fgetc(stream: *FILE): i32

    /// Write a byte. Returns the written byte or EOF on error.
    @lib pub fn fputc(c: i32, stream: *FILE): i32

    /// Push `c` back into `stream` so the next `fgetc` returns it.
    @lib pub fn ungetc(c: i32, stream: *FILE): i32

    /// Read at most `size - 1` bytes (stops at newline or EOF) into
    /// `buf`; NUL-terminates. Returns `buf` or NULL on EOF / error.
    @lib pub fn fgets(buf: *char, size: i32, stream: *FILE): *char

    /// Write the NUL-terminated `s` plus an implicit newline.
    @lib pub fn fputs(s: *const char, stream: *FILE): i32

    // ── EOF / error state ────────────────────────────────────────

    /// 0 / non-zero EOF indicator.
    @lib pub fn feof(stream: *FILE): i32

    /// 0 / non-zero error indicator. `clearerr` resets both.
    @lib pub fn ferror(stream: *FILE): i32

    @lib pub fn clearerr(stream: *FILE)

    // ── Formatted output ─────────────────────────────────────────

    /// `printf` family. ilang treats `...` as a variadic tail; pass
    /// arguments in the C convention.
    @lib pub fn printf(fmt: *const char, ...): i32
    @lib pub fn fprintf(stream: *FILE, fmt: *const char, ...): i32
    @lib pub fn sprintf(buf: *char, fmt: *const char, ...): i32

    /// Bounded `sprintf`. Always NUL-terminates when `n > 0`. Returns
    /// the number of bytes that *would* have been written excluding
    /// the NUL; ≥ `n` indicates truncation.
    @lib pub fn snprintf(buf: *char, n: size_t, fmt: *const char, ...): i32

    // ── Formatted input ──────────────────────────────────────────

    @lib pub fn scanf(fmt: *const char, ...): i32
    @lib pub fn fscanf(stream: *FILE, fmt: *const char, ...): i32
    @lib pub fn sscanf(s: *const char, fmt: *const char, ...): i32

    // ── Filesystem helpers ───────────────────────────────────────

    /// Remove a file. Returns 0 on success.
    @lib pub fn remove(path: *const char): i32

    /// Rename / move within a filesystem. Returns 0 on success.
    @lib pub fn rename(oldp: *const char, newp: *const char): i32

    /// Build a unique temp filename in `buf` (must hold at least
    /// `L_tmpnam` bytes). Prefer `mkstemp` on POSIX or
    /// `GetTempFileNameA` on Windows for new code; `tmpnam` is
    /// kept here for completeness.
    @lib pub fn tmpnam(buf: *char): *char

    /// Open a temporary file in `wb+` mode. Auto-deleted on close.
    @lib pub fn tmpfile(): *FILE

    // ── Convenience char / line helpers ──────────────────────────

    /// One-byte read from stdin. EOF on end of stream.
    @lib pub fn getchar(): i32

    /// One-byte write to stdout.
    @lib pub fn putchar(c: i32): i32

    /// Write `s` plus a newline to stdout. Returns ≥0 / EOF.
    @lib pub fn puts(s: *const char): i32

    /// Print a NUL-terminated message followed by ": " and
    /// `strerror(errno)` to stderr.
    @lib pub fn perror(s: *const char)

    // ── Buffering ────────────────────────────────────────────────

    /// Set buffer + mode for `stream`. `mode`:
    ///   0 = _IOFBF (full),
    ///   1 = _IOLBF (line),
    ///   2 = _IONBF (none).
    @lib pub fn setvbuf(stream: *FILE, buf: *char, mode: i32, size: size_t): i32

    /// `setbuf(s, NULL)` ≡ disable buffering; non-NULL ≡ full,
    /// BUFSIZ-sized buffer. Prefer `setvbuf`.
    @lib pub fn setbuf(stream: *FILE, buf: *char)

    // ── Positioning (extended) ───────────────────────────────────

    /// Opaque file position type. Layout differs per platform; treat
    /// as a 16-byte cookie obtained from `fgetpos` and consumed by
    /// `fsetpos`. The struct exists only so callers can size storage.
    pub struct fpos_t {
        _opaque0: i64
        _opaque1: i64
    }

    @lib pub fn fgetpos(stream: *FILE, pos: *fpos_t): i32
    @lib pub fn fsetpos(stream: *FILE, pos: *const fpos_t): i32

    // ── Variadic (va_list) variants ──────────────────────────────
    //
    // `va_list` doesn't have a portable ABI in pure ilang yet, so
    // these take `*void` for the list pointer. Build the va_list on
    // the call site through inline C / a forwarding wrapper, or call
    // the non-`v` form directly when possible.

    @lib pub fn vprintf(fmt: *const char, ap: *void): i32
    @lib pub fn vfprintf(stream: *FILE, fmt: *const char, ap: *void): i32
    @lib pub fn vsprintf(buf: *char, fmt: *const char, ap: *void): i32
    @lib pub fn vsnprintf(buf: *char, n: size_t, fmt: *const char, ap: *void): i32
    @lib pub fn vscanf(fmt: *const char, ap: *void): i32
    @lib pub fn vfscanf(stream: *FILE, fmt: *const char, ap: *void): i32
    @lib pub fn vsscanf(s: *const char, fmt: *const char, ap: *void): i32
}
