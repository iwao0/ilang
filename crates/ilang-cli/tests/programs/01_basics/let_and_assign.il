// `let` binding, plain reassignment, compound assignments, shadowing.
use test

// Type inference vs explicit annotation.
let a = 42
test.expect(a, 42)
let b: i64 = 7
test.expect(a + b, 49)

// Annotated literal coerces to the annotation (int → f64).
let f: f64 = 1
test.expectF64(f, 1.0)

// Plain reassignment (no `mut` keyword in ilang — every let is rebindable).
let c = 10
c = 20
test.expect(c, 20)
c = c + 1
test.expect(c, 21)

// Inner-scope shadowing — outer `c` restored on block exit.
let x = 1
{
    let x = 100
    test.expect(x, 100)
}
test.expect(x, 1)

// Shadowing with a different type.
let y = 5
{
    let y: string = "hi"
    test.expectStr(y, "hi")
}
test.expect(y, 5)

// Compound assignments.
let n = 10
n += 5;  test.expect(n, 15)
n -= 3;  test.expect(n, 12)
n *= 2;  test.expect(n, 24)
n /= 4;  test.expect(n, 6)
n %= 4;  test.expect(n, 2)
