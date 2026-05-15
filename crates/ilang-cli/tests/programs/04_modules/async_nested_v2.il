// Phase 2e: nested control flow inside async fn bodies.
//   - while body containing tail-If with awaits in branches
//   - while body containing mid-body let-if/match join
//   - if-arm containing while with awaits
//   - nested while with break/continue at the inner level
//
// expect: 28
// expect: 70
// expect: 14

// while body whose tail is an If with await(s) in each branch.
// Per iteration, picks one of two promises based on i parity.
async fn alternating(p: Promise<i64>, q: Promise<i64>): i64 {
    let total = 0
    let i = 0
    while i < 4 {
        i = i + 1
        if i % 2 == 1 {
            let a = await p
            total = total + a
        } else {
            let b = await q
            total = total + b
        }
    }
    total
}

// if branch contains a while-with-await.
async fn pick_loop(cond: bool, p: Promise<i64>): i64 {
    if cond {
        let total = 0
        let i = 0
        while i < 5 {
            let v = await p
            total = total + v
            i = i + 1
        }
        total
    } else {
        let v = await p
        v + 100
    }
}

// nested while: outer iterates over `outer_n`, inner iterates a
// fixed 2 times per outer. break in the inner only exits inner.
async fn nested(p: Promise<i64>, outer_n: i64): i64 {
    let total = 0
    let i = 0
    while i < outer_n {
        let j = 0
        while j < 100 {
            let v = await p
            total = total + v
            j = j + 1
            if j >= 2 {
                break
            }
        }
        i = i + 1
    }
    total
}

async fn run(): i64 {
    let a: i64 = await alternating(Promise.resolve(3), Promise.resolve(11))
    let b: i64 = await pick_loop(true, Promise.resolve(14))
    let cc: i64 = await nested(Promise.resolve(7), 1)
    console.log(a.toString())
    console.log(b.toString())
    console.log(cc.toString())
    0
}

let _ = run().then(fn(_n: i64) {})
let _ = 0
