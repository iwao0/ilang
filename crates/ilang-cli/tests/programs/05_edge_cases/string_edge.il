// Edge cases for the string built-ins: empty input, multibyte
// characters, out-of-range indices.
use std.test as test
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

// replace where the needle isn't present leaves the string unchanged.
test.expectStr("hello".replace("xyz", "_"), "hello")

// replace substitutes EVERY occurrence (Rust's str::replace), unlike
// JS's `String.replace(str, ...)` which only swaps the first.
test.expectStr("aXbXcX".replace("X", "-"), "a-b-c-")

// Indexing is by Unicode code point, not UTF-16 code unit: slicing
// around a non-BMP character (😀 is one code point, two UTF-16 units)
// never splits it into a lone surrogate the way JS's `slice` would.
test.expectStr("a😀b".slice(0, 2), "a😀")
test.expectStr("a😀b".slice(1, 2), "😀")
test.expectStr("a😀b".charAt(1), "😀")
// indexOf returns a code-point offset (2 here), not the UTF-16 unit 3.
test.expect("a😀b".indexOf("b"), 2)
test.expect("a😀b".indexOf("😀"), 1)

// split with no separator match returns the whole string in one slot.
let parts = "abc".split(",")
test.expect(parts.length, 1)
test.expectStr(parts[0], "abc")
