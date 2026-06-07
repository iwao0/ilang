// async fn `outer` calls another `async fn inner` and awaits it
// twice in sequence. Each `await inner(...)` suspends `outer`
// until the inner promise settles; sequential awaits read each
// other's results.
//
// expect: 32

async fn inner(x: i64): i64 {
    x + 1
}

async fn outer(): i64 {
    let a: i64 = await inner(10)
    let b: i64 = await inner(20)
    a + b
}

let _p = outer().then(fn(v: i64) {
    console.log(v.toString())
})

let _ = 0
