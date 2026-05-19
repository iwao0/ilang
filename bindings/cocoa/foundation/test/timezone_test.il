// NSTimeZone — construct + read offset / name / abbreviation.

use test
use foundation { NSTimeZone, NSDate, nsstr, ilstr }

let utc = NSTimeZone.withName(nsstr("UTC"))
// NSTimeZone normalises "UTC" → "GMT" in `.name`.
test.expectStr(ilstr(utc.name), "GMT")
test.expect(utc.secondsFromGMT, 0)

let jst = NSTimeZone.withName(nsstr("Asia/Tokyo"))
test.expectStr(ilstr(jst.name), "Asia/Tokyo")
test.expect(jst.secondsFromGMT, 9 * 3600)
// JST doesn't observe DST.
test.expectFalse(jst.isDaylightSavingTime)

// Fixed-offset zone.
let plus5 = NSTimeZone.forSecondsFromGMT(5 * 3600)
test.expect(plus5.secondsFromGMT, 18000)
let minus3 = NSTimeZone.forSecondsFromGMT(-3 * 3600)
test.expect(minus3.secondsFromGMT, -10800)

// secondsFromGMTForDate — pick a clearly non-DST UTC date.
let d = NSDate.dateWithTimeIntervalSince1970(1_700_000_000.0)
test.expect(utc.secondsFromGMTForDate(d), 0)
test.expect(jst.secondsFromGMTForDate(d), 9 * 3600)

0
