// `new Promise(fn(resolve, reject) { ... })` — the executor
// settles the promise the first time it calls `resolve` or
// `reject`; subsequent calls are silently ignored. Per
// syntax.md §13 "the first resolve/reject wins".
//
// expect: got: 42

// Resolve first then reject — `.then` fires, `.catch` doesn't.
// One chain only so the work-stealing pool can't reorder the
// stdout against a sibling chain.
let _p = new Promise<i64>(fn(resolve: fn(i64), reject: fn(string)) {
    resolve(42)
    reject("ignored")
})
    .then(fn(n: i64) { console.log("got: " + n.toString()) })
    .catch(fn(e: string) { console.log("unexpected: " + e) })

let _ = 0
