// NSNumber / NSDecimalNumber / NSValue.

use test
use cocoa { NSNumber, NSDecimalNumber, NSValue, NSRange,
            nsstr, ilstr, makeRange, makePoint, makeSize, makeRect }

// NSNumber round-trip.
test.expect(NSNumber.fromI64(42).i64Value, 42)
test.expectF64(NSNumber.fromF64(3.14).f64Value, 3.14)
test.expectBool(NSNumber.fromBool(true).boolValue, true)
test.expectBool(NSNumber.fromBool(false).boolValue, false)

// intValue (i32 narrowing).
test.expect(NSNumber.fromI64(99).intValue as i64, 99)

// stringValue.
test.expectStr(ilstr(NSNumber.fromI64(123).stringValue), "123")

// isEqualToNumber — value equality.
test.expectTrue(NSNumber.fromI64(5).isEqualToNumber(NSNumber.fromI64(5)))
test.expectFalse(NSNumber.fromI64(5).isEqualToNumber(NSNumber.fromI64(6)))

// NSDecimalNumber from string + add.
let a = NSDecimalNumber.decimalNumberWithString(nsstr("19.95"))
let b = NSDecimalNumber.decimalNumberWithString(nsstr("0.05"))
test.expectF64(a.adding(b).doubleValue, 20.0)
test.expectStr(ilstr(NSDecimalNumber.zero().stringValue), "0")

// NSValue — wrap an NSRange, read it back.
let r = makeRange(10 as u64, 5 as u64)
let nsv = NSValue.valueWithRange(r)
let back = nsv.rangeValue
test.expect(back.location as i64, 10)
test.expect(back.length as i64, 5)

// NSValue — round-trip an NSPoint.
let p = makePoint(1.5, 2.5)
let pv = NSValue.valueWithPoint(p)
let p2 = pv.pointValue
test.expectF64(p2.x, 1.5)
test.expectF64(p2.y, 2.5)

// NSValue — round-trip an NSSize.
let s = makeSize(100.0, 200.0)
let sv = NSValue.valueWithSize(s)
let s2 = sv.sizeValue
test.expectF64(s2.width, 100.0)
test.expectF64(s2.height, 200.0)

0
