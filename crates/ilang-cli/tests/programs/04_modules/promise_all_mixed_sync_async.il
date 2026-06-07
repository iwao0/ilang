// `Promise.all` with a mix of synchronously-Resolved inputs and an
// async-Resolved input (via executor). The agg has to wait for the
// async input to settle before its `.then` fires — the pre-scan
// only locks the aggregate when there's a sync rejection, so this
// path exercises the counter-based settle.
//
// expect: sum: 16

fn mk_async(v: i64): Promise<i64> {
    new Promise<i64>(fn(resolve: fn(i64), reject: fn(string)) {
        resolve(v)
    })
}

let arr: Promise<i64>[] = [
    Promise.resolve(3),
    mk_async(5),
    Promise.resolve(8),
]

let _p = Promise.all(arr).then(fn(vs: i64[]): i64[] {
    let s = vs[0] + vs[1] + vs[2]
    console.log("sum: " + s.toString())
    vs
})

let _ = 0
