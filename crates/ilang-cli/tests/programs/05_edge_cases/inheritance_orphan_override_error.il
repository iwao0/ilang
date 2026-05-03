// `override` without a matching parent method is an error.
// expect-error: no parent
class A {
    init() {}
    override bar(): i64 { 1 }
}
let _ = new A()
