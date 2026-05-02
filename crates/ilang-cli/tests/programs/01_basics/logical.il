// && / || / !  with short-circuit evaluation.
use test

// Truth tables.
test.expectBool(true && true, true)
test.expectBool(true && false, false)
test.expectBool(false && true, false)
test.expectBool(false && false, false)

test.expectBool(true || true, true)
test.expectBool(true || false, true)
test.expectBool(false || true, true)
test.expectBool(false || false, false)

test.expectBool(!true, false)
test.expectBool(!false, true)
test.expectBool(!!true, true)

// Precedence: ! > && > ||.
test.expectBool(!false && true, true)
test.expectBool(true || false && false, true)   // && binds tighter
test.expectBool((true || false) && false, false)

// Short-circuit: the rhs of `&&` after `false` and `||` after `true`
// isn't evaluated. Verify by giving it an effect (mutating a counter).
class Counter {
  n: i64
  init() { this.n = 0 }
  inc(): bool { this.n = this.n + 1; true }
  count(): i64 { this.n }
}
let c = new Counter()

let _ignore1 = false && c.inc()
test.expect(c.count(), 0)

let _ignore2 = true || c.inc()
test.expect(c.count(), 0)

// Both forced now.
let _f1 = true && c.inc()    // inc fires
let _f2 = false || c.inc()   // inc fires
test.expect(c.count(), 2)
