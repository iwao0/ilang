// Per-iteration `s = s + n.toString()` — the in-place reassign concat.
//
// `s = s + <expr>` on a string Local routes through the
// `StrConcatInplace` runtime helper, which grows `s`'s own buffer with
// the rhs bytes instead of allocating a fresh result each time. But it
// only COPIES the rhs; it does not consume it. Pre-fix the lowering
// never released a FRESH rhs (e.g. `n.toString()`), so every iteration
// leaked one string into the rc-tracked registry (linear growth:
// N iters -> N leaked). An interned literal rhs (`"-end"`) and a
// borrowed-var rhs were already fine — they aren't fresh.
//
// This is a sibling of `leak_string_concat_loop.il`, which covers the
// non-reassign `let s = a + b` (lower_binary StrConcat) path; the
// in-place reassign path is separate codegen.

use std.test as test

// Fresh rhs (toString temp) must be released after the in-place concat.
fn buildFresh(n: i64): string {
    let s = "v"
    s = s + n.toString()      // fresh rhs — was leaked
    s = s + "-end"            // interned literal — never leaked
    s
}

// Borrowed rhs must NOT be released (it's still owned by `other`).
fn buildBorrowed(): string {
    let s = "a"
    let other = "BB"
    s = s + other             // borrowed rhs — must stay alive
    s = s + other
    s + other.length.toString()
}

// Values stay correct (no corruption from an over-release).
test.expectStr(buildFresh(5), "v5-end")
test.expectStr(buildBorrowed(), "aBBBB2")

let baseline = test.liveStringCount()
let i: i64 = 0
while i < 1000 {
    let a = buildFresh(i)
    let b = buildBorrowed()
    let _ = a
    let _ = b
    i = i + 1
}
let grew = test.liveStringCount() - baseline
// Pre-fix this grew by ~1000 (one leaked toString temp per iteration).
// Allow a little slack for genuinely-cached registry internals.
test.expectTrue(grew < 50)
