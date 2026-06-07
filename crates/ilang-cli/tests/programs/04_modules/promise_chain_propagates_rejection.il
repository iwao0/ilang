// A rejection at link 1 of a `.then` chain propagates past every
// subsequent `.then` until it reaches a `.catch`. Each
// intermediate `.then` is supposed to skip its handler and pass
// the rejection through. Per syntax.md §13 "propagate the
// rejection to downstream".
//
// expect: caught: source-reject

let _p = new Promise<i64>(fn(_resolve: fn(i64), reject: fn(string)) {
        reject("source-reject")
    })
    .then(fn(_n: i64): i64 {
        console.log("link1 should not run")
        0
    })
    .then(fn(_n: i64): i64 {
        console.log("link2 should not run")
        0
    })
    .catch(fn(e: string): i64 {
        console.log("caught: " + e)
        return -1
    })

let _ = 0
