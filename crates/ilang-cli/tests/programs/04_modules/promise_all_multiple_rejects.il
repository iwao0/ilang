// `Promise.all` with multiple asynchronously-rejected upstreams —
// the aggregate catches the FIRST reject to actually settle. With
// all three executors running on the same pool, the deterministic
// guarantee is "the first reject_cb to reach `settle_reject(agg)`
// wins"; subsequent rejects find agg already settled and become
// no-ops. The visible answer therefore depends on pool scheduling
// but stays inside the input set.
//
// expect: caught: first
// expect-skip: race-prone

fn mk_reject(msg: string): Promise<string> {
    new Promise<string>(fn(resolve: fn(string), reject: fn(string)) {
        reject(msg)
    })
}

let arr: Promise<string>[] = [
    mk_reject("first"),
    mk_reject("second"),
    mk_reject("third"),
]

let _p = Promise.all(arr)
    .then(fn(vs: string[]): string[] {
        console.log("unexpected resolve")
        vs
    })
    .catch(fn(e: string): string[] {
        console.log("caught: " + e)
        []
    })

let _ = 0
