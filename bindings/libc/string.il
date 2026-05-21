/// C standard `<string.h>` — byte / string memory primitives.
///
/// Every symbol below is in the ANSI/ISO C surface, present on
/// Linux (`libc.so.6`), macOS (`libSystem.B.dylib`), and Windows
/// (`msvcrt.dll` / `ucrtbase.dll`) without a separate package.

@extern(C) {
    /// Copy `n` bytes from `src` to `dst`. Behaviour is undefined
    /// when the ranges overlap; use `memmove` for that case.
    @lib("c") pub fn memcpy(
        dst: *void,
        src: *const void,
        n:   size_t
    ): *void

    /// Like `memcpy` but defined for overlapping ranges.
    @lib("c") pub fn memmove(
        dst: *void,
        src: *const void,
        n:   size_t
    ): *void

    /// Fill `n` bytes of `buf` with the (unsigned char) value `byte`.
    @lib("c") pub fn memset(
        buf:  *void,
        byte: i32,
        n:    size_t
    ): *void

    /// Compare the first `n` bytes of `a` and `b`. Returns 0 when
    /// equal, negative if the first differing byte in `a` is
    /// less than the corresponding byte in `b`, positive otherwise.
    @lib("c") pub fn memcmp(
        a: *const void,
        b: *const void,
        n: size_t
    ): i32

    /// Return the length of `s` excluding the terminating NUL.
    @lib("c") pub fn strlen(s: *const char): size_t

    /// `strcmp(a, b)` — 0 if equal, ±1 lexicographic ordering.
    @lib("c") pub fn strcmp(a: *const char, b: *const char): i32

    /// `strncmp` — compare at most `n` bytes.
    @lib("c") pub fn strncmp(
        a: *const char,
        b: *const char,
        n: size_t
    ): i32
}
