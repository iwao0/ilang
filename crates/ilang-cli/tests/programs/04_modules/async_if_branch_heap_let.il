// Tail-position if-else with await(s) inside each branch, plus a
// heap-typed let bound BEFORE the if-else and used inside the
// branches. Legacy class-based lowering couldn't represent the
// Counter slot (no zero-value default); v2 with Branch terminator
// builds V_0 with `c` carried into both branch chains via the
// per-variant payload.
//
// expect: 17
// expect: 23

class Counter {
    pub n: i64
    pub init(n: i64) { this.n = n }
}

async fn pick(cond: bool, p: Promise<i64>, q: Promise<i64>): i64 {
    let c = new Counter(10)
    if cond {
        let x = await p
        c.n + x
    } else {
        let y = await q
        c.n + y + 3
    }
}

// Serialize the two calls so the test framework sees deterministic
// output order regardless of the runtime's microtask scheduling.
async fn run(): i64 {
    let a: i64 = await pick(true,  Promise.resolve(7),  Promise.resolve(0))
    let b: i64 = await pick(false, Promise.resolve(0), Promise.resolve(10))
    console.log(a.toString())
    console.log(b.toString())
    0
}
let _ = run().then(fn(_n: i64) {})
let _ = 0
