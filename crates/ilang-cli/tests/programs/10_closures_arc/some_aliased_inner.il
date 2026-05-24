// `some(<aliased heap>)` retains the inner so the Optional owns
// its own +1 (3be5dcc). Without retain the JIT used the value
// after its source binding had released it.
use std.test

fn run(): i64[]? {
    let arr: i64[] = [10, 20, 30]
    some(arr)
}

for _ in 0..200 {
    let r = run().unwrap()
    test.expect(r[0], 10)
    test.expect(r[2], 30)
}
test.expectBool(true, true)
