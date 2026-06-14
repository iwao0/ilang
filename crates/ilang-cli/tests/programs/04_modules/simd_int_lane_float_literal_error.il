// A SIMD lane element must fit its lane scalar type — the same rule as
// scalar assignment, where `let x: i32 = 1.0` is rejected (float -> int
// needs an explicit `as`). The SIMD construction check passed the LANE
// type as each element's own type, so every element trivially "fit" and
// a float literal landed in an integer lane: `simd.i32x4 = [1.0, ...]`
// compiled. It now checks each element's real inferred type against the
// lane, matching the array-literal path (and rejecting over-wide integer
// literals like `300` into an `i8` lane too).

// expect-error: expected simd.i32x4, got f64[]

let v: simd.i32x4 = [1.0, 2.0, 3.0, 4.0]
console.log("unreachable")
