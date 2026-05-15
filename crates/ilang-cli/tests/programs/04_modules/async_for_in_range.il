// `for v in 0..n { ...await... }` — v2 pre-pass rewrites this to
// the equivalent while loop and the state-machine builder takes
// over. A heap-typed Counter bound outside forces v2 (legacy
// can't represent it).
//
// expect: 21
// expect: 31

class Counter {
    pub n: i64
    pub init(n: i64) { this.n = n }
}

async fn sum_range(p: Promise<i64>, n: i64): i64 {
    let total = 0
    for i in 0..n {
        let v = await p
        total = total + v
    }
    total
}

async fn sum_range_with_heap(p: Promise<i64>): i64 {
    let c = new Counter(10)
    let total = 0
    for i in 0..3 {
        let v = await p
        total = total + v
    }
    total + c.n
}

async fn run(): i64 {
    let a: i64 = await sum_range(Promise.resolve(7), 3)
    let b: i64 = await sum_range_with_heap(Promise.resolve(7))
    console.log(a.toString())
    console.log(b.toString())
    0
}

let _ = run().then(fn(_n: i64) {})
let _ = 0
