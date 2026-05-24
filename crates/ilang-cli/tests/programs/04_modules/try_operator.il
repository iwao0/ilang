// Postfix `?` — Result short-circuit operator. `expr?` unwraps
// `Result.ok(v)` to `v`, and on `Result.err(e)` early-returns
// `Result.err(e)` from the enclosing fn (whose own return type
// must therefore be `Result<_, E>` with the same `E`).
use std.test

fn parse(s: string): Result<i32, string> {
    if s == "42" { Result.ok(42 as i32) }
    else { Result.err("not 42") }
}

fn doubled(input: string): Result<i32, string> {
    let v = parse(input)?
    Result.ok(v * 2 as i32)
}

// Ok path: parse succeeds, ? unwraps, doubled returns ok(84).
match doubled("42") {
    ok(n) { test.expect(n as i64, 84) }
    err(_) { test.expectTrue(false) }
}

// Err path: parse fails, ? short-circuits with err("not 42").
match doubled("foo") {
    ok(_) { test.expectTrue(false) }
    err(e) { test.expectStr(e, "not 42") }
}

// Chained `?` — three calls in a row, second fails.
fn step1(): Result<i32, string> { Result.ok(1 as i32) }
fn step2(n: i32): Result<i32, string> { Result.err("step2 failed") }
fn pipeline(): Result<i32, string> {
    let a = step1()?
    let b = step2(a)?
    Result.ok(b + 100 as i32)
}
match pipeline() {
    ok(_) { test.expectTrue(false) }
    err(e) { test.expectStr(e, "step2 failed") }
}
