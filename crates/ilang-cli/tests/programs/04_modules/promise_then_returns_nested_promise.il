// `.then(cb)` types out as `Promise<U>` where `U = cb`'s return
// type — there's no JS-style auto-unwrap of a returned promise.
// Returning `Promise<i64>` from a `.then` therefore lands in a
// `Promise<Promise<i64>>` slot at the next link. Per syntax.md
// §13 "Promise<U>" — no `.then.flat` behaviour today.
//
// expect: outer-then fired
// expect: inner-resolved: 21

let _p = Promise.resolve(10)
    .then(fn(n: i64): Promise<i64> { Promise.resolve(n * 2 + 1) })
    .then(fn(inner: Promise<i64>): Promise<i64> {
        // We get the inner promise here — chain its own resolution.
        console.log("outer-then fired")
        inner.then(fn(v: i64): i64 {
            console.log("inner-resolved: " + v.toString())
            v
        })
    })

let _ = 0
