// NSString — basic UTF-8 round-trip + length / case / search.

use test
use cocoa { NSString, nsstr, ilstr }

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

0
