// Higher-order array ops on f32[] / f64[] call the closure through an
// ABI matching its float parameter / return type. The integer-only
// call path used to feed the closure garbage (e.g. `map(x => x + 0.5)`
// returned the inputs unchanged) because floats travel in float
// registers. Mixed int/float arg-and-return maps are covered too.
use std.test as test

let fs: f32[] = [3.5, 1.5, 2.5]

let m: f32[] = fs.map(fn(x: f32): f32 { x + 0.5 })
test.expectF64(m[0] as f64, 4.0)
test.expectF64(m[1] as f64, 2.0)
test.expectF64(m[2] as f64, 3.0)

let srt: f32[] = fs.sort(fn(a: f32, b: f32): i64 { if a < b { -1 } else { 1 } })
test.expectF64(srt[0] as f64, 1.5)
test.expectF64(srt[2] as f64, 3.5)

let kept: f32[] = fs.filter(fn(x: f32): bool { x > 2.0 })
test.expect(kept.length, 2)

let found: f32? = fs.find(fn(x: f32): bool { x > 3.0 })
if let some(v) = found {
  test.expectF64(v as f64, 3.5)
} else {
  test.fail("expected to find 3.5")
}
test.expectBool(fs.every(fn(x: f32): bool { x > 0.0 }), true)
test.expectBool(fs.some(fn(x: f32): bool { x > 3.0 }), true)
test.expect(fs.findIndex(fn(x: f32): bool { x == 2.5 }), 2)

// f64 receivers.
let ds: f64[] = [3.5, 1.5, 2.5]
let dm: f64[] = ds.map(fn(x: f64): f64 { x * 2.0 })
test.expectF64(dm[0], 7.0)
test.expectF64(dm[1], 3.0)

// Mixed-type maps: float arg → int return, and int arg → float return.
let asInts: i32[] = fs.map(fn(x: f32): i32 { x as i32 })
test.expect(asInts[0] as i64, 3)
test.expect(asInts[1] as i64, 1)

let asFloats: f64[] = [1, 2, 3].map(fn(x: i32): f64 { (x as f64) + 0.5 })
test.expectF64(asFloats[0], 1.5)
test.expectF64(asFloats[2], 3.5)
