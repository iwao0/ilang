// String concat, equality, and every built-in method documented in §4.
use test

// + concatenation, ==/!= content equality.
test.expectStr("ab" + "c", "abc")
test.expectStr("hello, " + "world", "hello, world")
test.expectBool("abc" == "abc", true)
test.expectBool("abc" != "abd", true)

// Built-in property + methods.
test.expect("hello".length, 5)
test.expect("あいう".length, 3)            // Unicode code point count
test.expectStr("hello".charAt(1), "e")
test.expectStr("hello".charAt(99), "")    // out of range → ""
test.expectBool("hello".includes("ell"), true)
test.expectBool("hello".includes("zzz"), false)
test.expectBool("hello".startsWith("he"), true)
test.expectBool("hello".endsWith("lo"), true)
test.expectStr("Hi".toUpperCase(), "HI")
test.expectStr("Hi".toLowerCase(), "hi")
test.expectStr("  hi  ".trim(), "hi")
test.expectStr("abca".replace("a", "_"), "_bc_")
test.expectStr("hello world".slice(6, 11), "world")
test.expectStr("hi".slice(-5, 100), "hi")  // out-of-range clamps

// Split — returns string[]. Index into the result.
let parts = "a,b,c,d".split(",")
test.expect(parts.length, 4)
test.expectStr(parts[0], "a")
test.expectStr(parts[3], "d")

// Empty separator → per-character.
let chars = "abc".split("")
test.expect(chars.length, 3)
test.expectStr(chars[1], "b")
