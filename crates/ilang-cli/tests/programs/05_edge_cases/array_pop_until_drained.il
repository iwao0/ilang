// Repeated `pop()` past the end keeps returning none — never panics,
// never wraps the length below zero. `empty_container_ops.il` shows
// pop-once on empty; this fixture nails down the post-drain steady
// state.
use std.test as test
let xs: i64[] = [1, 2, 3]
test.expect(xs.pop().unwrap(), 3)
test.expect(xs.pop().unwrap(), 2)
test.expect(xs.pop().unwrap(), 1)
test.expect(xs.length, 0)

// Already empty — pop stays none, length stays 0, repeat.
test.expectBool(xs.pop().isNone, true)
test.expect(xs.length, 0)
test.expectBool(xs.pop().isNone, true)
test.expect(xs.length, 0)

// Push after drain re-grows correctly.
xs.push(99)
test.expect(xs.length, 1)
test.expect(xs[0], 99)
test.expect(xs.pop().unwrap(), 99)
test.expect(xs.length, 0)
