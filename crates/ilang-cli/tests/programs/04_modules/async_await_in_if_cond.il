// `await` inside an `if` condition / `match` scrutinee is now
// lifted by the sub-expression lift pass:
//   if await p { A } else { B }
// becomes
//   let __await_t0 = await p
//   if __await_t0 { A } else { B }
// so the state-machine sees only canonical let-await + plain
// branch. (`while` cond is intentionally NOT lifted because it's
// re-evaluated each iter — lifting once would change semantics.)
//
// expect: ok-true
// expect: ok-false
// expect: hello, world

async fn pickIf(p: Promise<bool>): string {
    if await p {
        "ok-true"
    } else {
        "ok-false"
    }
}

async fn greet(p: Promise<string>): string {
    let name = await p
    "hello, " + name
}

async fn run(): i64 {
    let a: string = await pickIf(Promise.resolve(true))
    let b: string = await pickIf(Promise.resolve(false))
    let c: string = await greet(Promise.resolve("world"))
    console.log(a)
    console.log(b)
    console.log(c)
    0
}

let _ = run().then(fn(_n: i64) {})
let _ = 0
