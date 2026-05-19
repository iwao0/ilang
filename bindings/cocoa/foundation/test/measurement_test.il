// NSMeasurement + NSUnit{Length, Mass, Duration, Temperature, Area}
// conversions.

use test
use foundation { NSMeasurement, NSUnitLength, NSUnitMass,
                 NSUnitDuration, NSUnitTemperature, NSUnitArea,
                 ilstr }

// 1 km = 1000 m.
let oneKm = NSMeasurement.alloc().initWithDoubleValue(
    1.0, NSUnitLength.kilometers()
)
let inM = oneKm.convertedToUnit(NSUnitLength.meters())
test.expectF64(inM.doubleValue, 1000.0)

// 1000 g = 1 kg.
let g1000 = NSMeasurement.alloc().initWithDoubleValue(
    1000.0, NSUnitMass.grams()
)
test.expectF64(g1000.convertedToUnit(NSUnitMass.kilograms()).doubleValue, 1.0)

// 1 hour = 3600 seconds.
let oneHour = NSMeasurement.alloc().initWithDoubleValue(
    1.0, NSUnitDuration.hours()
)
test.expectF64(
    oneHour.convertedToUnit(NSUnitDuration.seconds()).doubleValue,
    3600.0
)

// 0°C = 273.15 K.
let zeroC = NSMeasurement.alloc().initWithDoubleValue(
    0.0, NSUnitTemperature.celsius()
)
test.expectF64(
    zeroC.convertedToUnit(NSUnitTemperature.kelvin()).doubleValue,
    273.15
)
// 100°C = 212°F.
let boil = NSMeasurement.alloc().initWithDoubleValue(
    100.0, NSUnitTemperature.celsius()
)
test.expectF64(
    boil.convertedToUnit(NSUnitTemperature.fahrenheit()).doubleValue,
    212.0
)

// Adding like measurements.
let a = NSMeasurement.alloc().initWithDoubleValue(
    500.0, NSUnitLength.meters()
)
let b = NSMeasurement.alloc().initWithDoubleValue(
    0.5, NSUnitLength.kilometers()
)
let sum = a.measurementByAdding(b)
// Result unit is `a`'s unit; 500 m + 500 m = 1000 m.
test.expectF64(sum.doubleValue, 1000.0)

// canBeConvertedToUnit — same dimension family OK, cross family no.
test.expectTrue(a.canBeConvertedToUnit(NSUnitLength.miles()))
test.expectFalse(a.canBeConvertedToUnit(NSUnitMass.kilograms()))

// Unit symbol smoke.
test.expectStr(ilstr(NSUnitLength.meters().symbol), "m")
test.expectStr(ilstr(NSUnitMass.kilograms().symbol), "kg")

0
