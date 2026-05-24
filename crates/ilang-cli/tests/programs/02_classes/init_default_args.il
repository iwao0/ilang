// `init` and methods accept default-arg parameters too.
use std.test

class Box {
    value: i64
    label: string

    init(value: i64, label: string = "anon") {
        this.value = value
        this.label = label
    }

    bump(by: i64 = 1): i64 {
        this.value + by
    }
}

let a = new Box(42)
test.expect(a.value, 42)
test.expectStr(a.label, "anon")
test.expect(a.bump(), 43)
test.expect(a.bump(10), 52)

let b = new Box(7, "b")
test.expectStr(b.label, "b")
test.expect(b.bump(), 8)
