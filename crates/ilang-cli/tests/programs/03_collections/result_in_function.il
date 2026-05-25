// Functions returning Result; both branches annotated by the fn's
// declared return type so the typechecker can refine inferred Any.
use std.test as test
fn divide(a: i64, b: i64): Result<i64, string> {
    if b == 0 { Result.err("division by zero") } else { Result.ok(a / b) }
}

let good = match divide(10, 2) {
    ok(v) { v }
    err(_) { -1 }
}
test.expect(good, 5)

let bad = match divide(10, 0) {
    ok(v) { v }
    err(_) { -99 }
}
test.expect(bad, -99)

let bad_msg = match divide(1, 0) {
    ok(_) { "ok" }
    err(e) { e }
}
test.expectStr(bad_msg, "division by zero")

// Chained: parse one then another.
fn parse_pos(n: i64): Result<i64, string> {
    if n > 0 { Result.ok(n) } else { Result.err("not positive") }
}

fn double_if_pos(n: i64): Result<i64, string> {
    match parse_pos(n) {
        ok(v) { Result.ok(v * 2) }
        err(e) { Result.err(e) }
    }
}

let dp = match double_if_pos(7) {
    ok(v) { v }
    err(_) { 0 }
}
test.expect(dp, 14)

let dn = match double_if_pos(-3) {
    ok(_) { 0 }
    err(e) { e.length as i64 }
}
test.expect(dn, "not positive".length)
