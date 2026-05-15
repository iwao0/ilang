// Receive a `Promise<T>` into a `let` and await later. Useful for
// kicking off concurrent work before awaiting any of it.
//
// expect: 18
// expect: 30

fn makeNum(n: i64): Promise<i64> {
    Promise.resolve(n)
}

class Counter {
    pub n: i64
    pub init(n: i64) { this.n = n }
}

fn makeCounter(n: i64): Promise<Counter> {
    Promise.resolve(new Counter(n))
}

async fn sum_two(): i64 {
    let p = makeNum(7)
    let q = makeNum(11)
    let a = await p
    let b = await q
    a + b
}

async fn sum_counters(): i64 {
    let p1 = makeCounter(10)
    let total = 0
    let p2 = makeCounter(20)
    let c1 = await p1
    total = total + c1.n
    let c2 = await p2
    total = total + c2.n
    total
}

async fn run(): i64 {
    let a = await sum_two()
    let b = await sum_counters()
    console.log(a.toString())
    console.log(b.toString())
    0
}

let _ = run().then(fn(_n: i64) {})
let _ = 0
