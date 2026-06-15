// Assigning an f64-typed value (a bare float literal, or an f64
// expression) directly to an `f32` field must demote to f32 before the
// store. Object fields live in 8-byte slots; the store wrote the whole
// f64 bit pattern while the load read back the low 32 bits — so `1.5`
// (f64 `0x3FF8000000000000`) read back as `0.0` (its low word). Existing
// f32-field tests all used explicit `1.0f32` literals (already f32) and
// missed it.
//
// expect: ok
use std.test as test

class Vec2 {
    x: f32
    y: f32
    init() {
        this.x = 1.5        // f64 literal -> f32 field
        this.y = 2.5
    }
    len2(): f32 { this.x * this.x + this.y * this.y }
}
let v = new Vec2()
test.expectF64(v.x as f64, 1.5)
test.expectF64(v.y as f64, 2.5)
test.expectF64(v.len2() as f64, 1.5 * 1.5 + 2.5 * 2.5)

// post-construction assignment from an f64 literal and an f64 expression
v.x = 4.5
test.expectF64(v.x as f64, 4.5)
let d: f64 = 7.25
v.y = d as f32              // explicit demote still fine
test.expectF64(v.y as f64, 7.25)

// f32 field default-then-set
class P {
    p: f32
    init() { this.p = 0.0 }
}
let pp = new P()
pp.p = 3.25
test.expectF64(pp.p as f64, 3.25)

// f32 inside an array of objects
let arr: P[] = [new P(), new P()]
arr[0].p = 9.5
test.expectF64(arr[0].p as f64, 9.5)
test.expectF64(arr[1].p as f64, 0.0)

// f64 field is unaffected (already 8-byte), int field regression
class Mixed {
    f: f64
    g: i32
    h: f32
    init() {
        this.f = 3.14
        this.g = 2000000000
        this.h = 6.5
    }
}
let m = new Mixed()
test.expectF64(m.f, 3.14)
test.expect(m.g as i64, 2000000000)
test.expectF64(m.h as f64, 6.5)

console.log("ok")
