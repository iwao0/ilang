// A generic method on a generic class that RETURNS a generic
// instantiation of that class (`remap<U>: Box<U>` building `new Box<U>`)
// failed at MIR lowering ("unknown type: Box<U>"). Class monomorphization
// substitutes the class param `T` into the still-generic method, but its
// own param `U` arrives as `Object("U")`, which the mangle step didn't
// recognize as a type variable — so `Box<U>` got baked into a phantom
// `Box<U>` class name that method specialization (which substitutes
// types, not the New's class symbol) couldn't fix.
//
// The class-mono pass now re-substitutes generic methods with the
// method's own params mapped to `TypeVar`, keeping `Box<U>` un-mangled
// until method specialization fills U.
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
// Box<i64> -> Box<string>
let s = b.remap(fn(x: i64): string { x.toString() })
test.expectStr(s.v, "5")
// Box<i64> -> Box<i64> (the result instantiation equals the receiver's)
let d = b.remap(fn(x: i64): i64 { x * 2 })
test.expect(d.v, 10)
// A different receiver instantiation, Box<string> -> Box<i64>
let bs = new Box<string>("hi")
let n = bs.remap(fn(x: string): i64 { x.length })
test.expect(n.v, 2)

console.log("ok")
