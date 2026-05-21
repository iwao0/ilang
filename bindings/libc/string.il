/// C standard `<string.h>` — byte / string memory primitives.
///
/// Every symbol below is in the ANSI/ISO C surface, present on
/// Linux (`libc.so.6`), macOS (`libSystem.B.dylib`), and Windows
/// (`msvcrt.dll` / `ucrtbase.dll`) without a separate package.

@extern(C, "c") {
    /// Copy `n` bytes from `src` to `dst`. Behaviour is undefined
    /// when the ranges overlap; use `memmove` for that case.
    @lib pub fn memcpy(
        dst: *void,
        src: *const void,
        n:   size_t
    ): *void

    /// Like `memcpy` but defined for overlapping ranges.
    @lib pub fn memmove(
        dst: *void,
        src: *const void,
        n:   size_t
    ): *void

    /// Fill `n` bytes of `buf` with the (unsigned char) value `byte`.
    @lib pub fn memset(
        buf:  *void,
        byte: i32,
        n:    size_t
    ): *void

    /// Compare the first `n` bytes of `a` and `b`. Returns 0 when
    /// equal, negative if the first differing byte in `a` is
    /// less than the corresponding byte in `b`, positive otherwise.
    @lib pub fn memcmp(
        a: *const void,
        b: *const void,
        n: size_t
    ): i32

    /// Scan the first `n` bytes of `s` for `c` (an unsigned char).
    /// Returns a pointer to the match or NULL if not found.
    @lib pub fn memchr(
        s: *const void,
        c: i32,
        n: size_t
    ): *void

    /// Return the length of `s` excluding the terminating NUL.
    @lib pub fn strlen(s: *const char): size_t

    /// `strcmp(a, b)` — 0 if equal, ±1 lexicographic ordering.
    @lib pub fn strcmp(a: *const char, b: *const char): i32

    /// `strncmp` — compare at most `n` bytes.
    @lib pub fn strncmp(
        a: *const char,
        b: *const char,
        n: size_t
    ): i32

    /// Locate the first occurrence of `c` in `s`. NUL terminator
    /// is searched too; pass `0` to find the end of the string.
    @lib pub fn strchr(s: *const char, c: i32): *const char

    /// Locate the last occurrence of `c` in `s`.
    @lib pub fn strrchr(s: *const char, c: i32): *const char

    /// Locate the first occurrence of `needle` in `haystack`.
    @lib pub fn strstr(haystack: *const char, needle: *const char): *const char

    /// Span over the initial run of `s` made of bytes in `accept`.
    @lib pub fn strspn(s: *const char, accept: *const char): size_t

    /// Span over the initial run of `s` made of bytes NOT in `reject`.
    @lib pub fn strcspn(s: *const char, reject: *const char): size_t

    /// Locate the first byte in `s` that's also in `accept`.
    @lib pub fn strpbrk(s: *const char, accept: *const char): *const char

    /// Copy `src` (including the NUL) to `dst`; returns `dst`.
    /// Caller is responsible for `dst` having room; use `strncpy`
    /// when there's a known bound.
    @lib pub fn strcpy(dst: *char, src: *const char): *char

    /// Copy at most `n` bytes from `src` to `dst`, padding with NUL
    /// if `src` is shorter; does NOT NUL-terminate when `src`
    /// reaches `n` bytes without a NUL itself.
    @lib pub fn strncpy(dst: *char, src: *const char, n: size_t): *char

    /// Append `src` to the end of `dst` (which must contain a NUL).
    @lib pub fn strcat(dst: *char, src: *const char): *char

    /// Bounded `strcat` — append at most `n` bytes from `src`.
    @lib pub fn strncat(dst: *char, src: *const char, n: size_t): *char

    /// Localised error string for an `errno` value.
    @lib pub fn strerror(errnum: i32): *const char

    // ── Tokenisation ────────────────────────────────────────────

    /// Destructive tokeniser. Pass `s` on the first call, NULL on
    /// subsequent calls. Returns NULL when no more tokens remain.
    /// Not thread-safe; use `strtok_r` (POSIX) for re-entrancy.
    @lib pub fn strtok(s: *char, delim: *const char): *char

    // ── Locale-aware compare / transform ────────────────────────

    /// Compare per the current `LC_COLLATE` locale.
    @lib pub fn strcoll(a: *const char, b: *const char): i32

    /// Transform `src` so that a byte-wise compare of two transformed
    /// strings yields the same result as `strcoll`. Returns the
    /// length (excluding NUL) the transform would need; `n` may be
    /// 0 to query the size.
    @lib pub fn strxfrm(dst: *char, src: *const char, n: size_t): size_t
}
