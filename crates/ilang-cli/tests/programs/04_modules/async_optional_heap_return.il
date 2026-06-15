// An async fn that returns `Optional<heap>` (`Box?`) — the suspended
// state machine settles its result promise with a some/none Optional
// wrapping a heap value. Both the some-path Box and the box held across
// the await must be reclaimed exactly once; the none-path allocates no
// Box. Pairs round 100 (async heap return) with the Optional wrap.
//
// expect: ok
use std.time as time
use std.test as test

class Tally { n: i64; init() { this.n = 0 } }
let tally = new Tally()

class Box {
    pub v: i64
    init(v: i64) { this.v = v }
    deinit() { tally.n = tally.n + 1 }
}

fn unwrap(o: Box?): i64 {
    match o {
        some(b) { b.v }
        none { return -1 }
    }
}

// some-path: holds a box across the await, returns some(new Box)
async fn maybe(p: Promise<i64>): Box? {
    let kept = new Box(100)        // held across the suspension
    let n = await p
    if n > 0 {
        some(new Box(kept.v + n))  // wrap a fresh heap into Box?
    } else {
        none
    }
}

maybe(Promise.resolve(23)).then(fn(o: Box?) {
    test.expect(unwrap(o), 123)
})
maybe(Promise.resolve(-1)).then(fn(o: Box?) {
    test.expect(unwrap(o), -1)     // none path
})

let _ = time.setTimeout(fn() {
    // some-call: Box(100) (held) + Box(123) (returned) = 2 deinits.
    // none-call: Box(100) (held) only = 1 deinit. Total 3.
    test.expect(tally.n, 3)
    console.log("ok")
}, 10)
