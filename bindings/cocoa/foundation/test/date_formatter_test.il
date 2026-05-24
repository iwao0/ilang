/// NSDateFormatter — fixed-locale / fixed-timezone format + parse
/// round-trip. Pins en_US_POSIX + UTC so the test isn't affected
/// by the host's region settings.

use std.test
use foundation { NSDate, NSDateFormatter, NSTimeZone, NSLocale, nsstr, ilstr }

let df = NSDateFormatter.alloc().init()
df.dateFormat = nsstr("yyyy-MM-dd HH:mm:ss")
df.locale = NSLocale.localeWithIdentifier(nsstr("en_US_POSIX"))
df.timeZone = NSTimeZone.withName(nsstr("UTC"))

// 2024-01-15 12:34:56 UTC.
let d = NSDate.dateWithTimeIntervalSince1970(1_705_322_096.0)
let s = df.stringFromDate(d)
test.expectStr(ilstr(s), "2024-01-15 12:34:56")

// Round-trip back to NSDate.
let d2 = df.dateFromString(s)
test.expectF64(d2.timeIntervalSince1970, 1_705_322_096.0)

0
