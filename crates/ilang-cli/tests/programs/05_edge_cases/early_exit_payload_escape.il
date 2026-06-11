// Escape hatches across the early-exit sweeps — the value LEAVES
// through the exit the sweep is cleaning up after:
// - `return b` where `b` is a fresh scrutinee's payload: the sweep
//   releases the scrutinee (cascade would free the payload), so the
//   borrowed return value takes +1 first — exactly one deinit per
//   churn round, value intact.
// - `break b` with the payload: lower_break's borrowed-value retain
//   precedes its sweep the same way.
// - early return of a mutating-capture closure: the cell share the
//   sweep drops is the SCOPE's; the escaping closure keeps its own.
// - nested fresh-iterable for-ins pierced by one return: both
//   iterables release, all elements deinit exactly once.
// - async bodies: return out of a match arm / while loop / for-in
//   after (or between) awaits settles the result promise.
//
// expect: payload-return 7 during=0
// expect: payload-churn delta=0 deinits=200
// expect: payload-break 11 during=1
// expect: cell-calls 1,2
// expect: nested-forin deinits=800
// expect: match-halt: -100
// expect: match-go: 15
// expect: while-return: 8
// expect: forin-return: 5

use std.time as time
use std.test as test

let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}
enum Got { yes: (Box), no }
fn mk(k: i64): Got { Got.yes(new Box(k)) }

fn takeOut(k: i64): Box {
    match mk(k) {
        yes(b) { return b }
        no { return new Box(-1) }
    }
    return new Box(-2)
}
let before1 = deinits[0]
let got = takeOut(7)
test.expect(got.n, 7)
console.log("payload-return " + got.n.toString() + " during=" + (deinits[0] - before1).toString())

let b1 = test.liveAllocBytes()
let before2 = deinits[0]
let i = 0
while i < 200 {
    let g = takeOut(i)
    test.expect(g.n, i)
    i = i + 1
}
let d1 = test.liveAllocBytes() - b1
let dd1 = deinits[0] - before2
console.log("payload-churn delta=" + d1.toString() + " deinits=" + dd1.toString())

fn breakOut(k: i64): Box {
    let j = 0
    loop {
        match mk(k + j) {
            yes(b) { if j >= 1 { break b } }
            no { }
        }
        j = j + 1
    }
}
let before3 = deinits[0]
let g2 = breakOut(10)
test.expect(g2.n, 11)
console.log("payload-break " + g2.n.toString() + " during=" + (deinits[0] - before3).toString())

fn cellEarly(k: i64): fn(): i64 {
    let counter = 0
    let bump: fn(): i64 = fn(): i64 {
        counter = counter + 1
        counter
    }
    if k >= 0 { return bump }
    bump
}
let f = cellEarly(1)
let r1 = f()
let r2 = f()
console.log("cell-calls " + r1.toString() + "," + r2.toString())

fn mkArr(n: i64): Box[] {
    let out: Box[] = []
    out.push(new Box(n))
    out.push(new Box(n + 1))
    out
}
fn nested(k: i64): i64 {
    for a in mkArr(k) {
        for b in mkArr(a.n * 10) {
            if b.n >= 0 { return a.n + b.n }
        }
    }
    return -1
}
let before4 = deinits[0]
let q = 0
let acc: i64[] = [0]
while q < 200 { acc[0] = acc[0] + nested(q); q = q + 1 }
test.expectTrue(acc[0] > 0)
console.log("nested-forin deinits=" + (deinits[0] - before4).toString())

enum Cmd { halt, go: (i64) }
async fn dispatch(c: Cmd, p: Promise<i64>): i64 {
    let v = await p
    match c {
        halt { return -100 }
        go(n) { return v + n }
    }
    return 0
}
let _ = dispatch(Cmd.halt, Promise.resolve(1)).then(fn(v: i64) {
    console.log("match-halt: " + v.toString())
})
let _ = dispatch(Cmd.go(5), Promise.resolve(10)).then(fn(v: i64) {
    console.log("match-go: " + v.toString())
})
time.tick()

async fn firstBig(ps: Promise<i64>[], limit: i64): i64 {
    let i2 = 0
    while i2 < ps.length {
        let v = await ps[i2]
        if v > limit { return v }
        i2 = i2 + 1
    }
    return -1
}
let parr: Promise<i64>[] = [Promise.resolve(1), Promise.resolve(8), Promise.resolve(3)]
let _ = firstBig(parr, 5).then(fn(v: i64) {
    console.log("while-return: " + v.toString())
})
time.tick()

async fn scanAfterAwait(p: Promise<i64>, data: i64[]): i64 {
    let base = await p
    for d in data {
        if d > base { return d }
    }
    return -2
}
let nums: i64[] = [1, 5, 9]
let _ = scanAfterAwait(Promise.resolve(4), nums).then(fn(v: i64) {
    console.log("forin-return: " + v.toString())
})
time.tick()
