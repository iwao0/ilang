// Same fits-in-target rule that just landed for `let` annotations
// applied to fn return tails: `fn f(): i8 { 200 }` is rejected
// (and was previously silently accepted, returning -56 from the
// JIT and a wrap-violating 200 from the interpreter).
//
// In-range literals at the tail still work, and non-literal
// narrowing (`fn f(x: i64): i8 { x }`) continues to truncate per
// ilang's runtime-typed convention.
use test

fn ok_tail(): i8 { 100 }
fn ok_neg_tail(): i8 { -100 }
fn ok_explicit_return(): i8 { return 100 }

test.expect(ok_tail() as i64, 100)
test.expect(ok_neg_tail() as i64, -100)
test.expect(ok_explicit_return() as i64, 100)

// Non-literal narrowing path keeps working.
fn narrow(x: i64): i8 { x }
test.expect(narrow(100) as i64, 100)
