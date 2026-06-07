// Inside an executor body, calling `resolve(a)` then `reject(b)`
// in sequence leaves the Promise Resolved with `a` — the second
// call is a no-op per "first settle wins". Pairs with the
// existing `promise_executor_first_settle_wins.il` (which tests
// the sync path) by routing through the same async pool task
// and ensures the resolve→reject ordering is identical there.
//
// expect: got: 7

let _p = new Promise<i64>(fn(resolve: fn(i64), reject: fn(string)) {
    resolve(7)
    reject("ignored")
})
    .then(fn(n: i64) { console.log("got: " + n.toString()) })
    .catch(fn(e: string) { console.log("unexpected: " + e) })

let _ = 0
