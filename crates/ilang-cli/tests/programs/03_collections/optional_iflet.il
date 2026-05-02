// `if let some(v) = ...` — pattern match on Optional outside `match`.
use test

let a: i64? = some(42)
let b: i64? = none

let from_a = if let some(v) = a { v } else { -1 }
test.expect(from_a, 42)

let from_b = if let some(v) = b { v } else { -1 }
test.expect(from_b, -1)

// On a string Optional too.
let s_some: string? = some("ok")
let s_none: string? = none

let r1 = if let some(s) = s_some { s + "!" } else { "miss" }
test.expectStr(r1, "ok!")

let r2 = if let some(s) = s_none { s } else { "miss" }
test.expectStr(r2, "miss")

// Inside a function — early control flow.
fn or_default(x: i64?, fallback: i64): i64 {
    if let some(v) = x { return v }
    fallback
}
test.expect(or_default(some(7), 99), 7)
test.expect(or_default(none, 99), 99)
