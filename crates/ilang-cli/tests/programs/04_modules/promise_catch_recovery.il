// `.catch(handler)` recovers from a rejection — the handler's
// return value lands in the downstream Promise as Resolved.
// A `.then` after the recovery sees that value. Per syntax.md
// §13 ".catch's handler returns a recovery value of type T".
//
// expect: recovered:42

// `Promise.reject(s)` is typed `Promise<()>` (T is unconstrained
// in the static reject), so use the executor form to pin T=i64
// and let the recovery return an i64.
let _p = new Promise<i64>(fn(_resolve: fn(i64), reject: fn(string)) {
        reject("boom")
    })
    .catch(fn(_e: string): i64 { 42 })
    .then(fn(n: i64) {
        console.log("recovered:" + n.toString())
    })

let _ = 0
