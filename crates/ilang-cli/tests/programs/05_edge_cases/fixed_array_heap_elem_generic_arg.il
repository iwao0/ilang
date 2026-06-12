// A generic type parameter MAY resolve to a fixed-length
// heap-element array. Cells that take it by value copy on store
// (Optional via some()/coercion, enum payloads) — so the Optional
// below owns an independent copy and writing through `a` doesn't
// reach it. Cells that can't own one yet (tuple slots, dynamic
// array elements, Map cells, Promise values, reassignment) surface
// a "not supported" diagnostic at compile time instead of the
// double free this used to abort with.
//
// expect: hold val=1 deinits=3
// expect: maybe val=11 deinits=2
// expect: annot len=2 deinits=2

use std.test as test
let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

fn hold<T>(v: T): T? { some(v) }

// declared fixed binding into a generic — Optional owns a copy
fn run1(): i64 {
    let a: Box[2] = [new Box(1), new Box(2)]
    let o = hold(a)
    a[0] = new Box(9)            // mutating `a` ...
    match o {
        some(arr) { arr[0].n }   // ... doesn't reach the copy
        none { -1 }
    }
}
let before1 = deinits[0]
let r1 = run1()
console.log("hold val=" + r1.toString()
    + " deinits=" + (deinits[0] - before1).toString())

// enum payload cell copies too
enum Maybe<T> { has: (T), nothing }
fn run2(): i64 {
    let a: Box[2] = [new Box(5), new Box(6)]
    let m = Maybe.has(a)
    match m {
        has(arr) { arr[0].n + arr[1].n }
        nothing { -1 }
    }
}
let before2 = deinits[0]
let r2 = run2()
console.log("maybe val=" + r2.toString()
    + " deinits=" + (deinits[0] - before2).toString())

// the direct annotation works as well now
fn run3(): i64 {
    let a: Box[2] = [new Box(7), new Box(8)]
    let o: Box[2]? = some(a)
    let arr = o.unwrap()
    arr.length
}
let before3 = deinits[0]
let r3 = run3()
console.log("annot len=" + r3.toString()
    + " deinits=" + (deinits[0] - before3).toString())
