// Array literal as an unannotated `let` inside an async fn body.
// Used to fail at desugar with "couldn't infer this binding's type
// from the RHS shape". The mini-inferencer now recognises array
// literals, and the desugar stamps its inferred type back onto the
// source `let` so the post-desugar type checker agrees with the
// variant field shape — without that, the layout-mismatch between
// `i64[]` (typechecker) and `i64[3]` (variant field) corrupted the
// stored values across the await.
//
// expect: 1
// expect: 2
// expect: 3
// expect: 41

async fn run(p: Promise<i64>): i64 {
    let arr = [1, 2, 3]
    let v = await p
    console.log(arr[0].toString())
    console.log(arr[1].toString())
    console.log(arr[2].toString())
    arr[0] + v
}

let _ = run(Promise.resolve(40)).then(fn(n: i64) {
    console.log(n.toString())
})
let _ = 0
