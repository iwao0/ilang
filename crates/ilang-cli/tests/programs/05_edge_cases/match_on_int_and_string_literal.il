// Match arms on int and string literals — the discriminator is a
// value, not an enum tag. Wildcard `_` is mandatory since the value
// space is open. The basics fixtures only exercise enum-pattern
// matches; this fixture is the literal-pattern variant.
use std.test as test
fn category(n: i64): string {
    match n {
        0 { "zero" }
        1 { "one" }
        2 { "two" }
        _ { "many" }
    }
}
test.expectStr(category(0), "zero")
test.expectStr(category(1), "one")
test.expectStr(category(2), "two")
test.expectStr(category(99), "many")
test.expectStr(category(-1), "many")

fn dir(s: string): i64 {
    match s {
        "up" { 1 }
        "down" { -1 }
        "left" { -100 }
        "right" { 100 }
        _ { 0 }
    }
}
test.expect(dir("up"), 1)
test.expect(dir("down"), -1)
test.expect(dir("left"), -100)
test.expect(dir("right"), 100)
test.expect(dir(""), 0)
test.expect(dir("UP"), 0)             // case-sensitive

// Match on bool (closed value space — both arms required, no wildcard).
fn bn(b: bool): i64 {
    match b {
        true { 1 }
        false { 0 }
    }
}
test.expect(bn(true), 1)
test.expect(bn(false), 0)
