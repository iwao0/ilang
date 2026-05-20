/// NSCalendar / NSDateComponents — build a date, read fields back.

use test
use foundation { NSCalendar, NSDateComponents, NSTimeZone, NSDate,
                 NSCalendarUnit, NSCalendarOptions, nsstr }

let cal = NSCalendar.calendarWithIdentifier(nsstr("gregorian"))
let utc = NSTimeZone.withName(nsstr("UTC"))
cal.timeZone = utc

// Build 2024-03-14 09:26:53 UTC.
let comps = NSDateComponents.alloc().init()
comps.year = 2024
comps.month = 3
comps.day = 14
comps.hour = 9
comps.minute = 26
comps.second = 53
comps.timeZone = utc
comps.calendar = cal

let d = comps.date
// 2024-03-14 09:26:53 UTC = 1710408413 epoch seconds.
test.expectF64(d.timeIntervalSince1970, 1_710_408_413.0)

// Round-trip: extract year/month/day/hour/minute/second.
let units = NSCalendarUnit.year | NSCalendarUnit.month | NSCalendarUnit.day
    | NSCalendarUnit.hour | NSCalendarUnit.minute | NSCalendarUnit.second
let back = cal.componentsFromDate(units, d)
test.expect(back.year, 2024)
test.expect(back.month, 3)
test.expect(back.day, 14)
test.expect(back.hour, 9)
test.expect(back.minute, 26)
test.expect(back.second, 53)

// startOfDayForDate truncates to midnight UTC for our date.
// 2024-03-14 00:00:00 UTC = 1710374400.
let sod = cal.startOfDayForDate(d)
test.expectF64(sod.timeIntervalSince1970, 1_710_374_400.0)

// dateBySettingHour: replace just the hour, keep rest.
let setHour = cal.dateBySettingHour(
    12, 0, 0, d, NSCalendarOptions.none
)
test.expectF64(setHour.timeIntervalSince1970, 1_710_417_600.0)
// 2024-03-14 12:00:00 UTC = 1710417600.

// dateByAddingComponents: add 1 day.
let plusDay = NSDateComponents.alloc().init()
plusDay.day = 1
let nextDay = cal.dateByAddingComponents(plusDay, d, 0 as u64)
test.expectF64(nextDay.timeIntervalSince1970, d.timeIntervalSince1970 + 86400.0)

// isDate:equalToDate:toUnitGranularity — same day.
let sameDayDifferentSecond = NSDateComponents.alloc().init()
sameDayDifferentSecond.year = 2024
sameDayDifferentSecond.month = 3
sameDayDifferentSecond.day = 14
sameDayDifferentSecond.hour = 23
sameDayDifferentSecond.minute = 59
sameDayDifferentSecond.second = 59
sameDayDifferentSecond.timeZone = utc
sameDayDifferentSecond.calendar = cal
let dEnd = sameDayDifferentSecond.date
test.expectTrue(cal.isDate(d, dEnd, NSCalendarUnit.day))
test.expectFalse(cal.isDate(d, dEnd, NSCalendarUnit.hour))

// rangeOfUnit: number-of-days in the enclosing month.
// March has 31 days.
let monthRange = cal.rangeOfUnit(
    NSCalendarUnit.day, NSCalendarUnit.month, d
)
test.expect(monthRange.length as i64, 31)

// ordinalityOfUnit: which day-of-month? March 14 → ordinality 14.
let ord = cal.ordinalityOfUnit(
    NSCalendarUnit.day, NSCalendarUnit.month, d
)
test.expect(ord as i64, 14)

// compareDate via day granularity: same day → orderedSame.
test.expect(cal.compareDate(d, dEnd, NSCalendarUnit.day) as i64, 0)
let nextDay2 = cal.dateByAddingComponents(plusDay, d, 0 as u64)
test.expect(cal.compareDate(d, nextDay2, NSCalendarUnit.day) as i64, -1)

0
