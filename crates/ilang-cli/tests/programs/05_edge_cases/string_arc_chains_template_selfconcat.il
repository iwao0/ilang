// Confirmation pin (第44弾): string-ARC shapes around the in-place
// concat fix (第43弾). All sound — no string-registry growth across a
// long churn:
//
//   * method chain on a fresh string  ("v"+n).toUpper().slice(0,3)
//     — every fresh intermediate is released.
//   * template literal interpolating a heap object's method + fields
//     `${b.toString()} ... ${b.n}` — the interpolation temps and the
//     Box are released.
//   * `+=` on a string (desugars to `s = s + ...`) with a fresh rhs.
//   * self-concat `s = s + s` — the in-place path must NOT release the
//     rhs (it aliases the lhs buffer that becomes the result).
//   * `split` returning a string[].
//
// String count stays flat and the Box deinit count is exact.

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
    toString(): string { "Box(" + this.n.toString() + ")" }
}

// Method chain over a fresh string.
fn chain(n: i64): i64 {
    let r = ("v" + n.toString()).toUpper().slice(0, 3)
    r.length
}

// Template literal interpolating a heap object.
fn templated(n: i64): i64 {
    let b = new Box(n)
    let s = `box ${b.toString()} val ${b.n} end`
    s.length
}

// `+=` desugar with a fresh rhs, then a self-concat.
fn plusEqSelf(n: i64): i64 {
    let s = "v"
    s += n.toString()         // fresh rhs — released
    s = s + s                 // self-concat — rhs aliases lhs, not released
    s.length
}

// split -> string[].
fn splitLen(): i64 {
    ("a,b,c,d").split(",").length
}

let acc: i64[] = [0]

test.expect(chain(5), 2)            // "V5"
test.expect(templated(5), 20)       // "box Box(5) val 5 end"
test.expect(splitLen(), 4)

let baseStr = test.liveStringCount()
let bd = deinits[0]
let i = 0
while i < 500 {
    acc[0] = acc[0] + chain(i) + templated(i) + plusEqSelf(i) + splitLen()
    i = i + 1
}
// One Box per `templated` call, all freed.
test.expect(deinits[0] - bd, 500)
// String registry must not grow without bound (slack for cached internals).
test.expectTrue(test.liveStringCount() - baseStr < 50)
