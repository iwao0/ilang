// `Promise<Result<T, E>>` — await yields the Result, then match
// on ok/err. With `Result` now monomorphized per call site (the
// previous built-in registration is gone), the `err(e)` binding
// is properly typed `string` instead of `i64`.
//
// expect: ok: 7
// expect: err: bad

fn parseNum(s: string): Promise<Result<i64, string>> {
    if s == "good" {
        let r: Result<i64, string> = Result.ok(7)
        Promise.resolve(r)
    } else {
        let r: Result<i64, string> = Result.err("bad")
        Promise.resolve(r)
    }
}

async fn run(): i64 {
    let a = await parseNum("good")
    match a {
        ok(v) { console.log("ok: " + v.toString()) }
        err(e) { console.log("err: " + e) }
    }
    let b = await parseNum("oops")
    match b {
        ok(v) { console.log("ok: " + v.toString()) }
        err(e) { console.log("err: " + e) }
    }
    0
}

let _ = run().then(fn(_n: i64) {})
let _ = 0
