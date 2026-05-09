// Strings carry a leading `i64 length` header so embedded NUL
// bytes don't truncate `length`, comparisons, or concatenation.
// The trailing `\0` is for cstr-compat at the FFI boundary only —
// every other operation reads the explicit length.
//
// Pre-fix the JIT computed `length` via `strlen`, so a string
// containing `\0` would report a smaller length than the
// declared content and `+` would lose the post-NUL bytes.
use test

// Literal with embedded NUL — length counts every char.
let s = "hello\0world"
test.expect(s.length, 11)
test.expectStr(s.charAt(0), "h")
test.expectStr(s.charAt(5), "\0")
test.expectStr(s.charAt(6), "w")
test.expectStr(s.charAt(10), "d")

// Concat preserves NUL on both sides.
let a = "ab\0cd"
let b = "ef\0gh"
let c = a + b
test.expect(c.length, 10)
test.expectStr(c.charAt(2), "\0")
test.expectStr(c.charAt(7), "\0")

// Equality compares the full content, not just up to NUL.
let s1 = "x\0y"
let s2 = "x\0z"
test.expectBool(s1 == s2, false)

let s3 = "x\0y"
test.expectBool(s1 == s3, true)
