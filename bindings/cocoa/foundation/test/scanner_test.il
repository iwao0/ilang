// NSScanner — Optional-returning scan helpers.

use test
use foundation { NSScanner, nsstr }

let sc = NSScanner.scannerWithString(nsstr("42 -17 3.14"))
test.expectFalse(sc.isAtEnd)

// First int.
let n1 = sc.scanInteger()
test.expectTrue(n1.isSome)
test.expect(n1.unwrap(), 42)

// Second int (signed).
let n2 = sc.scanInteger()
test.expectTrue(n2.isSome)
test.expect(n2.unwrap(), -17)

// Third token is a double.
let d = sc.scanDouble()
test.expectTrue(d.isSome)
test.expectF64(d.unwrap(), 3.14)

test.expectTrue(sc.isAtEnd)

// scanInt on non-numeric input returns none.
let sc2 = NSScanner.scannerWithString(nsstr("hello"))
let bad = sc2.scanInt()
test.expectTrue(bad.isNone)

// scanFloat — narrower precision than double.
let sf = NSScanner.scannerWithString(nsstr("1.5"))
let f = sf.scanFloat()
test.expectTrue(f.isSome)

// scanLocation moves as the cursor advances.
let s3 = NSScanner.scannerWithString(nsstr("42abc"))
test.expect(s3.scanLocation as i64, 0)
let _ = s3.scanInteger()
test.expect(s3.scanLocation as i64, 2)  // past "42"

// scanString consumes a literal prefix.
let s4 = NSScanner.scannerWithString(nsstr("prefixrest"))
test.expectTrue(s4.scanString(nsstr("prefix")))
test.expect(s4.scanLocation as i64, 6)
test.expectFalse(s4.scanString(nsstr("zzz")))

// caseSensitive toggle survives a round-trip.
let s5 = NSScanner.scannerWithString(nsstr("X"))
test.expectFalse(s5.caseSensitive)  // default: false (case-insensitive)
s5.caseSensitive = true
test.expectTrue(s5.caseSensitive)

0
