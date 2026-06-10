// `console.log(p)` / `${p}` on a Promise print its settled state.
// Both dispatches used to fall through to the raw-int arm and render
// the pointer value.
//
// expect: <promise resolved>
// expect: inline: <promise resolved>
// expect: <promise pending>
use std.test as test
let p = Promise.resolve(5)
console.log(p)
console.log(`inline: ${p}`)
let q = Promise.all([Promise.resolve(1)])
console.log(q)
let _ = p.then(fn(v: i64) { test.expect(v, 5) })
