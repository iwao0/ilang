/// NSString — broad surface coverage.

use std.test as test
use foundation { NSString, NSCharacterSet, NSComparisonResult, NSStringEncoding,
                 nsstr, ilstr, makeRange }

// Round-trip: ilang string → NSString → ilang string.
let s = nsstr("hello")
test.expectStr(ilstr(s), "hello")

// length in UTF-16 code units (NSString's `length`).
test.expect(s.length as i64, 5)

// Case conversion (properties / getters).
test.expectStr(ilstr(s.uppercaseString), "HELLO")
test.expectStr(ilstr(nsstr("WORLD").lowercaseString), "world")

// hasPrefix / hasSuffix.
test.expectTrue(nsstr("foobar").hasPrefix(nsstr("foo")))
test.expectFalse(nsstr("foobar").hasPrefix(nsstr("bar")))
test.expectTrue(nsstr("foobar").hasSuffix(nsstr("bar")))

// isEqualToString.
test.expectTrue(nsstr("a").isEqualToString(nsstr("a")))
test.expectFalse(nsstr("a").isEqualToString(nsstr("b")))

// stringByAppendingString.
test.expectStr(
    ilstr(nsstr("foo").stringByAppendingString(nsstr("bar"))),
    "foobar"
)

// substring methods.
test.expectStr(ilstr(nsstr("abcdef").substringFromIndex(2 as u64)), "cdef")
test.expectStr(ilstr(nsstr("abcdef").substringToIndex(3 as u64)), "abc")
test.expectStr(
    ilstr(nsstr("abcdef").substringWithRange(makeRange(1 as u64, 3 as u64))),
    "bcd"
)

// containsString.
test.expectTrue(nsstr("hello world").containsString(nsstr("o w")))
test.expectFalse(nsstr("hello world").containsString(nsstr("xyz")))

// Find/replace.
test.expectStr(
    ilstr(nsstr("aXbXc").stringByReplacing(nsstr("X"), nsstr("-"))),
    "a-b-c"
)

// componentsSeparatedByString.
let parts = nsstr("a,b,c,d").componentsSeparatedByString(nsstr(","))
test.expect(parts.count, 4)

// componentsSeparatedByCharactersInSet (whitespace).
let words = nsstr("foo bar\tbaz\nqux")
    .componentsSeparatedByCharactersInSet(NSCharacterSet.whitespaceAndNewline())
test.expect(words.count, 4)

// trim whitespace.
test.expectStr(
    ilstr(nsstr("  hello  ").stringByTrimmingCharactersInSet(
        NSCharacterSet.whitespace()
    )),
    "hello"
)

// Parsing.
test.expect(nsstr("123").integerValue, 123)
test.expect(nsstr("-7").integerValue, -7)
test.expectF64(nsstr("2.5").doubleValue, 2.5)
test.expectBool(nsstr("YES").boolValue, true)
test.expectBool(nsstr("0").boolValue, false)

// Path-component helpers.
test.expectStr(ilstr(nsstr("/a/b/c.txt").lastPathComponent), "c.txt")
test.expectStr(ilstr(nsstr("/a/b/c.txt").pathExtension), "txt")
test.expectStr(
    ilstr(nsstr("/a/b/c.txt").stringByDeletingLastPathComponent),
    "/a/b"
)
test.expectStr(
    ilstr(nsstr("/a/b/c.txt").stringByDeletingPathExtension),
    "/a/b/c"
)
test.expectStr(
    ilstr(nsstr("/a/b").stringByAppendingPathComponent(nsstr("d.md"))),
    "/a/b/d.md"
)
let comps = nsstr("/a/b/c").pathComponents
test.expect(comps.count, 4)  // "/", "a", "b", "c"
test.expectTrue(nsstr("/x").isAbsolutePath)
test.expectFalse(nsstr("x").isAbsolutePath)

// Case-insensitive compare.
test.expect(nsstr("ABC").caseInsensitiveCompare(nsstr("abc")) as i64,
    NSComparisonResult.orderedSame as i64)
test.expect(nsstr("abc").compareTo(nsstr("abd")) as i64,
    NSComparisonResult.orderedAscending as i64)

// characterAtIndex.
test.expect(nsstr("ABC").characterAtIndex(1 as u64) as i64, 66)  // 'B'

0
