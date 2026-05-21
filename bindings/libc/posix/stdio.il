/// POSIX additions to `<stdio.h>`.

@extern(C, "c") {
    /// POSIX `fileno(stream)` → underlying OS file descriptor.
    /// Windows exposes the same symbol as `_fileno`.
    @lib @optional pub fn fileno(stream: *void): i32

    /// Open a stream over an existing fd. `mode` matches `fopen`.
    @lib @optional pub fn fdopen(fd: i32, mode: *const char): *void

    /// `popen(cmd, mode)` — pipe to / from a sub-shell.
    @lib @optional pub fn popen(cmd: *const char, mode: *const char): *void
    @lib @optional pub fn pclose(stream: *void): i32

    /// Read a line from `stream`, growing `*lineptr` as needed
    /// (via `realloc`). Returns the number of bytes including the
    /// newline, or -1 on EOF / error. The caller frees `*lineptr`.
    @lib @optional pub fn getline(lineptr: **char, n: *size_t, stream: *void): i64

    /// Same but stops at `delim` instead of `\n`.
    @lib @optional pub fn getdelim(
        lineptr: **char,
        n:       *size_t,
        delim:   i32,
        stream:  *void
    ): i64

    /// In-memory FILE — writes accumulate into a malloc'd buffer
    /// that's published when the stream is fflushed / fclosed.
    @lib @optional pub fn open_memstream(bufp: **char, sizep: *size_t): *void
    @lib @optional pub fn fmemopen(buf: *void, size: size_t, mode: *const char): *void

    /// Format into a freshly malloc'd buffer; assign the pointer
    /// to `*strp`. Caller frees.
    @lib @optional pub fn asprintf(strp: **char, fmt: *const char, ...): i32
}
