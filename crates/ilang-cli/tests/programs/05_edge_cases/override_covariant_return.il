// Method override may narrow the return type to a subclass —
// standard OO covariant-return semantics. Pre-fix the type
// checker rejected `override clone(): A` (A extends S) of
// `clone(): S` because the `is_subclass` closure looked the
// in-progress class up in `self.classes`, which doesn't have
// the entry yet (the very call we're checking is what populates
// it). Walk from `c.parent` directly when `child == c.name`.
//
// expect: A {}
// expect: A
// expect: A {}
// expect: A

class S {
    init() {}
    clone(): S { new S() }
}
class A extends S {
    init() { super() }
    override clone(): A { new A() }
}

let a: A = new A()
let cloned = a.clone()
console.log(cloned)
console.log(typeof(cloned).name)

// Through an upcast — virtual dispatch should still pick A's
// override and return an A.
let s: S = new A()
let scloned = s.clone()
console.log(scloned)
console.log(typeof(scloned).name)
