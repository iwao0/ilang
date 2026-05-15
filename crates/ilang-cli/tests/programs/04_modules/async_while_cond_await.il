// `while await p { body }`: cond is re-evaluated each iteration via
// await. v2 pre-pass rewrites to `while true { let cv = await p; if
// !cv { break }; body }`. A Counter (heap) bound outside the loop
// also forces v2.
//
// `should_continue` is a fn that returns a Promise<bool> based on
// the call count (held in an external Counter). It returns true
// three times, then false.
//
// expect: 30

class Counter {
    pub n: i64
    pub init(n: i64) { this.n = n }
}

fn should_continue(c: Counter): Promise<bool> {
    c.n = c.n + 1
    let alive: bool = c.n <= 3
    Promise.resolve(alive)
}

async fn loop_until_stop(c: Counter): i64 {
    let total = 0
    while await should_continue(c) {
        total = total + 10
    }
    total
}

let _ = loop_until_stop(new Counter(0)).then(fn(n: i64) {
    console.log(n.toString())
})
let _ = 0
