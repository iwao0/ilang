// `async fn` with a single `let x = await p` await site. The
// desugar's mini-inferencer figures out `x: i64` from `p`'s
// `Promise<i64>` parameter type, so no explicit annotation is
// required on the binding.
//
// expect: 42

async fn doubleAsync(p: Promise<i64>): i64 {
    let x = await p
    x * 2
}

let _ = doubleAsync(Promise.resolve(21)).then(fn(n: i64) {
    console.log(n.toString())
})

let _ = 0
