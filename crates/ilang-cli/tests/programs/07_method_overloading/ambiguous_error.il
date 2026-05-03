// Two method overloads tying for the best score → ambiguous.
// expect-error: ambiguous
class A {
    init() {}
    f(x: i64, y: i32): i64 { x + (y as i64) }
    f(x: i32, y: i64): i64 { (x as i64) + y }
}
let a = new A()
let n: i32 = 5
let _ = a.f(n, n)
