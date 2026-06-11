// `console.log(p)` / `${p}` on a Promise print its settled state.
// Both dispatches used to fall through to the raw-int arm and render
// the pointer value.
//
// Pending-state prints are deterministic under run-to-completion:
// continuations only run at drain points, never mid-statement, so
// printing a `Promise.all` of already-resolved inputs right after
// the call always shows `<promise pending>`. (Historical note:
// under the old worker-pool model that exact line raced the pool —
// `<promise resolved>` fired 1/400 under parallel load and was the
// probabilistic AOT-arm suite failure. df797334 swapped it for a
// never-resolving executor; the model migration restored it, and
// both probes are pinned here.)
//
// expect: <promise resolved>
// expect: inline: <promise resolved>
// expect: <promise pending>
// expect: <promise pending>
use std.test as test
let p = Promise.resolve(5)
console.log(p)
console.log(`inline: ${p}`)
let q = new Promise<i64>(fn(resolve: fn(i64), reject: fn(string)) {})
console.log(q)
let all = Promise.all([Promise.resolve(1), Promise.resolve(2)])
console.log(all)
let _ = all.then(fn(vs: i64[]) { test.expect(vs[0] + vs[1], 3) })
let _ = p.then(fn(v: i64) { test.expect(v, 5) })
