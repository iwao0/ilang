// Module-qualified const reference: `lib_const.PI` is resolved at
// compile time and substituted with the literal value, so no runtime
// lookup happens.
use std.test as test
use lib_const

test.expectF64(lib_const.PI, 3.141592653589793)
test.expectF64(lib_const.TAU, 6.283185307179586)
test.expectStr(lib_const.NAME, "ilang")

// Use inside an expression — proves it really is just a literal.
test.expectF64(lib_const.PI * 2.0, lib_const.TAU)
