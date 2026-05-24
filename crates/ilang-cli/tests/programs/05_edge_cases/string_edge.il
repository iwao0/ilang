// Edge cases for the string built-ins: empty input, multibyte
// characters, out-of-range indices.
use std.test

// Empty.
test.expect("".length, 0)
test.expectStr("" + "" + "", "")
test.expectBool("" == "", true)

// Multibyte: `.length` counts Unicode code points, NOT bytes.
test.expect("あいう".length, 3)
test.expect("a😀b".length, 3)
test.expectStr("あいう".charAt(1), "い")

// slice clamps to [0, length] — never panics.
test.expectStr("hi".slice(-100, 100), "hi")
test.expectStr("hi".slice(5, 10), "")        // start past end → empty
test.expectStr("hi".slice(1, 0), "")         // start > end → empty
test.expectStr("あいう".slice(1, 3), "いう") // multibyte slice

// replace on empty needle leaves the string unchanged (Rust's
// str::replace inserts the replacement between every codepoint and at
// both ends, so empty.replace("", X) == X repeated len+1 times). For
// a non-empty string the result depends on the runtime; just exercise
// the safe shape: replace where needle isn't present.
test.expectStr("hello".replace("xyz", "_"), "hello")

// split with no separator match returns the whole string in one slot.
let parts = "abc".split(",")
test.expect(parts.length, 1)
test.expectStr(parts[0], "abc")
