/// NSNumberFormatter — style + locale-pinned format / parse.

use std.test
use foundation { NSNumber, NSNumberFormatter, NSNumberFormatterStyle,
                 NSLocale, nsstr, ilstr }

let nf = NSNumberFormatter.alloc().init()
nf.locale = NSLocale.localeWithIdentifier(nsstr("en_US"))
nf.numberStyle = NSNumberFormatterStyle.decimal

let n = NSNumber.fromI64(1234567)
let s = nf.stringFromNumber(n)
test.expectStr(ilstr(s), "1,234,567")

// Parse back.
let parsed = nf.numberFromString(nsstr("9999"))
test.expectFalse(parsed.handle == 0)

// Toggle grouping separator off.
nf.usesGroupingSeparator = false
test.expectStr(ilstr(nf.stringFromNumber(NSNumber.fromI64(1000))), "1000")

// Min / max fraction digits on a decimal style.
nf.usesGroupingSeparator = true
nf.minimumFractionDigits = 2 as u64
nf.maximumFractionDigits = 2 as u64
test.expectStr(ilstr(nf.stringFromNumber(NSNumber.fromF64(3.1))), "3.10")
test.expectStr(ilstr(nf.stringFromNumber(NSNumber.fromF64(3.14159))), "3.14")

0
