// `await` on a REJECTED promise rejects the async fn's own result
// promise with the same message (JS semantics). The suspend
// registers a rejection forwarder (`$promise.rejectFollows`) next
// to its resume `.then`; before that, an awaited rejection was
// silently dropped and the result promise stayed pending forever
// (the caller's `.catch` never fired).
//
// Determinism note: the timer-driven case sleeps PAST the timer's
// due time before pumping, so its print position is fixed under
// any load (a due timer can only become "more due"). The churn
// delta asserts a bound, not an exact value — the async state
// machine's one-time lazy allocations land wherever the first
// async call runs.
//
// expect: main end
// expect: sync catch: sync-boom
// expect: nested catch: deep-boom
// expect: loop catch: loop-boom
// expect: deferred catch: late-boom
// expect: churn ok hits=200 strings=0

use std.time as time
use std.test as test

// 構築時に reject 済みの promise を await
async fn one(q: Promise<i64>): i64 {
    let v = await q
    v + 1
}
let pr = new Promise<i64>(fn(res: fn(i64), rej: fn(string)) { rej("sync-boom") })
let _ = one(pr).catch(fn(m: string): i64 {
    console.log("sync catch: " + m)
    return -1
})

// async fn が async fn を await — 2 段 propagate
async fn inner(): i64 {
    let p: Promise<i64> = new Promise<i64>(fn(res: fn(i64), rej: fn(string)) { rej("deep-boom") })
    let v: i64 = await p
    v
}
async fn outer(): i64 {
    let v = await inner()
    v + 1
}
let _ = outer().catch(fn(m: string): i64 {
    console.log("nested catch: " + m)
    return -3
})

// while ループ内 await の途中 reject
async fn loopy(ps: Promise<i64>[]): i64 {
    let total = 0
    let i = 0
    while i < ps.length {
        total = total + await ps[i]
        i = i + 1
    }
    total
}
let arr: Promise<i64>[] = [
    Promise.resolve(10),
    new Promise<i64>(fn(res: fn(i64), rej: fn(string)) { rej("loop-boom") }),
]
let _ = loopy(arr).catch(fn(m: string): i64 {
    console.log("loop catch: " + m)
    return -4
})

console.log("main end")
time.tick()

// timer が後から reject する deferred を await (2 つ目の await)。
// 期限 (5ms) を sleep で確実に跨いでから pump する。
let sr: fn(string)[] = []
let pb = new Promise<i64>(fn(res: fn(i64), rej: fn(string)) { sr.push(rej) })
let _t = time.setTimeout(fn() {
    let j = sr[0]
    j("late-boom")
}, 5)
async fn two(a: Promise<i64>, b: Promise<i64>): i64 {
    let x = await a
    let y = await b
    x + y
}
let _ = two(Promise.resolve(1), pb).catch(fn(m: string): i64 {
    console.log("deferred catch: " + m)
    return -2
})
time.sleep(20)
time.tick()

// churn: 200 周の reject propagate で leak しない
async fn three(a: Promise<i64>): i64 {
    let v = await a
    v * 3
}
let b1 = test.liveAllocBytes()
let s1 = test.liveStringCount()
let hits: i64[] = [0]
let k = 0
while k < 200 {
    let p = new Promise<i64>(fn(res: fn(i64), rej: fn(string)) { rej("c" + k.toString()) })
    let _ = three(p).catch(fn(m: string): i64 {
        hits[0] = hits[0] + 1
        return -1
    })
    time.tick()
    k = k + 1
}
let db = test.liveAllocBytes() - b1
let ds = test.liveStringCount() - s1
test.expectTrue(db < 1024)
test.expectTrue(db > -1024)
console.log("churn ok hits=" + hits[0].toString() + " strings=" + ds.toString())
