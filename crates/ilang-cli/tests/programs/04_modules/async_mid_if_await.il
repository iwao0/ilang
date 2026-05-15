// Mid-body `let r = if-else { ... } / match { ... }` whose arms
// contain awaits. The BlockBuilder allocates a join state, gives
// each arm a `BodyEnd::AssignAndJump` so they write their value
// into `state.r` before flowing to the join, then the body's
// remaining stmts read from `state.r`.
//
// expect: 43
// expect: 108
// expect: 43
// expect: 108

async fn compute_if(cond: bool, p: Promise<i64>, q: Promise<i64>): i64 {
    let r = if cond {
        let x = await p
        x * 2
    } else {
        let y = await q
        y + 100
    }
    r + 1
}

async fn compute_match(o: i64?, fallback: Promise<i64>): i64 {
    let r = match o {
        some(v) { v * 2 }
        none {
            let f = await fallback
            f + 100
        }
    }
    r + 1
}

async fn run(): i64 {
    let a = await compute_if(true,  Promise.resolve(21), Promise.resolve(0))
    let b = await compute_if(false, Promise.resolve(0),  Promise.resolve(7))
    let c = await compute_match(some(21), Promise.resolve(0))
    let d = await compute_match(none, Promise.resolve(7))
    console.log(a.toString())
    console.log(b.toString())
    console.log(c.toString())
    console.log(d.toString())
    0
}

let _ = run().then(fn(_n: i64) {})
let _ = 0
