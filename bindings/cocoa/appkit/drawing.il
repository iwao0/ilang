/// NSBezierPath / NSGraphicsContext / NSAffineTransform — custom
/// drawing primitives. Used inside NSView subclass
/// `drawRect:` overrides for shapes the standard controls
/// don't cover.

use foundation { NSObject, NSPoint, NSSize, NSRect, NSArray }

/// NSBezierPathElement — one record kind in an NSBezierPath's
/// element stream. Returned by `elementAtIndex:associatedPoints:`
/// when iterating an existing path.
pub enum NSBezierPathElement: u64 {
    moveTo    = 0
    lineTo    = 1
    curveTo   = 2
    closePath = 3
    quadraticCurveTo = 4
}

/// NSLineCapStyle — endpoint shape of an open path's stroke.
pub enum NSLineCapStyle: u64 {
    butt   = 0
    round  = 1
    square = 2
}

/// NSLineJoinStyle — corner shape where two stroked segments
/// meet.
pub enum NSLineJoinStyle: u64 {
    miter = 0
    round = 1
    bevel = 2
}

/// NSWindingRule — fill-rule the path uses when filling. Most
/// callers want `nonZero` (the default); `evenOdd` is the SVG
/// "even-odd fill" rule.
pub enum NSWindingRule: u64 {
    nonZero = 0
    evenOdd = 1
}

