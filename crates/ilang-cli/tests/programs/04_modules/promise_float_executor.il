// Float-typed promises through the executor path. The executor's
// `resolve` parameter is `fn(f64)` / `fn(f32)`, so the call site
// passes the value in a FLOAT register — the runtime must hand out
// a float-ABI resolve stub. The old i64-only stub read the env
// pointer as the value (garbage floats) and garbage as the env
// (SIGSEGV on the capture reads in the deferred form).
//
// expect: inline f64: 1.5
// expect: inline f32: 0.5
// expect: main end
// expect: float-all: 4.0
// expect: deferred f64: 2.25

use std.time as time
use std.test as test

let p1 = new Promise<f64>(fn(resolve: fn(f64), reject: fn(string)) {
    resolve(1.5)
})
let _ = p1.then(fn(x: f64) {
    console.log("inline f64: " + x.toString())
})

let p3 = new Promise<f32>(fn(resolve: fn(f32), reject: fn(string)) {
    resolve(0.5 as f32)
})
let _ = p3.then(fn(x: f32) {
    console.log("inline f32: " + (x as f64).toString())
})
time.tick()

let saved: fn(f64)[] = []
let p2 = new Promise<f64>(fn(resolve: fn(f64), reject: fn(string)) {
    saved.push(resolve)
})
let _t = time.setTimeout(fn() {
    let r = saved[0]
    r(2.25)
}, 1)
let _ = p2.then(fn(x: f64) {
    test.expectTrue(x == 2.25)
    console.log("deferred f64: " + x.toString())
})

let fa = Promise.all([Promise.resolve(1.5), Promise.resolve(2.5)])
let _ = fa.then(fn(vs: f64[]) {
    console.log("float-all: " + (vs[0] + vs[1]).toString())
})

console.log("main end")
