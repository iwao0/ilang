// Template literals fold parts through `fmt_value` + `str_concat`,
// both of which mint fresh registry strings. The lowerer used to keep
// every intermediate (and the result was classified borrowed, so
// consumers never released it) — each `${i}` evaluation leaked two
// registry strings, multi-part templates even more. This pins the
// balance: intermediates drop inside `lower_template`, the final
// string is fresh and dropped by its consumer.

use std.test as test

// edge shapes stay correct
test.expectStr(``, "")
test.expectStr(`plain`, "plain")
test.expectStr(`a${1}b${2}c`, "a1b2c")
let who = "world"
test.expectStr(`hello ${who}`, "hello world")

let base = test.liveStringCount()
let i = 0
while i < 200 {
    // discarded result
    let _ = `${i}`
    // bound result, dropped at scope exit
    let s = `x=${i} y=${i * 2}`
    test.expectTrue(s.length >= 7)
    // string interpolation (fmt of a Str copies, both copies drop)
    let t = `<${s}>`
    test.expectTrue(t.length == s.length + 2)
    // consumed as a fresh call arg
    test.expectTrue(("v" + `${i}`).length >= 2)
    // fresh values INSIDE ${}: a nested template and a concat — the
    // part's transient +1 drops after fmt_value copies it
    let _ = `out${`in${i}`}er`
    let _ = `x${"a" + `${i}`}y`
    i = i + 1
}
let after = test.liveStringCount()
test.expectTrue(after - base < 16)