@extern(ObjC, "/System/Library/Frameworks/AppKit.framework/AppKit") {
    /// `NSBezierPath` — sequence of move-to / line-to / curve-to
    /// segments. Build by calling the typed `moveTo` /
    /// `lineToPoint` / `curveToPoint…` methods, then `stroke()`
    /// or `fill()` inside the enclosing
    /// `NSView.drawRect:` to paint it. Sub-paths are separated
    /// by `closePath`.
    @objc pub class NSBezierPath : NSObject {
        @objc("bezierPath")
        pub static bezierPath(): NSBezierPath
        @objc("bezierPathWithRect:")
        pub static bezierPathWithRect(rect: NSRect): NSBezierPath
        @objc("bezierPathWithOvalInRect:")
        pub static bezierPathWithOvalInRect(rect: NSRect): NSBezierPath
        @objc("bezierPathWithRoundedRect:xRadius:yRadius:")
        pub static bezierPathWithRoundedRect(
            rect: NSRect, xRadius: f64, yRadius: f64
        ): NSBezierPath

        @objc("alloc")
        pub static alloc(): NSBezierPath
        @objc("init")
        pub init(): NSBezierPath

        /// Path construction.
        @objc("moveToPoint:")
        pub moveToPoint(p: NSPoint)
        @objc("lineToPoint:")
        pub lineToPoint(p: NSPoint)
        @objc("curveToPoint:controlPoint1:controlPoint2:")
        pub curveToPoint(p: NSPoint, controlPoint1: NSPoint, controlPoint2: NSPoint)
        @objc("relativeMoveToPoint:")
        pub relativeMoveToPoint(p: NSPoint)
        @objc("relativeLineToPoint:")
        pub relativeLineToPoint(p: NSPoint)
        @objc("closePath")
        pub closePath()

        /// Rect / oval shortcuts (append to current path).
        @objc("appendBezierPathWithRect:")
        pub appendBezierPathWithRect(rect: NSRect)
        @objc("appendBezierPathWithOvalInRect:")
        pub appendBezierPathWithOvalInRect(rect: NSRect)
        @objc("appendBezierPathWithRoundedRect:xRadius:yRadius:")
        pub appendBezierPathWithRoundedRect(
            rect: NSRect, xRadius: f64, yRadius: f64
        )
        @objc("appendBezierPathWithArcWithCenter:radius:startAngle:endAngle:")
        pub appendBezierPathWithArc(
            center: NSPoint, radius: f64, startAngle: f64, endAngle: f64
        )
        @objc("appendBezierPathWithArcWithCenter:radius:startAngle:endAngle:clockwise:")
        pub appendBezierPathWithArcClockwise(
            center: NSPoint, radius: f64,
            startAngle: f64, endAngle: f64, clockwise: bool
        )

        /// Inspection.
        @objc("currentPoint")
        pub get currentPoint(): NSPoint
        @objc("bounds")
        pub get bounds(): NSRect
        @objc("isEmpty")
        pub get isEmpty(): bool
        @objc("elementCount")
        pub get elementCount(): i64

        /// Stroke / fill state.
        @objc("lineWidth")
        pub get lineWidth(): f64
        @objc("setLineWidth:")
        pub set lineWidth(w: f64)
        @objc("lineCapStyle")
        pub get lineCapStyle(): NSLineCapStyle
        @objc("setLineCapStyle:")
        pub set lineCapStyle(s: NSLineCapStyle)
        @objc("lineJoinStyle")
        pub get lineJoinStyle(): NSLineJoinStyle
        @objc("setLineJoinStyle:")
        pub set lineJoinStyle(s: NSLineJoinStyle)
        @objc("miterLimit")
        pub get miterLimit(): f64
        @objc("setMiterLimit:")
        pub set miterLimit(l: f64)
        @objc("flatness")
        pub get flatness(): f64
        @objc("setFlatness:")
        pub set flatness(f: f64)
        @objc("windingRule")
        pub get windingRule(): NSWindingRule
        @objc("setWindingRule:")
        pub set windingRule(r: NSWindingRule)

        /// Drawing.
        @objc("stroke")
        pub stroke()
        @objc("fill")
        pub fill()
        @objc("addClip")
        pub addClip()
        @objc("removeAllPoints")
        pub removeAllPoints()
        @objc("transformUsingAffineTransform:")
        pub transformUsingAffineTransform(t: NSAffineTransform)

        /// Hit-test.
        @objc("containsPoint:")
        pub containsPoint(p: NSPoint): bool
    }

    /// `NSGraphicsContext` — wrapper around the current drawing
    /// destination (a window's backing store during a
    /// `drawRect:`, an off-screen bitmap during a CGContext
    /// save, etc.). Most callers reach for the static
    /// `currentContext` inside their draw method.
    @objc pub class NSGraphicsContext : NSObject {
        @objc("currentContext")
        pub static currentContext(): NSGraphicsContext
        @objc("setCurrentContext:")
        pub static setCurrentContext(c: NSGraphicsContext)

        @objc("saveGraphicsState")
        pub static saveGraphicsState()
        @objc("restoreGraphicsState")
        pub static restoreGraphicsState()

        @objc("flushGraphics")
        pub flushGraphics()
        @objc("shouldAntialias")
        pub get shouldAntialias(): bool
        @objc("setShouldAntialias:")
        pub set shouldAntialias(flag: bool)
    }

    /// `NSAffineTransform` — 2-D affine transform builder.
    /// Useful for rotating / scaling / translating an
    /// `NSBezierPath` before stroking or filling.
    @objc pub class NSAffineTransform : NSObject {
        @objc("transform")
        pub static transform(): NSAffineTransform

        @objc("alloc")
        pub static alloc(): NSAffineTransform
        @objc("init")
        pub init(): NSAffineTransform

        @objc("translateXBy:yBy:")
        pub translateXBy(x: f64, yBy: f64)
        @objc("rotateByDegrees:")
        pub rotateByDegrees(degrees: f64)
        @objc("rotateByRadians:")
        pub rotateByRadians(radians: f64)
        @objc("scaleBy:")
        pub scaleBy(factor: f64)
        @objc("scaleXBy:yBy:")
        pub scaleXBy(x: f64, yBy: f64)

        @objc("concat")
        pub concat()
        @objc("invert")
        pub invert()
        @objc("appendTransform:")
        pub appendTransform(t: NSAffineTransform)
        @objc("prependTransform:")
        pub prependTransform(t: NSAffineTransform)

        @objc("transformPoint:")
        pub transformPoint(p: NSPoint): NSPoint
        @objc("transformSize:")
        pub transformSize(s: NSSize): NSSize
    }
}
