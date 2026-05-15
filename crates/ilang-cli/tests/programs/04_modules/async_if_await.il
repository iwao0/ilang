// `async fn` with `if-else` at the body tail and awaits inside
// each arm. The BlockBuilder builds one state-block per chunk
// between awaits and emits a Branch terminator that picks which
// arm's first block to enter; each arm settles the result
// promise independently.
//
// expect: 42
// expect: 107

async fn pickAsync(cond: bool, p: Promise<i64>, q: Promise<i64>): i64 {
    if cond {
        let x = await p
        x * 2
    } else {
        let y = await q
        y + 100
    }
}

async fn run(): i64 {
    let a = await pickAsync(true,  Promise.resolve(21), Promise.resolve(0))
    let b = await pickAsync(false, Promise.resolve(0),  Promise.resolve(7))
    console.log(a.toString())
    console.log(b.toString())
    0
}

let _ = run().then(fn(_n: i64) {})
let _ = 0
