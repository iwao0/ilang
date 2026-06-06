// SIMD vector types — array-literal coercion against an annotated
// type slot. Per syntax.md §13b. First-class operations / lane
// access are not exposed yet; this fixture only exercises
// construction and the value flowing through function calls.
//
// `simd.i64x2` and SIMD-returning functions are documented but
// currently rejected by mir-codegen; they're left out here.
use std.test as test

fn take_f32x4(v: simd.f32x4): i64 { return 1 }
fn take_f32x2(v: simd.f32x2): i64 { return 2 }
fn take_f64x2(v: simd.f64x2): i64 { return 3 }
fn take_i32x4(v: simd.i32x4): i64 { return 4 }
fn take_i16x8(v: simd.i16x8): i64 { return 6 }
fn take_i8x16(v: simd.i8x16): i64 { return 7 }

let v4: simd.f32x4 = [1.0, 2.0, 3.0, 4.0]
test.expect(take_f32x4(v4), 1)

let v2: simd.f32x2 = [0.5, 1.5]
test.expect(take_f32x2(v2), 2)

let d2: simd.f64x2 = [1.0, 2.0]
test.expect(take_f64x2(d2), 3)

let i4: simd.i32x4 = [1, 2, 3, 4]
test.expect(take_i32x4(i4), 4)

let s8: simd.i16x8 = [1, 2, 3, 4, 5, 6, 7, 8]
test.expect(take_i16x8(s8), 6)

let b16: simd.i8x16 = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
test.expect(take_i8x16(b16), 7)

// Contiguous arrays of SIMD values.
let rows: simd.f32x2[] = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]
test.expect(rows.length, 3)
