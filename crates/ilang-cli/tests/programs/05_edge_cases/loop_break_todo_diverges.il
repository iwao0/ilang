// A `break <diverging-expr>` (e.g. `break todo()`) must NOT pin the
// `loop`'s break-value type. The diverging break never actually breaks
// — control aborts while evaluating `todo()` — so it contributes no
// type to the break-value join, exactly like a diverging match arm.
//
// Before the fix, mixing `break todo()` with a real `break v` pinned
// the loop type to `Any` and the real break mismatched: `expected any,
// got i64` (scalar) / `expected any, got Box` (heap). `todo()` was
// wired into the match-arm and if-join divergence skip (8e5d13a8) but
// the `loop` break-value join was the missing sibling.
//
// Here the diverging arm is never reached (c = false), so both the
// scalar and heap loops yield their real break value normally, and the
// heap path stays ARC-balanced (churn deinit is exact, no leak).
//
// expect: 5
// expect: n=7
// expect: churn deinits=100

let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// scalar break value mixed with `break todo()`
fn scalar(c: bool): i64 {
    let r = loop {
        if c { break todo() }
        break 5
    }
    r
}

// heap break value mixed with `break todo()`
fn heap(c: bool): Box {
    let r = loop {
        if c { break todo() }
        break new Box(7)
    }
    r
}

console.log(scalar(false).toString())          // 5
let b = heap(false)
console.log("n=" + b.n.toString())             // n=7

// 100 fresh Box created and dropped — deinit exactly 100, no leak.
// `b` above is still alive (top-level), so it is not counted here.
let i = 0
while i < 100 {
    let tmp = heap(false)
    i = i + 1
}
console.log("churn deinits=" + deinits[0].toString())
