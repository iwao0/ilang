/// C standard `<ctype.h>` — single-byte character classification.
///
/// Inputs are `int` because the C contract accepts any value
/// representable as `unsigned char` or `EOF`. Passing a `char` from
/// ilang requires `c as i32`. Returns are non-zero / 0 boolean-style.

@extern(C, "c") {
    @lib pub fn isalpha(c: i32): i32
    @lib pub fn isdigit(c: i32): i32
    @lib pub fn isalnum(c: i32): i32
    @lib pub fn isspace(c: i32): i32
    @lib pub fn isxdigit(c: i32): i32
    @lib pub fn isupper(c: i32): i32
    @lib pub fn islower(c: i32): i32
    @lib pub fn iscntrl(c: i32): i32
    @lib pub fn isprint(c: i32): i32
    @lib pub fn isgraph(c: i32): i32
    @lib pub fn ispunct(c: i32): i32

    /// To-lower / to-upper. Returns the input unchanged when it
    /// has no case mapping.
    @lib pub fn tolower(c: i32): i32
    @lib pub fn toupper(c: i32): i32
}
