/// C99 `<inttypes.h>` — integer functions on `intmax_t` / `uintmax_t`.
///
/// The type aliases (`int8_t`, `uint64_t`, …) are typedef-only in C
/// and are already covered by ilang's primitives (`i8`, `u64`, …).
/// The `PRI*` / `SCN*` format-string macros are preprocessor-only.
///
/// `intmax_t` is at least 64-bit on every supported target; we
/// model it as `i64` / `u64`.

@extern(C, "c") {
    /// imaxdiv(numer, denom) — quotient + remainder pair.
    pub struct imaxdiv_t {
        quot: i64
        rem:  i64
    }

    /// Absolute value of `intmax_t`. UB at INT64_MIN.
    @lib pub fn imaxabs(x: i64): i64

    @lib pub fn imaxdiv(numer: i64, denom: i64): imaxdiv_t

    /// Parse `intmax_t` from `s` with arbitrary base (0 = auto).
    /// `endptr` (out) points at the first unparsed character.
    @lib pub fn strtoimax(s: *const char, endptr: **char, base: i32): i64
    @lib pub fn strtoumax(s: *const char, endptr: **char, base: i32): u64

    /// Wide-string variants.
    @lib @optional pub fn wcstoimax(s: *const u32, endptr: **u32, base: i32): i64
    @lib @optional pub fn wcstoumax(s: *const u32, endptr: **u32, base: i32): u64
}
