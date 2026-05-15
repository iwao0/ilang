// `async fn` with a single `let x: T = await p` await site.
// The desugar pass rewrites this to:
//   fn doubleAsync(p): Promise<i64> {
//       p.then(fn(x: i64): i64 { x * 2 })
//   }
// so the asynchronously-arriving value of `p` flows through the
// trailing expression. Explicit type annotation on the `let x: i64`
// is currently required (the AST-level desugar can't see the type
// checker's inference yet).
//
// expect: 42

async fn doubleAsync(p: Promise<i64>): i64 {
    let x: i64 = await p
    x * 2
}

let _ = doubleAsync(Promise.resolve(21)).then(fn(n: i64) {
    console.log(n.toString())
})

let _ = 0
