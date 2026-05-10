// `super` only makes sense inside a class that `extends` another.
// Calling `super.method()` from a method on a class with no
// parent must be a compile-time error so users can't pretend an
// implicit base class exists.
//
// expect-error: `super` used in class "A", which has no parent

class A {
    foo(): i64 {
        super.foo()
    }
}

let _ = new A()
