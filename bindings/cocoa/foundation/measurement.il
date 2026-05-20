/// Physical-unit measurement bindings — NSMeasurement (value +
/// unit pair) and the NSUnit class tree it indexes into.
///
/// NSUnit's subtree has ~25 dimensional categories. This file
/// binds the abstract bases (NSUnit / NSDimension) plus the eight
/// most commonly used dimensions (length / mass / time / area /
/// volume / speed / temperature / energy). Each dimension exposes
/// a handful of typed `NSUnit*** *` singletons via class methods
/// — pass them to `NSMeasurement.initWithDoubleValue:unit:`.

use core { NSObject, NSString }

@extern(ObjC, "/System/Library/Frameworks/Foundation.framework/Foundation") {
    /// `NSUnit` — abstract root of the unit hierarchy. Holds the
    /// symbol (`"kg"`, `"m/s"`, …); concrete behaviour lives on
    /// `NSDimension` and its subclasses.
    @objc pub class NSUnit : NSObject {
        @objc("alloc")
        pub static alloc(): NSUnit
        @objc("initWithSymbol:")
        pub initWithSymbol(s: NSString): NSUnit
        @objc("symbol")
        pub get symbol(): NSString
    }

    /// `NSDimension` — units that compose along a numeric scale
    /// (`m`, `kg`, …) versus categorical units (`Hz` vs `kHz`).
    /// Each concrete dimension subclass picks one base unit and
    /// expresses the rest as a linear conversion from it.
    @objc pub class NSDimension : NSUnit {
        @objc("baseUnit")
        pub static baseUnit(): NSDimension
    }

    /// `NSUnitLength` — meters / kilometers / inches / etc. Use
    /// `NSUnitLength.meters()` etc. as the unit argument for
    /// `NSMeasurement`.
    @objc pub class NSUnitLength : NSDimension {
        @objc("meters")
        pub static meters(): NSUnitLength
        @objc("kilometers")
        pub static kilometers(): NSUnitLength
        @objc("centimeters")
        pub static centimeters(): NSUnitLength
        @objc("millimeters")
        pub static millimeters(): NSUnitLength
        @objc("inches")
        pub static inches(): NSUnitLength
        @objc("feet")
        pub static feet(): NSUnitLength
        @objc("yards")
        pub static yards(): NSUnitLength
        @objc("miles")
        pub static miles(): NSUnitLength
        @objc("nauticalMiles")
        pub static nauticalMiles(): NSUnitLength
        @objc("astronomicalUnits")
        pub static astronomicalUnits(): NSUnitLength
    }

    /// `NSUnitMass` — grams / kilograms / pounds / ounces / etc.
    @objc pub class NSUnitMass : NSDimension {
        @objc("kilograms")
        pub static kilograms(): NSUnitMass
        @objc("grams")
        pub static grams(): NSUnitMass
        @objc("milligrams")
        pub static milligrams(): NSUnitMass
        @objc("ounces")
        pub static ounces(): NSUnitMass
        @objc("pounds")
        pub static pounds(): NSUnitMass
        @objc("stones")
        pub static stones(): NSUnitMass
        @objc("metricTons")
        pub static metricTons(): NSUnitMass
    }

    /// `NSUnitDuration` — seconds / minutes / hours / etc.
    @objc pub class NSUnitDuration : NSDimension {
        @objc("seconds")
        pub static seconds(): NSUnitDuration
        @objc("minutes")
        pub static minutes(): NSUnitDuration
        @objc("hours")
        pub static hours(): NSUnitDuration
        @objc("milliseconds")
        pub static milliseconds(): NSUnitDuration
        @objc("microseconds")
        pub static microseconds(): NSUnitDuration
        @objc("nanoseconds")
        pub static nanoseconds(): NSUnitDuration
    }

    /// `NSUnitTemperature` — Kelvin / Celsius / Fahrenheit.
    @objc pub class NSUnitTemperature : NSDimension {
        @objc("kelvin")
        pub static kelvin(): NSUnitTemperature
        @objc("celsius")
        pub static celsius(): NSUnitTemperature
        @objc("fahrenheit")
        pub static fahrenheit(): NSUnitTemperature
    }

    /// `NSUnitArea` — square meters / square feet / hectares /
    /// acres / etc.
    @objc pub class NSUnitArea : NSDimension {
        @objc("squareMeters")
        pub static squareMeters(): NSUnitArea
        @objc("squareKilometers")
        pub static squareKilometers(): NSUnitArea
        @objc("squareCentimeters")
        pub static squareCentimeters(): NSUnitArea
        @objc("squareInches")
        pub static squareInches(): NSUnitArea
        @objc("squareFeet")
        pub static squareFeet(): NSUnitArea
        @objc("squareYards")
        pub static squareYards(): NSUnitArea
        @objc("squareMiles")
        pub static squareMiles(): NSUnitArea
        @objc("hectares")
        pub static hectares(): NSUnitArea
        @objc("acres")
        pub static acres(): NSUnitArea
    }

    /// `NSUnitVolume` — liters / cubic meters / gallons / etc.
    @objc pub class NSUnitVolume : NSDimension {
        @objc("liters")
        pub static liters(): NSUnitVolume
        @objc("milliliters")
        pub static milliliters(): NSUnitVolume
        @objc("cubicMeters")
        pub static cubicMeters(): NSUnitVolume
        @objc("cubicCentimeters")
        pub static cubicCentimeters(): NSUnitVolume
        @objc("cubicInches")
        pub static cubicInches(): NSUnitVolume
        @objc("cubicFeet")
        pub static cubicFeet(): NSUnitVolume
        @objc("gallons")
        pub static gallons(): NSUnitVolume
        @objc("quarts")
        pub static quarts(): NSUnitVolume
        @objc("pints")
        pub static pints(): NSUnitVolume
        @objc("cups")
        pub static cups(): NSUnitVolume
    }

    /// `NSUnitSpeed` — m/s, km/h, mph, knots.
    @objc pub class NSUnitSpeed : NSDimension {
        @objc("metersPerSecond")
        pub static metersPerSecond(): NSUnitSpeed
        @objc("kilometersPerHour")
        pub static kilometersPerHour(): NSUnitSpeed
        @objc("milesPerHour")
        pub static milesPerHour(): NSUnitSpeed
        @objc("knots")
        pub static knots(): NSUnitSpeed
    }

    /// `NSUnitEnergy` — joules / kilojoules / calories / kWh.
    @objc pub class NSUnitEnergy : NSDimension {
        @objc("joules")
        pub static joules(): NSUnitEnergy
        @objc("kilojoules")
        pub static kilojoules(): NSUnitEnergy
        @objc("calories")
        pub static calories(): NSUnitEnergy
        @objc("kilocalories")
        pub static kilocalories(): NSUnitEnergy
        @objc("kilowattHours")
        pub static kilowattHours(): NSUnitEnergy
    }

    /// `NSMeasurement` — value + unit pair. Convert across
    /// dimensional units via `convertedToUnit:`; arithmetic
    /// across like dimensions via `measurementByAdding:` and
    /// `measurementBySubtracting:`. The unit must be an
    /// `NSDimension` (or subclass) for conversion to be defined.
    @objc pub class NSMeasurement : NSObject {
        @objc("alloc")
        pub static alloc(): NSMeasurement
        @objc("initWithDoubleValue:unit:")
        pub initWithDoubleValue(value: f64, unit: NSUnit): NSMeasurement

        @objc("doubleValue")
        pub get doubleValue(): f64
        @objc("unit")
        pub get unit(): NSUnit

        @objc("canBeConvertedToUnit:")
        pub canBeConvertedToUnit(unit: NSUnit): bool
        @objc("measurementByConvertingToUnit:")
        pub convertedToUnit(unit: NSUnit): NSMeasurement
        @objc("measurementByAddingMeasurement:")
        pub measurementByAdding(other: NSMeasurement): NSMeasurement
        @objc("measurementBySubtractingMeasurement:")
        pub measurementBySubtracting(other: NSMeasurement): NSMeasurement
    }
}
