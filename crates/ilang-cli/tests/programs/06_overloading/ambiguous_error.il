// Two overloads tying for the best score → typechecker rejects the
// call as ambiguous. (Cross-type widening cost is symmetric here.)
// expect-error: ambiguous
fn f(x: i64, y: i32): i64 { x + (y as i64) }
fn f(x: i32, y: i64): i64 { (x as i64) + y }
let n: i32 = 5
let _ = f(n, n)
