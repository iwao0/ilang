// A bare method call inside a method-internal closure (`compute()`
// instead of `this.compute()`) is deliberately not supported: only
// fields and properties are routed through capture (`name_is_this_member`),
// not methods. Previously this panicked the compiler at call_fn.rs with
// "Option::unwrap() on a None value" because `this` is not a local
// inside a closure. Now it is a clean diagnostic pointing at the fix.
//
// expect-error: bare method call `compute(...)` inside a closure is not supported

class Svc {
    base: i64
    init(x: i64) { this.base = x }
    compute(): i64 { this.base * 2 }
    make(): fn(): i64 {
        fn(): i64 { compute() }
    }
}

let s = new Svc(5)
let f = s.make()
console.log(f().toString())
