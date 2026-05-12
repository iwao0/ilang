// Static field types are limited to numeric primitives, bool,
// string, or dynamic arrays of numeric primitives. Object types
// still need a slot-init phase the JIT doesn't ship yet.
// expect-error: not yet supported
class Inner {
    init() {}
}
class Outer {
    static handle: Inner = new Inner()
}
let _ = new Outer()
