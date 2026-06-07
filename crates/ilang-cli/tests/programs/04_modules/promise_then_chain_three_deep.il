// `.then(cb1).then(cb2).then(cb3)` — each link receives the
// previous link's return value, and the chain settles in order.
// The intermediate Promises are released as their handlers fire;
// only `_p` (the final link) is bound in the entry. Verifies the
// chain ARC doesn't drop an intermediate too early.
//
// expect: link1:1
// expect: link2:2
// expect: link3:6

let _p = Promise.resolve(1)
    .then(fn(n: i64): i64 {
        console.log("link1:" + n.toString())
        n + 1
    })
    .then(fn(n: i64): i64 {
        console.log("link2:" + n.toString())
        n * 3
    })
    .then(fn(n: i64): i64 {
        console.log("link3:" + n.toString())
        n
    })

let _ = 0
