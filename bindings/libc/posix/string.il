/// POSIX additions to `<string.h>` / `<strings.h>`.

@extern(C, "c") {
    /// Duplicate a NUL-terminated string into a fresh `malloc`'d
    /// buffer. Free with `free`. POSIX since 2008; ucrt exposes it
    /// as `_strdup` (without the leading underscore the symbol
    /// isn't present on most Windows builds).
    @lib @optional pub fn strdup(s: *const char): *char

    /// Duplicate at most `n` bytes of `s`; the result is always
    /// NUL-terminated.
    @lib @optional pub fn strndup(s: *const char, n: size_t): *char

    /// Length of `s` with a hard cap of `maxlen` — returns `maxlen`
    /// if no NUL is found in the first `maxlen` bytes.
    @lib @optional pub fn strnlen(s: *const char, maxlen: size_t): size_t

    /// Re-entrant tokeniser. `saveptr` holds the cross-call state.
    @lib @optional pub fn strtok_r(
        s:       *char,
        delim:   *const char,
        saveptr: **char
    ): *char

    /// strerror_r — thread-safe `strerror`. Returns 0 on success.
    @lib @optional pub fn strerror_r(errnum: i32, buf: *char, buflen: size_t): i32

    // ── Case-insensitive compare (BSD / POSIX) ───────────────

    @lib @optional pub fn strcasecmp(a: *const char, b: *const char): i32
    @lib @optional pub fn strncasecmp(a: *const char, b: *const char, n: size_t): i32

    // ── Memory ─────────────────────────────────────────────

    /// Copy `src` to `dst` until `c` is encountered or `n` bytes
    /// are copied. Returns the pointer one past the copied `c` in
    /// `dst`, or NULL when `c` wasn't found.
    @lib @optional pub fn memccpy(
        dst: *void,
        src: *const void,
        c:   i32,
        n:   size_t
    ): *void
}
