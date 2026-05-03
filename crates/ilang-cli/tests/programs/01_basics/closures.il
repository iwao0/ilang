// Closures capture outer variables by value at creation time.
// jit: skip
use test

let factor = 10
let scale = fn(x: i64): i64 { x * factor }
test.expect(scale(3), 30)
test.expect(scale(7), 70)

// Capture-by-value: changing the outer variable after creation
// doesn't affect the closure.
let factor_local = factor      // shadowed
let factor = 999               // not visible to scale
test.expect(scale(3), 30)

// Closures can be returned from functions.
fn make_adder(n: i64): fn(i64): i64 {
    fn(x: i64): i64 { x + n }
}
let add5 = make_adder(5)
let add100 = make_adder(100)
test.expect(add5(3), 8)
test.expect(add100(3), 103)

// Closures + array.map.
let xs: i64[] = [1, 2, 3, 4]
let mult = 7
let ys = xs.map(fn(x: i64): i64 { x * mult })
test.expect(ys[0], 7)
test.expect(ys[1], 14)
test.expect(ys[2], 21)
test.expect(ys[3], 28)

// Nested closures.
fn make_double_adder(a: i64): fn(i64): fn(i64): i64 {
    fn(b: i64): fn(i64): i64 {
        fn(c: i64): i64 { a + b + c }
    }
}
let stage1 = make_double_adder(1)
let f = stage1(2)
test.expect(f(3), 6)

// String capture.
let prefix = "Hi, "
let greet = fn(name: string): string { prefix + name }
test.expectStr(greet("Alice"), "Hi, Alice")
