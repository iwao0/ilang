// While-with-await containing a heap-typed let. Legacy class-based
// lowering can't represent the Counter slot (no zero-value default);
// v2 with per-variant payloads carries `c` through each iteration.
//
// expect: 52

class Counter {
    pub n: i64
    pub init(n: i64) { this.n = n }
}

async fn run(p: Promise<i64>): i64 {
    let c = new Counter(10)
    let total = 0
    let i = 0
    while i < 3 {
        let v = await p
        total = total + c.n + v
        i = i + 1
        if i >= 3 {
            break
        }
    }
    total + 1
}

let _ = run(Promise.resolve(7)).then(fn(n: i64) {
    console.log(n.toString())
})
let _ = 0
