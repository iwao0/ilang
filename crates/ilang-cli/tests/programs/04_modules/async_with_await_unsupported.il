// `async fn` with TWO `await` sites — multi-await desugar (which
// needs a `.then` flattening primitive) isn't implemented yet.
// The single-await case works (see `async_single_await.il`); this
// fixture pins the rejection path for the next case up.
//
// expect-error: multi-await desugar

async fn sumTwo(p: Promise<i64>, q: Promise<i64>): i64 {
    let x: i64 = await p
    let y: i64 = await q
    x + y
}
