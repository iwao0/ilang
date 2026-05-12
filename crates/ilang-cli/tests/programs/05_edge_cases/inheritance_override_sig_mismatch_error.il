// Override must have the same signature as the parent.
// expect-error: different signature
class A {
    init() {}
    foo(n: i64): i64 { n }
}
class B: A {
    init() { super() }
    override foo(n: string): i64 { 0 }
}
let _ = new B()
