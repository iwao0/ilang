// Fixed-length heap-element arrays in EVERY container cell —
// tuple slots, dynamic-array elements (literal / push / index
// write), Map values (literal / set), closure captures, Promise
// values. Each store takes a value COPY (`$array.copyShallow`):
// mutating the source afterwards is never visible through the
// cell, and per-iteration churn across all cells is leak-free.
// Internally a fixed array is an ordinary rc'd array whose length
// is fixed at the type level.
//
// expect: tuple val=11 deinits=3
// expect: dynarr val=6 deinits=3
// expect: map val=10 deinits=3
// expect: capture val=7 deinits=3
// expect: idx val=12 deinits=3
// expect: promise val=1
// expect: churn delta=0 deinits=200

use std.test as test
let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// tuple slot
fn t1(): i64 {
    let a: Box[2] = [new Box(1), new Box(2)]
    let t = (a, 10)
    a[0] = new Box(9)
    let (arr, k) = t
    arr[0].n + k
}
let b1 = deinits[0]
console.log("tuple val=" + t1().toString()
    + " deinits=" + (deinits[0] - b1).toString())

// dynamic-array elements: literal store + push
fn t2(): i64 {
    let a: Box[2] = [new Box(3), new Box(4)]
    let xs: Box[2][] = [a]
    xs.push(a)
    a[0] = new Box(99)
    xs[0][0].n + xs[1][0].n
}
let b2 = deinits[0]
console.log("dynarr val=" + t2().toString()
    + " deinits=" + (deinits[0] - b2).toString())

// Map values: literal + set
fn t3(): i64 {
    let a: Box[2] = [new Box(5), new Box(6)]
    let m: Map<string, Box[2]> = {"k": a}
    m.set("j", a)
    a[0] = new Box(77)
    let x = m.get("k")
    let y = m.get("j")
    let vx = match x {
        some(arr) { arr[0].n }
        none { -1 }
    }
    let vy = match y {
        some(arr) { arr[0].n }
        none { -1 }
    }
    vx + vy
}
let b3 = deinits[0]
console.log("map val=" + t3().toString()
    + " deinits=" + (deinits[0] - b3).toString())

// closure capture (value copy — later writes invisible)
fn t4(): i64 {
    let a: Box[2] = [new Box(7), new Box(8)]
    let f = fn(): i64 { a[0].n }
    a[0] = new Box(70)
    f()
}
let b4 = deinits[0]
console.log("capture val=" + t4().toString()
    + " deinits=" + (deinits[0] - b4).toString())

// index write into a dynamic array of fixed arrays
fn t5(): i64 {
    let a: Box[2] = [new Box(11), new Box(12)]
    let xs: Box[2][] = [a]
    xs[0] = a
    a[1] = new Box(120)
    xs[0][1].n
}
let b5 = deinits[0]
console.log("idx val=" + t5().toString()
    + " deinits=" + (deinits[0] - b5).toString())

// Promise value (resolve copies; await sees the copy)
async fn t6(): i64 {
    let a: Box[2] = [new Box(1), new Box(2)]
    let p = Promise.resolve(a)
    a[0] = new Box(9)
    let arr = await p
    arr[0].n
}
let pr = t6()
pr.then(fn(v: i64) {
    console.log("promise val=" + v.toString())
})

// churn across all cells — leak-free, exact deinits
fn churn(k: i64): i64 {
    let a: Box[2] = [new Box(k), new Box(k + 1)]
    let t = (a, 1)
    let xs: Box[2][] = [a]
    let m: Map<string, Box[2]> = {"k": a}
    let f = fn(): i64 { a[1].n }
    let o = some(a)
    let (arr, _) = t
    arr[0].n + xs[0][0].n + f()
}
let acc: i64[] = [0]
let b6 = test.liveAllocBytes()
let bd = deinits[0]
let i = 0
while i < 100 { acc[0] = acc[0] + churn(i); i = i + 1 }
console.log("churn delta=" + (test.liveAllocBytes() - b6).toString()
    + " deinits=" + (deinits[0] - bd).toString())
