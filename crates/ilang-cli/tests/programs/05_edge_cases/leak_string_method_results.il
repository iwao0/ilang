// String methods that return fresh strings — `toUpper`, `toLower`,
// `trim`, `replace`, `slice`, `charAt` — must register with the
// rc-tracked string registry and release at scope exit. Without
// that, every call leaks one entry into `STRING_REGISTRY`
// (per-frame HUD-style code paths in long-running programs).
//

use test

let s = "  Hello, World!  "

let baseline = test.liveStringCount()

let i: i64 = 0
while i < 1000 {
    let a = s.toUpper()
    let b = s.toLower()
    let c = s.trim()
    let d = s.replace("o", "0")
    let e = s.slice(2, 7)
    let f = s.charAt(3)
    let _ = a
    let _ = b
    let _ = c
    let _ = d
    let _ = e
    let _ = f
    i = i + 1
}

let after = test.liveStringCount()
let grew = after - baseline

// All 6 results per iteration are let-bound and released at
// loop-body scope exit. Registry should stay flat (allow tiny
// slack for any one-shot internals).
test.expectTrue(grew < 20)
