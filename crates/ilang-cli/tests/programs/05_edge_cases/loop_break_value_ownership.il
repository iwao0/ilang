// `loop { break v }` evaluates to v, and `lower_break` guarantees the
// exit-block value owns a +1 (borrowed break values get a retain,
// fresh ones keep their own). The loop expression itself was
// classified borrowed though, so `let b = loop { break new Box(i) }`
// stacked a binding retain on top and leaked one occupant per
// evaluation. `Loop` is fresh now.

use std.test as test

class Box {
    n: i64
    init(x: i64) { this.n = x }
}

// fresh break value churn
let base = test.liveAllocBytes()
let i = 0
while i < 200 {
    let b = loop { break new Box(i) }
    test.expect(b.n, i)
    i = i + 1
}
let after = test.liveAllocBytes()
test.expectTrue(after - base < 1024)

// borrowed break value: the binding must keep it alive independently
// of the source slot
let keeper = new Box(7)
let alias = loop { break keeper }
test.expect(alias.n, 7)

// break out of a deeper iteration still hands ownership out
fn firstOver(limit: i64): Box {
    let j = 0
    loop {
        let candidate = new Box(j)
        if candidate.n > limit { break candidate }
        j = j + 1
    }
}
test.expect(firstOver(5).n, 6)
let f = firstOver(2)
test.expect(f.n, 3)
