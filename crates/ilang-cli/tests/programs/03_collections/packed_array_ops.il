// Packed small-element arrays (i8/u8/i16/i32 store at stride 1/2/4)
// must give the same results as i64 cells across the search, mutate,
// and higher-order operations — the runtime reads and rebuilds them
// honouring the header stride rather than assuming 8-byte cells.
use std.test as test

// indexOf / includes / remove with i32 (stride 4).
let xs: i32[] = [10, 20, 30]
test.expect(xs.indexOf(20), 1)
test.expect(xs.indexOf(99), -1)
test.expectBool(xs.includes(30), true)
test.expectBool(xs.includes(99), false)

// Negative small-int needles must match (the needle is masked to the
// element width, not sign-extended past it).
let i8s: i8[] = [1, -2, 3]
test.expect(i8s.indexOf(-2), 1)
i8s.remove(-2)
test.expect(i8s.length, 2)
test.expect(i8s[1] as i64, 3)

// u8 round-trips a value above the signed-byte range.
let bytes: u8[] = [255, 0, 128]
test.expect(bytes.indexOf(128), 2)

// i16 negative.
let i16s: i16[] = [1000, -2000, 3000]
test.expect(i16s.indexOf(-2000), 1)

// Higher-order ops preserve the packed element width.
let mapped: i32[] = xs.map(fn(x: i32): i32 { x + 1 })
test.expect(mapped[0] as i64, 11)
test.expect(mapped[1] as i64, 21)
test.expect(mapped[2] as i64, 31)

let kept: i32[] = xs.filter(fn(x: i32): bool { x > 15 })
test.expect(kept.length, 2)
test.expect(kept[0] as i64, 20)

let sliced: i32[] = xs.slice(1, 3)
test.expect(sliced[0] as i64, 20)
test.expect(sliced[1] as i64, 30)

let rev: i32[] = xs.reverse()
test.expect(rev[0] as i64, 30)
test.expect(rev[2] as i64, 10)

let joined: i32[] = xs.concat([40, 50])
test.expect(joined.length, 5)
test.expect(joined[4] as i64, 50)

let unsorted: i32[] = [5, 1, 4, 2, 3]
let sorted: i32[] = unsorted.sort(fn(a: i32, b: i32): i64 { (a - b) as i64 })
test.expect(sorted[0] as i64, 1)
test.expect(sorted[4] as i64, 5)

let found: i32? = xs.find(fn(x: i32): bool { x == 20 })
if let some(v) = found {
  test.expect(v as i64, 20)
} else {
  test.fail("expected to find 20")
}
test.expect(xs.findIndex(fn(x: i32): bool { x == 30 }), 2)
test.expectBool(xs.every(fn(x: i32): bool { x > 5 }), true)
test.expectBool(xs.some(fn(x: i32): bool { x > 25 }), true)
