// A class implementing `B` (where `interface B: A`) must also implement
// A's inherited methods. `Bad` provides `bar` (B's own) but not `foo`
// (inherited from A), so conformance fails at the A boundary — otherwise
// a `b.foo()` call would dispatch into an unfilled vtable slot.
//
// expect-error: class "Bad" does not implement "A"."foo"
interface A { foo(): i64 }
interface B: A { bar(): i64 }

class Bad: B {
    init() {}
    bar(): i64 { 1 }
}

let b: B = new Bad()
console.log(b.bar().toString())
