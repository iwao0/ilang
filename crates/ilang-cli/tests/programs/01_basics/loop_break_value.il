// `loop` evaluates to a `break v` value.
use std.test as test
let n = loop {
    break 42
}
test.expect(n, 42)

// Multiple break sites — types must agree.
let i = 0
let first_even = loop {
    if i % 2 == 0 && i > 0 {
        break i
    }
    i = i + 1
}
test.expect(first_even, 2)

// Bare `break` in a loop with no `break v` keeps the loop's type Unit.
loop {
    break
}

// Strings work too.
let s = loop { break "hello" }
test.expectStr(s, "hello")
