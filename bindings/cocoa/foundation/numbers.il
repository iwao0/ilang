// Numeric / value-box bindings — NSValue (geometry / range
// boxing), NSNull (singleton for "absent" in collections that
// don't accept nil), NSDecimalNumber (exact decimal arithmetic),
// NSNumberFormatter (locale-aware string ↔ number), and
// NSByteCountFormatter (`"1.5 KB"`-style sizing). NSMeasurement /
// NSMeasurementFormatter need the NSUnit class tree which we
// haven't bound yet — deferred.

use core { NSObject, NSString, NSPoint, NSSize, NSRect, NSVector, NSRange }

// NSNumberFormatterStyle — how `stringFromNumber:` renders its
// input. Defaults to `none` (raw digits); change to `decimal` /
// `currency` / `percent` / `scientific` / `spellOut` to opt into
// locale-aware formatting.
pub enum NumberFormatterStyle: u64 {
    none       = 0
    decimal    = 1
    currency   = 2
    percent    = 3
    scientific = 4
    spellOut   = 5
    ordinal    = 6
    currencyISOCode      = 8
    currencyPlural       = 9
    currencyAccounting   = 10
}

// NSByteCountFormatterCountStyle — base unit set the formatter
// picks from. `file` is binary (KB = 1024 bytes); `memory` is
// the same; `decimal` is SI (KB = 1000 bytes); `binary` is
// explicit kibibyte / mebibyte.
pub enum ByteCountFormatterCountStyle: i64 {
    file    = 0
    memory  = 1
    decimal = 2
    binary  = 3
}

// NSDecimalNumberRoundingMode — rounding rule applied by
// `decimalNumberByRoundingAccordingToBehavior:`. Mirrors
// `<Foundation/NSDecimalNumber.h>`.
pub enum DecimalNumberRoundingMode: u64 {
    plain    = 0
    down     = 1
    up       = 2
    bankers  = 3
}

