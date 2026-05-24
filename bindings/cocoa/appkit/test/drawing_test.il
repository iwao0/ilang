/// NSBezierPath / NSAffineTransform / NSColor — pure construct +
/// inspect; no NSGraphicsContext / drawing actually happens.

use std.test
use appkit { NSBezierPath, NSAffineTransform, NSColor,
             NSWindingRule, NSLineCapStyle, NSLineJoinStyle }
use foundation { makePoint, makeRect, makeSize }

// Empty bezier path has no elements.
let p = NSBezierPath.bezierPath()
test.expectTrue(p.isEmpty)
test.expect(p.elementCount, 0)

// moveToPoint + lineToPoint: currentPoint follows the cursor.
p.moveToPoint(makePoint(10.0, 20.0))
test.expectFalse(p.isEmpty)
test.expect(p.elementCount, 1)
let cp1 = p.currentPoint
test.expectF64(cp1.x, 10.0)
test.expectF64(cp1.y, 20.0)

p.lineToPoint(makePoint(50.0, 60.0))
let cp2 = p.currentPoint
test.expectF64(cp2.x, 50.0)
test.expectF64(cp2.y, 60.0)
test.expect(p.elementCount, 2)

// Rectangle factory + bounds.
let r = NSBezierPath.bezierPathWithRect(makeRect(0.0, 0.0, 100.0, 50.0))
let b = r.bounds
test.expectF64(b.size.width, 100.0)
test.expectF64(b.size.height, 50.0)

// Round-trip stroke state.
r.lineWidth = 3.5
test.expectF64(r.lineWidth, 3.5)
r.lineCapStyle = NSLineCapStyle.round
test.expect(r.lineCapStyle as i64, NSLineCapStyle.round as i64)
r.lineJoinStyle = NSLineJoinStyle.bevel
test.expect(r.lineJoinStyle as i64, NSLineJoinStyle.bevel as i64)
r.windingRule = NSWindingRule.evenOdd
test.expect(r.windingRule as i64, NSWindingRule.evenOdd as i64)

// containsPoint — point inside the rectangle is inside the path
// after filling, but `containsPoint:` uses the receiver's
// winding fill rule, so the simple unfilled rect path returns
// true for inside.
test.expectTrue(r.containsPoint(makePoint(50.0, 25.0)))
test.expectFalse(r.containsPoint(makePoint(200.0, 200.0)))

// NSAffineTransform — translate then transform a point.
let t = NSAffineTransform.transform()
t.translateXBy(10.0, 5.0)
let pt = t.transformPoint(makePoint(1.0, 2.0))
test.expectF64(pt.x, 11.0)
test.expectF64(pt.y, 7.0)

// Scale.
let t2 = NSAffineTransform.transform()
t2.scaleBy(2.0)
let pt2 = t2.transformPoint(makePoint(3.0, 4.0))
test.expectF64(pt2.x, 6.0)
test.expectF64(pt2.y, 8.0)

// Anisotropic scale.
let t3 = NSAffineTransform.transform()
t3.scaleXBy(2.0, 3.0)
let s3 = t3.transformSize(makeSize(5.0, 5.0))
test.expectF64(s3.width, 10.0)
test.expectF64(s3.height, 15.0)

// NSColor — RGBA round-trip.
let c = NSColor.rgba(0.25, 0.5, 0.75, 1.0)
test.expectF64(c.redComponent, 0.25)
test.expectF64(c.greenComponent, 0.5)
test.expectF64(c.blueComponent, 0.75)
test.expectF64(c.alphaComponent, 1.0)

0
