// `async fn` with no `await` — the desugar pass wraps the body
// in `Promise.resolve(...)` and exposes the fn as
// `Promise<T>`-returning. Multi-state lowering for bodies with
// awaits is the next phase; bodies that contain `await` still
// fail with an actionable error.
//
// expect: 42

async fn foo(): i64 {
    42
}

let _p = foo().then(fn(n: i64) {
    console.log(n.toString())
})

let _ = 0
