// A generic method on a generic class that calls ANOTHER generic method
// through `this` (`twice<U> { this.apply(f) }`) failed at MIR lowering
// ("no method apply<string> on class"). The checker records a `this`-call
// under the GENERIC base name (`Box`) because at check time the class was
// still generic, so method specialization couldn't match it to the
// specialized `Box<i64>`. The method monomorphizer now remaps a
// `this`-call's recorded base name to the specialized class it is
// processing, and seeds the inner call BEFORE rewriting its name (the
// rewrite would otherwise hide the call from the seed).
//
// expect: ok
use std.test as test

class Box<T> {
    pub v: T
    init(v: T) { this.v = v }
    apply<U>(f: fn(T): U): U { f(this.v) }
    twice<U>(f: fn(T): U): U { this.apply(f) }              // this -> apply
    thrice<U>(f: fn(T): U): U { this.twice(f) }             // this -> twice -> apply
    remapSelf<U>(f: fn(T): U): Box<U> { new Box<U>(this.apply(f)) } // this-call + generic return
}

let b = new Box<i64>(7)
test.expectStr(b.twice(fn(x: i64): string { x.toString() }), "7")
test.expectStr(b.thrice(fn(x: i64): string { x.toString() }), "7")
test.expect(b.thrice(fn(x: i64): i64 { x * 2 }), 14)
let bx = b.remapSelf(fn(x: i64): string { x.toString() })
test.expectStr(bx.v, "7")

console.log("ok")
