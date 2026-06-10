// `await` in assignment RHS positions. The await-lift pre-pass had no
// arms for Assign / AssignField / AssignIndex and didn't descend into
// nested blocks (while / loop / for-in bodies, if branches, match
// arms), so `total = total + await p` either hit the compiler panic
// "NoAwait after body_contains_await=true" (statement position) or a
// misleading "await only allowed inside async fn" error (inside a
// loop). The lift now hoists these into `let __await_tN = await ...`
// inside the block they appear in.
use std.test as test

class Counter {
    pub n: i64
    pub init(n: i64) { this.n = n }
    pub async incr(p: Promise<i64>): i64 {
        let v = await p
        this.n + v
    }
}

class Holder {
    pub slot: i64
    pub init() { this.slot = 0 }
}

async fn run(): i64 {
    // plain assignment RHS
    let total = 0
    total = total + await Promise.resolve(5)

    // assignment RHS with an async method call
    let c = new Counter(10)
    total = total + await c.incr(Promise.resolve(1))   // +11

    // inside a while body
    let i = 0
    while i < 4 {
        total = total + await Promise.resolve(i)        // +0+1+2+3
        i = i + 1
    }

    // inside if / else branches
    if total > 0 {
        total = total + await Promise.resolve(100)
    } else {
        total = total + 1
    }

    // field and index assignment RHS
    let h = new Holder()
    h.slot = await Promise.resolve(7)
    total = total + h.slot
    let arr = [0, 0]
    arr[1] = await Promise.resolve(8)
    total = total + arr[1]

    total
}

let p = run()
let _ = p.then(fn(v: i64) {
    test.expect(v, 5 + 11 + 6 + 100 + 7 + 8)
})
