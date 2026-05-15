// `await` inside sub-expressions — the lifting pre-pass extracts
// each await into a `let __await_tN = await E` statement above
// the use site, so the analyser + state-machine synth keep
// operating on the canonical "await as let-RHS" form.
//
// `add(await p, await q)` becomes
//     let __await_t0 = await p
//     let __await_t1 = await q
//     add(__await_t0, __await_t1)
//
// `(await p) * 2 + 5` becomes
//     let __await_t0 = await p
//     __await_t0 * 2 + 5
//
// expect: 30
// expect: 19

fn add(a: i64, b: i64): i64 { a + b }

async fn computeAsync(p: Promise<i64>, q: Promise<i64>): i64 {
    add(await p, await q)
}

async fn doubleWithExtra(p: Promise<i64>): i64 {
    (await p) * 2 + 5
}

// Drive both promises off one chain so the print order stays
// deterministic across the work-stealing pool.
async fn run(): i64 {
    let a = await computeAsync(Promise.resolve(10), Promise.resolve(20))
    let b = await doubleWithExtra(Promise.resolve(7))
    console.log(a.toString())
    console.log(b.toString())
    0
}

let _ = run().then(fn(_n: i64) {})
let _ = 0
