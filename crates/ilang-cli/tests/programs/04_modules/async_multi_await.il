// `async fn` with multiple `await` sites — exercises the
// state-machine lowering. The desugar emits:
//   - a `__sumThree_State` class with one field per param + one
//     per `let` binding + state_idx + __async_promise,
//   - a `__sumThree_poll` fn that switches on state_idx and runs
//     the next chunk of the original body,
//   - a thin `sumThree` wrapper that allocates state + promise
//     and kicks the first poll.
//
// expect: 60

async fn sumThree(a: Promise<i64>, b: Promise<i64>, c: Promise<i64>): i64 {
    // Bindings drop their `: i64` annotation — the desugar's mini-
    // inferencer pulls `i64` out of each param's `Promise<i64>`.
    let x = await a
    let y = await b
    let z = await c
    x + y + z
}

let _ = sumThree(
    Promise.resolve(10),
    Promise.resolve(20),
    Promise.resolve(30)
).then(fn(n: i64) {
    console.log(n.toString())
})

let _ = 0
