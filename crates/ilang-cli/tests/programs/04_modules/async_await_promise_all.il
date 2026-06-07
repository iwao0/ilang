// `await Promise.all([...])` inside an async fn — the await
// blocks the fn until every input resolves, then surfaces the
// aggregated `T[]` directly. The desugar needs an explicit
// `let xs: T[] = await ...` annotation because the await
// rewrite runs before full type inference (see the error the
// `let arr = await Promise.all(...)` form trips on).
//
// expect: count: 3
// expect: first: 1
// expect: last: 9

async fn run(): i64[] {
    let arr: Promise<i64>[] = [
        Promise.resolve(1),
        Promise.resolve(4),
        Promise.resolve(9),
    ]
    let xs: i64[] = await Promise.all(arr)
    xs
}

let _p = run().then(fn(xs: i64[]): i64[] {
    console.log("count: " + xs.length.toString())
    console.log("first: " + xs[0].toString())
    console.log("last: " + xs[2].toString())
    xs
})

let _ = 0
