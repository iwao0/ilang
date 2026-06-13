// A `pub enum E: T` value compares against a `T`-typed VARIABLE
// (`msg == WindowMessage.destroy`, `msg: u32`) via the enum-repr
// promotion. But the same comparison against an int LITERAL
// (`Msg.close == 18`) was rejected: the promotion only fired when the
// other side matched the repr exactly, and an int literal defaults to
// `i64`, so it never matched a `u32`-repr enum — "cannot apply binary op
// between Msg and i64".
//
// Fix: when one side is a repr enum and the other is an int literal that
// fits the repr, promote the enum to its repr AND adopt the literal, the
// same shape the variable case already used. Works in both directions and
// for every comparison operator.

// expect: repr enum literal compare ok

use std.test as test

enum Msg: u32 {
    quit = 16
    paint = 15
    close = 18
}

// enum vs int literal — both directions.
test.expectTrue(Msg.close == 18)
test.expectTrue(18 == Msg.close)
test.expectTrue(Msg.close != 17)
test.expectTrue(17 != Msg.quit)
test.expectTrue(Msg.quit < 17)
test.expectTrue(Msg.paint <= 15)
test.expectTrue(Msg.close > 16)
test.expectTrue(20 > Msg.close)

// still works against a repr-typed variable (unchanged path).
let m: u32 = 18
test.expectTrue(m == Msg.close)
test.expectTrue(Msg.close == m)

// the value really is the declared tag.
test.expect((Msg.close as u32) as i64, 18)

console.log("repr enum literal compare ok")
