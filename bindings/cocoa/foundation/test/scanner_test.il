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

0
