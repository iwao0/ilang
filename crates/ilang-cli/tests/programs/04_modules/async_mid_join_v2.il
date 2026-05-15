// Mid-body if-else / match with awaits and a join point. v2's
// JumpBind terminator threads each arm's tail value into the join
// variant's payload field; the rest of the fn body (and a
// heap-typed let bound before the join) flow through the join
// variant. Legacy handles these via its BlockBuilder; v2 now
// covers them too.
//
// expect: 53
// expect: 118
// expect: 53
// expect: 118

class Counter {
    pub n: i64
    pub init(n: i64) { this.n = n }
}

async fn compute_if(cond: bool, p: Promise<i64>, q: Promise<i64>): i64 {
    let c = new Counter(10)
    let r = if cond {
        let x = await p
        x * 2
    } else {
        let y = await q
        y + 100
    }
    r + c.n + 1
}

async fn compute_match(o: i64?, fallback: Promise<i64>): i64 {
    let c = new Counter(10)
    let r = match o {
        some(v) {
            let n = await fallback
            v + n
        }
        none {
            let f = await fallback
            f + 100
        }
    }
    r + c.n + 1
}

async fn run(): i64 {
    let a: i64 = await compute_if(true,  Promise.resolve(21), Promise.resolve(0))
    let b: i64 = await compute_if(false, Promise.resolve(0),  Promise.resolve(7))
    let cc: i64 = await compute_match(some(35), Promise.resolve(7))
    let d: i64 = await compute_match(none,     Promise.resolve(7))
    console.log(a.toString())
    console.log(b.toString())
    console.log(cc.toString())
    console.log(d.toString())
    0
}

let _ = run().then(fn(_n: i64) {})
let _ = 0
