// Three `await` sub-expressions used as arguments to a sync fn —
// the awaits evaluate left-to-right per syntax.md §13's "Inside a
// sub-expression, awaits evaluate left-to-right" rule.
// `async_subexpr_await.il` covers the 2-await case; this fixture
// extends to three to verify the order generalises.
//
// expect: 111

fn sum3(a: i64, b: i64, c: i64): i64 { a + b + c }

async fn run(): i64 {
    sum3(
        await Promise.resolve(1),
        await Promise.resolve(10),
        await Promise.resolve(100)
    )
}

let _p = run().then(fn(v: i64) {
    console.log(v.toString())
})

let _ = 0
