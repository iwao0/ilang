/// C11 `<uchar.h>` — UTF-16 / UTF-32 code-unit conversion.
///
/// Bridges multi-byte (locale-encoded) sequences to / from `char16_t`
/// (UTF-16 code unit, modelled as `u16`) and `char32_t` (UTF-32
/// scalar, modelled as `u32`).
///
/// Each call advances `mbstate_t` to allow streaming over a
/// surrogate pair (UTF-16) or a multi-byte sequence (UTF-8 etc.).
/// Pass an `mbstate_t` zero-initialised on the first call of a
/// fresh conversion.

@extern(C, "c") {
    /// Opaque conversion state. glibc / musl: 8 bytes; ucrt: 16.
    /// Reserve 32 to cover all known layouts.
    pub struct mbstate_t {
        _opaque: u8[32]
    }

    /// Decode one `char16_t` from `s`. Returns:
    ///   * 0      — `*s` is a NUL byte
    ///   * 1..=n  — bytes consumed
    ///   * -1 cast — encoding error (errno = EILSEQ)
    ///   * -2 cast — incomplete (need more input)
    ///   * -3 cast — wrote the trailing surrogate; consumed 0 bytes
    @lib @optional pub fn mbrtoc16(
        pc16:  *u16,
        s:     *const char,
        n:     size_t,
        state: *mbstate_t
    ): size_t

    /// Encode `c16` as multi-byte bytes into `s`. Returns the
    /// number of bytes written or (size_t)-1 on error.
    @lib @optional pub fn c16rtomb(
        s:     *char,
        c16:   u16,
        state: *mbstate_t
    ): size_t

    /// Same as `mbrtoc16` but decodes a full UTF-32 scalar.
    @lib @optional pub fn mbrtoc32(
        pc32:  *u32,
        s:     *const char,
        n:     size_t,
        state: *mbstate_t
    ): size_t

    @lib @optional pub fn c32rtomb(
        s:     *char,
        c32:   u32,
        state: *mbstate_t
    ): size_t
}
