// Date / time bindings — NSDate, NSDateFormatter, NSCalendar,
// NSDateComponents. The `formatters.il` sibling carries the
// auxiliary NSDateComponentsFormatter / NSDateIntervalFormatter
// / NSRelativeDateTimeFormatter trio that builds on the types
// here.

use core { NSObject, NSString }

// NSDateFormatterStyle — the locale-aware default presets you
// pick by assigning to `dateStyle` / `timeStyle`. Setting
// `dateFormat` directly to a Unicode TR35 pattern overrides
// these.
pub enum DateFormatterStyle: u64 {
    none   = 0
    short  = 1
    medium = 2
    long   = 3
    full   = 4
}

// NSCalendarOptions — controls how
// `dateByAddingComponents:toDate:options:` and the
// `nextDateAfterDate:matchingComponents:options:` family
// resolve missing / ambiguous time-zone transitions.
@flags
pub enum CalendarOptions: u64 {
    none                              = 0x00
    wrapCalendarComponents            = 1
    matchStrictly                     = 2
    searchBackwards                   = 4
    matchPreviousTimePreservingSmallerUnits = 256
    matchNextTimePreservingSmallerUnits     = 512
    matchNextTime                     = 1024
    matchFirst                        = 4096
    matchLast                         = 8192
}

// NSCalendarUnit — bit set used by `[NSCalendar
// components:fromDate:]` to say which fields the returned
// `NSDateComponents` should be populated with. Combine with `|`.
@flags
pub enum CalendarUnit: u64 {
    none       = 0x00
    era        = 0x02
    year       = 0x04
    month      = 0x08
    day        = 0x10
    hour       = 0x20
    minute     = 0x40
    second     = 0x80
    weekday    = 0x200
    quarter    = 0x800
    weekOfYear = 0x4000
    yearForWeekOfYear = 0x8000
    nanosecond = 0x10000
    calendar   = 0x100000
    timeZone   = 0x200000
}

