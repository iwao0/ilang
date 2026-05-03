// `override` without a matching parent method is an error.
// jit: skip
// expect-error: no parent
class A {
    init() {}
    override bar(): i64 { 1 }
}
let _ = new A()
