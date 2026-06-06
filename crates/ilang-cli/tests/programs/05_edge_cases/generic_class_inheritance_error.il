// "Inheritance on generic classes isn't supported" (syntax.md §7
// L990). A child class can't name an instantiated generic parent
// like `class Sub: Box<i64> { ... }` — the parser rejects the `<`
// after the parent class name in an `extends` position.
//
// expect-error: unexpected token Lt

class Box<T> {
    x: T
    init(v: T) { this.x = v }
}
class Sub: Box<i64> {
    init(v: i64) { super(v) }
}
