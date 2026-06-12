// `?` on an Optional — `none` propagates (early `return none` from
// the enclosing fn, which must be a sync fn returning T?), `some`
// unwraps. The parser's type-blind `?` desugar is rewritten at
// check/lower time when the operand is an Optional.
//
// Fixing this exposed a use-after-free in the EXISTING Result `?`:
// the desugar's `__try_*` temp releases at its block's scope exit
// BEFORE the outer `let v = e?` could retain the payload — a heap
// payload read after `?` saw freed memory (the round-15 probes
// never carried the payload past the desugar block). The bare-Var
// arm tail now mints the caller's +1 inside the arm, for both
// Result and Optional.
//
// expect: basic some=42 none=-1
// expect: heap ok=112 none=-1 deinits=3
// expect: churn delta=0 deinits=150
// expect: chain=21
// expect: result r=12 deinits=1

use std.test as test
let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

fn first(xs: i64[]): i64? {
    if xs.length > 0 { some(xs[0]) } else { none }
}
fn double(xs: i64[]): i64? {
    let v = first(xs)?
    some(v * 2)
}
let a = match double([21]) {
    some(v) { v }
    none { -1 }
}
let b = match double([]) {
    some(v) { v }
    none { -1 }
}
console.log("basic some=" + a.toString() + " none=" + b.toString())

// heap payload survives past the desugar block; the none path
// sweeps live heap lets (s, keeper) on the early return
fn findBox(k: i64): Box? {
    if k >= 0 { some(new Box(k)) } else { none }
}
fn pick(k: i64): i64? {
    let s = "alive-" + k.toString()
    let keeper = new Box(100)
    let bx = findBox(k)?
    some(bx.n + keeper.n + s.length)
}
let bd = deinits[0]
let okv = match pick(5) {
    some(v) { v }
    none { -1 }
}
let nv = match pick(-1) {
    some(v) { v }
    none { -1 }
}
console.log("heap ok=" + okv.toString() + " none=" + nv.toString()
    + " deinits=" + (deinits[0] - bd).toString())

let acc: i64[] = [0]
let b2 = test.liveAllocBytes()
let bd2 = deinits[0]
let i = 0
while i < 100 {
    acc[0] = acc[0] + (match pick(i - 50) {
        some(v) { v }
        none { 0 }
    })
    i = i + 1
}
console.log("churn delta=" + (test.liveAllocBytes() - b2).toString()
    + " deinits=" + (deinits[0] - bd2).toString())

fn chain(xs: i64[]): i64? {
    let x = first(xs)?
    let y = first([x + 1])?
    some(x + y)
}
let cv = match chain([10]) {
    some(v) { v }
    none { -1 }
}
console.log("chain=" + cv.toString())

// the Result `?` UAF regression: heap payload read after the `?`
fn findBoxR(k: i64): Result<Box, string> {
    if k >= 0 { Result.ok(new Box(k)) } else { Result.err("neg") }
}
fn pickR(k: i64): Result<i64, string> {
    let s = "alive-" + k.toString()
    let bx = findBoxR(k)?
    Result.ok(bx.n + s.length)
}
let bd3 = deinits[0]
let rr = match pickR(5) {
    ok(v) { v }
    err(e) { -1 }
}
console.log("result r=" + rr.toString()
    + " deinits=" + (deinits[0] - bd3).toString())
