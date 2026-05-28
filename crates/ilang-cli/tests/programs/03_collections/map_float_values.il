// A `Map<_, f32>` must store the float at its declared width — a bare
// literal narrows to f32 instead of staying f64 (whose low 32 bits
// read back as 0) — and `map.forEach` must hand float keys / values to
// the callback through a float-register ABI, not the integer path.
use std.test as test

let m = new Map<string, f32>()
m.set("x", 0.5)
m.set("y", 1.5)

// Stored / read at f32 width.
if let some(v) = m.get("x") {
  test.expectF64(v as f64, 0.5)
} else {
  test.fail("expected m[x]")
}

// forEach receives the real f32 values.
let seen: f32[] = []
m.forEach(fn(k: string, v: f32) { seen.push(v) })
test.expect(seen.length, 2)

// f64 values round-trip through forEach too.
let d = new Map<string, f64>()
d.set("a", 2.5)
let acc: f64[] = []
d.forEach(fn(k: string, v: f64) { acc.push(v) })
test.expect(acc.length, 1)
test.expectF64(acc[0], 2.5)

// .values() reflects the f32 width.
let vs: f32[] = m.values()
test.expect(vs.length, 2)
