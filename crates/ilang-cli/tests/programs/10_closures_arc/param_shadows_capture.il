// A parameter with the same name as an outer binding shadows it
// inside the closure body — capture vs. parameter resolution is
// decided at the closure's call site, not at construction. Per
// syntax.md §6 "Parameters with the same name as a capture shadow
// the capture."
use std.test as test

fn makeAdder(base: i64): fn(i64): i64 {
    // The inner closure parameter `base` shadows the outer
    // `makeAdder` parameter — the body reads the call-time `base`,
    // not the captured one.
    fn(base: i64): i64 { base + 1 }
}

let f = makeAdder(99)
test.expect(f(7), 8)
test.expect(f(0), 1)
test.expect(f(-5), -4)
// Different `makeAdder` calls produce indistinguishable closures
// because the captured outer `base` was never read.
let g = makeAdder(0)
test.expect(g(7), 8)
