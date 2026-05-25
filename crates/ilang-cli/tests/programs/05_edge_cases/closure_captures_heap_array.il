// Closures capture by value at creation time — but for heap-typed
// captures the value IS the handle, so a mutation through the outer
// binding is visible inside the closure (and vice-versa). The
// `closures.il` fixture only covers immutable primitive captures
// (`factor: i64`), so the array-capture path here is fresh.
use std.test as test
let xs: i64[] = [1, 2, 3]
let read_first = fn(): i64 { xs[0] }
let read_len = fn(): i64 { xs.length }

xs[0] = 999
test.expect(read_first(), 999)

xs.push(42)
test.expect(read_len(), 4)

// Closure pushes; outer sees it.
let push_to = fn(v: i64) { xs.push(v) }
push_to(7)
push_to(8)
test.expect(xs.length, 6)
test.expect(xs[5], 8)

// Two closures over the same array — order of effects preserved.
let a = fn(): i64 { xs.length }
let b = fn() { xs.push(100) }
test.expect(a(), 6)
b()
test.expect(a(), 7)
test.expect(xs[6], 100)
