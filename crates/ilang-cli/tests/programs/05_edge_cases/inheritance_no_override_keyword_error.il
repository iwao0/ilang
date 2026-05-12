// Hiding a parent's method without `override` is an error.
// expect-error: hides a parent method without the `override`
class A {
    init() {}
    foo(): i64 { 1 }
}
class B: A {
    init() { super() }
    foo(): i64 { 2 }
}
let _ = new B()
