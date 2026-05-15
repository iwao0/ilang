// `elif` chain in tail-position with awaits in each arm. The
// parser produces else_branch as a bare `If` expr (not a Block);
// v2 wraps it via coerce_to_block before recursing, so each chain
// link becomes a regular tail-If recursion.
//
// A heap-typed `Counter` bound before the chain forces v2 (legacy
// can't represent the slot's missing zero-default), so a passing
// run confirms v2 covers the elif chain.
//
// expect: 11
// expect: 32
// expect: 343
// expect: 4454

class Counter {
    pub n: i64
    pub init(n: i64) { this.n = n }
}

async fn pick(n: i64, p: Promise<i64>): i64 {
    let c = new Counter(10)
    if n == 1 {
        let a = await p
        a + c.n
    } elif n == 2 {
        let b = await p
        b * 2 + c.n
    } elif n == 3 {
        let cc = await p
        cc * 3 + c.n
    } else {
        let d = await p
        d * 4 + c.n
    }
}

// mid-body let-if with else-if chain.
async fn pick_join(n: i64, p: Promise<i64>): i64 {
    let r = if n == 1 {
        let a = await p
        a
    } elif n == 2 {
        let b = await p
        b * 2
    } else {
        let c = await p
        c * 3
    }
    r + 1000
}

async fn run(): i64 {
    let a: i64 = await pick(1, Promise.resolve(1))
    let b: i64 = await pick(2, Promise.resolve(11))
    let cc: i64 = await pick(3, Promise.resolve(111))
    let d: i64 = await pick(4, Promise.resolve(1111))
    console.log(a.toString())
    console.log(b.toString())
    console.log(cc.toString())
    console.log(d.toString())
    0
}

let _ = run().then(fn(_n: i64) {})
let _ = 0
