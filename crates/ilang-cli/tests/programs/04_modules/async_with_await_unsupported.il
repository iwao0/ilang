// `async fn` with an `await` site — multi-state lowering isn't
// implemented yet, the loader rejects with an actionable error.
//
// expect-error: only zero-await async fns are lowered

async fn waitFor(p: Promise<i64>): i64 {
    let x = await p
    x + 1
}
