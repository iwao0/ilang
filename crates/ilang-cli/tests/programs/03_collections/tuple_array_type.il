// Postfix type modifiers apply after a tuple / grouped type, so
// `(i32, f32)[]` (an array of tuples) parses. The tuple elements are
// built with the annotated widths (f32, not the default f64), and the
// array supports the usual operations.
use std.test as test

let a: (i32, f32)[] = [(1, 1.5), (2, 2.5)]
test.expect(a.length, 2)
let (x0, y0) = a[0]
test.expect(x0 as i64, 1)
test.expectF64(y0 as f64, 1.5)
let (x1, y1) = a[1]
test.expect(x1 as i64, 2)
test.expectF64(y1 as f64, 2.5)

// push a fresh tuple.
a.push((3, 3.5))
test.expect(a.length, 3)
let (x2, y2) = a[2]
test.expect(x2 as i64, 3)
test.expectF64(y2 as f64, 3.5)

// Array nested inside a tuple, and a tuple nested inside an array.
let b: (i32[], f64) = ([10, 20], 3.5)
let (arr, f) = b
test.expect(arr[1] as i64, 20)
test.expectF64(f, 3.5)

// map over a tuple array.
let firsts: i32[] = a.map(fn(t: (i32, f32)): i32 {
  let (xx, _yy) = t
  xx
})
test.expect(firsts[0] as i64, 1)
test.expect(firsts[2] as i64, 3)
