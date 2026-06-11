// `console.log(p)` / `${p}` on a Promise print its settled state.
// Both dispatches used to fall through to the raw-int arm and render
// the pointer value.
//
// The pending-state line uses a promise whose executor never calls
// resolve — permanently pending, so the printed state is
// deterministic. (Historical note: an earlier revision printed
// `Promise.all([...])` here, and under the old worker-pool model
// its resolution ran CONCURRENTLY with main — on a loaded machine
// the pool occasionally won the race and printed
// `<promise resolved>`, 1/400 under parallel load. The current
// run-to-completion model would make even that deterministic:
// continuations only run at drain points, never mid-statement.)
//
// expect: <promise resolved>
// expect: inline: <promise resolved>
// expect: <promise pending>
use std.test as test
let p = Promise.resolve(5)
console.log(p)
console.log(`inline: ${p}`)
let q = new Promise<i64>(fn(resolve: fn(i64), reject: fn(string)) {})
console.log(q)
let all = Promise.all([Promise.resolve(1), Promise.resolve(2)])
let _ = all.then(fn(vs: i64[]) { test.expect(vs[0] + vs[1], 3) })
let _ = p.then(fn(v: i64) { test.expect(v, 5) })
