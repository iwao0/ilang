// A generic method (with its own `<U>` type parameter) on a GENERIC
// class failed at MIR lowering ("no method apply<string> on class"):
// class monomorphization renames `Box<T>` to the mangled `Box<i64>`
// before the method monomorphizer runs, but the checker recorded the
// call site under the bare class name `Box`, so the lookup missed and
// the call was left dangling. The checker now records the call under the
// mangled class name when the receiver is a generic instantiation.
//
// (Method-on-non-generic-class and non-generic-method-on-generic-class
// already worked; only the both-generic combination was broken.)
//
// expect: ok
use std.test as test

class Tally { n: i64; init() { this.n = 0 } }
let t = new Tally()

class Box<T> {
    pub v: T
    init(v: T) { this.v = v }
    apply<U>(f: fn(T): U): U { f(this.v) }
    deinit() { t.n = t.n + 1 }
}

// Box<i64>, apply specialized to U = string.
let bi = new Box<i64>(5)
test.expectStr(bi.apply(fn(x: i64): string { x.toString() }), "5")
// Same receiver, apply specialized to a different U = i64.
test.expect(bi.apply(fn(x: i64): i64 { x * 2 }), 10)
// A different class instantiation Box<string>, apply -> i64.
let bs = new Box<string>("hello")
test.expect(bs.apply(fn(s: string): i64 { s.length }), 5)

console.log("ok")
