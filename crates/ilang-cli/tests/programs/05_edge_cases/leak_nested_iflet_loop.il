// Two layers of `if let some(...)` — the inner if-let extracts
// from another Optional value derived inside the outer's body.
// Both Optional shells must drop on their respective scope exits.
// A regression that lets the outer's binding leak when the inner
// short-circuits (or vice versa) would grow the cell tracker per
// iteration.
//

use test

fn lookup(n: i64): i64? {
    if n < 0 { return none }
    return some(n * 2)
}

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 200 {
    let outer = lookup(i)
    if let some(a) = outer {
        let inner = lookup(a)
        if let some(b) = inner {
            let _ = b
        }
    }
    i = i + 1
}

let after = test.liveAllocBytes()
test.expectTrue(after - baseline < 32768)