@extern(ObjC, "/System/Library/Frameworks/Foundation.framework/Foundation") {
    /// `NSDate` — a moment in time. `date` is "now"; the interval
    /// methods give you the offset in seconds.
    @objc pub class NSDate : NSObject {
        @objc("date")
        pub static now(): NSDate
        @objc("dateWithTimeIntervalSince1970:")
        pub static dateWithTimeIntervalSince1970(secs: f64): NSDate
        @objc("dateWithTimeIntervalSinceNow:")
        pub static dateWithTimeIntervalSinceNow(secs: f64): NSDate
        @objc("distantFuture")
        pub static distantFuture(): NSDate
        @objc("distantPast")
        pub static distantPast(): NSDate

        @objc("timeIntervalSince1970")
        pub get timeIntervalSince1970(): f64
        @objc("timeIntervalSinceNow")
        pub get timeIntervalSinceNow(): f64
        @objc("timeIntervalSinceDate:")
        pub timeIntervalSinceDate(other: NSDate): f64

        @objc("dateByAddingTimeInterval:")
        pub dateByAddingTimeInterval(secs: f64): NSDate
        @objc("isEqualToDate:")
        pub isEqualToDate(other: NSDate): bool
        @objc("earlierDate:")
        pub earlierDate(other: NSDate): NSDate
        @objc("laterDate:")
        pub laterDate(other: NSDate): NSDate
    }

    /// `NSDateFormatter` — string ↔ date conversion. Mutate
    /// `dateStyle` / `timeStyle` for a locale-aware default
    /// (`short` / `medium` / `long` / `full`) or set `dateFormat`
    /// directly for a Unicode TR35 pattern like `"yyyy-MM-dd"`.
    /// Set `locale` to an `NSLocale` if the system locale isn't
    /// what you want (e.g. for log files: `en_US_POSIX`).
    @objc pub class NSDateFormatter : NSObject {
        @objc("alloc")
        pub static alloc(): NSDateFormatter
        @objc("init")
        pub init(): NSDateFormatter

        @objc("dateFormat")
        pub get dateFormat(): NSString
        @objc("setDateFormat:")
        pub set dateFormat(s: NSString)

        @objc("dateStyle")
        pub get dateStyle(): DateFormatterStyle
        @objc("setDateStyle:")
        pub set dateStyle(v: DateFormatterStyle)
        @objc("timeStyle")
        pub get timeStyle(): DateFormatterStyle
        @objc("setTimeStyle:")
        pub set timeStyle(v: DateFormatterStyle)

        @objc("locale")
        pub get locale(): NSObject
        @objc("setLocale:")
        pub set locale(l: NSObject)
        @objc("timeZone")
        pub get timeZone(): NSObject
        @objc("setTimeZone:")
        pub set timeZone(tz: NSObject)

        @objc("stringFromDate:")
        pub stringFromDate(d: NSDate): NSString
        @objc("dateFromString:")
        pub dateFromString(s: NSString): NSDate
    }

    /// `NSCalendar` — Gregorian / Japanese / ISO8601 / etc.
    /// calendar arithmetic and component extraction. Use
    /// `currentCalendar()` for the user's locale or
    /// `calendarWithIdentifier:` to pin one explicitly.
    @objc pub class NSCalendar : NSObject {
        @objc("currentCalendar")
        pub static currentCalendar(): NSCalendar
        @objc("autoupdatingCurrentCalendar")
        pub static autoupdatingCurrentCalendar(): NSCalendar
        @objc("calendarWithIdentifier:")
        pub static calendarWithIdentifier(ident: NSString): NSCalendar

        @objc("components:fromDate:")
        pub componentsFromDate(units: CalendarUnit, date: NSDate): NSDateComponents
        @objc("dateFromComponents:")
        pub dateFromComponents(comps: NSDateComponents): NSDate
        @objc("dateByAddingComponents:toDate:options:")
        pub dateByAddingComponents(
            comps: NSDateComponents, toDate: NSDate, options: u64
        ): NSDate
        @objc("isDateInToday:")
        pub isDateInToday(d: NSDate): bool
        @objc("isDateInYesterday:")
        pub isDateInYesterday(d: NSDate): bool
        @objc("isDateInTomorrow:")
        pub isDateInTomorrow(d: NSDate): bool
    }

    /// `NSDateInterval` — half-open date range `[startDate, endDate)`.
    /// Comparison / intersection / containment all use the
    /// half-open convention. Pair with NSDateIntervalFormatter to
    /// render it as a locale-aware string.
    @objc pub class NSDateInterval : NSObject {
        @objc("alloc")
        pub static alloc(): NSDateInterval
        @objc("init")
        pub init(): NSDateInterval
        @objc("initWithStartDate:duration:")
        pub initWithStartDateAndDuration(start: NSDate, duration: f64): NSDateInterval
        @objc("initWithStartDate:endDate:")
        pub initWithStartDateAndEndDate(start: NSDate, end: NSDate): NSDateInterval

        @objc("startDate")
        pub get startDate(): NSDate
        @objc("endDate")
        pub get endDate(): NSDate
        @objc("duration")
        pub get duration(): f64

        @objc("compare:")
        pub compare(other: NSDateInterval): i64
        @objc("isEqualToDateInterval:")
        pub isEqualToDateInterval(other: NSDateInterval): bool
        @objc("intersectsDateInterval:")
        pub intersectsDateInterval(other: NSDateInterval): bool
        @objc("intersectionWithDateInterval:")
        pub intersectionWithDateInterval(other: NSDateInterval): NSDateInterval
        @objc("containsDate:")
        pub containsDate(d: NSDate): bool
    }

    /// `NSDateComponents` — bag of optional calendar fields. Set
    /// the ones you care about; the rest are sentinel values
    /// (`NSDateComponentUndefined`, which surfaces as the
    /// platform-specific "missing" code — most Foundation APIs
    /// just ignore unset components).
    @objc pub class NSDateComponents : NSObject {
        @objc("alloc")
        pub static alloc(): NSDateComponents
        @objc("init")
        pub init(): NSDateComponents

        @objc("year")
        pub get year(): i64
        @objc("setYear:")
        pub set year(v: i64)
        @objc("month")
        pub get month(): i64
        @objc("setMonth:")
        pub set month(v: i64)
        @objc("day")
        pub get day(): i64
        @objc("setDay:")
        pub set day(v: i64)
        @objc("hour")
        pub get hour(): i64
        @objc("setHour:")
        pub set hour(v: i64)
        @objc("minute")
        pub get minute(): i64
        @objc("setMinute:")
        pub set minute(v: i64)
        @objc("second")
        pub get second(): i64
        @objc("setSecond:")
        pub set second(v: i64)
        @objc("weekday")
        pub get weekday(): i64
        @objc("setWeekday:")
        pub set weekday(v: i64)
    }
}
