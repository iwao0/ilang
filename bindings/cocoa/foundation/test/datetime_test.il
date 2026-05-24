/// NSDate — construct from epoch, arithmetic, comparison.

use std.test
use foundation { NSDate }

// Build a date from a fixed Unix timestamp.
let d1 = NSDate.dateWithTimeIntervalSince1970(1_700_000_000.0)
test.expectF64(d1.timeIntervalSince1970, 1_700_000_000.0)

// Add 60 seconds — round-trip the offset.
let d2 = d1.dateByAddingTimeInterval(60.0)
test.expectF64(d2.timeIntervalSince1970, 1_700_000_060.0)

// timeIntervalSinceDate.
test.expectF64(d2.timeIntervalSinceDate(d1), 60.0)

// isEqualToDate / pointer-different but same instant.
let d1b = NSDate.dateWithTimeIntervalSince1970(1_700_000_000.0)
test.expectTrue(d1.isEqualToDate(d1b))
test.expectFalse(d1.isEqualToDate(d2))

// earlier / later helpers pick the right one.
test.expectTrue(d1.earlierDate(d2).isEqualToDate(d1))
test.expectTrue(d1.laterDate(d2).isEqualToDate(d2))

0
