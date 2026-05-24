// Built-in Result<T, E>. Variants `ok` / `err`.
use std.test

let r1: Result<i64, string> = Result.ok(42)
let r2: Result<i64, string> = Result.err("boom")

let v1 = match r1 {
    ok(v) { v }
    err(_) { -1 }
}
test.expect(v1, 42)

let v2 = match r2 {
    ok(v) { v }
    err(_) { -1 }
}
test.expect(v2, -1)

// Pattern bindings work for both arms.
let msg = match r2 {
    ok(_) { "" }
    err(e) { e }
}
test.expectStr(msg, "boom")

// Long-form pattern with explicit Result. prefix also accepted.
let v3 = match r1 {
    Result.ok(v) { v }
    Result.err(_) { -1 }
}
test.expect(v3, 42)
