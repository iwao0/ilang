// `console.log(p)` / `${p}` on a Promise print its settled state.
// Both dispatches used to fall through to the raw-int arm and render
// the pointer value.
//
// The pending-state line uses a promise whose executor never calls
// resolve — permanently pending, so the printed state is
// deterministic. An earlier revision printed `Promise.all([...])`
// here, but `Promise.all` resolution runs on the worker pool
// CONCURRENTLY with the main thread: on a loaded machine the pool
// occasionally won the race and the line printed
// `<promise resolved>` (1/400 under parallel load) — the source of
// the "AOT arm fails once in a blue moon, rerun passes" mystery.
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
