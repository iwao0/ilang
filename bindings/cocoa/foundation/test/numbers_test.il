// NSNumber / NSDecimalNumber / NSValue.

use test
use foundation { NSNumber, NSDecimalNumber, NSValue, NSRange,
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

// NSValue — round-trip an NSRect.
let rr = makeRect(1.0, 2.0, 3.0, 4.0)
let rv = NSValue.valueWithRect(rr)
let r2 = rv.rectValue
test.expectF64(r2.origin.x, 1.0)
test.expectF64(r2.size.width, 3.0)

// NSValue.isEqualToValue.
let pa = NSValue.valueWithPoint(makePoint(1.0, 2.0))
let pb = NSValue.valueWithPoint(makePoint(1.0, 2.0))
let pc = NSValue.valueWithPoint(makePoint(9.0, 9.0))
test.expectTrue(pa.isEqualToValue(pb))
test.expectFalse(pa.isEqualToValue(pc))

// NSNumber.compareNumber.
let smaller = NSNumber.fromI64(1)
let bigger = NSNumber.fromI64(2)
// ComparisonResult: orderedAscending = -1 here.
test.expect(smaller.compareNumber(bigger) as i64, -1)
test.expect(bigger.compareNumber(smaller) as i64, 1)
test.expect(smaller.compareNumber(smaller) as i64, 0)

// NSNumber.stringValue across types.
test.expectStr(ilstr(NSNumber.fromBool(true).stringValue), "1")
test.expectStr(ilstr(NSNumber.fromF64(2.5).stringValue), "2.5")

// NSDecimalNumber.compare.
let d1 = NSDecimalNumber.decimalNumberWithString(nsstr("1.5"))
let d2 = NSDecimalNumber.decimalNumberWithString(nsstr("2.5"))
test.expect(d1.compare(d2), -1)
test.expect(d2.compare(d1), 1)

// NSDecimalNumber arithmetic chain.
let sum = d1.adding(d2)
test.expectF64(sum.doubleValue, 4.0)
let diff = d2.subtracting(d1)
test.expectF64(diff.doubleValue, 1.0)
let prod = d1.multiplyingBy(d2)
test.expectF64(prod.doubleValue, 3.75)

0
