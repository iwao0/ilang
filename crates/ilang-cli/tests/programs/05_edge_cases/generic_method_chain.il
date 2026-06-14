// A chained call through a generic method that returns a generic
// instantiation — `b.remap(f).remap(g).remap(h)` — needs FIXED-POINT
// monomorphization. Each link's result class (`Box<string>`, then back
// to `Box<i64>`, then `Box<bool>`) is synthesized only after the
// previous link's method is specialized, which in turn lets the next
// link's method be specialized. A single class/method pass stopped after
// the first link ("no method remap on class"); the pipeline now iterates
// class + method monomorphization until the program stops growing.
//
// expect: ok
use std.test as test

class Tally { n: i64; init() { this.n = 0 } }
let t = new Tally()

class Box<T> {
    pub v: T
    init(v: T) { this.v = v }
    remap<U>(f: fn(T): U): Box<U> { new Box<U>(f(this.v)) }
    deinit() { t.n = t.n + 1 }
}

let b = new Box<i64>(5)
// Box<i64> -> Box<string> -> Box<i64> -> Box<bool>
let r = b.remap(fn(x: i64): string { x.toString() })
        .remap(fn(s: string): i64 { s.length })
        .remap(fn(n: i64): bool { n > 0 })
test.expectBool(r.v, true)

console.log("ok")
