// Early `return`, value `return`, value-less Unit `return`, and the
// implicit "block tail = return value" path.
use std.test

// Early return with a value.
fn abs(n: i64): i64 {
    if n < 0 { return -n }
    n
}
test.expect(abs(-5), 5)
test.expect(abs(7), 7)

// Multiple return paths.
fn classify(n: i64): string {
    if n < 0 { return "neg" }
    if n == 0 { return "zero" }
    "pos"
}
test.expectStr(classify(-1), "neg")
test.expectStr(classify(0), "zero")
test.expectStr(classify(42), "pos")

// Value-less return in a Unit-returning fn.
class Counter {
  n: i64
  init() { this.n = 0 }
  bump_if_pos(k: i64) {
      if k <= 0 { return }
      this.n = this.n + k
  }
  get(): i64 { this.n }
}
let c = new Counter()
c.bump_if_pos(0)
c.bump_if_pos(-3)
test.expect(c.get(), 0)
c.bump_if_pos(7)
test.expect(c.get(), 7)

// Tail expression as the implicit return.
fn double(n: i64): i64 { n * 2 }
test.expect(double(21), 42)

// Nested early return inside a loop.
fn first_multiple_of(n: i64, of: i64): i64 {
    let i = 1
    while i < 100 {
        if i % of == 0 && i >= n { return i }
        i += 1
    }
    return -1
}
test.expect(first_multiple_of(20, 7), 21)
test.expect(first_multiple_of(50, 13), 52)
