// `replace` patterns where the replacement text contains characters
// from the search needle, where the needle appears repeatedly, and
// where the replacement length differs from the needle's. None of
// these should re-scan the just-substituted bytes (otherwise
// `"aaa".replace("a", "ab")` would diverge or hang).
use std.test

// Replacement contains the needle — scan must consume input only.
test.expectStr("aaa".replace("a", "ab"), "ababab")
test.expectStr("aaa".replace("a", "bb"), "bbbbbb")

// Repeated needle — every occurrence replaced.
test.expectStr("ababab".replace("ab", "x"), "xxx")
test.expectStr("hello hello hello".replace("hello", "hi"), "hi hi hi")

// Replacement shorter than needle.
test.expectStr("xxxxx".replace("xxx", "y"), "yxx")
test.expectStr("aaaaaa".replace("aa", ""), "")

// Replacement longer than needle.
test.expectStr("ab".replace("ab", "abcabc"), "abcabc")

// Needle at boundaries.
test.expectStr("axxbxxc".replace("xx", "_"), "a_b_c")
test.expectStr("xxhello".replace("xx", "_"), "_hello")
test.expectStr("helloxx".replace("xx", "_"), "hello_")

// Needle longer than haystack — no match, no change.
test.expectStr("hi".replace("hello", "x"), "hi")

// trim across various whitespace characters.
test.expectStr("  hello  ".trim(), "hello")
test.expectStr("\t\nhi\n\t".trim(), "hi")
test.expectStr("".trim(), "")
test.expectStr("   ".trim(), "")
test.expectStr("a b".trim(), "a b")          // internal whitespace kept
test.expectStr(" \ta\tb \n".trim(), "a\tb")  // leading/trailing only
