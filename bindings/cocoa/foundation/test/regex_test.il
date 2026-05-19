// NSRegularExpression — compile + match count + replace.

use test
use cocoa { regex, RegexOptions, MatchOptions, fullRange,
            nsstr, ilstr }

let re = regex("[0-9]+", RegexOptions.none)
let s = nsstr("a12 b3 cc 4")

// numberOfMatches: 3 digit runs.
let n = re.numberOfMatchesInString(s, MatchOptions.none, fullRange(s))
test.expect(n, 3)

// Replace digits with "#".
let replaced = re.stringByReplacingMatchesInString(
    s, MatchOptions.none, fullRange(s), nsstr("#")
)
test.expectStr(ilstr(replaced), "a# b# cc #")

// caseInsensitive flag.
let reci = regex("[A-Z]", RegexOptions.caseInsensitive)
test.expect(reci.numberOfMatchesInString(nsstr("abc"),
    MatchOptions.none, fullRange(nsstr("abc"))), 3)

0
