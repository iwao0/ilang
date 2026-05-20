/// NSPoint / NSSize / NSRect / NSRange struct helpers + accessors.

use test
use foundation { makePoint, makeSize, makeRect, makeRange }

let p = makePoint(1.5, 2.5)
test.expectF64(p.x, 1.5)
test.expectF64(p.y, 2.5)

let s = makeSize(10.0, 20.0)
test.expectF64(s.width, 10.0)
test.expectF64(s.height, 20.0)

let r = makeRect(0.0, 0.0, 100.0, 50.0)
test.expectF64(r.origin.x, 0.0)
test.expectF64(r.origin.y, 0.0)
test.expectF64(r.size.width, 100.0)
test.expectF64(r.size.height, 50.0)

let rng = makeRange(5 as u64, 12 as u64)
test.expect(rng.location as i64, 5)
test.expect(rng.length as i64, 12)

0
