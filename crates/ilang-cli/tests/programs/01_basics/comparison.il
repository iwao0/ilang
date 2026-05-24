// All comparison operators, on ints / floats / strings / bools.
use std.test

// Integer comparisons.
test.expectBool(1 < 2, true)
test.expectBool(2 < 1, false)
test.expectBool(1 <= 1, true)
test.expectBool(1 < 1, false)
test.expectBool(2 > 1, true)
test.expectBool(1 >= 1, true)
test.expectBool(1 == 1, true)
test.expectBool(1 != 2, true)

// Float comparisons.
test.expectBool(1.5 < 2.5, true)
test.expectBool(1.0 == 1.0, true)
test.expectBool(0.1 + 0.2 != 0.3, true)   // float precision quirk

// String content equality.
test.expectBool("abc" == "abc", true)
test.expectBool("abc" == "abd", false)
test.expectBool("a" + "b" == "ab", true)

// Bool equality.
test.expectBool(true == true, true)
test.expectBool(true != false, true)

// Mixed-width int comparison via implicit widening.
let n: i32 = 5
test.expectBool(n == 5, true)
test.expectBool(n < 10, true)
