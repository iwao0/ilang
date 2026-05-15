// `break` / `continue` inside an async `while` body now jump
// to the user's logical while — NOT the generated outer
// `loop { switch state_idx }` driver. The pre-pass rewrites
// each `break` to `state.state_idx = after_idx; continue` and
// each `continue` to `state.state_idx = header_idx; continue`,
// so the driver re-dispatches at the correct target state.
//
// Test 1: break after 3 iterations (would otherwise hang the
// result promise — previously a silent failure).
// Test 2: continue inside the body skips total accumulation
// for odd `i`, accumulating only even-i contributions.
//
// expect: 21
// expect: 14

async fn breakAfterThree(p: Promise<i64>): i64 {
    let total = 0
    let i = 0
    while i < 100 {
        let v = await p
        total = total + v
        i = i + 1
        if i >= 3 {
            break
        }
    }
    total
}

async fn skipOdd(p: Promise<i64>): i64 {
    let total = 0
    let i = 0
    while i < 4 {
        let v = await p
        i = i + 1
        if i % 2 == 1 {
            continue
        }
        total = total + v
    }
    total
}

async fn run(): i64 {
    let a: i64 = await breakAfterThree(Promise.resolve(7))
    let b: i64 = await skipOdd(Promise.resolve(7))
    console.log(a.toString())
    console.log(b.toString())
    0
}

let _ = run().then(fn(_n: i64) {})
let _ = 0
