// Mirror of `mk_reject_via_executor`: an executor body that
// resolves the Promise asynchronously through the pool. Lock in
// the resolve-side of the double-release fix (e1de6e02) — the
// runtime no longer releases the resolve/reject callback cells
// itself, so the upstream Promise's refcount lands at 0 only
// when the array `arr` and the chain are fully done.
//
// expect: got: 42

fn mk_resolve(v: i64): Promise<i64> {
    new Promise<i64>(fn(resolve: fn(i64), reject: fn(string)) {
        resolve(v)
    })
}

let _p = mk_resolve(42).then(fn(n: i64) {
    console.log("got: " + n.toString())
})

let _ = 0
