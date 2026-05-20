/// NSLocale — construct by identifier and read back the parts.

use test
use foundation { NSLocale, nsstr, ilstr }

let l = NSLocale.localeWithIdentifier(nsstr("en_US"))
test.expectStr(ilstr(l.localeIdentifier), "en_US")
test.expectStr(ilstr(l.languageCode), "en")
test.expectStr(ilstr(l.countryCode), "US")
test.expectStr(ilstr(l.currencyCode), "USD")
test.expectTrue(l.usesMetricSystem == false)

let j = NSLocale.localeWithIdentifier(nsstr("ja_JP"))
test.expectStr(ilstr(j.languageCode), "ja")
test.expectStr(ilstr(j.countryCode), "JP")
test.expectStr(ilstr(j.currencyCode), "JPY")
test.expectTrue(j.usesMetricSystem)

// ISO arrays return non-empty.
test.expectTrue(NSLocale.isoCountryCodes().count > 100)
test.expectTrue(NSLocale.isoLanguageCodes().count > 100)

0
