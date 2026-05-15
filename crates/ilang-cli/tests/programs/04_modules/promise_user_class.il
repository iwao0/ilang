// `Promise<UserClass>` — await yields the class instance, field
// access works through it. Exercises the retain/release plumbing
// in the v2 state machine for heap-typed await bindings.
//
// expect: 42
// expect: 100

class Counter {
    pub n: i64
    pub init(n: i64) { this.n = n }
}

fn makeCounter(n: i64): Promise<Counter> {
    Promise.resolve(new Counter(n))
}

async fn run(): i64 {
    let c: Counter = await makeCounter(42)
    console.log(c.n.toString())
    let d: Counter = await makeCounter(100)
    console.log(d.n.toString())
    0
}

let _ = run().then(fn(_n: i64) {})
let _ = 0
