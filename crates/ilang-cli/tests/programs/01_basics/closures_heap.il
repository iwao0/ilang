// Closures capturing heap-typed values (string / array / object).
// Stage C: ARC-managed closures retain captures on construct and
// release them when the last reference drops.
use std.test

// String capture.
let prefix = "Hi, "
let greet = fn(name: string): string { prefix + name }
test.expectStr(greet("Alice"), "Hi, Alice")
test.expectStr(greet("Bob"), "Hi, Bob")

// Array capture (read-only use).
let nums: i64[] = [10, 20, 30]
let pick = fn(i: i64): i64 { nums[i] }
test.expect(pick(0), 10)
test.expect(pick(2), 30)

// Object capture.
class Counter {
    n: i64
    init() { this.n = 0 }
    bump(): i64 { this.n = this.n + 1; this.n }
}
let c = new Counter()
let bump = fn(): i64 { c.bump() }
test.expect(bump(), 1)
test.expect(bump(), 2)
test.expect(bump(), 3)
test.expect(c.n, 3)             // shared with the outer binding

// Multiple captures of mixed types — array + i64.
let arr: i64[] = [7, 8, 9]
let mult = 3
let pair = fn(i: i64): i64 { arr[i] * mult }
test.expect(pair(0), 21)
test.expect(pair(2), 27)