@extern(ObjC, "/System/Library/Frameworks/Foundation.framework/Foundation") {
    /// `NSValue` — wraps a C struct value (geometry / range) into
    /// an NSObject so it can ride inside an NSArray or NSDictionary.
    /// The typed factories below cover the four most common cases;
    /// the general `valueWithBytes:objCType:` form lives in C-only
    /// territory and is intentionally not exposed.
    @objc pub class NSValue : NSObject {
        @objc("valueWithPoint:")
        pub static valueWithPoint(p: NSPoint): NSValue
        @objc("valueWithSize:")
        pub static valueWithSize(s: NSSize): NSValue
        @objc("valueWithRect:")
        pub static valueWithRect(r: NSRect): NSValue
        @objc("valueWithRange:")
        pub static valueWithRange(r: NSRange): NSValue

        @objc("pointValue")
        pub get pointValue(): NSPoint
        @objc("sizeValue")
        pub get sizeValue(): NSSize
        @objc("rectValue")
        pub get rectValue(): NSRect
        @objc("rangeValue")
        pub get rangeValue(): NSRange

        @objc("isEqualToValue:")
        pub isEqualToValue(other: NSValue): bool
    }

    /// `NSNull` — the singleton "absent / nil placeholder" object
    /// for collections that don't accept ObjC's `nil` (NSArray,
    /// NSDictionary). `null()` is the only instance — pointer
    /// equality on it is the canonical way to test for "this slot
    /// is intentionally empty".
    @objc pub class NSNull : NSObject {
        @objc("null")
        pub static null(): NSNull
    }

    /// `NSDecimalNumber` — exact decimal arithmetic for currency
    /// math. Avoids the binary-float rounding surprises that
    /// `f64` would carry in `1.10 - 1.00`. Build from a string
    /// (`decimalNumberWithString("19.95")`) or one of the
    /// singletons (`zero` / `one` / `notANumber`); the
    /// arithmetic methods return a new NSDecimalNumber rather
    /// than mutating in place.
    @objc pub class NSDecimalNumber : NSObject {
        @objc("decimalNumberWithString:")
        pub static decimalNumberWithString(s: NSString): NSDecimalNumber

        @objc("zero")
        pub static zero(): NSDecimalNumber
        @objc("one")
        pub static one(): NSDecimalNumber
        @objc("notANumber")
        pub static notANumber(): NSDecimalNumber
        @objc("minimumDecimalNumber")
        pub static minimum(): NSDecimalNumber
        @objc("maximumDecimalNumber")
        pub static maximum(): NSDecimalNumber

        @objc("decimalNumberByAdding:")
        pub adding(other: NSDecimalNumber): NSDecimalNumber
        @objc("decimalNumberBySubtracting:")
        pub subtracting(other: NSDecimalNumber): NSDecimalNumber
        @objc("decimalNumberByMultiplyingBy:")
        pub multiplyingBy(other: NSDecimalNumber): NSDecimalNumber
        @objc("decimalNumberByDividingBy:")
        pub dividingBy(other: NSDecimalNumber): NSDecimalNumber
        @objc("decimalNumberByRaisingToPower:")
        pub raisingToPower(power: u64): NSDecimalNumber
        @objc("decimalNumberByMultiplyingByPowerOf10:")
        pub multiplyingByPowerOf10(power: i16): NSDecimalNumber

        @objc("compare:")
        pub compare(other: NSDecimalNumber): i64

        @objc("stringValue")
        pub get stringValue(): NSString
        @objc("doubleValue")
        pub get doubleValue(): f64
    }

    /// `NSNumberFormatter` — locale-aware `Number ↔ String`.
    /// Pick a built-in style by assigning `numberStyle`
    /// (`.currency`, `.percent`, …) or drive the output
    /// manually via `positiveFormat` / `negativeFormat` /
    /// `minimum/maximumFractionDigits`.
    @objc pub class NSNumberFormatter : NSObject {
        @objc("alloc")
        pub static alloc(): NSNumberFormatter
        @objc("init")
        pub init(): NSNumberFormatter

        @objc("numberStyle")
        pub get numberStyle(): NumberFormatterStyle
        @objc("setNumberStyle:")
        pub set numberStyle(v: NumberFormatterStyle)

        @objc("locale")
        pub get locale(): NSObject
        @objc("setLocale:")
        pub set locale(l: NSObject)
        @objc("currencyCode")
        pub get currencyCode(): NSString
        @objc("setCurrencyCode:")
        pub set currencyCode(c: NSString)
        @objc("currencySymbol")
        pub get currencySymbol(): NSString
        @objc("setCurrencySymbol:")
        pub set currencySymbol(s: NSString)

        @objc("minimumIntegerDigits")
        pub get minimumIntegerDigits(): u64
        @objc("setMinimumIntegerDigits:")
        pub set minimumIntegerDigits(v: u64)
        @objc("maximumIntegerDigits")
        pub get maximumIntegerDigits(): u64
        @objc("setMaximumIntegerDigits:")
        pub set maximumIntegerDigits(v: u64)
        @objc("minimumFractionDigits")
        pub get minimumFractionDigits(): u64
        @objc("setMinimumFractionDigits:")
        pub set minimumFractionDigits(v: u64)
        @objc("maximumFractionDigits")
        pub get maximumFractionDigits(): u64
        @objc("setMaximumFractionDigits:")
        pub set maximumFractionDigits(v: u64)

        @objc("usesGroupingSeparator")
        pub get usesGroupingSeparator(): bool
        @objc("setUsesGroupingSeparator:")
        pub set usesGroupingSeparator(v: bool)
        @objc("groupingSeparator")
        pub get groupingSeparator(): NSString
        @objc("setGroupingSeparator:")
        pub set groupingSeparator(s: NSString)
        @objc("decimalSeparator")
        pub get decimalSeparator(): NSString
        @objc("setDecimalSeparator:")
        pub set decimalSeparator(s: NSString)

        @objc("stringFromNumber:")
        pub stringFromNumber(n: NSObject): NSString
        @objc("numberFromString:")
        pub numberFromString(s: NSString): NSObject
    }

    /// `NSByteCountFormatter` — `"1.5 KB"` / `"512 MB"` style file
    /// size rendering. Use the static `stringFromByteCount:countStyle:`
    /// for one-shot calls, or an instance for repeated formatting
    /// with bespoke `allowedUnits` / `includesCount` /
    /// `includesUnit` settings.
    @objc pub class NSByteCountFormatter : NSObject {
        @objc("alloc")
        pub static alloc(): NSByteCountFormatter
        @objc("init")
        pub init(): NSByteCountFormatter

        @objc("stringFromByteCount:countStyle:")
        pub static stringFromByteCountWithStyle(
            bytes: i64, countStyle: ByteCountFormatterCountStyle
        ): NSString

        @objc("countStyle")
        pub get countStyle(): ByteCountFormatterCountStyle
        @objc("setCountStyle:")
        pub set countStyle(v: ByteCountFormatterCountStyle)
        @objc("allowedUnits")
        pub get allowedUnits(): u64
        @objc("setAllowedUnits:")
        pub set allowedUnits(v: u64)
        @objc("includesCount")
        pub get includesCount(): bool
        @objc("setIncludesCount:")
        pub set includesCount(v: bool)
        @objc("includesUnit")
        pub get includesUnit(): bool
        @objc("setIncludesUnit:")
        pub set includesUnit(v: bool)
        @objc("zeroPadsFractionDigits")
        pub get zeroPadsFractionDigits(): bool
        @objc("setZeroPadsFractionDigits:")
        pub set zeroPadsFractionDigits(v: bool)

        @objc("stringFromByteCount:")
        pub stringFromByteCount(bytes: i64): NSString
    }
}
