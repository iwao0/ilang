/// NSRegularExpression — compile + match count + replace.

use std.test as test
use foundation { NSDataDetector, regex, NSRegularExpressionOptions, NSMatchingOptions,
                 fullRange, nsstr, ilstr }

let re = regex("[0-9]+", NSRegularExpressionOptions.none)
let s = nsstr("a12 b3 cc 4")

// numberOfMatches: 3 digit runs.
let n = re.numberOfMatchesInString(s, NSMatchingOptions.none, fullRange(s))
test.expect(n, 3)

// Replace digits with "#".
let replaced = re.stringByReplacingMatchesInString(
    s, NSMatchingOptions.none, fullRange(s), nsstr("#")
)
test.expectStr(ilstr(replaced), "a# b# cc #")

// caseInsensitive flag.
let reci = regex("[A-Z]", NSRegularExpressionOptions.caseInsensitive)
test.expect(reci.numberOfMatchesInString(nsstr("abc"),
    NSMatchingOptions.none, fullRange(nsstr("abc"))), 3)

// regex properties (pattern, options).
let re2 = regex("\\d+", NSRegularExpressionOptions.none)
test.expectStr(ilstr(re2.pattern), "\\d+")
test.expect(re2.options as i64, NSRegularExpressionOptions.none as i64)

// NSDataDetector — pre-loaded detector for URLs / phone numbers
// etc. We only need to confirm construction + the inherited
// numberOfMatchesInString fires.
let dd = NSDataDetector.dataDetectorWithTypes(
    32 as u64,  // NSTextCheckingTypeLink
    0
)
test.expectFalse(dd.handle == 0)
let s2 = nsstr("see https://example.com and http://foo.test/")
let hits = dd.numberOfMatchesInString(s2, NSMatchingOptions.none, fullRange(s2))
test.expect(hits, 2)
test.expect(dd.checkingTypes as i64, 32)

0
