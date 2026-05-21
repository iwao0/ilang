/// C standard `<wctype.h>` — wide-character classification.
///
/// Inputs are `wint_t`; we use `u32`. Returns are 0 / non-zero
/// boolean-style.

@extern(C, "c") {
    @lib @optional pub fn iswalpha(c: u32): i32
    @lib @optional pub fn iswdigit(c: u32): i32
    @lib @optional pub fn iswalnum(c: u32): i32
    @lib @optional pub fn iswspace(c: u32): i32
    @lib @optional pub fn iswxdigit(c: u32): i32
    @lib @optional pub fn iswupper(c: u32): i32
    @lib @optional pub fn iswlower(c: u32): i32
    @lib @optional pub fn iswcntrl(c: u32): i32
    @lib @optional pub fn iswprint(c: u32): i32
    @lib @optional pub fn iswgraph(c: u32): i32
    @lib @optional pub fn iswpunct(c: u32): i32

    @lib @optional pub fn towlower(c: u32): u32
    @lib @optional pub fn towupper(c: u32): u32
}
