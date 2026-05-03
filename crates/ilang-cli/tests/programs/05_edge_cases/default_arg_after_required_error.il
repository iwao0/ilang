// Once a parameter has a default, every later parameter must too.
// expect-error: '=' (parameter without default cannot follow one with a default)
fn bad(a: i64 = 0, b: i64): i64 { a + b }
let _ = bad(1, 2)
