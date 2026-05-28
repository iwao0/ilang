// `console.log` and template interpolation read array elements at the
// header stride, so packed small-int arrays print their real values.
// Fixed-length `T[N]` arrays are header-less inline storage — the
// formatter uses the value pointer directly with the static length
// instead of reading a phantom dynamic-array header (which segfaulted).
//
// expect: [1, 2, 3]
// expect: [1, -2, 3, -4]
// expect: [255, 0, 128]
// expect: interp [10, 20, 30]
// expect: [1, 2, 3]
// expect: [1, -2, 3]
// expect: [(1, 10), (2, 20)]
// expect: [[1, -2], [-3, 4]]

let a: i32[] = [1, 2, 3]
console.log(a)

let b: i8[] = [1, -2, 3, -4]
console.log(b)

let c: u8[] = [255, 0, 128]
console.log(c)

let d: i32[] = [10, 20, 30]
console.log(`interp ${d}`)

// Fixed-length arrays.
let fi: i32[3] = [1, 2, 3]
console.log(fi)

let f8: i8[3] = [1, -2, 3]
console.log(f8)

let ft: (i32, i64)[2] = [(1, 10), (2, 20)]
console.log(ft)

// Nested packed arrays.
let n: i8[][] = [[1, -2], [-3, 4]]
console.log(n)
