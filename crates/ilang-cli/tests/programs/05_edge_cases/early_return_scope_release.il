// An early `return` jumps past `lower_block`'s scope-exit release
// pass, so `lower_return` sweeps every live heap binding above the
// param scope (mirroring `lower_break`'s sweep for loops). Before
// the sweep, EVERY `return` past a live heap `let` leaked it —
// plain fns, match arms, loop bodies, and the async poll fn's
// generated suspend `return` (which leaked the awaited promise and
// every local carried in the state). A borrowed return value gets
// +1 before the sweep so the caller still receives an owned
// reference.
//
// expect: fn-top strings=0
// expect: match-arm strings=0
// expect: loop strings=0
// expect: borrowed-return ok
// expect: borrowed-box 7

use std.test as test
class Box { n: i64; init(x: i64) { this.n = x } }
enum E { a, b }

fn fnTop(k: i64): i64 {
    let s: string = "x" + k.toString()
    if k > -1 { return s.length }
    0
}
fn matchArm(e: E, k: i64): i64 {
    match e {
        a {
            let s: string = "y" + k.toString()
            return s.length
        }
        b { return 0 }
    }
    0
}
fn loopBody(k: i64): i64 {
    let j = 0
    while j < 10 {
        let s: string = "z" + k.toString()
        if j == 3 { return s.length }
        j = j + 1
    }
    0
}

let acc: i64[] = [0]
let s1 = test.liveStringCount()
let i = 0
while i < 200 { acc[0] = acc[0] + fnTop(i); i = i + 1 }
console.log("fn-top strings=" + (test.liveStringCount() - s1).toString())

let s2 = test.liveStringCount()
let j = 0
while j < 200 { acc[0] = acc[0] + matchArm(E.a, j); j = j + 1 }
console.log("match-arm strings=" + (test.liveStringCount() - s2).toString())

let s3 = test.liveStringCount()
let m = 0
while m < 200 { acc[0] = acc[0] + loopBody(m); m = m + 1 }
console.log("loop strings=" + (test.liveStringCount() - s3).toString())

// 借用値の return: sweep が束縛を解放しても caller は所有参照を
// 受け取る (use-after-free しない)
fn pickString(k: i64): string {
    let s: string = "val" + k.toString()
    if k >= 0 { return s }
    ""
}
let got = pickString(123)
test.expectTrue(got == "val123")
console.log("borrowed-return ok")

fn pickBox(k: i64): Box {
    let b = new Box(k)
    if k >= 0 { return b }
    new Box(-1)
}
let bx = pickBox(7)
console.log("borrowed-box " + bx.n.toString())
