// Stmt-position `if cond { ...await... }` with no else. Both arms
// (the synthesized empty else and the then) converge on an
// after-segment that picks up the remaining body.
//
// expect: 17
// expect: 10

class Counter {
    pub n: i64
    pub init(n: i64) { this.n = n }
}

async fn maybe_add(cond: bool, p: Promise<i64>): i64 {
    let c = new Counter(10)
    let total = 0
    if cond {
        let v = await p
        total = total + v
    }
    total + c.n
}

async fn run(): i64 {
    let a: i64 = await maybe_add(true,  Promise.resolve(7))
    let b: i64 = await maybe_add(false, Promise.resolve(7))
    console.log(a.toString())
    console.log(b.toString())
    0
}

let _ = run().then(fn(_n: i64) {})
let _ = 0
