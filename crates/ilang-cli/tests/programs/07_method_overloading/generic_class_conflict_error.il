// Methods on a generic class cannot be overloaded (keeps mono+overload
// resolution paths separate).
// expect-error: overload
class Box<T> {
    v: T
    init(v: T) { this.v = v }
    f(x: i64): i64 { x }
    f(x: string): i64 { 0 }
}
let b = new Box<i64>(1)
let _ = b.f(1)
