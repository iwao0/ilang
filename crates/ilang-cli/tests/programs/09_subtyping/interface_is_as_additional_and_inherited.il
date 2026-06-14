// `is` / `as?` must recognise EVERY interface a class conforms to, not
// just the first declared base.
//
// Before: the runtime type test (`emit_is_subclass`) computed its accept
// set by walking only the single `parent` field. A class records its
// FIRST interface base as `parent`, so `is`/`as?` answered true only for
// that one — an interface in the `interfaces` list (`class C: A, X`) or
// an interface reached through interface inheritance (`interface C: B`,
// `B: A`) both returned FALSE, and `as?` returned `none`. The class now
// carries its full transitive interface set (`implements`) and the
// accept-set scan follows those edges too.
//
// expect: ok
use std.test as test

interface A { foo(): i64 }
interface B: A { bar(): i64 }      // interface inheritance
interface C: B { baz(): i64 }
interface X { qux(): i64 }         // unrelated extra interface
interface Unrel { zap(): i64 }     // implemented by nobody here

// Impl conforms to C directly and to B, A transitively; also to X via
// the `interfaces` list (not the first base).
class Impl: C, X {
    foo(): i64 { 1 }
    bar(): i64 { 2 }
    baz(): i64 { 3 }
    qux(): i64 { 4 }
}
class OnlyA: A {
    foo(): i64 { 9 }
}

let a: A = new Impl()

// `is` against directly-declared, inherited, and additional interfaces
test.expectTrue(a is C)            // direct first base
test.expectTrue(a is B)            // inherited (C: B)
test.expectTrue(a is X)            // additional, from the interfaces list
test.expectTrue(a is Impl)         // concrete class
test.expectTrue(!(a is Unrel))     // not implemented -> false

// a class that implements only A is none of the others
let oa: A = new OnlyA()
test.expectTrue(!(oa is B))
test.expectTrue(!(oa is C))
test.expectTrue(!(oa is X))

// `as?` to an inherited interface yields some, and the inherited method
// dispatches
let mb = a as? B
test.expect(match mb {
    some(b) { b.bar() }
    none { -1 }
}, 2)

// `as?` to an additional interface yields some
let mx = a as? X
test.expect(match mx {
    some(x) { x.qux() }
    none { -1 }
}, 4)

// `as?` that must fail returns none
let bad = oa as? C
test.expect(match bad {
    some(_) { 1 }
    none { 0 }
}, 0)

// class hierarchy is unaffected: siblings stay distinct
class Animal { tag(): i64 { 0 } }
class Dog: Animal { override tag(): i64 { 1 } }
class Cat: Animal { override tag(): i64 { 2 } }
let an: Animal = new Dog()
test.expectTrue(an is Dog)
test.expectTrue(an is Animal)
test.expectTrue(!(an is Cat))

console.log("ok")
