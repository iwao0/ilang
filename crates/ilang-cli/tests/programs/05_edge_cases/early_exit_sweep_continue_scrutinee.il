// Early-exit ARC sweeps, round 2 (the `return` sweep landed in
// early_return_scope_release.il):
// - `continue` jumps past the loop body's scope-exit pass and
//   released NOTHING before — every heap let live at a `continue`
//   leaked once per skip.
// - a FRESH match scrutinee's arm-end Release was skipped when the
//   arm body diverged (`return` / `break`) — the cell leaked with
//   its payload. The lowerer now tracks live fresh scrutinees and
//   every early-exit sweep releases the ones it jumps out of.
// - string matches never released a fresh scrutinee AT ALL — even
//   straight-through `match "k" + s { ... }` leaked one registry
//   string per evaluation.
// - deinit fires at the early return (sweep order: innermost
//   binding first).
//
// expect: continue strings=0
// expect: scrutinee-return delta-ok
// expect: scrutinee-break delta=0
// expect: str-match strings=0
// expect: deinit-order: b,a,after-call

use std.test as test
class Box { n: i64; init(x: i64) { this.n = x } }
enum Got { yes: (Box), no }

fn skipOdd(): i64 {
    let total = 0
    let j = 0
    while j < 200 {
        j = j + 1
        let s: string = "c" + j.toString()
        if j % 2 == 1 { continue }
        total = total + s.length
    }
    total
}
let s1 = test.liveStringCount()
let r1 = skipOdd()
test.expectTrue(r1 > 0)
console.log("continue strings=" + (test.liveStringCount() - s1).toString())

fn mk(k: i64): Got { Got.yes(new Box(k)) }
fn pick(k: i64): i64 {
    match mk(k) {
        yes(b) { return b.n }
        no { return -1 }
    }
    0
}
let b2 = test.liveAllocBytes()
let i = 0
let acc: i64[] = [0]
while i < 200 { acc[0] = acc[0] + pick(i); i = i + 1 }
let d2 = test.liveAllocBytes() - b2
// 一回きりの遅延確保 (定数) は許容、反復比例の leak は不許容
test.expectTrue(d2 < 256)
console.log("scrutinee-return delta-ok")

fn pickLoop(): i64 {
    let j = 0
    let acc2 = 0
    while j < 200 {
        j = j + 1
        match mk(j) {
            yes(b) { if j % 2 == 0 { break } acc2 = acc2 + b.n }
            no { }
        }
    }
    acc2
}
let b3 = test.liveAllocBytes()
let r3 = pickLoop()
test.expectTrue(r3 > 0)
console.log("scrutinee-break delta=" + (test.liveAllocBytes() - b3).toString())

fn pickStr(k: i64): i64 {
    match "key" + k.toString() {
        "key1" { return 100 }
        _ { return k }
    }
    0
}
fn pickStrThrough(k: i64): i64 {
    match "key" + k.toString() {
        "key1" { 100 }
        _ { k }
    }
}
let s4 = test.liveStringCount()
let q = 0
while q < 200 { acc[0] = acc[0] + pickStr(q) + pickStrThrough(q); q = q + 1 }
console.log("str-match strings=" + (test.liveStringCount() - s4).toString())

let order: string[] = []
class Loud {
    tag: string
    init(t: string) { this.tag = t }
    deinit() { order.push(this.tag) }
}
fn makeTwo(): i64 {
    let a = new Loud("a")
    let b = new Loud("b")
    if a.tag.length > 0 { return 1 }
    0
}
let _r = makeTwo()
order.push("after-call")
console.log("deinit-order: " + order[0] + "," + order[1] + "," + order[2])
