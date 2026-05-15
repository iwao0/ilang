// `async fn` whose declared return type already includes the
// `Promise<...>` wrapper. The desugar must NOT double-wrap to
// `Promise<Promise<...>>`; it should treat the declared inner as
// the value type and use the declared Promise<...> as the outer.
//
// expect: 42
// expect: 100

async fn nowrap_int(): Promise<i64> {
    let p: Promise<i64> = Promise.resolve(42)
    await p
}

async fn nowrap_int_no_await(): Promise<i64> {
    Promise.resolve(100)
}

async fn run(): i64 {
    let a = await nowrap_int()
    let b = await nowrap_int_no_await()
    console.log(a.toString())
    console.log(b.toString())
    0
}

let _ = run().then(fn(_n: i64) {})
let _ = 0
