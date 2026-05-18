// Miscellaneous Foundation formatter bindings — the date /
// duration / personal-name / physical-unit variants that didn't
// fit `numbers.il` or `datetime.il`.
//
// All of them follow the same shape: `alloc().init()` to mint
// an instance, set the units / style / locale properties, then
// call `stringFrom...`. NSDateIntervalFormatter is the one
// outlier whose input is a pair of dates rather than a
// pre-built components value.

use core { NSObject, NSString }
use datetime { NSDate, NSDateComponents }

// NSDateComponentsFormatterUnitsStyle — text style for the
// emitted duration (e.g. `"1h 23m"` vs `"1 hour, 23 minutes"`).
pub enum DateComponentsFormatterUnitsStyle: i64 {
    positional   = 0
    abbreviated  = 1
    short        = 2
    full         = 3
    spellOut     = 4
    brief        = 5
}

// NSDateIntervalFormatterStyle — full / long / medium / short /
// none. Same conventions as the DateFormatterStyle in
// `numbers.il`'s NSDateFormatter sibling.
pub enum DateIntervalFormatterStyle: u64 {
    none    = 0
    short   = 1
    medium  = 2
    long    = 3
    full    = 4
}

// NSRelativeDateTimeFormatterUnitsStyle — natural-language unit
// style for the "N units ago" / "in N units" relative phrasing.
pub enum RelativeDateTimeFormatterUnitsStyle: i64 {
    full              = 0
    spellOut          = 1
    short             = 2
    abbreviated       = 3
    narrow            = 4
}

// NSRelativeDateTimeFormatterStyle — whether the output is
// numeric (`"1 day ago"`) or named (`"yesterday"`).
pub enum RelativeDateTimeFormatterStyle: i64 {
    numeric = 0
    named   = 1
}

// NSFormattingUnitStyle — generic short / medium / long unit
// style shared by NSEnergyFormatter / NSMassFormatter /
// NSLengthFormatter.
pub enum FormattingUnitStyle: i64 {
    short  = 1
    medium = 2
    long   = 3
}

// NSPersonNameComponentsFormatterStyle — full / abbreviated /
// short / medium / long display for a name.
pub enum PersonNameComponentsFormatterStyle: u64 {
    default     = 0
    short       = 1
    medium      = 2
    long        = 3
    abbreviated = 4
}

