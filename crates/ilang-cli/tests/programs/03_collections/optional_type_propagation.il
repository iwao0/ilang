// An expected `Optional<T>` type flows into a `some(..)` so the wrapped
// value is built as T — narrowing a float literal to f32 and building a
// nested tuple with the declared widths — instead of defaulting to
// f64 / (f64, f64). A bare `none` adopts the expected `T?` so it
// unifies with sibling `some(..)` elements in an array literal.
use std.test as test

// `some(float literal)` against an f32 optional (previously failed to
// lower with "no coercion from f64? to f32?").
let a: f32? = some(1.5)
if let some(v) = a {
  test.expectF64(v as f64, 1.5)
} else {
  test.fail("expected some(1.5)")
}

// `some(tuple)` against an (f32, f64) optional: the f32 slot used to
// read back as 0.0 because the tuple was built as (f64, f64).
let b: (f32, f64)? = some((1.5, 2.5))
if let some(t) = b {
  let (x, y) = t
  test.expectF64(x as f64, 1.5)
  test.expectF64(y, 2.5)
} else {
  test.fail("expected some tuple")
}

// `[some(..), none]` against `T?[]` — works regardless of order.
let arr: f32?[] = [some(1.5), none, some(2.5)]
test.expect(arr.length, 3)
if let some(v0) = arr[0] {
  test.expectF64(v0 as f64, 1.5)
} else {
  test.fail("arr[0] should be some")
}
if let some(_v1) = arr[1] {
  test.fail("arr[1] should be none")
}

// Returning an optional from a function.
let r: i32? = mkOpt()
if let some(v) = r {
  test.expect(v as i64, 42)
} else {
  test.fail("expected some(42)")
}

fn mkOpt(): i32? { some(42) }
