// A generic-class instance flows into its declared interface in every
// covariance position, not just a plain parameter / local / array (which
// `generic_class_implements_interface.il` already pins): covariant
// return, `Optional<Sized>`, `Map<string, Sized>` value, a tuple element,
// and a downcast back to the instantiation.
//
// expect: ok
use std.test as test

interface Sized { size(): i64 }

class Box<T>: Sized {
    pub v: T
    init(v: T) { this.v = v }
    size(): i64 { 1 }
}
class Empty: Sized { init() {} size(): i64 { 0 } }

// Covariant return.
fn make(): Sized { new Box<i64>(5) }
test.expect(make().size(), 1)

// Optional<interface>.
let o: Sized? = some(new Box<i64>(1))
if let some(s) = o { test.expect(s.size(), 1) }

// Map value typed as the interface.
let m: Map<string, Sized> = {"a": new Box<i64>(2)}
test.expect(m["a"].size(), 1)

// Tuple element typed as the interface.
let t: (Sized, i64) = (new Box<i64>(3), 99)
test.expect(t[0].size(), 1)
test.expect(t[1], 99)

// Interface array mixing a generic and a non-generic implementer.
let xs: Sized[] = [new Box<i64>(1), new Empty()]
test.expect(xs[0].size() + xs[1].size(), 1)

// Downcast the interface back to the generic instantiation.
let s: Sized = new Box<i64>(7)
if let some(b) = s as? Box<i64> { test.expect(b.v, 7) }

console.log("ok")
