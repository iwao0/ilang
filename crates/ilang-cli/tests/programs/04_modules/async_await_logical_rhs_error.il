// `await` on the short-circuiting side of `&&` / `||` can't be
// hoisted (it would run unconditionally), and the state-machine
// can't reach it in place. This used to be an internal compiler
// panic ("NoAwait after body_contains_await=true"); it's a proper
// diagnostic now.
//
// expect-error: an `await` sits in a position
async fn run(): bool {
    let ok = true
    ok && await Promise.resolve(true)
}
let _ = run()
