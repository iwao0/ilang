// `Promise<T?>` — await yields an Optional, then match on
// some/none. Exercises pattern-binding type resolution for
// `Optional<T>` payload (`some(v)` binding is `T`).
//
// expect: got: 42
// expect: empty

fn maybe(n: i64): Promise<i64?> {
    if n > 0 {
        let v: i64? = some(n)
        Promise.resolve(v)
    } else {
        let v: i64? = none
        Promise.resolve(v)
    }
}

async fn run(): i64 {
    let a: i64? = await maybe(42)
    match a {
        some(v) { console.log("got: " + v.toString()) }
        none { console.log("empty") }
    }
    let b: i64? = await maybe(0)
    match b {
        some(v) { console.log("got: " + v.toString()) }
        none { console.log("empty") }
    }
    0
}

let _ = run().then(fn(_n: i64) {})
let _ = 0
