// Ordering (`<` `<=` `>` `>=`) between two values of the SAME repr
// enum must compare the discriminant tags, NOT the boxed pointers.
// The checker permits the comparison (a repr enum's tag carries a
// natural order, and `enum vs int-literal` already worked for every
// operator), but the MIR lowering fell through to `cmp_op` over the
// raw operands — i.e. it compared the two heap-box ADDRESSES. The
// answer was whatever the allocator / intern order produced:
//
//     Msg.paint < Msg.quit   // 15 < 16, but read FALSE
//     Msg.quit  < Msg.paint  // 16 < 15, but read TRUE
//
// while `Msg.close > Msg.quit` happened to land right and `Msg.quit
// < 18` (enum vs int literal, a different promotion path) was always
// fine — which is what made the bug easy to miss. This is the same
// family as the `==` pointer-compare fixed for combined `@flags`
// values (06_enums/flags_enum_combined_identity.il); interning
// canonicalises pointers so `==` reads right, but pointer ORDER is
// meaningless no matter how canonical, so ordering needed the tag.
//
// Fix: lower every same-enum comparison (`==` `!=` `<` `<=` `>` `>=`)
// by extracting both tags and comparing as the repr integer — its
// signedness picks the signed vs unsigned `cmp_op`, so a negative
// `i32`-repr discriminant orders correctly too.

// expect: repr enum comparison ok

use std.test as test

// ---- unsigned repr (u32) ----
enum Msg: u32 {
    paint = 15
    quit  = 16
    close = 18
}

test.expectTrue(Msg.paint < Msg.quit)
test.expectTrue(Msg.quit < Msg.close)
test.expectTrue(Msg.close > Msg.quit)
test.expectTrue(Msg.quit > Msg.paint)
test.expectTrue(!(Msg.quit < Msg.paint))
test.expectTrue(!(Msg.paint > Msg.close))
test.expectTrue(Msg.paint <= Msg.paint)
test.expectTrue(Msg.paint <= Msg.quit)
test.expectTrue(Msg.close >= Msg.close)
test.expectTrue(Msg.close >= Msg.quit)

// equality (same family — must agree with ordering)
test.expectTrue(Msg.quit == Msg.quit)
test.expectTrue(Msg.quit != Msg.paint)

// enum vs int literal still works in both directions (unchanged path)
test.expectTrue(Msg.quit < 18)
test.expectTrue(18 > Msg.quit)
test.expectTrue(Msg.quit == 16)

// ---- signed repr (i32) with a negative discriminant ----
// Comparing as unsigned would turn -10 into a huge value and invert
// every order; the repr's signedness must drive the compare.
enum Temp: i32 {
    cold = -10
    mild = 0
    hot  = 30
}

test.expectTrue(Temp.cold < Temp.mild)
test.expectTrue(Temp.cold < Temp.hot)
test.expectTrue(Temp.mild < Temp.hot)
test.expectTrue(Temp.hot > Temp.cold)
test.expectTrue(!(Temp.mild < Temp.cold))
test.expectTrue(Temp.cold <= Temp.cold)
test.expectTrue(Temp.cold == Temp.cold)
test.expectTrue(Temp.cold != Temp.hot)

console.log("repr enum comparison ok")