@extern(ObjC, "/System/Library/Frameworks/Foundation.framework/Foundation") {
    /// `NSDateComponentsFormatter` — render an NSDateComponents
    /// (or a raw `NSTimeInterval` seconds value) as a
    /// human-readable duration. Set `allowedUnits` to the bit
    /// mask of `NSCalendarUnit` flags you want included; the
    /// formatter picks the largest applicable units.
    @objc pub class NSDateComponentsFormatter : NSObject {
        @objc("alloc")
        pub static alloc(): NSDateComponentsFormatter
        @objc("init")
        pub init(): NSDateComponentsFormatter

        @objc("unitsStyle")
        pub get unitsStyle(): DateComponentsFormatterUnitsStyle
        @objc("setUnitsStyle:")
        pub set unitsStyle(v: DateComponentsFormatterUnitsStyle)
        @objc("allowedUnits")
        pub get allowedUnits(): u64
        @objc("setAllowedUnits:")
        pub set allowedUnits(v: u64)
        @objc("collapsesLargestUnit")
        pub get collapsesLargestUnit(): bool
        @objc("setCollapsesLargestUnit:")
        pub set collapsesLargestUnit(v: bool)
        @objc("includesApproximationPhrase")
        pub get includesApproximationPhrase(): bool
        @objc("setIncludesApproximationPhrase:")
        pub set includesApproximationPhrase(v: bool)
        @objc("maximumUnitCount")
        pub get maximumUnitCount(): i64
        @objc("setMaximumUnitCount:")
        pub set maximumUnitCount(n: i64)

        @objc("stringFromDateComponents:")
        pub stringFromDateComponents(comps: NSDateComponents): NSString
        @objc("stringFromTimeInterval:")
        pub stringFromTimeInterval(seconds: f64): NSString
        @objc("stringFromDate:toDate:")
        pub stringFromDateToDate(from: NSDate, to: NSDate): NSString
    }

    /// `NSDateIntervalFormatter` — render a `(start, end)`
    /// date pair as a locale-aware range string
    /// (`"Mar 5 – 12, 2026"`). Style separately controls the
    /// date and time parts.
    @objc pub class NSDateIntervalFormatter : NSObject {
        @objc("alloc")
        pub static alloc(): NSDateIntervalFormatter
        @objc("init")
        pub init(): NSDateIntervalFormatter

        @objc("dateStyle")
        pub get dateStyle(): DateIntervalFormatterStyle
        @objc("setDateStyle:")
        pub set dateStyle(v: DateIntervalFormatterStyle)
        @objc("timeStyle")
        pub get timeStyle(): DateIntervalFormatterStyle
        @objc("setTimeStyle:")
        pub set timeStyle(v: DateIntervalFormatterStyle)
        @objc("dateTemplate")
        pub get dateTemplate(): NSString
        @objc("setDateTemplate:")
        pub set dateTemplate(s: NSString)
        @objc("locale")
        pub get locale(): NSObject
        @objc("setLocale:")
        pub set locale(l: NSObject)
        @objc("timeZone")
        pub get timeZone(): NSObject
        @objc("setTimeZone:")
        pub set timeZone(tz: NSObject)

        @objc("stringFromDate:toDate:")
        pub stringFromDateToDate(from: NSDate, to: NSDate): NSString
    }

    /// `NSRelativeDateTimeFormatter` — natural-language "N units
    /// ago" / "in N units" phrasing. `unitsStyle` picks how
    /// units render; `dateTimeStyle` decides between numeric
    /// ("1 day ago") and named ("yesterday").
    @objc pub class NSRelativeDateTimeFormatter : NSObject {
        @objc("alloc")
        pub static alloc(): NSRelativeDateTimeFormatter
        @objc("init")
        pub init(): NSRelativeDateTimeFormatter

        @objc("unitsStyle")
        pub get unitsStyle(): RelativeDateTimeFormatterUnitsStyle
        @objc("setUnitsStyle:")
        pub set unitsStyle(v: RelativeDateTimeFormatterUnitsStyle)
        @objc("dateTimeStyle")
        pub get dateTimeStyle(): RelativeDateTimeFormatterStyle
        @objc("setDateTimeStyle:")
        pub set dateTimeStyle(v: RelativeDateTimeFormatterStyle)
        @objc("locale")
        pub get locale(): NSObject
        @objc("setLocale:")
        pub set locale(l: NSObject)

        @objc("localizedStringFromTimeInterval:")
        pub localizedStringFromTimeInterval(seconds: f64): NSString
        @objc("localizedStringFromDate:relativeToDate:")
        pub localizedStringFromDateRelativeToDate(
            from: NSDate, relativeTo: NSDate
        ): NSString
    }

    /// `NSEnergyFormatter` — render an energy value (Joules) as
    /// a localized string, optionally with unit selection.
    @objc pub class NSEnergyFormatter : NSObject {
        @objc("alloc")
        pub static alloc(): NSEnergyFormatter
        @objc("init")
        pub init(): NSEnergyFormatter

        @objc("unitStyle")
        pub get unitStyle(): FormattingUnitStyle
        @objc("setUnitStyle:")
        pub set unitStyle(v: FormattingUnitStyle)
        @objc("isForFoodEnergyUse")
        pub get isForFoodEnergyUse(): bool
        @objc("setForFoodEnergyUse:")
        pub set isForFoodEnergyUse(v: bool)

        @objc("stringFromJoules:")
        pub stringFromJoules(joules: f64): NSString
        @objc("stringFromValue:unit:")
        pub stringFromValueAndUnit(value: f64, unit: i64): NSString
    }

    /// `NSMassFormatter` — same shape as the energy formatter
    /// but for grams / kilograms / pounds / etc.
    @objc pub class NSMassFormatter : NSObject {
        @objc("alloc")
        pub static alloc(): NSMassFormatter
        @objc("init")
        pub init(): NSMassFormatter

        @objc("unitStyle")
        pub get unitStyle(): FormattingUnitStyle
        @objc("setUnitStyle:")
        pub set unitStyle(v: FormattingUnitStyle)
        @objc("isForPersonMassUse")
        pub get isForPersonMassUse(): bool
        @objc("setForPersonMassUse:")
        pub set isForPersonMassUse(v: bool)

        @objc("stringFromKilograms:")
        pub stringFromKilograms(kg: f64): NSString
        @objc("stringFromValue:unit:")
        pub stringFromValueAndUnit(value: f64, unit: i64): NSString
    }

    /// `NSLengthFormatter` — meters / feet / etc. presentation.
    @objc pub class NSLengthFormatter : NSObject {
        @objc("alloc")
        pub static alloc(): NSLengthFormatter
        @objc("init")
        pub init(): NSLengthFormatter

        @objc("unitStyle")
        pub get unitStyle(): FormattingUnitStyle
        @objc("setUnitStyle:")
        pub set unitStyle(v: FormattingUnitStyle)
        @objc("isForPersonHeightUse")
        pub get isForPersonHeightUse(): bool
        @objc("setForPersonHeightUse:")
        pub set isForPersonHeightUse(v: bool)

        @objc("stringFromMeters:")
        pub stringFromMeters(m: f64): NSString
        @objc("stringFromValue:unit:")
        pub stringFromValueAndUnit(value: f64, unit: i64): NSString
    }

    /// `NSPersonNameComponents` — name-part bag (given /
    /// middle / family / nickname / prefix / suffix). Build
    /// manually or have `NSPersonNameComponentsFormatter` parse
    /// a full name into one.
    @objc pub class NSPersonNameComponents : NSObject {
        @objc("alloc")
        pub static alloc(): NSPersonNameComponents
        @objc("init")
        pub init(): NSPersonNameComponents

        @objc("namePrefix")
        pub get namePrefix(): NSString
        @objc("setNamePrefix:")
        pub set namePrefix(s: NSString)
        @objc("givenName")
        pub get givenName(): NSString
        @objc("setGivenName:")
        pub set givenName(s: NSString)
        @objc("middleName")
        pub get middleName(): NSString
        @objc("setMiddleName:")
        pub set middleName(s: NSString)
        @objc("familyName")
        pub get familyName(): NSString
        @objc("setFamilyName:")
        pub set familyName(s: NSString)
        @objc("nameSuffix")
        pub get nameSuffix(): NSString
        @objc("setNameSuffix:")
        pub set nameSuffix(s: NSString)
        @objc("nickname")
        pub get nickname(): NSString
        @objc("setNickname:")
        pub set nickname(s: NSString)
    }

    /// `NSPersonNameComponentsFormatter` — render an
    /// NSPersonNameComponents in a locale-appropriate style
    /// (`"Hideyuki Iwao"` vs `"岩尾 英之"`).
    @objc pub class NSPersonNameComponentsFormatter : NSObject {
        @objc("alloc")
        pub static alloc(): NSPersonNameComponentsFormatter
        @objc("init")
        pub init(): NSPersonNameComponentsFormatter

        @objc("style")
        pub get style(): PersonNameComponentsFormatterStyle
        @objc("setStyle:")
        pub set style(v: PersonNameComponentsFormatterStyle)
        @objc("isPhonetic")
        pub get isPhonetic(): bool
        @objc("setPhonetic:")
        pub set isPhonetic(v: bool)
        @objc("locale")
        pub get locale(): NSObject
        @objc("setLocale:")
        pub set locale(l: NSObject)

        @objc("stringFromPersonNameComponents:")
        pub stringFromPersonNameComponents(
            components: NSPersonNameComponents
        ): NSString
        @objc("personNameComponentsFromString:")
        pub personNameComponentsFromString(
            s: NSString
        ): NSPersonNameComponents
    }
}
