// Front-of-array operations — `shift()` (remove & return the
// first element) and `unshift(v)` (prepend). Per syntax.md §8.
// `array_basics.il` covers push/pop/remove/removeAt; this fills
// the other end of the array.
use std.test as test

// shift returns `T?` so the caller can tell empty from value.
let a: i64[] = [1, 2, 3]
test.expect(a.shift().unwrap(), 1)
test.expect(a.length, 2)
test.expect(a[0], 2)
test.expect(a[1], 3)

// unshift puts a value back at the front and shifts the rest right.
a.unshift(99)
test.expect(a.length, 3)
test.expect(a[0], 99)
test.expect(a[1], 2)
test.expect(a[2], 3)

// shift drains down to empty; another shift on empty is `none`.
test.expect(a.shift().unwrap(), 99)
test.expect(a.shift().unwrap(), 2)
test.expect(a.shift().unwrap(), 3)
test.expect(a.length, 0)
test.expectBool(a.shift().isNone, true)

// unshift onto an empty array.
let empty: string[] = []
empty.unshift("hi")
test.expect(empty.length, 1)
test.expectStr(empty[0], "hi")
empty.unshift("hey")
test.expectStr(empty[0], "hey")
test.expectStr(empty[1], "hi")
