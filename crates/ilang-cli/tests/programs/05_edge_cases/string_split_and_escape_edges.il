// String escape characters round-trip through `.length` (which
// counts code points), and split() handles consecutive separators
// + trailing separator the JS way (every position contributes a
// slot, including empty ones). `string_edge.il` covers empty input
// and multibyte; this is the splitter-specific corner.
use std.test as test
// Escape codepoints all count as single chars.
test.expect("\r\n\t\0".length, 4)
test.expect("\\".length, 1)
test.expect("\"".length, 1)

// Consecutive separators yield empty slots.
let p = "a,b,,c".split(",")
test.expect(p.length, 4)
test.expectStr(p[0], "a")
test.expectStr(p[1], "b")
test.expectStr(p[2], "")
test.expectStr(p[3], "c")

// Trailing separator yields an empty trailing slot.
let q = "a,b,".split(",")
test.expect(q.length, 3)
test.expectStr(q[2], "")

// Leading separator yields an empty leading slot.
let r = ",a,b".split(",")
test.expect(r.length, 3)
test.expectStr(r[0], "")
test.expectStr(r[1], "a")

// Only-separator input.
let s = ",,,".split(",")
test.expect(s.length, 4)        // 4 empty slots
test.expectStr(s[0], "")
test.expectStr(s[3], "")

// Multi-char separator.
let t = "axxbxxcxxd".split("xx")
test.expect(t.length, 4)
test.expectStr(t[2], "c")

// startsWith / endsWith with empty needle is always true (empty
// string is a prefix and suffix of everything).
test.expectBool("hello".startsWith(""), true)
test.expectBool("hello".endsWith(""), true)
test.expectBool("".startsWith(""), true)
test.expectBool("".endsWith(""), true)
test.expectBool("".endsWith("x"), false)

// charAt on empty / out-of-range is empty (slice-style clamp).
test.expectStr("".charAt(0), "")
test.expectStr("abc".charAt(100), "")
test.expectStr("abc".charAt(-1), "")
